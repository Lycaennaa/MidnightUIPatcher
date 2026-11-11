use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use midnight_ui_patcher::patches::TARGETS;
use sha2::{Digest, Sha256};

pub(super) const PREVIOUS_TARGETS: [&str; 5] = [
    "usr/lib/qml/QuickSettings/Utils/Style.qml",
    "usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml",
    "usr/lib/qml/SystemUI/Notification/NotificationStyle.qml",
    "usr/lib/qml/SystemUI/Notification/Toast.qml",
    "usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml",
];

pub(super) fn old_targets_for_added_target_migration() -> Vec<&'static str> {
    assert!(
        !TARGETS.is_empty(),
        "migration fixture needs a current target"
    );
    if TARGETS.len() > 1 {
        return TARGETS[..TARGETS.len() - 1].to_vec();
    }

    let previous_target = PREVIOUS_TARGETS
        .iter()
        .copied()
        .find(|target| !TARGETS.contains(target))
        .expect("migration fixture needs a target absent from the current set");
    vec![previous_target]
}

pub(super) struct Scenario {
    pub(super) root: PathBuf,
    pub(super) webosbrew: String,
    pub(super) run: String,
    pub(super) qml: String,
    pub(super) mounts: String,
    pub(super) package: PathBuf,
    pub(super) bin: PathBuf,
    pub(super) old_targets: Vec<&'static str>,
    pub(super) old_manifest: String,
    pub(super) backup_manifest: String,
    pub(super) current_manifest: String,
    pub(super) old_apply: String,
    pub(super) old_unmount: String,
    pub(super) old_uninstall: String,
}

impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_file(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn write_script(path: &Path, contents: &str) {
    write_file(path, contents.as_bytes());
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn replace_all(mut script: String, from: &str, to: &str) -> String {
    assert!(script.contains(from), "test seam missing: {from}");
    script = script.replace(from, to);
    script
}

fn embed_manifest_functions(source: &str) -> String {
    replace_all(
        source.to_owned(),
        "# @include-manifest-functions@",
        include_str!("../../preview/manifest-functions.sh"),
    )
}

fn sandbox_update_script() -> String {
    let mut script = embed_manifest_functions(include_str!("../../preview/update.sh"));
    script = replace_all(
        script,
        "ROOT=/var/lib/webosbrew",
        "ROOT=\"$TEST_WEBOSBREW\"",
    );
    script = replace_all(
        script,
        "RUN_MARKER=/run/black-theme-overlay.active",
        "RUN_MARKER=\"$TEST_RUN/black-theme-overlay.active\"",
    );
    script = replace_all(
        script,
        "OP_LOCK=/run/black-theme-overlay.lock",
        "OP_LOCK=\"$TEST_RUN/black-theme-overlay.lock\"",
    );
    script = replace_all(script, "! -d /run", "! -d \"$TEST_RUN\"");
    script = replace_all(script, "! -w /run", "! -w \"$TEST_RUN\"");
    script = replace_all(script, "/proc/mounts", "\"$TEST_MOUNTS\"");
    script = replace_all(script, "cd / &&", "cd \"$TEST_QML_ROOT\" &&");
    script = replace_all(script, "target=/$rel", "target=$TEST_QML_ROOT/$rel");
    replace_all(script, "\"/$rel\"", "\"$TEST_QML_ROOT/$rel\"")
}

fn sandbox_runtime_script(source: &str, state: bool) -> String {
    let mut script = embed_manifest_functions(source);
    if state {
        script = replace_all(
            script,
            "STATE=/var/lib/webosbrew/black-theme",
            "STATE=\"$TEST_WEBOSBREW/black-theme\"",
        );
    }
    script = replace_all(
        script,
        "RUN_MARKER=/run/black-theme-overlay.active",
        "RUN_MARKER=\"$TEST_RUN/black-theme-overlay.active\"",
    );
    script = replace_all(
        script,
        "LOCK_DIR=/run/black-theme-overlay.lock",
        "LOCK_DIR=\"$TEST_RUN/black-theme-overlay.lock\"",
    );
    script = replace_all(script, "target=/$rel", "target=$TEST_QML_ROOT/$rel");
    script = replace_all(script, "/proc/mounts", "\"$TEST_MOUNTS\"");
    script = script.replace("cd / &&", "cd \"$TEST_QML_ROOT\" &&");
    script = script.replace("\"/$rel\"", "\"$TEST_QML_ROOT/$rel\"");
    script
}

pub(super) fn build_scenario(old_targets: &[&'static str], fail_new_mount: bool) -> Scenario {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_relative =
        PathBuf::from(".build").join(format!("update-migration-{}-{nonce}", std::process::id()));
    let root = workspace.join(&root_relative);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let webosbrew = root.join("webosbrew").to_string_lossy().into_owned();
    let run = root.join("run").to_string_lossy().into_owned();
    let qml = root_relative.join("qml").to_string_lossy().into_owned();
    let mounts = root.join("run/mounts").to_string_lossy().into_owned();
    let package = root.join("package");
    let bin = root.join("bin");
    let state = root.join("webosbrew/black-theme");
    let qml_root = root.join("qml");

    let managed_targets = old_targets
        .iter()
        .copied()
        .chain(TARGETS.iter().copied())
        .collect::<BTreeSet<_>>();
    let mut originals = BTreeMap::new();
    for target in managed_targets {
        let original = format!("original:{target}\n");
        originals.insert(target.to_owned(), original.clone());
        write_file(&qml_root.join(target), original.as_bytes());
    }

    let mut old_manifest = String::new();
    let mut backup_manifest = String::new();
    let mut mounts_contents = String::new();
    for target in old_targets {
        let original = originals.get(*target).unwrap();
        write_file(&state.join("backups").join(*target), original.as_bytes());
        write_file(
            &state.join("overlay").join(*target),
            format!("old-overlay:{target}\n").as_bytes(),
        );
        let digest = sha256(original.as_bytes());
        old_manifest.push_str(&format!("{digest}  {target}\n"));
        backup_manifest.push_str(&format!("{digest}  {target}\n"));
        mounts_contents.push_str(&format!("none {qml}/{target} bind\n"));
    }
    let current_manifest = TARGETS
        .iter()
        .map(|target| {
            let original = originals.get(*target).unwrap();
            format!("{}  {target}\n", sha256(original.as_bytes()))
        })
        .collect::<String>();
    write_file(&state.join("TARGET-SHA256SUMS"), old_manifest.as_bytes());
    write_file(&state.join("BACKUP-SHA256SUMS"), backup_manifest.as_bytes());
    write_file(&state.join("COMPATIBILITY"), b"previous compatibility\n");
    let old_uninstall = "#!/bin/sh\nexit 0\n".to_owned();
    write_file(&state.join("uninstall.sh"), old_uninstall.as_bytes());
    write_file(
        &root.join("webosbrew/init.d/50-black-theme"),
        b"#!/bin/sh\n",
    );
    write_file(&root.join("run/black-theme-overlay.active"), b"");
    write_file(&root.join("run/mounts"), mounts_contents.as_bytes());

    let old_files = old_targets.join("\n");
    let old_apply = format!(
        "#!/bin/sh\nset -eu\nSTATE=\"$TEST_WEBOSBREW/black-theme\"\nRUN_MARKER=\"$TEST_RUN/black-theme-overlay.active\"\nFILES='\n{old_files}\n'\nfor rel in $FILES; do mount --bind \"$STATE/overlay/$rel\" \"$TEST_QML_ROOT/$rel\"; done\n: > \"$RUN_MARKER\"\n"
    );
    let old_unmount = format!(
        "#!/bin/sh\nset -eu\nRUN_MARKER=\"$TEST_RUN/black-theme-overlay.active\"\nFILES='\n{old_files}\n'\n[ -f \"$RUN_MARKER\" ] || exit 0\nfor rel in $FILES; do umount \"$TEST_QML_ROOT/$rel\"; done\nrm -f \"$RUN_MARKER\"\n"
    );
    write_script(&state.join("apply.sh"), &old_apply);
    write_script(&state.join("unmount.sh"), &old_unmount);

    let mut payloads = BTreeMap::<String, Vec<u8>>::new();
    payloads.insert(
        "install.sh".into(),
        embed_manifest_functions(include_str!("../../preview/install.sh")).into_bytes(),
    );
    payloads.insert(
        "apply.sh".into(),
        sandbox_runtime_script(include_str!("../../preview/apply.sh"), true).into_bytes(),
    );
    payloads.insert(
        "unmount.sh".into(),
        sandbox_runtime_script(include_str!("../../preview/unmount.sh"), false).into_bytes(),
    );
    payloads.insert(
        "uninstall.sh".into(),
        include_str!("../../preview/uninstall.sh")
            .as_bytes()
            .to_vec(),
    );
    payloads.insert("update.sh".into(), sandbox_update_script().into_bytes());
    payloads.insert(
        "TARGET-SHA256SUMS".into(),
        current_manifest.as_bytes().to_vec(),
    );
    payloads.insert("COMPATIBILITY".into(), b"new compatibility\n".to_vec());
    let failed_target = TARGETS.last().copied();
    for target in TARGETS {
        let contents = if fail_new_mount && Some(target) == failed_target {
            "FAIL_NEW\n".to_owned()
        } else {
            format!("new-overlay:{target}\n")
        };
        payloads.insert(format!("overlay/{target}"), contents.into_bytes());
    }
    let mut package_checksums = String::new();
    for (name, contents) in &payloads {
        package_checksums.push_str(&format!("{}  {name}\n", sha256(contents)));
        write_file(&package.join(name), contents);
    }
    write_file(&package.join("SHA256SUMS"), package_checksums.as_bytes());

    write_script(&bin.join("id"), "#!/bin/sh\nprintf '0\\n'\n");
    write_script(
        &bin.join("sha256sum"),
        "#!/bin/sh\nset -eu\nif [ \"${1-}\" = \"-c\" ]; then\n    manifest=$2\n    while IFS=' ' read -r expected path; do\n        [ -n \"$expected\" ] || continue\n        actual=$(shasum -a 256 \"$path\" | awk '{print $1}')\n        if [ \"$actual\" != \"$expected\" ]; then echo \"$path: FAILED\" >&2; exit 1; fi\n    done < \"$manifest\"\nelse\n    for path in \"$@\"; do shasum -a 256 \"$path\"; done\nfi\n",
    );
    write_script(
        &bin.join("mount"),
        "#!/bin/sh\nset -eu\n[ \"$1\" = \"--bind\" ] || exit 2\nsource=$2\ntarget=$3\nif [ \"$(cat \"$source\")\" = \"FAIL_NEW\" ]; then exit 1; fi\nprintf 'none %s bind\\n' \"$target\" >> \"$TEST_MOUNTS\"\n",
    );
    write_script(
        &bin.join("umount"),
        "#!/bin/sh\nset -eu\ntarget=$1\nawk -v target=\"$target\" '$2 == target { found=1 } END { exit !found }' \"$TEST_MOUNTS\" || exit 1\nawk -v target=\"$target\" '$2 != target { print }' \"$TEST_MOUNTS\" > \"$TEST_MOUNTS.tmp\"\nmv \"$TEST_MOUNTS.tmp\" \"$TEST_MOUNTS\"\n",
    );

    Scenario {
        root,
        webosbrew,
        run,
        qml,
        mounts,
        package,
        bin,
        old_targets: old_targets.to_vec(),
        old_manifest,
        backup_manifest,
        current_manifest,
        old_apply,
        old_unmount,
        old_uninstall,
    }
}

pub(super) fn run_update(scenario: &Scenario) -> Output {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("/bin/sh")
        .arg(scenario.package.join("update.sh"))
        .arg("--confirm-target-qml-source")
        .arg("--confirm-root-ssh-recovery")
        .current_dir(&workspace)
        .env("TEST_WEBOSBREW", &scenario.webosbrew)
        .env("TEST_RUN", &scenario.run)
        .env("TEST_QML_ROOT", &scenario.qml)
        .env("TEST_MOUNTS", &scenario.mounts)
        .env("PATH", format!("{}:/usr/bin:/bin", scenario.bin.display()))
        .output()
        .unwrap();
    output
}

pub(super) fn mount_targets(scenario: &Scenario) -> BTreeSet<String> {
    fs::read_to_string(scenario.root.join("run/mounts"))
        .unwrap()
        .lines()
        .map(|line| line.split_whitespace().nth(1).unwrap().to_owned())
        .collect()
}

pub(super) fn expected_mount_targets(scenario: &Scenario, targets: &[&str]) -> BTreeSet<String> {
    targets
        .iter()
        .map(|target| format!("{}/{target}", scenario.qml))
        .collect()
}

pub(super) fn verify_backup_manifest(scenario: &Scenario) {
    let output = Command::new(scenario.bin.join("sha256sum"))
        .arg("-c")
        .arg(
            scenario
                .root
                .join("webosbrew")
                .join("black-theme/BACKUP-SHA256SUMS"),
        )
        .current_dir(scenario.root.join("webosbrew").join("black-theme/backups"))
        .env("PATH", format!("{}:/usr/bin:/bin", scenario.bin.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn direct_backup_targets(scenario: &Scenario) -> BTreeSet<String> {
    fs::read_to_string(
        scenario
            .root
            .join("webosbrew/black-theme/BACKUP-SHA256SUMS"),
    )
    .unwrap()
    .lines()
    .filter_map(|line| {
        let path = line.split_whitespace().nth(1)?;
        path.starts_with("usr/lib/qml/").then(|| path.to_owned())
    })
    .collect()
}

pub(super) fn expected_original_targets(scenario: &Scenario) -> Vec<&'static str> {
    scenario
        .old_targets
        .iter()
        .copied()
        .chain(TARGETS.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn assert_original_backups(scenario: &Scenario, targets: &[&str]) {
    let expected = targets
        .iter()
        .map(|target| (*target).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(direct_backup_targets(scenario), expected);
    let backups = scenario.root.join("webosbrew/black-theme/backups");
    for target in targets {
        assert_eq!(
            fs::read_to_string(backups.join(*target)).unwrap(),
            format!("original:{target}\n"),
            "captured original changed for {target}"
        );
    }
}

pub(super) fn assert_overlay_targets(scenario: &Scenario, targets: &[&str], content_prefix: &str) {
    let expected = targets.iter().copied().collect::<BTreeSet<_>>();
    let managed = scenario
        .old_targets
        .iter()
        .copied()
        .chain(TARGETS.iter().copied())
        .collect::<BTreeSet<_>>();
    let overlay = scenario.root.join("webosbrew/black-theme/overlay");
    for target in managed {
        let path = overlay.join(target);
        if expected.contains(target) {
            assert_eq!(
                fs::read_to_string(path).unwrap(),
                format!("{content_prefix}:{target}\n"),
                "unexpected overlay contents for {target}"
            );
        } else {
            assert!(!path.exists(), "retired overlay remains for {target}");
        }
    }
}

pub(super) fn replace_package_target_manifest(scenario: &Scenario, manifest: &str) {
    write_file(
        &scenario.package.join("TARGET-SHA256SUMS"),
        manifest.as_bytes(),
    );
    let checksums_path = scenario.package.join("SHA256SUMS");
    let checksums = fs::read_to_string(&checksums_path).unwrap();
    let mut updated = String::new();
    let mut replaced = false;
    for line in checksums.lines() {
        let name = line.split_whitespace().nth(1).unwrap();
        if name == "TARGET-SHA256SUMS" {
            updated.push_str(&format!(
                "{}  TARGET-SHA256SUMS\n",
                sha256(manifest.as_bytes())
            ));
            replaced = true;
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    assert!(
        replaced,
        "bundle checksum entry for target manifest missing"
    );
    write_file(&checksums_path, updated.as_bytes());
}

pub(super) fn invalid_target_manifests(scenario: &Scenario) -> Vec<(&'static str, String)> {
    let lines = scenario
        .current_manifest
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(
        !lines.is_empty(),
        "target manifest fixture must be nonempty"
    );
    let first_fields = lines[0]
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut malformed = lines.clone();
    malformed[0] = format!("not-a-sha256  {}", first_fields[1]);
    let malformed = format!("{}\n", malformed.join("\n"));

    let duplicate = format!("{}\n{}\n", lines[0], lines[0]);
    let mut unsafe_path = lines;
    unsafe_path[0] = format!("{}  usr/lib/qml/../unsafe.qml", first_fields[0]);
    let unsafe_path = format!("{}\n", unsafe_path.join("\n"));

    vec![
        ("malformed hash", malformed),
        ("duplicate target", duplicate),
        ("traversal path", unsafe_path),
    ]
}

pub(super) fn assert_preupdate_snapshot(scenario: &Scenario) {
    let backups = scenario.root.join("webosbrew/black-theme/backups");
    let snapshots = fs::read_dir(&backups)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("pre-update-")
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 1, "expected one pre-update snapshot");
    let snapshot = &snapshots[0];
    let manifest = fs::read_to_string(
        scenario
            .root
            .join("webosbrew/black-theme/BACKUP-SHA256SUMS"),
    )
    .unwrap();
    let snapshot_name = snapshot.file_name().unwrap().to_string_lossy();
    for target in &scenario.old_targets {
        let snapshot_file = snapshot.join("overlay").join(*target);
        assert_eq!(
            fs::read_to_string(snapshot_file).unwrap(),
            format!("old-overlay:{target}\n")
        );
        let relative = format!("{snapshot_name}/overlay/{target}");
        assert!(manifest.lines().any(|line| line.ends_with(&relative)));
    }
    let metadata: [(&str, &[u8]); 5] = [
        ("TARGET-SHA256SUMS", scenario.old_manifest.as_bytes()),
        ("COMPATIBILITY", b"previous compatibility\n"),
        ("apply.sh", scenario.old_apply.as_bytes()),
        ("unmount.sh", scenario.old_unmount.as_bytes()),
        ("uninstall.sh", scenario.old_uninstall.as_bytes()),
    ];
    for (name, expected) in metadata {
        assert_eq!(
            fs::read(snapshot.join(name)).unwrap(),
            expected.to_vec(),
            "{name}"
        );
        let relative = format!("{snapshot_name}/{name}");
        assert!(manifest.lines().any(|line| line.ends_with(&relative)));
    }
    verify_backup_manifest(scenario);
}
