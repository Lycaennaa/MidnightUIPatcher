use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use midnight_ui_patcher::config::{self, UserConfig};
use midnight_ui_patcher::deploy::{self, DeploymentPlan};
use midnight_ui_patcher::patcher::{PatchError, patch_qml, selected_patch_ids};
use midnight_ui_patcher::patches::{PATCHES, TARGETS};

const DEPLOY_SUCCESS: &str = "Installed/updated; five mounts and backup manifest verified";
const DEPLOY_REFRESH_PENDING: &str = "Pending owner approval";

#[derive(Debug, Parser)]
#[command(
    name = "midnight-ui-patcher",
    version,
    about = "Inspect and manage the LG webOS Quick Settings theme",
    after_help = "Start with `patches list` or `device inspect`. Applying rechecks the inspected plan and requires explicit safety confirmations.",
    arg_required_else_help = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// Output as concise text or one JSON object.
    #[arg(
        long,
        global = true,
        value_name = "FORMAT",
        value_enum,
        default_value = "text"
    )]
    format: OutputFormat,

    /// Use a specific local settings file instead of the default user config.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum AppCommand {
    /// Read or update the local SSH connection settings.
    #[command(subcommand)]
    Config(ConfigCommand),

    /// List the independent QML edits.
    #[command(subcommand)]
    Patches(PatchesCommand),

    /// Inspect, apply, or refresh the TV theme.
    #[command(
        subcommand,
        after_help = "Inspection fetches current QML over root SSH. Apply re-inspects and compares the reviewed fingerprint before changing device state."
    )]
    Device(DeviceCommand),

    /// Print a manual status report to stdout.
    Report(ReportArgs),

    /// Open the graphical application.
    Gui,

    /// Maintainer-only local checks.
    #[command(subcommand, hide = true)]
    Dev(DevCommand),
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Show saved connection settings; passwords and key contents are never stored.
    Show,
    /// Save only the settings supplied here; an empty string clears a text setting.
    Set(ConfigSetArgs),
}

#[derive(Debug, Args)]
struct ConfigSetArgs {
    /// Root SSH target or OpenSSH alias.
    #[arg(long, value_name = "TARGET")]
    ssh_target: Option<String>,
    /// Optional OpenSSH config file for aliases and ProxyJump.
    #[arg(long, value_name = "PATH")]
    ssh_config_file: Option<String>,
    /// Optional TV model label.
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
    /// Optional firmware label.
    #[arg(long, value_name = "LABEL")]
    firmware_label: Option<String>,
    /// Optional path to an existing private key; key contents are not read or saved.
    #[arg(long, value_name = "PATH")]
    identity_file: Option<String>,
    /// SSH port; 0 restores the SSH client's default port.
    #[arg(long, value_name = "PORT")]
    port: Option<u16>,
}

#[derive(Debug, Subcommand)]
enum PatchesCommand {
    /// List patch IDs, labels, and affected target files.
    List,
}

#[derive(Debug, Args, Clone, Default)]
struct PatchSelectionArgs {
    /// Include only these IDs; repeat to select multiple edits. Defaults to all edits.
    #[arg(long = "patch", value_name = "ID", action = ArgAction::Append)]
    patch: Vec<String>,
    /// Exclude these IDs from the current selection; repeat as needed.
    #[arg(long, value_name = "ID", action = ArgAction::Append)]
    exclude: Vec<String>,
}

#[derive(Debug, Args)]
struct InspectArgs {
    #[command(flatten)]
    selection: PatchSelectionArgs,
    /// Include the raw QML diff in the output.
    #[arg(long)]
    diff: bool,
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// Fetch current TV state and preview the selected edits without changing the TV.
    Inspect(InspectArgs),
    /// Re-inspect and apply only if the reviewed plan fingerprint still matches.
    #[command(
        after_help = "Run `device inspect` first. Pass its `plan_fingerprint` here. This command fetches the QML again and aborts if the fingerprint changed. Root recovery must be confirmed; unverified firmware needs a separate acknowledgement."
    )]
    Apply(ApplyArgs),
    /// Restart the display service after confirming it may interrupt the TV UI.
    #[command(
        after_help = "The remote script verifies the active overlay, backups, and all five mounts before restarting surface-manager-daemon.service. The TV UI may be interrupted."
    )]
    RestartDisplay(RestartDisplayArgs),
}

#[derive(Debug, Args)]
struct ApplyArgs {
    #[command(flatten)]
    selection: PatchSelectionArgs,
    /// Include the raw QML diff in the output.
    #[arg(long)]
    diff: bool,
    /// Fingerprint returned by `device inspect`; apply aborts if the fresh plan differs.
    #[arg(long, value_name = "SHA256", required = true)]
    expect_plan_fingerprint: String,
    /// Confirm working root SSH recovery is available.
    #[arg(long)]
    confirm_root_recovery: bool,
    /// Confirm this firmware is unverified and structural matches are not official support.
    #[arg(long)]
    confirm_unverified: bool,
}

#[derive(Debug, Args)]
struct RestartDisplayArgs {
    /// Approve restarting surface-manager-daemon.service now; the TV UI may be interrupted.
    #[arg(long)]
    confirm_owner_approval: bool,
}

#[derive(Debug, Args)]
struct ReportArgs {
    #[command(flatten)]
    selection: PatchSelectionArgs,
    /// Number of structural patch sites verified by an earlier inspection.
    #[arg(long, value_name = "COUNT")]
    structurally_verified: Option<usize>,
    /// Deployment status from the workflow being reported.
    #[arg(long, value_enum, default_value = "not-applied")]
    deployment: ReportDeployment,
    /// Display refresh status from the workflow being reported.
    #[arg(long, value_enum, default_value = "not-restarted")]
    refresh: ReportRefresh,
    /// Owner's visual check after refreshing Quick Settings.
    #[arg(long, value_enum, default_value = "not-checked")]
    visual_result: VisualResult,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportDeployment {
    NotApplied,
    Succeeded,
    FailedOrUncertain,
}

impl ReportDeployment {
    fn label(self) -> &'static str {
        match self {
            Self::NotApplied => "Not applied",
            Self::Succeeded => DEPLOY_SUCCESS,
            Self::FailedOrUncertain => "Failed or uncertain",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReportRefresh {
    NotRestarted,
    PendingOwnerApproval,
    RestartedVisualCheckPending,
    FailedOrUncertain,
}

impl ReportRefresh {
    fn label(self) -> &'static str {
        match self {
            Self::NotRestarted => "Not restarted",
            Self::PendingOwnerApproval => DEPLOY_REFRESH_PENDING,
            Self::RestartedVisualCheckPending => "Restarted; owner visual check pending",
            Self::FailedOrUncertain => "Failed/uncertain",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum VisualResult {
    NotChecked,
    LooksGood,
    IssueNoticed,
}

impl VisualResult {
    fn label(self) -> &'static str {
        match self {
            Self::NotChecked => "Not checked",
            Self::LooksGood => "Looks good",
            Self::IssueNoticed => "Issue noticed",
        }
    }
}

#[derive(Debug, Subcommand)]
enum DevCommand {
    /// Compare generated patches with maintainer-only local preview fixtures.
    VerifyLocalPreview {
        #[arg(value_name = "PROJECT_ROOT", default_value = ".")]
        root: PathBuf,
    },
}

#[derive(Debug)]
struct CommandResult {
    text: String,
    data: Value,
}

#[derive(Debug)]
struct CommandFailure {
    code: &'static str,
    message: String,
    details: Option<Value>,
    context: Option<String>,
    exit_code: u8,
}

impl CommandFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            context: None,
            exit_code: 1,
        }
    }

    fn usage(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            ..Self::new(code, message)
        }
    }

    fn confirmation(message: impl Into<String>) -> Self {
        Self::usage("confirmation_required", message)
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

pub fn run(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut argv = Vec::new();
    argv.push(OsString::from("midnight-ui-patcher"));
    argv.extend(args);
    let json_requested = wants_json(&argv);
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let informational = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            );
            if informational {
                print!("{error}");
                return ExitCode::SUCCESS;
            }
            if json_requested {
                println!(
                    "{}",
                    json!({
                        "ok": false,
                        "error": {
                            "code": "usage",
                            "message": error.to_string(),
                        }
                    })
                );
                return ExitCode::from(2);
            }
            eprint!("{error}");
            return ExitCode::from(2);
        }
    };

    let format = cli.format;
    if matches!(&cli.command, AppCommand::Gui) {
        #[cfg(feature = "desktop")]
        {
            midnight_ui_patcher::ui::run(cli.config);
            return ExitCode::SUCCESS;
        }
        #[cfg(not(feature = "desktop"))]
        {
            return emit_failure(
                format,
                CommandFailure::new(
                    "gui_unavailable",
                    "This build does not include the desktop feature.",
                ),
            );
        }
    }

    match dispatch(cli.command, cli.config.as_deref()) {
        Ok(result) => emit_success(format, result),
        Err(error) => emit_failure(format, error),
    }
}

fn wants_json(args: &[OsString]) -> bool {
    args.iter()
        .any(|argument| argument.to_str() == Some("--format=json"))
        || args
            .windows(2)
            .any(|pair| pair[0].to_str() == Some("--format") && pair[1].to_str() == Some("json"))
}

fn emit_success(format: OutputFormat, result: CommandResult) -> ExitCode {
    match format {
        OutputFormat::Text => print!("{}", result.text),
        OutputFormat::Json => println!("{}", json!({ "ok": true, "data": result.data })),
    }
    ExitCode::SUCCESS
}

fn emit_failure(format: OutputFormat, error: CommandFailure) -> ExitCode {
    match format {
        OutputFormat::Text => {
            if let Some(context) = error.context {
                print!("{context}");
            }
            eprintln!("Error: {}", error.message);
        }
        OutputFormat::Json => {
            let mut body = json!({
                "ok": false,
                "error": {
                    "code": error.code,
                    "message": error.message,
                }
            });
            if let Some(details) = error.details {
                body["error"]["details"] = details;
            }
            println!("{body}");
        }
    }
    ExitCode::from(error.exit_code)
}

fn dispatch(
    command: AppCommand,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    match command {
        AppCommand::Config(command) => run_config(command, config_path),
        AppCommand::Patches(command) => run_patches(command),
        AppCommand::Device(command) => run_device(command, config_path),
        AppCommand::Report(args) => run_report(args, config_path),
        AppCommand::Gui => unreachable!("GUI command is handled before CLI dispatch"),
        AppCommand::Dev(command) => run_dev(command),
    }
}

fn load_settings(config_path: Option<&Path>) -> Result<(PathBuf, UserConfig), CommandFailure> {
    match config_path {
        Some(path) => {
            let settings = config::load_or_create_at(path).map_err(config_failure)?;
            Ok((path.to_owned(), settings))
        }
        None => config::load_or_create().map_err(config_failure),
    }
}

fn run_config(
    command: ConfigCommand,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    match command {
        ConfigCommand::Show => {
            let (_, settings) = load_settings(config_path)?;
            let data = config_value(&settings);
            let text = format!(
                "SSH target: {}\nSSH config file: {}\nSSH port: {}\nIdentity file: {}\nTV model: {}\nFirmware label: {}\n",
                shown(&settings.ssh_target),
                shown(&settings.ssh_config_file),
                if settings.port == 0 {
                    "default".to_owned()
                } else {
                    settings.port.to_string()
                },
                shown(&settings.identity_file),
                shown(&settings.model),
                shown(&settings.firmware_label),
            );
            Ok(CommandResult { text, data })
        }
        ConfigCommand::Set(args) => {
            if args.ssh_target.is_none()
                && args.ssh_config_file.is_none()
                && args.model.is_none()
                && args.firmware_label.is_none()
                && args.identity_file.is_none()
                && args.port.is_none()
            {
                return Err(CommandFailure::usage(
                    "missing_settings",
                    "Supply at least one setting to `config set`.",
                ));
            }
            let (path, mut settings) = load_settings(config_path)?;
            if let Some(value) = args.ssh_target {
                settings.ssh_target = value;
            }
            if let Some(value) = args.ssh_config_file {
                settings.ssh_config_file = value;
            }
            if let Some(value) = args.model {
                settings.model = value;
            }
            if let Some(value) = args.firmware_label {
                settings.firmware_label = value;
            }
            if let Some(value) = args.identity_file {
                settings.identity_file = value;
            }
            if let Some(value) = args.port {
                settings.port = value;
            }
            match config_path {
                Some(_) => config::save_at(&path, &settings),
                None => config::save(&path, &settings),
            }
            .map_err(config_failure)?;
            let text = "Connection settings saved locally. Passwords and private-key contents are never stored.\n".to_owned();
            Ok(CommandResult {
                text,
                data: config_value(&settings),
            })
        }
    }
}

fn config_value(settings: &UserConfig) -> Value {
    json!({
        "ssh_target": &settings.ssh_target,
        "ssh_config_file": &settings.ssh_config_file,
        "port": settings.port,
        "identity_file": &settings.identity_file,
        "model": &settings.model,
        "firmware_label": &settings.firmware_label,
    })
}

fn run_patches(command: PatchesCommand) -> Result<CommandResult, CommandFailure> {
    match command {
        PatchesCommand::List => {
            let patches = PATCHES
                .iter()
                .map(|patch| {
                    json!({
                        "id": patch.id,
                        "label": patch.label,
                        "target": patch.target,
                    })
                })
                .collect::<Vec<_>>();
            let mut text = format!("{} independent edits:\n", PATCHES.len());
            for patch in &PATCHES {
                text.push_str(&format!(
                    "  {}  {}\n      {}\n",
                    patch.id, patch.label, patch.target
                ));
            }
            Ok(CommandResult {
                text,
                data: json!({ "patches": patches }),
            })
        }
    }
}

fn run_device(
    command: DeviceCommand,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    match command {
        DeviceCommand::Inspect(selection) => inspect(selection, config_path),
        DeviceCommand::Apply(args) => apply(args, config_path),
        DeviceCommand::RestartDisplay(args) => restart_display(args, config_path),
    }
}

fn inspect(
    selection: InspectArgs,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    let selected = resolve_selection(&selection.selection)?;
    let (_, settings) = load_settings(config_path)?;
    let plan = deploy::prepare_plan(&settings, &selected).map_err(deploy_failure)?;
    let fingerprint = plan_fingerprint(&settings, &plan)?;
    let data = plan_value(&plan, &fingerprint, selection.diff);
    let text = format!(
        "{}No deployment was started.\n",
        format_plan(&plan, &fingerprint, selection.diff)
    );
    Ok(CommandResult { text, data })
}

fn apply(args: ApplyArgs, config_path: Option<&Path>) -> Result<CommandResult, CommandFailure> {
    let selected = resolve_selection(&args.selection)?;
    if !args.confirm_root_recovery {
        return Err(CommandFailure::confirmation(
            "Confirm working root SSH recovery with `--confirm-root-recovery` before applying.",
        ));
    }

    let (_, settings) = load_settings(config_path)?;
    let plan = deploy::prepare_plan(&settings, &selected).map_err(deploy_failure)?;
    let fingerprint = plan_fingerprint(&settings, &plan)?;
    let data = plan_value(&plan, &fingerprint, args.diff);
    let summary = format_plan(&plan, &fingerprint, args.diff);

    if fingerprint != args.expect_plan_fingerprint {
        return Err(CommandFailure::usage(
            "plan_changed",
            format!(
                "The fresh plan fingerprint differs from the reviewed value; nothing was applied. Current fingerprint: {fingerprint}"
            ),
        )
        .with_details(json!({
            "expected_plan_fingerprint": safe_cli_text(&args.expect_plan_fingerprint),
            "current_plan": data,
        }))
        .with_context(summary));
    }
    if !plan.firmware_is_officially_tested() && !args.confirm_unverified {
        return Err(CommandFailure::confirmation(
            "This firmware is unverified. Re-run with `--confirm-unverified`; nothing was applied.",
        )
        .with_details(json!({ "current_plan": data }))
        .with_context(summary));
    }

    match deploy::deploy_plan(
        &settings,
        &plan,
        args.confirm_unverified,
        args.confirm_root_recovery,
    ) {
        Ok(()) => {
            let report =
                deploy::manual_report(&plan, DEPLOY_SUCCESS, DEPLOY_REFRESH_PENDING, "Not checked");
            let mut data = data;
            data["deployment"] = json!("succeeded");
            data["display_refresh"] = json!("pending_owner_approval");
            data["report"] = json!(report);
            let text = format!(
                "{summary}Deployment succeeded; five mounts and the backup manifest verified.\nDisplay refresh is still pending separate owner approval.\n\n{report}"
            );
            Ok(CommandResult { text, data })
        }
        Err(error) => Err(
            CommandFailure::new("deployment_failed_or_uncertain", error.to_string())
                .with_details(json!({
                    "current_plan": data,
                    "deployment": "failed_or_uncertain",
                    "preserve_backups": true,
                }))
                .with_context(summary),
        ),
    }
}

fn restart_display(
    args: RestartDisplayArgs,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    if !args.confirm_owner_approval {
        return Err(CommandFailure::confirmation(
            "The display service can interrupt the TV UI. Re-run with `--confirm-owner-approval` only after approving the restart.",
        ));
    }
    let (_, settings) = load_settings(config_path)?;
    deploy::restart_display_service(&settings, true).map_err(deploy_failure)?;
    Ok(CommandResult {
        text: "Display service restarted; all five overlay mounts remain active. Check Quick Settings visually on the TV.\n".to_owned(),
        data: json!({
            "display_refresh": "restarted",
            "visual_check": "pending_owner_visual_check",
        }),
    })
}

fn run_report(
    args: ReportArgs,
    config_path: Option<&Path>,
) -> Result<CommandResult, CommandFailure> {
    let selected = resolve_selection(&args.selection)?;
    let (_, settings) = load_settings(config_path)?;
    if args
        .structurally_verified
        .is_some_and(|count| count > PATCHES.len())
    {
        return Err(CommandFailure::usage(
            "invalid_structural_count",
            format!(
                "Structural verification count cannot exceed {}.",
                PATCHES.len()
            ),
        ));
    }
    let selected_labels = PATCHES
        .iter()
        .filter(|patch| selected.contains(patch.id))
        .map(|patch| patch.label)
        .collect::<Vec<_>>();
    let report = deploy::manual_report_for(
        args.structurally_verified.map(|_| settings.model.as_str()),
        args.structurally_verified
            .map(|_| settings.firmware_label.as_str()),
        &selected_labels,
        args.structurally_verified,
        args.deployment.label(),
        args.refresh.label(),
        args.visual_result.label(),
    );
    Ok(CommandResult {
        text: report.clone(),
        data: json!({
            "report": report,
            "structurally_verified": args.structurally_verified,
            "deployment": args.deployment.label(),
            "display_refresh": args.refresh.label(),
            "visual_result": args.visual_result.label(),
        }),
    })
}

fn run_dev(command: DevCommand) -> Result<CommandResult, CommandFailure> {
    match command {
        DevCommand::VerifyLocalPreview { root } => {
            let applied = verify_local_preview(&root).map_err(|message| {
                CommandFailure::new("local_preview_verification_failed", message)
            })?;
            let text = format!(
                "Validated {applied} structural edits against all {} local target files.\n",
                TARGETS.len()
            );
            Ok(CommandResult {
                text,
                data: json!({
                    "structural_edits_validated": applied,
                    "target_files_validated": TARGETS.len(),
                }),
            })
        }
    }
}

fn resolve_selection(selection: &PatchSelectionArgs) -> Result<BTreeSet<String>, CommandFailure> {
    let named = selection
        .patch
        .iter()
        .chain(&selection.exclude)
        .cloned()
        .collect::<Vec<_>>();
    if !named.is_empty() {
        selected_patch_ids(named).map_err(patch_selection_failure)?;
    }

    let mut selected: BTreeSet<String> = if selection.patch.is_empty() {
        PATCHES.iter().map(|patch| patch.id.to_owned()).collect()
    } else {
        selection.patch.iter().cloned().collect()
    };
    for id in &selection.exclude {
        selected.remove(id);
    }
    selected_patch_ids(selected).map_err(patch_selection_failure)
}

fn patch_selection_failure(error: PatchError) -> CommandFailure {
    match error {
        PatchError::UnknownPatch(id) => CommandFailure::usage(
            "unknown_patch_id",
            format!(
                "Unknown patch ID `{}`. Use `patches list` to see valid IDs.",
                safe_cli_text(&id)
            ),
        ),
        PatchError::EmptySelection => CommandFailure::usage(
            "empty_selection",
            "No edits selected. Select at least one before inspection.",
        ),
        other => CommandFailure::new("invalid_patch_selection", other.to_string()),
    }
}

fn plan_value(plan: &DeploymentPlan, fingerprint: &str, include_diff: bool) -> Value {
    let selected_edits = PATCHES
        .iter()
        .filter(|patch| plan.selected_ids.contains(patch.id))
        .map(|patch| {
            json!({
                "id": patch.id,
                "label": patch.label,
                "target": patch.target,
            })
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "action": plan.mode.label(),
        "model": plan.model,
        "firmware_label": plan.firmware_label,
        "firmware_officially_tested": plan.firmware_is_officially_tested(),
        "compatibility_warning": plan.compatibility_warning(),
        "source_files_fetched": TARGETS.len(),
        "patch_sites_matched": plan.structurally_verified,
        "patch_sites_total": PATCHES.len(),
        "selected_edits": selected_edits,
        "plan_fingerprint": fingerprint,
    });
    if include_diff {
        value["diff"] = json!(plan.diff);
    }
    value
}

fn format_plan(plan: &DeploymentPlan, fingerprint: &str, include_diff: bool) -> String {
    let mut text = format!(
        "Action: {}\nSource check: fetched {} target files; {} of {} patch sites matched.\nModel: {}\nFirmware: {}\nSelected edits:\n",
        plan.mode.label(),
        TARGETS.len(),
        plan.structurally_verified,
        PATCHES.len(),
        shown(&plan.model),
        shown(&plan.firmware_label),
    );
    if plan.selected_ids.is_empty() {
        text.push_str("  (none)\n");
    } else {
        for patch in &PATCHES {
            if plan.selected_ids.contains(patch.id) {
                text.push_str(&format!("  {}  {}\n", patch.id, patch.label));
            }
        }
    }
    text.push_str(&format!(
        "Compatibility: {}\nPlan fingerprint: {fingerprint}\n",
        plan.compatibility_warning(),
    ));
    if include_diff {
        text.push_str("\nRaw QML diff:\n");
        text.push_str(&plan.diff);
        if !plan.diff.ends_with('\n') {
            text.push('\n');
        }
    }
    text
}

fn plan_fingerprint(
    settings: &UserConfig,
    plan: &DeploymentPlan,
) -> Result<String, CommandFailure> {
    let ssh_routing_fingerprint =
        deploy::ssh_routing_fingerprint(settings).map_err(deploy_failure)?;
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, plan.mode.label());
    hash_field(&mut hasher, &settings.ssh_target);
    hash_field(&mut hasher, &settings.ssh_config_file);
    hasher.update(settings.port.to_be_bytes());
    hash_field(&mut hasher, &settings.identity_file);
    hasher.update(ssh_routing_fingerprint);
    hash_field(&mut hasher, &plan.model);
    hash_field(&mut hasher, &plan.firmware_label);
    hasher.update((plan.selected_ids.len() as u64).to_be_bytes());
    for id in &plan.selected_ids {
        hash_field(&mut hasher, id);
    }
    hasher.update((plan.target_hashes.len() as u64).to_be_bytes());
    for (path, hash) in &plan.target_hashes {
        hash_field(&mut hasher, path);
        hash_field(&mut hasher, hash);
    }
    hasher.update((plan.patched_files.len() as u64).to_be_bytes());
    for (path, contents) in &plan.patched_files {
        hash_field(&mut hasher, path);
        hash_field(&mut hasher, contents);
    }
    hasher.update((plan.structurally_verified as u64).to_be_bytes());

    let digest = hasher.finalize();
    let hex = b"0123456789abcdef";
    let mut fingerprint = String::with_capacity(7 + digest.len() * 2);
    fingerprint.push_str("sha256:");
    for byte in digest {
        fingerprint.push(hex[(byte >> 4) as usize] as char);
        fingerprint.push(hex[(byte & 0x0f) as usize] as char);
    }
    Ok(fingerprint)
}

fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn verify_local_preview(root: &Path) -> Result<usize, String> {
    let mut sources = std::collections::BTreeMap::new();
    for target in TARGETS {
        let path = root.join("preview/base").join(target);
        let source = fs::read_to_string(&path)
            .map_err(|_| format!("Cannot read local source file: {}", path.display()))?;
        sources.insert(target.to_owned(), source);
    }
    let selected = selected_patch_ids(PATCHES.iter().map(|patch| patch.id))
        .map_err(|error| error.to_string())?;
    let output = patch_qml(&sources, &selected).map_err(|error| error.to_string())?;
    for target in TARGETS {
        let overlay_path = root.join("preview/overlay").join(target);
        let expected = fs::read_to_string(&overlay_path).map_err(|_| {
            format!(
                "Cannot read local comparison file: {}",
                overlay_path.display()
            )
        })?;
        if output.files.get(target) != Some(&expected) {
            return Err(format!(
                "Structural patch output differs from the local overlay: {target}"
            ));
        }
    }
    Ok(output.applied.len())
}

fn shown(value: &str) -> String {
    if value.is_empty() {
        "(not set)".to_owned()
    } else {
        value.to_owned()
    }
}

fn safe_cli_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(3000)
        .collect()
}

fn config_failure(error: config::ConfigError) -> CommandFailure {
    CommandFailure::new("configuration_error", safe_cli_text(&error.to_string()))
}

fn deploy_failure(error: deploy::DeployError) -> CommandFailure {
    CommandFailure::new("device_operation_failed", safe_cli_text(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn args(patch: &[&str], exclude: &[&str]) -> PatchSelectionArgs {
        PatchSelectionArgs {
            patch: patch.iter().map(|value| (*value).to_owned()).collect(),
            exclude: exclude.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn isolated_config_path(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("cli-{label}-{}-{nonce}", std::process::id()))
            .join("config.toml")
    }

    fn sample_plan() -> DeploymentPlan {
        DeploymentPlan {
            mode: deploy::InstallMode::Fresh,
            model: "OLED55".to_owned(),
            firmware_label: "webOS".to_owned(),
            selected_ids: BTreeSet::from(["main-panel-color".to_owned()]),
            selected_labels: vec!["Set panel backdrop color to black"],
            patched_files: BTreeMap::from([("target.qml".to_owned(), "patched".to_owned())]),
            target_hashes: BTreeMap::from([("target.qml".to_owned(), "before".to_owned())]),
            diff: String::new(),
            structurally_verified: PATCHES.len(),
        }
    }

    #[test]
    fn config_set_updates_requested_values_and_preserves_the_rest() {
        let path = isolated_config_path("settings");
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut original = UserConfig::default();
        original.ssh_target = "root@tv.local".to_owned();
        original.ssh_config_file = "old/ssh_config".to_owned();
        original.model = "Older model".to_owned();
        original.firmware_label = "Older firmware".to_owned();
        original.identity_file = "~/.ssh/tv".to_owned();
        original.port = 2222;
        config::save_at(&path, &original).unwrap();

        let result = run_config(
            ConfigCommand::Set(ConfigSetArgs {
                ssh_target: None,
                ssh_config_file: Some("custom/ssh_config".to_owned()),
                model: Some("OLED55".to_owned()),
                firmware_label: None,
                identity_file: None,
                port: None,
            }),
            Some(&path),
        )
        .unwrap_or_else(|error| panic!("{}", error.message));
        let saved = config::load_or_create_at(&path).unwrap();

        assert_eq!(saved.model, "OLED55");
        assert_eq!(saved.ssh_target, "root@tv.local");
        assert_eq!(saved.ssh_config_file, "custom/ssh_config");
        assert_eq!(saved.firmware_label, "Older firmware");
        assert_eq!(saved.identity_file, "~/.ssh/tv");
        assert_eq!(saved.port, 2222);
        assert_eq!(result.data["model"], "OLED55");
        assert_eq!(result.data["ssh_config_file"], "custom/ssh_config");
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(parent).unwrap().permissions().mode() & 0o777,
            0o755
        );

        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn report_command_includes_the_owner_visual_check_and_selected_edit() {
        let path = isolated_config_path("report");
        let mut settings = UserConfig::default();
        settings.model = "OLED55".to_owned();
        settings.firmware_label = "webOS 24".to_owned();
        config::save(&path, &settings).unwrap();

        let result = run_report(
            ReportArgs {
                selection: args(&["main-panel-color"], &[]),
                structurally_verified: Some(PATCHES.len()),
                deployment: ReportDeployment::Succeeded,
                refresh: ReportRefresh::RestartedVisualCheckPending,
                visual_result: VisualResult::LooksGood,
            },
            Some(&path),
        )
        .unwrap_or_else(|error| panic!("{}", error.message));

        assert!(result.text.contains("Model: OLED55"));
        assert!(result.text.contains("- Set panel backdrop color to black"));
        assert!(
            result.text.contains(
                "Deployment: Installed/updated; five mounts and backup manifest verified"
            )
        );
        assert!(
            result
                .text
                .contains("Display refresh: Restarted; owner visual check pending")
        );
        assert!(result.text.contains("Visual check: Looks good"));

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn patch_selection_matches_gui_defaults_and_independent_toggles() {
        assert_eq!(
            resolve_selection(&args(&[], &[])).unwrap().len(),
            PATCHES.len()
        );
        assert_eq!(
            resolve_selection(&args(&["main-panel-color"], &[])).unwrap(),
            BTreeSet::from(["main-panel-color".to_owned()])
        );
        assert_eq!(
            resolve_selection(&args(&[], &["main-panel-color"]))
                .unwrap()
                .len(),
            PATCHES.len() - 1
        );
        let all_ids = PATCHES.iter().map(|patch| patch.id).collect::<Vec<_>>();
        assert_eq!(
            resolve_selection(&args(&[], &all_ids)).unwrap_err().code,
            "empty_selection"
        );
        assert_eq!(
            resolve_selection(&args(&["not-a-patch"], &[]))
                .unwrap_err()
                .code,
            "unknown_patch_id"
        );
    }

    #[test]

    fn reviewed_fingerprint_changes_when_connection_or_patch_plan_changes() {
        let ssh_config_path = isolated_config_path("ssh-fingerprint");
        let alternate_ssh_config_path = isolated_config_path("ssh-fingerprint-alternate");
        let jump_config_path = ssh_config_path.with_file_name("jump_hosts.conf");
        std::fs::create_dir_all(ssh_config_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(alternate_ssh_config_path.parent().unwrap()).unwrap();
        let include = jump_config_path.display().to_string();
        let first_route = format!(
            "Include \"{include}\"\nHost lg-tv\n    HostName 192.0.2.1\n    User root\n    ProxyJump jump\n"
        );
        std::fs::write(
            &jump_config_path,
            "Host jump\n    HostName bastion-a.example\n    User pi\n",
        )
        .unwrap();
        std::fs::write(&ssh_config_path, &first_route).unwrap();
        std::fs::write(&alternate_ssh_config_path, &first_route).unwrap();

        let mut settings = UserConfig::default();
        settings.ssh_target = "lg-tv".to_owned();
        settings.ssh_config_file = ssh_config_path.display().to_string();
        let mut plan = sample_plan();
        let original = plan_fingerprint(&settings, &plan).unwrap();
        assert_eq!(original, plan_fingerprint(&settings, &plan).unwrap());

        std::fs::write(
            &ssh_config_path,
            first_route.replace("192.0.2.1", "192.0.2.2"),
        )
        .unwrap();
        assert_ne!(original, plan_fingerprint(&settings, &plan).unwrap());
        std::fs::write(&ssh_config_path, &first_route).unwrap();
        std::fs::write(
            &jump_config_path,
            "Host jump\n    HostName bastion-b.example\n    User pi\n",
        )
        .unwrap();
        assert_ne!(original, plan_fingerprint(&settings, &plan).unwrap());

        settings.ssh_config_file = alternate_ssh_config_path.display().to_string();
        assert_ne!(original, plan_fingerprint(&settings, &plan).unwrap());

        let reviewed_plan = plan_fingerprint(&settings, &plan).unwrap();
        plan.patched_files
            .insert("target.qml".to_owned(), "different patch".to_owned());
        assert_ne!(reviewed_plan, plan_fingerprint(&settings, &plan).unwrap());

        std::fs::remove_dir_all(ssh_config_path.parent().unwrap()).unwrap();
        std::fs::remove_dir_all(alternate_ssh_config_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn safety_confirmations_fail_before_loading_settings_or_contacting_a_tv() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "cli-confirmation-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let config_path = root.join("config.toml");
        let apply_args = ApplyArgs {
            selection: PatchSelectionArgs::default(),
            diff: false,
            expect_plan_fingerprint: "sha256:reviewed".to_owned(),
            confirm_root_recovery: false,
            confirm_unverified: false,
        };
        let apply_error = apply(apply_args, Some(&config_path)).unwrap_err();
        assert_eq!(apply_error.code, "confirmation_required");

        let refresh_error = restart_display(
            RestartDisplayArgs {
                confirm_owner_approval: false,
            },
            Some(&config_path),
        )
        .unwrap_err();
        assert_eq!(refresh_error.code, "confirmation_required");
        assert!(!config_path.exists());
    }

    #[test]
    fn json_format_is_detected_before_command_line_validation() {
        assert!(wants_json(&[
            OsString::from("--format"),
            OsString::from("json"),
        ]));
        assert!(wants_json(&[OsString::from("--format=json")]));
        assert!(!wants_json(&[OsString::from("--format=text")]));
    }

    #[test]
    fn cli_parses_nested_commands_and_global_json_output() {
        let cli = Cli::try_parse_from([
            "midnight-ui-patcher",
            "device",
            "inspect",
            "--patch",
            "main-panel-color",
            "--format",
            "json",
        ])
        .unwrap();
        assert_eq!(cli.format, OutputFormat::Json);
        assert!(matches!(
            cli.command,
            AppCommand::Device(DeviceCommand::Inspect(_))
        ));
    }
}
