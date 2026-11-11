use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use similar::TextDiff;
use tar::{Archive, Builder, Header};
use thiserror::Error;

use crate::config::{self, ConfigError, UserConfig, identity_path, ssh_config_path};
use crate::patcher::{self, PatchError};
use crate::patches::{PATCHES, TARGETS};

const ASKPASS_SCRIPT: &str = include_str!("../support/ssh-askpass.sh");
const TESTED_FIRMWARE_LABEL: Option<&str> = None;
const MAX_QML_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PROXY_JUMP_HOSTS: usize = 64;

macro_rules! remote_script {
    ($body:literal) => {
        concat!(
            "#!/bin/sh\nset -eu\n",
            include_str!("../preview/manifest-functions.sh"),
            r#"
validate_target_arguments() {
    [ "$#" -gt 0 ] || return 1
    seen=' '
    for rel do
        valid_qml_path "$rel" || return 1
        case "$seen" in *" $rel "*) return 1 ;; esac
        seen="$seen$rel "
    done
}
manifest_paths() {
    awk '{ print $2 }' "$1"
}
manifest_has_path() {
    awk -v name="$2" '$2 == name { found = 1 } END { exit !found }' "$1"
}
manifest_hash() {
    awk -v name="$2" '$2 == name { count++; value = tolower($1) } END { if (count != 1) exit 1; print value }' "$1"
}
contains_path() {
    for candidate in $1; do
        [ "$candidate" = "$2" ] && return 0
    done
    return 1
}
mount_present() {
    awk -v target="$1" '$2 == target { found = 1 } END { exit !found }' /proc/mounts
}
"#,
            $body
        )
    };
}

const PREFLIGHT_SCRIPT: &str = remote_script!(
    r#"
STATE=/var/lib/webosbrew/black-theme
HOOK=/var/lib/webosbrew/init.d/50-black-theme
RUN_MARKER=/run/black-theme-overlay.active
LOCK=/run/black-theme-overlay.lock
if [ "$(id -u)" != 0 ]; then echo "SSH target is not root." >&2; exit 1; fi
if [ -e "$LOCK" ] || [ -e /run/black-theme-update.lock ]; then echo "A theme operation is active; retry after it finishes." >&2; exit 1; fi
if ! command -v awk >/dev/null 2>&1 || ! command -v sha256sum >/dev/null 2>&1 || ! command -v tar >/dev/null 2>&1; then
    echo "Required inspection tools are unavailable." >&2; exit 1
fi
validate_target_arguments "$@" || { echo "Current target arguments are empty, duplicated, or unsafe." >&2; exit 1; }
TARGET_FILES=
for rel do TARGET_FILES="$TARGET_FILES $rel"; done
for recovery in /var/lib/webosbrew/black-theme.previous.* /var/lib/webosbrew/black-theme.failed-overlay.* /var/lib/webosbrew/black-theme.rollback-overlay.*; do
    [ -e "$recovery" ] || continue
    echo "Unresolved update recovery path found; preserve and inspect: $recovery" >&2
    exit 1
done
if [ -e "$STATE" ] || [ -L "$STATE" ]; then
    if [ ! -d "$STATE" ] || [ -L "$STATE" ] || [ ! -d "$STATE/backups" ] || [ -L "$STATE/backups" ] || [ ! -d "$STATE/overlay" ] || [ -L "$STATE/overlay" ] || [ ! -f "$STATE/COMPATIBILITY" ] || [ -L "$STATE/COMPATIBILITY" ] || [ ! -f "$HOOK" ] || [ -L "$HOOK" ]; then
        echo "Existing install state or boot hook is incomplete; refusing update." >&2; exit 1
    fi
    if [ -f "$STATE/disabled" ]; then echo "Theme is disabled; refusing update." >&2; exit 1; fi
    if [ ! -f "$STATE/TARGET-SHA256SUMS" ] || [ ! -f "$STATE/BACKUP-SHA256SUMS" ] || [ -L "$STATE/TARGET-SHA256SUMS" ] || [ -L "$STATE/BACKUP-SHA256SUMS" ]; then
        echo "Existing target or backup manifest is missing/unsafe." >&2; exit 1
    fi
    validate_target_manifest "$STATE/TARGET-SHA256SUMS" 5 || { echo "Existing target manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
    validate_backup_manifest "$STATE/BACKUP-SHA256SUMS" 5 || { echo "Existing backup manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
    if ! (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null); then
        echo "Existing backup manifest does not verify; no update performed." >&2; exit 1
    fi
    INSTALLED_FILES=$(manifest_paths "$STATE/TARGET-SHA256SUMS")
    HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
    for rel in $INSTALLED_FILES; do
        target=/$rel
        if [ ! -f "$target" ] || [ -L "$target" ] || [ ! -f "$STATE/backups/$rel" ] || [ -L "$STATE/backups/$rel" ] || [ ! -f "$STATE/overlay/$rel" ] || [ -L "$STATE/overlay/$rel" ] || ! manifest_has_path "$STATE/BACKUP-SHA256SUMS" "$rel"; then
            echo "Required target, backup, or overlay is missing/unsafe: $target" >&2; exit 1
        fi
        expected=$(manifest_hash "$STATE/TARGET-SHA256SUMS" "$rel") || {
            echo "Target manifest is missing or duplicates $rel." >&2; exit 1;
        }
        actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
        if [ "$expected" != "$actual" ]; then echo "Captured original hash mismatch: $rel" >&2; exit 1; fi
    done
    if [ -f "$RUN_MARKER" ] && [ ! -L "$RUN_MARKER" ]; then
        for rel in $INSTALLED_FILES; do
            mount_present "/$rel" || { echo "Expected overlay mount missing: /$rel" >&2; exit 1; }
        done
        for rel in $HISTORICAL_FILES; do
            if ! contains_path "$INSTALLED_FILES" "$rel" && mount_present "/$rel"; then
                echo "Unexpected mount remains on retired target: /$rel" >&2; exit 1
            fi
        done
        for rel in $TARGET_FILES; do
            if ! contains_path "$INSTALLED_FILES" "$rel" && mount_present "/$rel"; then
                echo "Unexpected mount exists on a new or retired target: /$rel" >&2; exit 1
            fi
        done
    elif [ -e "$RUN_MARKER" ] || [ -L "$RUN_MARKER" ]; then
        echo "Active marker is unsafe." >&2; exit 1
    else
        for rel in $HISTORICAL_FILES; do
            mount_present "/$rel" && { echo "Mount exists without the active marker: /$rel" >&2; exit 1; }
        done
        for rel in $TARGET_FILES; do
            mount_present "/$rel" && { echo "Current target is mounted without the active marker: /$rel" >&2; exit 1; }
        done
        (cd / && sha256sum -c "$STATE/TARGET-SHA256SUMS" >/dev/null) || {
            echo "Live target hashes differ from the captured originals." >&2; exit 1;
        }
    fi
    echo EXISTING
else
    if [ -e "$HOOK" ] || [ -L "$HOOK" ] || [ -e "$RUN_MARKER" ] || [ -L "$RUN_MARKER" ]; then echo "A hook or active marker exists without install state." >&2; exit 1; fi
    [ -d /var/lib/webosbrew/init.d ] && command -v run-parts >/dev/null 2>&1 || { echo "Homebrew init.d/run-parts is unavailable." >&2; exit 1; }
    for rel in $TARGET_FILES; do
        target=/$rel
        if [ ! -f "$target" ] || [ -L "$target" ]; then echo "Expected regular target missing/unsafe: $target" >&2; exit 1; fi
        if mount_present "/$rel"; then echo "Current target is already mounted: /$rel" >&2; exit 1; fi
    done
    echo FRESH
fi
"#
);
const POSTFLIGHT_SCRIPT: &str = remote_script!(
    r#"
STATE=/var/lib/webosbrew/black-theme
HOOK=/var/lib/webosbrew/init.d/50-black-theme
RUN_MARKER=/run/black-theme-overlay.active
LOCK=/run/black-theme-overlay.lock
if [ "$(id -u)" != 0 ] || [ -e "$LOCK" ] || [ -e /run/black-theme-update.lock ]; then echo "Target is not idle root state." >&2; exit 1; fi
[ -d "$STATE" ] && [ ! -L "$STATE" ] && [ -d "$STATE/backups" ] && [ ! -L "$STATE/backups" ] && [ -d "$STATE/overlay" ] && [ ! -L "$STATE/overlay" ] || { echo "Install state/backups missing or unsafe." >&2; exit 1; }
[ -f "$HOOK" ] && [ -x "$HOOK" ] && [ ! -L "$HOOK" ] || { echo "Boot hook missing or unsafe." >&2; exit 1; }
[ -f "$RUN_MARKER" ] && [ ! -L "$RUN_MARKER" ] || { echo "Overlay marker is not active." >&2; exit 1; }
[ ! -f "$STATE/disabled" ] || { echo "Theme is disabled." >&2; exit 1; }
[ -f "$STATE/TARGET-SHA256SUMS" ] && [ ! -L "$STATE/TARGET-SHA256SUMS" ] && [ -f "$STATE/BACKUP-SHA256SUMS" ] && [ ! -L "$STATE/BACKUP-SHA256SUMS" ] || { echo "Target/backup manifest missing or unsafe." >&2; exit 1; }
validate_target_manifest "$STATE/TARGET-SHA256SUMS" 5 || { echo "Target manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
validate_backup_manifest "$STATE/BACKUP-SHA256SUMS" 5 || { echo "Backup manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
(cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null) || { echo "Backup manifest verification failed." >&2; exit 1; }
INSTALLED_FILES=$(manifest_paths "$STATE/TARGET-SHA256SUMS")
HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
for rel in $INSTALLED_FILES; do
    [ -f "$STATE/backups/$rel" ] && [ ! -L "$STATE/backups/$rel" ] && [ -f "$STATE/overlay/$rel" ] && [ ! -L "$STATE/overlay/$rel" ] && manifest_has_path "$STATE/BACKUP-SHA256SUMS" "$rel" || { echo "Backup/overlay file missing or unsafe: $rel" >&2; exit 1; }
    expected=$(manifest_hash "$STATE/TARGET-SHA256SUMS" "$rel") || { echo "Target manifest missing or duplicates $rel." >&2; exit 1; }
    actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
    [ "$expected" = "$actual" ] || { echo "Captured original hash mismatch: $rel" >&2; exit 1; }
    mount_present "/$rel" || { echo "Expected overlay mount missing: /$rel" >&2; exit 1; }
done
for rel in $HISTORICAL_FILES; do
    contains_path "$INSTALLED_FILES" "$rel" && continue
    if mount_present "/$rel"; then echo "Retired target remains mounted: /$rel" >&2; exit 1; fi
done
echo ACTIVE
"#
);
const FETCH_SCRIPT: &str = remote_script!(
    r#"
STATE=/var/lib/webosbrew/black-theme
RUN_MARKER=/run/black-theme-overlay.active
LOCK=/run/black-theme-overlay.lock
if [ "$(id -u)" != 0 ] || [ -e "$LOCK" ] || [ -e /run/black-theme-update.lock ]; then
    echo "Root or idle target state required to fetch source." >&2; exit 1
fi
validate_target_arguments "$@" || { echo "Current target arguments are empty, duplicated, or unsafe." >&2; exit 1; }
TARGET_FILES=
for rel do TARGET_FILES="$TARGET_FILES $rel"; done
if [ -e "$STATE" ] || [ -L "$STATE" ]; then
    [ -d "$STATE" ] && [ ! -L "$STATE" ] && [ -d "$STATE/backups" ] && [ ! -L "$STATE/backups" ] && [ -f "$STATE/TARGET-SHA256SUMS" ] && [ ! -L "$STATE/TARGET-SHA256SUMS" ] && [ -f "$STATE/BACKUP-SHA256SUMS" ] && [ ! -L "$STATE/BACKUP-SHA256SUMS" ] || { echo "Existing backups unavailable or unsafe." >&2; exit 1; }
    validate_target_manifest "$STATE/TARGET-SHA256SUMS" 5 || { echo "Existing target manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
    validate_backup_manifest "$STATE/BACKUP-SHA256SUMS" 5 || { echo "Existing backup manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
    (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null) || { echo "Existing backups failed verification." >&2; exit 1; }
    INSTALLED_FILES=$(manifest_paths "$STATE/TARGET-SHA256SUMS")
    HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
    for rel in $INSTALLED_FILES; do
        [ -f "$STATE/backups/$rel" ] && [ ! -L "$STATE/backups/$rel" ] && manifest_has_path "$STATE/BACKUP-SHA256SUMS" "$rel" || { echo "Installed original backup is missing or unsafe: $rel" >&2; exit 1; }
        expected=$(manifest_hash "$STATE/TARGET-SHA256SUMS" "$rel") || { echo "Installed target manifest is missing or duplicates $rel." >&2; exit 1; }
        actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
        [ "$expected" = "$actual" ] || { echo "Installed captured original hash mismatch: $rel" >&2; exit 1; }
    done
HAS_STATE=1
else
    if [ -e "$RUN_MARKER" ] || [ -L "$RUN_MARKER" ]; then echo "Active marker exists without install state." >&2; exit 1; fi
    HAS_STATE=0
    INSTALLED_FILES=
    HISTORICAL_FILES=
fi
set --
for rel in $TARGET_FILES; do
    if [ "$HAS_STATE" -eq 1 ] && contains_path "$HISTORICAL_FILES" "$rel"; then
        [ -f "$STATE/backups/$rel" ] && [ ! -L "$STATE/backups/$rel" ] || { echo "Retained original backup is missing or unsafe: $rel" >&2; exit 1; }
        set -- "$@" -C "$STATE/backups" "$rel"
    else
        if contains_path "$INSTALLED_FILES" "$rel"; then echo "Installed target has no retained original backup: $rel" >&2; exit 1; fi
        if mount_present "/$rel"; then echo "New target is already mounted and cannot be fetched live: /$rel" >&2; exit 1; fi
        [ -f "/$rel" ] && [ ! -L "/$rel" ] || { echo "New live target is missing or unsafe: /$rel" >&2; exit 1; }
        set -- "$@" -C / "$rel"
    fi
done
tar -czf - "$@"
"#
);
const REFRESH_SCRIPT: &str = remote_script!(
    r#"
STATE=/var/lib/webosbrew/black-theme
RUN_MARKER=/run/black-theme-overlay.active
LOCK=/run/black-theme-overlay.lock
if [ "$(id -u)" != 0 ]; then echo "SSH target is not root." >&2; exit 1; fi
if ! mkdir "$LOCK" 2>/dev/null; then echo "Another theme operation is active." >&2; exit 1; fi
release_lock() { rmdir "$LOCK" 2>/dev/null || true; }
trap 'release_lock' 0
command -v systemctl >/dev/null 2>&1 || { echo "systemctl is unavailable." >&2; exit 1; }
[ -d "$STATE" ] && [ ! -L "$STATE" ] && [ -d "$STATE/backups" ] && [ ! -L "$STATE/backups" ] && [ -d "$STATE/overlay" ] && [ ! -L "$STATE/overlay" ] || { echo "Install state/backups missing or unsafe." >&2; exit 1; }
[ -f "$RUN_MARKER" ] && [ ! -L "$RUN_MARKER" ] && [ -f "$STATE/TARGET-SHA256SUMS" ] && [ ! -L "$STATE/TARGET-SHA256SUMS" ] && [ -f "$STATE/BACKUP-SHA256SUMS" ] && [ ! -L "$STATE/BACKUP-SHA256SUMS" ] || { echo "The overlay is not active or manifests are unavailable." >&2; exit 1; }
validate_target_manifest "$STATE/TARGET-SHA256SUMS" 5 || { echo "Target manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
validate_backup_manifest "$STATE/BACKUP-SHA256SUMS" 5 || { echo "Backup manifest is empty, malformed, duplicated, or unsafe." >&2; exit 1; }
(cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null) || { echo "Backup manifest verification failed." >&2; exit 1; }
INSTALLED_FILES=$(manifest_paths "$STATE/TARGET-SHA256SUMS")
HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
for rel in $INSTALLED_FILES; do
    [ -f "$STATE/backups/$rel" ] && [ ! -L "$STATE/backups/$rel" ] && [ -f "$STATE/overlay/$rel" ] && [ ! -L "$STATE/overlay/$rel" ] && manifest_has_path "$STATE/BACKUP-SHA256SUMS" "$rel" || { echo "Backup/overlay file missing or unsafe: $rel" >&2; exit 1; }
    expected=$(manifest_hash "$STATE/TARGET-SHA256SUMS" "$rel") || { echo "Target manifest missing or duplicates $rel." >&2; exit 1; }
    actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
    [ "$expected" = "$actual" ] || { echo "Captured original hash mismatch: $rel" >&2; exit 1; }
    mount_present "/$rel" || { echo "Expected overlay mount missing: /$rel" >&2; exit 1; }
done
for rel in $HISTORICAL_FILES; do
    contains_path "$INSTALLED_FILES" "$rel" && continue
    if mount_present "/$rel"; then echo "Retired target remains mounted: /$rel" >&2; exit 1; fi
done
systemctl restart surface-manager-daemon.service
systemctl is-active --quiet surface-manager-daemon.service || { echo "Display service did not return to active state." >&2; exit 1; }
[ -f "$RUN_MARKER" ] && [ ! -L "$RUN_MARKER" ] || { echo "Overlay marker missing after display refresh." >&2; exit 1; }
for rel in $INSTALLED_FILES; do
    mount_present "/$rel" || { echo "Overlay mount missing after display refresh: /$rel" >&2; exit 1; }
done
for rel in $HISTORICAL_FILES; do
    contains_path "$INSTALLED_FILES" "$rel" && continue
    if mount_present "/$rel"; then echo "Retired target mounted after display refresh: /$rel" >&2; exit 1; fi
done
echo "surface-manager-daemon.service restarted; all target overlay mounts remain active."
"#
);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InstallMode {
    Fresh,
    Update,
}

impl InstallMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fresh => "fresh install",
            Self::Update => "in-place update",
        }
    }

    fn script(self) -> &'static str {
        match self {
            Self::Fresh => "install.sh",
            Self::Update => "update.sh",
        }
    }
}

#[derive(Debug)]
pub struct DeploymentPlan {
    pub mode: InstallMode,
    pub model: String,
    pub firmware_label: String,
    pub selected_ids: BTreeSet<String>,
    pub selected_labels: Vec<&'static str>,
    pub patched_files: BTreeMap<String, String>,
    pub target_hashes: BTreeMap<String, String>,
    pub diff: String,
    pub structurally_verified: usize,
}

impl DeploymentPlan {
    pub fn firmware_is_officially_tested(&self) -> bool {
        TESTED_FIRMWARE_LABEL.is_some_and(|tested| self.firmware_label.trim() == tested)
    }

    pub fn compatibility_warning(&self) -> String {
        if let Some(tested) = TESTED_FIRMWARE_LABEL {
            if self.firmware_is_officially_tested() {
                return format!(
                    "Firmware label matches maintainer-tested label {tested}; model-specific coverage still needs owner confirmation."
                );
            }
            return format!(
                "Firmware label is unverified. Only {tested} has maintainer test coverage; structural matches alone are not official support."
            );
        }
        "No firmware label is currently registered as officially tested. Structural matches are unverified and require explicit owner confirmation.".to_owned()
    }
}

#[derive(Debug, Error)]
pub enum DeployError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Patch(#[from] PatchError),
    #[error("SSH setup failed: {0}")]
    SshSetup(String),
    #[error("SSH {operation} failed: {detail}")]
    Ssh {
        operation: &'static str,
        detail: String,
    },
    #[error("TV reported an unsafe state: {0}")]
    UnsafeRemoteState(String),
    #[error("source archive is invalid: {0}")]
    InvalidArchive(String),
    #[error("local deployment bundle could not be built: {0}")]
    Bundle(String),
    #[error("firmware is unverified; explicit owner confirmation is required before deployment")]
    UnverifiedFirmware,
    #[error("confirm that root SSH recovery is available before deployment")]
    RecoveryNotConfirmed,
    #[error("current owner approval is required before restarting the display service")]
    RefreshNotConfirmed,
}

pub fn prepare_plan(
    config: &UserConfig,
    selected_ids: &BTreeSet<String>,
) -> Result<DeploymentPlan, DeployError> {
    config::validate(config)?;
    validate_report_field("model", &config.model)?;
    validate_report_field("firmware label", &config.firmware_label)?;
    let mode = preflight(config)?;
    let archive = fetch_sources(config)?;
    if archive.len() > MAX_SOURCE_ARCHIVE_BYTES {
        return Err(DeployError::InvalidArchive(
            "archive exceeded the size limit".into(),
        ));
    }
    let sources = decode_source_archive(&archive)?;
    let patched = patcher::patch_qml(&sources, selected_ids)?;
    let diff = make_diff(&sources, &patched.files);
    let target_hashes = sources
        .iter()
        .map(|(path, source)| (path.clone(), sha256(source.as_bytes())))
        .collect();
    let selected_labels = PATCHES
        .iter()
        .filter(|patch| selected_ids.contains(patch.id))
        .map(|patch| patch.label)
        .collect();
    Ok(DeploymentPlan {
        mode,
        model: config.model.trim().to_owned(),
        firmware_label: config.firmware_label.trim().to_owned(),
        selected_ids: selected_ids.clone(),
        selected_labels,
        patched_files: patched.files,
        target_hashes,
        diff,
        structurally_verified: PATCHES.len(),
    })
}

pub fn deploy_plan(
    config: &UserConfig,
    plan: &DeploymentPlan,
    confirm_unverified: bool,
    confirm_root_recovery: bool,
) -> Result<(), DeployError> {
    if !confirm_root_recovery {
        return Err(DeployError::RecoveryNotConfirmed);
    }
    if !plan.firmware_is_officially_tested() && !confirm_unverified {
        return Err(DeployError::UnverifiedFirmware);
    }
    let archive = build_bundle(config, plan)?;
    let stage = format!(
        "/var/lib/webosbrew/black-theme-transfer-{}-{}",
        std::process::id(),
        unix_seconds()
    );
    let transfer_command = format!("mkdir -m 700 '{stage}' && tar -xzf - -C '{stage}'");
    ssh_input(config, &transfer_command, &archive, "bundle transfer")?;

    let script = plan.mode.script();
    let install_command = format!(
        "if sh '{stage}/{script}' --confirm-target-qml-source --confirm-root-ssh-recovery; then rm -rf '{stage}'; else status=$?; echo 'Deployment bundle preserved for recovery at {stage}.' >&2; exit $status; fi"
    );
    ssh_input(config, &install_command, &[], "root-SSH deployment")?;
    let postflight = ssh_input(
        config,
        "sh -s",
        POSTFLIGHT_SCRIPT.as_bytes(),
        "deployment verification",
    )?;
    if String::from_utf8_lossy(&postflight.stdout).trim() != "ACTIVE" {
        return Err(DeployError::UnsafeRemoteState(
            "deployment completed without the expected active state".into(),
        ));
    }
    Ok(())
}

pub fn restart_display_service(
    config: &UserConfig,
    confirm_owner_approval: bool,
) -> Result<(), DeployError> {
    if !confirm_owner_approval {
        return Err(DeployError::RefreshNotConfirmed);
    }
    ssh_input(
        config,
        "sh -s",
        REFRESH_SCRIPT.as_bytes(),
        "display-service refresh",
    )?;
    Ok(())
}

pub fn manual_report(
    plan: &DeploymentPlan,
    deployment: &str,
    refresh: &str,
    visual_result: &str,
) -> String {
    manual_report_for(
        Some(&plan.model),
        Some(&plan.firmware_label),
        &plan.selected_labels,
        Some(plan.structurally_verified),
        deployment,
        refresh,
        visual_result,
    )
}

pub fn manual_report_for(
    model: Option<&str>,
    firmware_label: Option<&str>,
    selected_labels: &[&str],
    structurally_verified: Option<usize>,
    deployment: &str,
    refresh: &str,
    visual_result: &str,
) -> String {
    let Some(structurally_verified) = structurally_verified else {
        return format!(
            "Midnight UI Patcher report\nNo device inspection completed.\nVisual check: {visual_result}\n"
        );
    };
    let model = safe_report_value(model.unwrap_or_default());
    let firmware = safe_report_value(firmware_label.unwrap_or_default());
    let patch_lines = selected_labels
        .iter()
        .map(|label| format!("- {}", safe_report_value(label)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Midnight UI Patcher report\nModel: {model}\nFirmware label: {firmware}\nStructural checks: {structurally_verified}/{} patch sites matched\nSelected edits:\n{patch_lines}\nDeployment: {deployment}\nDisplay refresh: {refresh}\nVisual check: {visual_result}\n",
        PATCHES.len(),
    )
}

fn target_script_command() -> String {
    let arguments = TARGETS
        .iter()
        .map(|target| format!("'{}'", target.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    format!("sh -s -- {arguments}")
}

fn preflight(config: &UserConfig) -> Result<InstallMode, DeployError> {
    let output = ssh_input(
        config,
        &target_script_command(),
        PREFLIGHT_SCRIPT.as_bytes(),
        "device preflight",
    )?;
    let status = String::from_utf8_lossy(&output.stdout);
    match status.trim() {
        "FRESH" => Ok(InstallMode::Fresh),
        "EXISTING" => Ok(InstallMode::Update),
        _ => Err(DeployError::UnsafeRemoteState(
            "preflight returned an unexpected status".into(),
        )),
    }
}

fn fetch_sources(config: &UserConfig) -> Result<Vec<u8>, DeployError> {
    let output = ssh_input(
        config,
        &target_script_command(),
        FETCH_SCRIPT.as_bytes(),
        "QML source fetch",
    )?;
    Ok(output.stdout)
}

fn decode_source_archive(bytes: &[u8]) -> Result<BTreeMap<String, String>, DeployError> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    let mut sources = BTreeMap::new();
    let mut total = 0_u64;
    let entries = archive
        .entries()
        .map_err(|error| DeployError::InvalidArchive(error.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| DeployError::InvalidArchive(error.to_string()))?;
        if !entry.header().entry_type().is_file() {
            return Err(DeployError::InvalidArchive(
                "only regular QML files are accepted".into(),
            ));
        }
        let path = entry
            .path()
            .map_err(|error| DeployError::InvalidArchive(error.to_string()))?;
        let path = path
            .to_str()
            .ok_or_else(|| DeployError::InvalidArchive("non-UTF-8 path".into()))?;
        let path = path.strip_prefix("./").unwrap_or(path).to_owned();
        if !TARGETS.contains(&path.as_str()) {
            return Err(DeployError::InvalidArchive(format!(
                "unexpected file entry: {path}"
            )));
        }
        let size = entry.size();
        if size > MAX_QML_FILE_BYTES || total.saturating_add(size) > MAX_SOURCE_ARCHIVE_BYTES as u64
        {
            return Err(DeployError::InvalidArchive(
                "source file size limit exceeded".into(),
            ));
        }
        let mut source = String::with_capacity(size as usize);
        entry
            .read_to_string(&mut source)
            .map_err(|error| DeployError::InvalidArchive(error.to_string()))?;
        total += size;
        if sources.insert(path.clone(), source).is_some() {
            return Err(DeployError::InvalidArchive(format!(
                "duplicate file entry: {path}"
            )));
        }
    }
    if sources.len() != TARGETS.len() {
        return Err(DeployError::InvalidArchive(format!(
            "archive must contain each of the {} target files exactly once",
            TARGETS.len()
        )));
    }
    Ok(sources)
}

fn make_diff(sources: &BTreeMap<String, String>, patched: &BTreeMap<String, String>) -> String {
    let mut output = String::new();
    for path in TARGETS {
        let Some(before) = sources.get(path) else {
            continue;
        };
        let Some(after) = patched.get(path) else {
            continue;
        };
        let before_header = format!("original/{path}");
        let after_header = format!("patched/{path}");
        let text_diff = TextDiff::from_lines(before, after);
        let mut diff = text_diff.unified_diff();
        diff.context_radius(3);
        diff.header(&before_header, &after_header);
        output.push_str(&diff.to_string());
    }
    output
}

const MANIFEST_FUNCTIONS_MARKER: &str = "# @include-manifest-functions@";
const MANIFEST_FUNCTIONS: &str = include_str!("../preview/manifest-functions.sh");

fn embed_manifest_functions(script: &str) -> Result<Vec<u8>, DeployError> {
    let Some((prefix, suffix)) = script.split_once(MANIFEST_FUNCTIONS_MARKER) else {
        return Err(DeployError::Bundle(
            "preview script is missing the shared manifest functions marker".into(),
        ));
    };
    if suffix.contains(MANIFEST_FUNCTIONS_MARKER) {
        return Err(DeployError::Bundle(
            "preview script contains multiple shared manifest functions markers".into(),
        ));
    }

    let mut embedded =
        String::with_capacity(prefix.len() + MANIFEST_FUNCTIONS.len() + suffix.len());
    embedded.push_str(prefix);
    embedded.push_str(MANIFEST_FUNCTIONS);
    embedded.push_str(suffix);
    Ok(embedded.into_bytes())
}

fn build_bundle(config: &UserConfig, plan: &DeploymentPlan) -> Result<Vec<u8>, DeployError> {
    let mut files = BTreeMap::<String, (Vec<u8>, u32)>::new();
    files.insert(
        "install.sh".into(),
        (
            embed_manifest_functions(include_str!("../preview/install.sh"))?,
            0o755,
        ),
    );
    files.insert(
        "apply.sh".into(),
        (
            embed_manifest_functions(include_str!("../preview/apply.sh"))?,
            0o755,
        ),
    );
    files.insert(
        "unmount.sh".into(),
        (
            embed_manifest_functions(include_str!("../preview/unmount.sh"))?,
            0o755,
        ),
    );
    files.insert(
        "uninstall.sh".into(),
        (include_bytes!("../preview/uninstall.sh").to_vec(), 0o755),
    );
    files.insert(
        "update.sh".into(),
        (
            embed_manifest_functions(include_str!("../preview/update.sh"))?,
            0o755,
        ),
    );
    files.insert(
        "TARGET-SHA256SUMS".into(),
        (target_manifest(&plan.target_hashes).into_bytes(), 0o644),
    );
    files.insert(
        "COMPATIBILITY".into(),
        (compatibility_file(config, plan).into_bytes(), 0o644),
    );
    for (target, source) in &plan.patched_files {
        files.insert(
            format!("overlay/{target}"),
            (source.as_bytes().to_vec(), 0o644),
        );
    }
    let mut checksums = String::new();
    for (path, (contents, _)) in &files {
        checksums.push_str(&format!("{}  {path}\n", sha256(contents)));
    }
    files.insert("SHA256SUMS".into(), (checksums.into_bytes(), 0o644));

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    for (path, (contents, mode)) in files {
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        builder
            .append_data(&mut header, path, contents.as_slice())
            .map_err(|error| DeployError::Bundle(error.to_string()))?;
    }
    builder
        .finish()
        .map_err(|error| DeployError::Bundle(error.to_string()))?;
    let encoder = builder
        .into_inner()
        .map_err(|error| DeployError::Bundle(error.to_string()))?;
    encoder
        .finish()
        .map_err(|error| DeployError::Bundle(error.to_string()))
}

fn target_manifest(hashes: &BTreeMap<String, String>) -> String {
    let mut output = String::new();
    for target in TARGETS {
        if let Some(hash) = hashes.get(target) {
            output.push_str(&format!("{hash}  {target}\n"));
        }
    }
    output
}

fn compatibility_file(config: &UserConfig, plan: &DeploymentPlan) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Model (owner supplied): {}\n",
        safe_report_value(&config.model)
    ));
    output.push_str(&format!(
        "Firmware label (owner supplied): {}\n",
        safe_report_value(&config.firmware_label)
    ));
    output.push_str(&format!(
        "Compatibility: {}\n",
        plan.compatibility_warning()
    ));
    output.push_str(&format!(
        "Structural checks: all {} patch sites matched on fetched source.\nSelected patches:\n",
        PATCHES.len()
    ));
    for id in &plan.selected_ids {
        if let Some(patch) = PATCHES.iter().find(|patch| patch.id == id) {
            output.push_str(&format!("{} — {}\n", patch.id, patch.label));
        }
    }
    output
}

fn ssh_common_command(config: &UserConfig) -> Result<Command, DeployError> {
    config::validate(config)?;
    if config.ssh_target.is_empty() {
        return Err(DeployError::SshSetup(
            "set ssh_target in the user config first".into(),
        ));
    }
    let mut command = Command::new("ssh");
    if let Some(ssh_config) = ssh_config_path(config) {
        if !ssh_config.is_file() {
            return Err(DeployError::SshSetup(
                "configured OpenSSH config file does not exist".into(),
            ));
        }
        command.arg("-F").arg(ssh_config);
    }
    command
        .arg("-T")
        .args(["-o", "BatchMode=no"])
        .args(["-o", "NumberOfPasswordPrompts=1"])
        .args(["-o", "StrictHostKeyChecking=ask"])
        .args(["-o", "ConnectTimeout=10"])
        .args(["-o", "ServerAliveInterval=15"])
        .args(["-o", "LogLevel=ERROR"]);
    Ok(command)
}

fn configured_ssh_command(config: &UserConfig) -> Result<Command, DeployError> {
    let mut command = ssh_common_command(config)?;
    if config.port != 0 {
        command.arg("-p").arg(config.port.to_string());
    }
    if let Some(identity) = identity_path(config) {
        if !identity.is_file() {
            return Err(DeployError::SshSetup(
                "configured identity file does not exist".into(),
            ));
        }
        command.arg("-i").arg(identity);
    }
    Ok(command)
}

fn ssh_config_output(mut command: Command, target: &str) -> Result<Vec<u8>, DeployError> {
    let output = command.arg("-G").arg(target).output().map_err(|error| {
        DeployError::SshSetup(format!(
            "OpenSSH could not resolve connection settings: {error}"
        ))
    })?;
    if !output.status.success() {
        return Err(DeployError::SshSetup(
            "OpenSSH could not resolve connection settings".into(),
        ));
    }
    Ok(output.stdout)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProxyJumpTarget {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

fn proxy_jump_targets(output: &[u8]) -> Result<Vec<ProxyJumpTarget>, DeployError> {
    let output = std::str::from_utf8(output).map_err(|_| {
        DeployError::SshSetup("OpenSSH returned invalid resolved connection settings".into())
    })?;
    let jumps = output
        .lines()
        .find_map(|line| line.strip_prefix("proxyjump "))
        .map(str::trim)
        .unwrap_or("none");
    if jumps.is_empty() || jumps == "none" {
        return Ok(Vec::new());
    }
    jumps
        .split(',')
        .map(|jump| {
            let jump = jump.trim();
            let (user, authority) = match jump.rsplit_once('@') {
                Some((user, authority)) if !user.is_empty() => (Some(user.to_owned()), authority),
                Some(_) => return Err(invalid_proxy_jump()),
                None => (None, jump),
            };
            let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
                let close = bracketed.find(']').ok_or_else(invalid_proxy_jump)?;
                let host = bracketed[..close].to_owned();
                let suffix = &bracketed[close + 1..];
                let port = if suffix.is_empty() {
                    None
                } else if let Some(port) = suffix.strip_prefix(':') {
                    Some(parse_proxy_jump_port(port)?)
                } else {
                    return Err(invalid_proxy_jump());
                };
                (host, port)
            } else if let Some((host, port)) = authority.rsplit_once(':') {
                if host.contains(':') {
                    return Err(invalid_proxy_jump());
                }
                (host.to_owned(), Some(parse_proxy_jump_port(port)?))
            } else {
                (authority.to_owned(), None)
            };
            if host.is_empty() {
                return Err(invalid_proxy_jump());
            }
            Ok(ProxyJumpTarget { host, user, port })
        })
        .collect()
}

fn parse_proxy_jump_port(port: &str) -> Result<u16, DeployError> {
    port.parse().map_err(|_| invalid_proxy_jump())
}

fn invalid_proxy_jump() -> DeployError {
    DeployError::SshSetup("OpenSSH returned an invalid ProxyJump setting".into())
}

fn hash_ssh_config_output(hasher: &mut Sha256, output: &[u8]) {
    hasher.update((output.len() as u64).to_be_bytes());
    hasher.update(output);
}

fn hash_selected_ssh_config(hasher: &mut Sha256, config: &UserConfig) -> Result<(), DeployError> {
    let Some(path) = ssh_config_path(config) else {
        return Ok(());
    };
    let mut file = std::fs::File::open(path).map_err(|_| {
        DeployError::SshSetup("configured OpenSSH config file could not be read".into())
    })?;
    let length = file
        .metadata()
        .map_err(|_| {
            DeployError::SshSetup("configured OpenSSH config file could not be read".into())
        })?
        .len();
    hasher.update(length.to_be_bytes());
    let mut buffer = [0; 8192];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            DeployError::SshSetup("configured OpenSSH config file could not be read".into())
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

pub fn ssh_routing_fingerprint(config: &UserConfig) -> Result<[u8; 32], DeployError> {
    let destination = ssh_config_output(configured_ssh_command(config)?, &config.ssh_target)?;
    let mut hasher = Sha256::new();
    hash_ssh_config_output(&mut hasher, &destination);
    hash_selected_ssh_config(&mut hasher, config)?;

    let mut pending_jumps = proxy_jump_targets(&destination)?;
    let mut visited_jumps = BTreeSet::new();
    while let Some(jump) = pending_jumps.pop() {
        if !visited_jumps.insert(jump.clone()) {
            continue;
        }
        if visited_jumps.len() > MAX_PROXY_JUMP_HOSTS {
            return Err(DeployError::SshSetup(
                "OpenSSH ProxyJump chain exceeds the supported limit".into(),
            ));
        }
        let mut command = ssh_common_command(config)?;
        if let Some(user) = jump.user.as_deref() {
            command.arg("-l").arg(user);
        }
        if let Some(port) = jump.port {
            command.arg("-p").arg(port.to_string());
        }
        let settings = ssh_config_output(command, &jump.host)?;
        hash_ssh_config_output(&mut hasher, &settings);
        pending_jumps.extend(proxy_jump_targets(&settings)?);
    }
    Ok(hasher.finalize().into())
}

fn ssh_input(
    config: &UserConfig,
    remote_command: &str,
    input: &[u8],
    operation: &'static str,
) -> Result<Output, DeployError> {
    let mut command = configured_ssh_command(config)?;
    let askpass = prepare_askpass()?;
    command
        .arg(&config.ssh_target)
        .arg(remote_command)
        .env("SSH_ASKPASS", askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(
            "DISPLAY",
            std::env::var_os("DISPLAY").unwrap_or_else(|| "1".into()),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| DeployError::SshSetup(format!("OpenSSH could not start: {error}")))?;
    if !input.is_empty() {
        child
            .stdin
            .as_mut()
            .ok_or_else(|| DeployError::SshSetup("OpenSSH input pipe unavailable".into()))?
            .write_all(input)
            .map_err(|error| {
                DeployError::SshSetup(format!("could not send remote operation: {error}"))
            })?;
    }
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| DeployError::SshSetup(format!("OpenSSH did not complete: {error}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        let detail = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(3000)
            .collect::<String>();
        Err(DeployError::Ssh {
            operation,
            detail: if detail.is_empty() {
                "OpenSSH command failed".into()
            } else {
                detail
            },
        })
    }
}

fn prepare_askpass() -> Result<PathBuf, DeployError> {
    #[cfg(target_os = "macos")]
    {
        let config_path = config::config_path()?;
        let directory = config_path
            .parent()
            .ok_or_else(|| DeployError::SshSetup("user config path unavailable".into()))?;
        write_askpass_helper(directory)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ASKPASS_SCRIPT;
        Err(DeployError::SshSetup("one-time SSH prompts are currently supported only on macOS; use an SSH agent on this platform".into()))
    }
}

#[cfg(any(target_os = "macos", test))]
fn write_askpass_helper(directory: &Path) -> Result<PathBuf, DeployError> {
    config::ensure_private_directory(directory)?;
    let path = directory.join("ssh-askpass.sh");
    std::fs::write(&path, ASKPASS_SCRIPT).map_err(|error| {
        DeployError::SshSetup(format!("could not create SSH prompt helper: {error}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| DeployError::SshSetup(format!("could not protect SSH prompt helper: {error}")),
        )?;
    }
    Ok(path)
}

fn validate_report_field(name: &str, value: &str) -> Result<(), DeployError> {
    if value.len() > 160 || value.chars().any(|character| character.is_control()) {
        return Err(DeployError::SshSetup(format!(
            "{name} must be at most 160 printable characters"
        )));
    }
    Ok(())
}

fn safe_report_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect::<String>()
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_script_command_passes_every_declared_target() {
        let arguments = TARGETS
            .iter()
            .map(|target| format!("'{target}'"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(target_script_command(), format!("sh -s -- {arguments}"));
    }

    fn manifest_validator_script(source: &str, minimum_depth: u8) -> String {
        let start_marker = "# BEGIN shared manifest functions";
        let end_marker = "# END shared manifest functions";
        let start = source
            .find(start_marker)
            .expect("script must embed the shared manifest functions");
        let end = source[start..]
            .find(end_marker)
            .map(|offset| start + offset + end_marker.len())
            .expect("shared manifest functions must have an end marker");
        let invocation = if minimum_depth == 4 {
            "validate_backup_manifest \"$1\"".to_owned()
        } else {
            format!("validate_backup_manifest \"$1\" {minimum_depth}")
        };
        format!("{}\n{invocation}\n", &source[start..end])
    }

    fn assert_backup_manifest_validator(source: &str, minimum_depth: u8, target: &str) {
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::current_dir()
            .unwrap()
            .join(".build")
            .join(format!(
                "shared-backup-manifest-{}-{nonce}",
                std::process::id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let manifest = root.join("BACKUP-SHA256SUMS");
        let hash = "0".repeat(64);
        let script = manifest_validator_script(source, minimum_depth);
        std::fs::write(
            &manifest,
            format!("{hash}  {target}\n{hash}  pre-update-20261111-1234/overlay/{target}\n"),
        )
        .unwrap();
        let validate = || {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(&script)
                .arg("validate_backup_manifest")
                .arg(&manifest)
                .output()
                .unwrap()
        };
        let output = validate();
        assert!(
            output.status.success(),
            "valid pre-update backup path rejected: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        std::fs::write(
            &manifest,
            format!(
                "{hash}  {target}\n{hash}  pre-update-20261111-1234/overlay/usr/lib/qml/../Legacy.qml\n"
            ),
        )
        .unwrap();
        let output = validate();
        assert!(
            !output.status.success(),
            "traversal path unexpectedly accepted"
        );

        if minimum_depth == 5 {
            std::fs::write(
                &manifest,
                format!(
                    "{hash}  usr/lib/qml/Legacy.qml\n{hash}  pre-update-20261111-1234/overlay/usr/lib/qml/Legacy.qml\n"
                ),
            )
            .unwrap();
            let output = validate();
            assert!(
                !output.status.success(),
                "remote policy unexpectedly accepted a depth-four QML path"
            );
        }
    }

    #[test]
    fn remote_shared_backup_manifest_validator_preserves_remote_path_policy() {
        assert_backup_manifest_validator(
            PREFLIGHT_SCRIPT,
            5,
            "usr/lib/qml/QuickSettings/Legacy.qml",
        );
    }

    #[test]
    fn askpass_helper_creates_a_private_directory_on_first_use() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "askpass-dir-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let directory = root.join("managed-config");
        assert!(!directory.exists());

        let path = write_askpass_helper(&directory).unwrap();
        assert!(path.is_file());
        assert!(!directory.join("config.toml").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn proxy_jump_parser_handles_aliases_users_ports_and_ipv6() {
        assert_eq!(
            proxy_jump_targets(b"proxyjump jump-a,root@jump-b:2222,[2001:db8::1]:2200\n").unwrap(),
            vec![
                ProxyJumpTarget {
                    host: "jump-a".into(),
                    user: None,
                    port: None,
                },
                ProxyJumpTarget {
                    host: "jump-b".into(),
                    user: Some("root".into()),
                    port: Some(2222),
                },
                ProxyJumpTarget {
                    host: "2001:db8::1".into(),
                    user: None,
                    port: Some(2200),
                },
            ]
        );
        assert!(proxy_jump_targets(b"proxyjump none\n").unwrap().is_empty());
        assert!(proxy_jump_targets(b"proxyjump user@[2001:db8::1:2200\n").is_err());
    }

    fn source_archive(files: &[(&str, &str)]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);
        for (path, contents) in files {
            let mut header = Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, *path, contents.as_bytes())
                .unwrap();
        }
        builder.finish().unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn valid_source_entries() -> Vec<(&'static str, &'static str)> {
        TARGETS
            .iter()
            .copied()
            .map(|path| (path, "Item {}\\n"))
            .collect()
    }

    #[test]
    fn source_archive_requires_each_target_exactly_once() {
        let entries = valid_source_entries();
        let archive = source_archive(&entries);
        let decoded = decode_source_archive(&archive).unwrap();
        assert_eq!(decoded.len(), TARGETS.len());
        assert!(TARGETS.iter().all(|path| {
            decoded
                .get(*path)
                .is_some_and(|source| source == "Item {}\\n")
        }));

        let missing = source_archive(&entries[..entries.len() - 1]);
        assert!(matches!(
            decode_source_archive(&missing),
            Err(DeployError::InvalidArchive(_))
        ));

        let mut duplicate = entries.clone();
        duplicate.push(entries[0]);
        let duplicate = source_archive(&duplicate);
        assert!(matches!(
            decode_source_archive(&duplicate),
            Err(DeployError::InvalidArchive(_))
        ));

        let mut extra = entries;
        extra.push(("usr/lib/qml/SystemUI/Notification/Toast.qml", "Item {}\\n"));
        let extra = source_archive(&extra);
        assert!(matches!(
            decode_source_archive(&extra),
            Err(DeployError::InvalidArchive(_))
        ));
    }

    #[test]
    fn deployment_bundle_contains_only_patched_targets_and_valid_checksums() {
        let config = UserConfig {
            model: "owner model".into(),
            firmware_label: "owner firmware".into(),
            ..UserConfig::default()
        };
        let selected_ids = BTreeSet::from([PATCHES[0].id.to_owned()]);
        let plan = DeploymentPlan {
            mode: InstallMode::Fresh,
            model: config.model.clone(),
            firmware_label: config.firmware_label.clone(),
            selected_ids,
            selected_labels: vec![PATCHES[0].label],
            patched_files: TARGETS
                .iter()
                .map(|path| ((*path).to_owned(), "patched QML\\n".to_owned()))
                .collect(),
            target_hashes: TARGETS
                .iter()
                .map(|path| ((*path).to_owned(), "0".repeat(64)))
                .collect(),
            diff: String::new(),
            structurally_verified: PATCHES.len(),
        };
        let bundle = build_bundle(&config, &plan).unwrap();
        let decoder = GzDecoder::new(Cursor::new(bundle));
        let mut archive = Archive::new(decoder);
        let mut files = BTreeMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).unwrap();
            assert!(files.insert(path, contents).is_none());
        }
        let manifest = String::from_utf8(files.remove("SHA256SUMS").unwrap()).unwrap();
        assert_eq!(files.len(), 7 + TARGETS.len());
        for line in manifest.lines() {
            let (expected, path) = line.split_once("  ").unwrap();
            assert_eq!(sha256(files.get(path).unwrap()), expected);
        }
        assert_eq!(manifest.lines().count(), files.len());
        assert!(files.keys().all(|path| !path.starts_with("base/")));
        for script_name in ["install.sh", "apply.sh", "unmount.sh", "update.sh"] {
            let bundled_script = std::str::from_utf8(files.get(script_name).unwrap()).unwrap();
            assert_backup_manifest_validator(bundled_script, 4, "usr/lib/qml/Legacy.qml");
        }
        for retired in [
            "usr/lib/qml/QuickSettings/Utils/Style.qml",
            "usr/lib/qml/SystemUI/Notification/NotificationStyle.qml",
            "usr/lib/qml/SystemUI/Notification/Toast.qml",
        ] {
            assert!(!files.contains_key(&format!("overlay/{retired}")));
        }
        for target in TARGETS {
            assert_eq!(
                files.get(&format!("overlay/{target}")).unwrap(),
                b"patched QML\\n"
            );
        }
    }
    #[test]
    fn deployment_and_refresh_require_distinct_owner_confirmations() {
        let config = UserConfig::default();
        let plan = DeploymentPlan {
            mode: InstallMode::Fresh,
            model: String::new(),
            firmware_label: String::new(),
            selected_ids: BTreeSet::new(),
            selected_labels: Vec::new(),
            patched_files: BTreeMap::new(),
            target_hashes: BTreeMap::new(),
            diff: String::new(),
            structurally_verified: 0,
        };

        assert!(matches!(
            deploy_plan(&config, &plan, false, false),
            Err(DeployError::RecoveryNotConfirmed)
        ));
        assert!(matches!(
            deploy_plan(&config, &plan, false, true),
            Err(DeployError::UnverifiedFirmware)
        ));
        assert!(matches!(
            restart_display_service(&config, false),
            Err(DeployError::RefreshNotConfirmed)
        ));
    }
}
