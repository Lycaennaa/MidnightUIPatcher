use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, AppContext, Application, Bounds, ClipboardItem, Context, Entity, InteractiveElement,
    Render, SharedString, StatefulInteractiveElement, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

use crate::config::{self, UserConfig};
use crate::deploy::{self, DeploymentPlan};
use crate::patches::{PATCHES, PatchSpec, TARGETS};
use crate::text_input::{self, LineInput};
const DEPLOY_SUCCESS: &str = "Installed/updated; two mounts and backup manifest verified";

pub fn run(config_path: Option<PathBuf>) {
    Application::new().run(move |cx: &mut App| {
        text_input::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(980.0), px(900.0)), cx);
        let config_path = config_path.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| cx.new(move |cx| AppView::new(config_path, cx)),
        )
        .expect("failed to open patcher window");
    });
}

enum SettingsFile {
    Managed(PathBuf),
    Explicit(PathBuf),
}

impl SettingsFile {
    fn load(path: Option<PathBuf>) -> (Option<Self>, UserConfig, Option<String>) {
        match path {
            Some(path) => match config::load_or_create_at(&path) {
                Ok(settings) => (Some(Self::Explicit(path)), settings, None),
                Err(error) => (
                    Some(Self::Explicit(path)),
                    UserConfig::default(),
                    Some(error.to_string()),
                ),
            },
            None => match config::load_or_create() {
                Ok((path, settings)) => (Some(Self::Managed(path)), settings, None),
                Err(error) => (
                    config::config_path().ok().map(Self::Managed),
                    UserConfig::default(),
                    Some(error.to_string()),
                ),
            },
        }
    }

    fn save(&self, settings: &UserConfig) -> Result<(), config::ConfigError> {
        match self {
            Self::Managed(path) => config::save(path, settings),
            Self::Explicit(path) => config::save_at(path, settings),
        }
    }
}
struct AppView {
    settings_file: Option<SettingsFile>,
    config: UserConfig,
    settings_fields: SettingsFields,
    settings_open: bool,
    selected_ids: BTreeSet<String>,
    plan: Option<Arc<DeploymentPlan>>,
    busy: bool,
    error: Option<String>,
    status: String,
    show_diff: bool,
    confirm_unverified: bool,
    confirm_root_recovery: bool,
    confirm_refresh: bool,
    deployment_status: String,
    refresh_status: String,
    visual_result: String,
}

struct SettingsFields {
    ssh_target: Entity<LineInput>,
    ssh_config_file: Entity<LineInput>,
    model: Entity<LineInput>,
    firmware_label: Entity<LineInput>,
    identity_file: Entity<LineInput>,
    port: Entity<LineInput>,
}

impl SettingsFields {
    fn new(config: &UserConfig, cx: &mut Context<AppView>) -> Self {
        Self {
            ssh_target: cx.new(|cx| {
                LineInput::new(cx, config.ssh_target.clone(), "root@TV_IP or host alias")
            }),
            ssh_config_file: cx.new(|cx| {
                LineInput::new(
                    cx,
                    config.ssh_config_file.clone(),
                    "~/.ssh/config (optional)",
                )
            }),
            model: cx.new(|cx| LineInput::new(cx, config.model.clone(), "Optional TV model")),
            firmware_label: cx.new(|cx| {
                LineInput::new(cx, config.firmware_label.clone(), "Optional firmware label")
            }),
            identity_file: cx
                .new(|cx| LineInput::new(cx, config.identity_file.clone(), "~/.ssh/tv (optional)")),
            port: cx.new(|cx| {
                LineInput::new(
                    cx,
                    if config.port == 0 {
                        String::new()
                    } else {
                        config.port.to_string()
                    },
                    "Default SSH port",
                )
            }),
        }
    }

    fn to_config(
        &self,
        saved: &UserConfig,
        cx: &mut Context<AppView>,
    ) -> Result<UserConfig, String> {
        let port_text = self.port.read_with(cx, |input, _| input.text());
        let port = if port_text.trim().is_empty() {
            0
        } else {
            port_text
                .trim()
                .parse::<u16>()
                .map_err(|_| "SSH port must be blank or a number from 0 to 65535.".to_owned())?
        };
        let mut config = saved.clone();
        config.ssh_target = self
            .ssh_target
            .read_with(cx, |input, _| input.text())
            .trim()
            .to_owned();
        config.ssh_config_file = self
            .ssh_config_file
            .read_with(cx, |input, _| input.text())
            .trim()
            .to_owned();
        config.model = self.model.read_with(cx, |input, _| input.text());
        config.firmware_label = self.firmware_label.read_with(cx, |input, _| input.text());
        config.identity_file = self.identity_file.read_with(cx, |input, _| input.text());
        config.port = port;
        Ok(config)
    }
}

impl AppView {
    fn new(config_path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let (settings_file, config, error) = SettingsFile::load(config_path);
        let settings_open = config.ssh_target.is_empty();
        let settings_fields = SettingsFields::new(&config, cx);
        Self {
            settings_file,
            config,
            settings_fields,
            settings_open,
            selected_ids: PATCHES.iter().map(|patch| patch.id.to_owned()).collect(),
            plan: None,
            busy: false,
            error,
            status: "Edit connection settings, then inspect the TV before applying any changes."
                .into(),
            show_diff: false,
            confirm_unverified: false,
            confirm_root_recovery: false,
            confirm_refresh: false,
            deployment_status: "Not applied".into(),
            refresh_status: "Not restarted".into(),
            visual_result: "Not checked".into(),
        }
    }

    fn edit_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_fields = SettingsFields::new(&self.config, cx);
        self.settings_open = true;
        self.error = None;
        cx.notify();
    }

    fn discard_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_fields = SettingsFields::new(&self.config, cx);
        self.settings_open = false;
        self.error = None;
        self.status = "Unsaved connection settings discarded.".into();
        cx.notify();
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let settings = match self.settings_fields.to_config(&self.config, cx) {
            Ok(settings) => settings,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(settings_file) = self.settings_file.as_ref() else {
            self.error = Some("User-local settings path is unavailable.".into());
            cx.notify();
            return;
        };
        match settings_file.save(&settings) {
            Ok(()) => {
                self.config = settings;
                self.settings_open = false;
                self.plan = None;
                self.deployment_status = "Not applied".into();
                self.refresh_status = "Not restarted".into();
                self.visual_result = "Not checked".into();
                self.confirm_unverified = false;
                self.confirm_root_recovery = false;
                self.confirm_refresh = false;
                self.error = None;
                self.status = "Connection settings saved locally. Inspect the TV to fetch and verify current QML.".into();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.status = "Could not save connection settings.".into();
            }
        }
        cx.notify();
    }

    fn inspect(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.settings_open {
            return;
        }
        let config = self.config.clone();
        let selected = self.selected_ids.clone();
        self.plan = None;
        self.deployment_status = "Not applied".into();
        self.refresh_status = "Not restarted".into();
        self.visual_result = "Not checked".into();
        self.confirm_unverified = false;
        self.confirm_root_recovery = false;
        self.confirm_refresh = false;
        self.error = None;
        self.busy = true;
        self.status = format!(
            "Checking the live hook, lock, mounts, hashes, and backups; fetching {} QML files over root SSH…",
            TARGETS.len()
        );
        cx.notify();
        cx.spawn(async move |weak, async_cx| {
            let result = async_cx.background_spawn(async move {
                deploy::prepare_plan(&config, &selected)
            }).await;
            let _ = weak.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(plan) => {
                        this.status = format!(
                            "Fetched {} target files. All {} patch sites matched. Review the summary and optional diff before applying.",
                            TARGETS.len(),
                            plan.structurally_verified,
                        );
                        this.deployment_status = "Not applied".into();
                        this.refresh_status = "Not restarted".into();
                        this.visual_result = "Not checked".into();
                        this.confirm_unverified = false;
                        this.confirm_root_recovery = false;
                        this.plan = Some(Arc::new(plan));
                    }
                    Err(error) => {
                        this.status = "Inspection stopped; no deployment was started.".into();
                        this.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    fn deploy(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.settings_open {
            return;
        }
        let Some(plan) = self.plan.clone() else {
            return;
        };
        if !self.confirm_root_recovery {
            self.error =
                Some("Confirm that working root SSH recovery is available before applying.".into());
            cx.notify();
            return;
        }
        if !plan.firmware_is_officially_tested() && !self.confirm_unverified {
            self.error = Some(
                "Confirm that you understand this firmware is unverified before applying.".into(),
            );
            cx.notify();
            return;
        }
        let config = self.config.clone();
        let confirmed = self.confirm_unverified;
        let root_recovery_confirmed = self.confirm_root_recovery;
        self.busy = true;
        self.error = None;
        self.status = format!(
            "Transferring a local-only package over root SSH for {}…",
            plan.mode.label()
        );
        cx.notify();
        cx.spawn(async move |weak, async_cx| {
            let result = async_cx.background_spawn(async move {
                deploy::deploy_plan(&config, &plan, confirmed, root_recovery_confirmed)
            }).await;
            let _ = weak.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.status = "Deployment succeeded. Both overlay mounts are active, retired targets are unmounted, and original hashes plus the backup manifest verified; display refresh is still pending.".into();
                        this.deployment_status = DEPLOY_SUCCESS.into();
                        this.refresh_status = "Pending owner approval".into();
                        this.confirm_refresh = false;
                    }
                    Err(error) => {
                        this.status = "Deployment did not report success. Preserve backups and review the error before retrying.".into();
                        this.deployment_status = "Failed or uncertain".into();
                        this.error = Some(error.to_string());
                    }
                }
                this.confirm_root_recovery = false;
                cx.notify();
            });
        }).detach();
    }

    fn refresh_display(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.settings_open
            || self.deployment_status != DEPLOY_SUCCESS
            || !self.confirm_refresh
        {
            return;
        }
        let config = self.config.clone();
        self.busy = true;
        self.error = None;
        self.status =
            "Restarting surface-manager-daemon.service now. The TV UI may be interrupted…".into();
        self.confirm_refresh = false;
        cx.notify();
        cx.spawn(async move |weak, async_cx| {
            let result = async_cx.background_spawn(async move {
                deploy::restart_display_service(&config, true)
            }).await;
            let _ = weak.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.status = "Display service restarted; both overlay mounts remain active and retired target paths remain unmounted. Check Quick Settings visually on the TV.".into();
                        this.refresh_status = "Restarted; owner visual check pending".into();
                    }
                    Err(error) => {
                        this.status = "Refresh failed or its final state is uncertain; visually check the TV and inspect state before retrying.".into();
                        this.refresh_status = "Failed/uncertain".into();
                        this.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    fn report(&self) -> String {
        match &self.plan {
            Some(plan) => deploy::manual_report(plan, &self.deployment_status, &self.refresh_status, &self.visual_result),
            None => "Midnight UI Patcher report\nNo device inspection completed.\nVisual check: Not checked\n".into(),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_count = self.selected_ids.len();
        let target = if self.config.ssh_target.is_empty() {
            "(not set)"
        } else {
            &self.config.ssh_target
        };
        let firmware = if self.config.firmware_label.is_empty() {
            "(not supplied)"
        } else {
            &self.config.firmware_label
        };
        let plan_ready = self.plan.is_some();
        let unverified = self
            .plan
            .as_ref()
            .is_some_and(|plan| !plan.firmware_is_officially_tested());
        let can_apply = plan_ready
            && !self.busy
            && !self.settings_open
            && self.confirm_root_recovery
            && (!unverified || self.confirm_unverified)
            && self.deployment_status != DEPLOY_SUCCESS;
        let report = self.report();

        div()
            .id("main-scroll")
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            .p_4()
            .bg(rgb(0x17191d))
            .text_color(rgb(0xe8e9eb))
            .overflow_y_scroll()
            .child(div().text_xl().font_weight(gpui::FontWeight::BOLD).child("Midnight UI Patcher"))
            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Patches selected LG webOS Quick Settings QML files locally, then transfers the result directly over root SSH. No QML, logs, or device details are sent anywhere else."))
            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Independent, unofficial project; not affiliated with or endorsed by LG Electronics."))
            .child(
                div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                    .when(self.settings_open, |card| {
                        card.child(
                            div().flex().flex_wrap().items_center().gap_2()
                                .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Connection settings"))
                                .child(div().debug_selector(|| "connection-settings-explainer".to_owned()).flex_1().min_w_0().text_sm().text_color(rgb(0xb7bbc2)).child("Saved locally outside the repository. Use an OpenSSH alias (including ProxyJump) or root@TV_IP. Passwords and private-key contents are never saved."))
                        )
                            .child(settings_field("SSH target", "Use root@TV_IP or an OpenSSH host alias.", self.settings_fields.ssh_target.clone()))
                            .child(settings_field("OpenSSH config file (optional)", "Custom config for aliases and ProxyJump; blank uses the SSH client's default.", self.settings_fields.ssh_config_file.clone()))
                            .child(settings_field("SSH port", "Leave blank to use the SSH client's default port.", self.settings_fields.port.clone()))
                            .child(settings_field("SSH identity file (optional)", "Path to an existing private key; the key stays on this computer.", self.settings_fields.identity_file.clone()))
                            .child(settings_field("TV model (optional)", "Used only as local device metadata.", self.settings_fields.model.clone()))
                            .child(settings_field("Firmware label (optional)", "Shown in compatibility and review information.", self.settings_fields.firmware_label.clone()))
                            .child(
                                div().flex().gap_2()
                                    .child(
                                        div().px_3().py_2().bg(if self.busy { rgb(0x383a3d) } else { rgb(0x315a88) }).rounded_md()
                                            .id("save-settings")
                                            .when(!self.busy, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))))
                                            .child("Save settings"),
                                    )
                                    .child(
                                        div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                                            .id("discard-settings")
                                            .on_click(cx.listener(|this, _, _, cx| this.discard_settings(cx)))
                                            .child("Discard changes"),
                                    ),
                            )
                    })
                    .when(!self.settings_open, |card| {
                        card.child(div().child(format!("SSH target: {target}")))
                            .child(div().child(format!("Model: {}", if self.config.model.is_empty() { "(not supplied)" } else { &self.config.model })))
                            .child(div().child(format!("Firmware label: {firmware}")))
                            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Authentication uses your SSH agent/key or a one-time macOS prompt. Passwords are never saved."))
                            .child(
                                div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                                    .id("edit-settings")
                                    .on_click(cx.listener(|this, _, _, cx| this.edit_settings(cx)))
                                    .child("Edit connection settings"),
                            )
                    }),
            )
            .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child(format!("Choose edits ({selected_count}/{} selected)", PATCHES.len())))
            .children(PATCHES.iter().map(|patch| patch_row(patch, self.selected_ids.contains(patch.id), cx)))
            .child(
                div().flex().gap_2().child(
                    div().px_3().py_2().bg(if self.busy || self.settings_open { rgb(0x383a3d) } else { rgb(0x315a88) }).rounded_md()
                        .id("inspect-tv")
                        .when(!self.busy && !self.settings_open, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.inspect(cx))))
                        .child(if self.busy { "Working…" } else if self.settings_open { "Save or discard settings first" } else { "Inspect TV and preview" }),
                ).child(
                    div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                        .id("select-all")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.busy { return; }
                            this.selected_ids = PATCHES.iter().map(|patch| patch.id.to_owned()).collect();
                            this.plan = None;
                            this.confirm_unverified = false;
                            this.confirm_root_recovery = false;
                            this.deployment_status = "Not applied".into();
                            this.refresh_status = "Not restarted".into();
                            this.visual_result = "Not checked".into();
                            this.status = format!("All {} edits selected. Inspect the TV to validate the current source.", PATCHES.len());
                            cx.notify();
                        }))
                        .child("Select all"),
                ).child(
                    div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                        .id("clear-all")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.busy { return; }
                            this.selected_ids.clear();
                            this.plan = None;
                            this.confirm_unverified = false;
                            this.confirm_root_recovery = false;
                            this.deployment_status = "Not applied".into();
                            this.refresh_status = "Not restarted".into();
                            this.visual_result = "Not checked".into();
                            this.status = "No edits selected. Select at least one before inspection.".into();
                            cx.notify();
                        }))
                        .child("Clear all"),
                ),
            )
            .child(div().p_2().bg(rgb(0x22252b)).rounded_md().child(self.status.clone()))
            .when_some(self.error.clone(), |element, error| {
                element.child(div().p_2().bg(rgb(0x512b2b)).rounded_md().text_color(rgb(0xffdede)).child(error))
            })
            .when_some(self.plan.as_ref(), |element, plan| {
                let selected_list = plan.selected_labels.iter().map(|label| format!("• {label}")).collect::<Vec<_>>().join("\n");
                element.child(
                    div().flex().flex_col().gap_2().p_3().bg(rgb(0x202a24)).rounded_md()
                        .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Pre-install summary"))
                        .child(div().child(format!("Action: {}", plan.mode.label())))
                        .child(div().child(format!("Source check: all {} fetched files parsed; all {} structural patch sites matched.", TARGETS.len(), plan.structurally_verified)))
                        .child(div().child(format!("Selected edits:\n{selected_list}")))
                        .child(div().text_color(rgb(0xf0d28a)).child(plan.compatibility_warning()))
                        .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Apply preserves the original backups and current overlay. It updates state under /var/lib/webosbrew and creates/removes bind mounts; it does not overwrite QML files in /usr or flash firmware. The service refresh is a separate action."))
                        .child(
                            div().flex().items_center().gap_2().cursor_pointer()
                                .id("diff-toggle")
                                .on_click(cx.listener(|this, _, _, cx| { this.show_diff = !this.show_diff; cx.notify(); }))
                                .child(if self.show_diff { "▾ Advanced: hide raw diff" } else { "▸ Advanced: view raw diff" }),
                        )
                        .when(self.show_diff, |element| {
                            element.child(div().p_2().bg(rgb(0x111317)).text_sm().child(plan.diff.clone()))
                        })
                        .when(unverified, |element| {
                            element.child(
                                div().flex().items_center().gap_2().cursor_pointer()
                                    .id("confirm-unverified")
                                    .on_click(cx.listener(|this, _, _, cx| { this.confirm_unverified = !this.confirm_unverified; cx.notify(); }))
                                    .child(if self.confirm_unverified { "[x]" } else { "[ ]" })
                                    .child("I understand this firmware is unverified; matching QML structure is not official support."),
                            )
                        })
                        .child(
                            div().flex().items_center().gap_2().cursor_pointer()
                                .id("confirm-root-recovery")
                                .on_click(cx.listener(|this, _, _, cx| { this.confirm_root_recovery = !this.confirm_root_recovery; cx.notify(); }))
                                .child(if self.confirm_root_recovery { "[x]" } else { "[ ]" })
                                .child("I confirm root SSH is working and available for recovery if the TV UI becomes unusable."),
                        )
                        .child(
                            div().px_3().py_2().bg(if can_apply { rgb(0x25634a) } else { rgb(0x383a3d) }).rounded_md()
                                .id("apply-selected-edits")
                                .when(can_apply, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.deploy(cx))))
                                .child(if self.busy { "Working…" } else { "Apply selected edits" }),
                        ),
                )
            })
            .when(self.deployment_status == DEPLOY_SUCCESS, |element| {
                element.child(
                    div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                        .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Display refresh (separate approval)"))
                        .child(div().text_color(rgb(0xf0d28a)).child("Restarting surface-manager-daemon.service may interrupt the TV UI. Nothing is restarted automatically."))
                        .child(div().child(format!("Deployment: {}", self.deployment_status)))
                        .child(div().child(format!("Refresh: {}", self.refresh_status)))
                        .child(
                            div().flex().items_center().gap_2().cursor_pointer()
                                .id("confirm-display-refresh")
                                .on_click(cx.listener(|this, _, _, cx| { this.confirm_refresh = !this.confirm_refresh; cx.notify(); }))
                                .child(if self.confirm_refresh { "[x]" } else { "[ ]" })
                                .child("I approve restarting this service now; the TV UI may be interrupted."),
                        )
                        .child(
                            div().px_3().py_2().bg(if self.confirm_refresh && !self.busy { rgb(0x8a4f20) } else { rgb(0x383a3d) }).rounded_md()
                                .id("restart-display")
                                .when(self.confirm_refresh && !self.busy, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.refresh_display(cx))))
                                .child(if self.busy { "Working…" } else { "Restart display service" }),
                        )
                        .child(div().child("Visual check:"))
                        .child(
                            div().flex().gap_2()
                                .child(visual_button("Not checked", &self.visual_result, cx))
                                .child(visual_button("Looks good", &self.visual_result, cx))
                                .child(visual_button("Issue noticed", &self.visual_result, cx)),
                        )
                        .child(
                            div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                                .id("copy-report")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(report.clone()));
                                    this.status = "Manual report copied. It excludes QML, logs, host addresses, and credentials.".into();
                                    cx.notify();
                                }))
                                .child("Copy manual report"),
                        ),
                )
            })
    }
}

fn patch_row(patch: &'static PatchSpec, selected: bool, cx: &Context<AppView>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .p_2()
        .bg(rgb(0x22252b))
        .rounded_md()
        .cursor_pointer()
        .id(patch.id)
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.busy { return; }
            if !this.selected_ids.remove(patch.id) {
                this.selected_ids.insert(patch.id.to_owned());
            }
            this.plan = None;
            this.confirm_unverified = false;
            this.deployment_status = "Not applied".into();
            this.confirm_root_recovery = false;
            this.refresh_status = "Not restarted".into();
            this.status = "Patch selection changed; inspect the TV again to validate and preview this selection.".into();
            this.visual_result = "Not checked".into();
            cx.notify();
        }))
        .child(if selected { "[x]" } else { "[ ]" })
        .child(patch.label)
}

fn visual_button(label: &'static str, current: &str, cx: &Context<AppView>) -> impl IntoElement {
    let selected = current == label;
    div()
        .px_2()
        .py_1()
        .bg(if selected {
            rgb(0x315a88)
        } else {
            rgb(0x343944)
        })
        .rounded_md()
        .cursor_pointer()
        .id(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.visual_result = label.to_owned();
            cx.notify();
        }))
        .child(SharedString::from(label))
}
fn settings_field(
    label: &'static str,
    help: &'static str,
    input: Entity<LineInput>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().child(label))
        .child(input)
        .child(div().text_sm().text_color(rgb(0xb7bbc2)).child(help))
}
#[cfg(test)]
mod visual_layout_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn settings_explainer_stays_inside_screenshot_width(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let config = UserConfig::default();
            let settings_fields = SettingsFields::new(&config, cx);
            AppView {
                settings_file: None,
                config,
                settings_fields,
                settings_open: true,
                selected_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(1195.0)));
        window_cx.run_until_parked();

        let bounds = window_cx
            .debug_bounds("connection-settings-explainer")
            .expect("settings explainer should be rendered");
        assert!(bounds.size.width > px(0.0));
        assert!(bounds.origin.x >= px(0.0));
        assert!(bounds.origin.x + bounds.size.width <= px(1568.0));
    }
    #[test]
    fn explicit_settings_file_is_loaded_and_saved_by_the_gui_store() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "gui-config-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let path = root.join("config.toml");
        let mut initial = UserConfig::default();
        initial.ssh_target = "root@tv.local".into();
        initial.ssh_config_file = "before/ssh_config".into();
        config::save_at(&path, &initial).unwrap();

        let (store, loaded, error) = SettingsFile::load(Some(path.clone()));
        assert!(error.is_none());
        assert_eq!(loaded.ssh_config_file, "before/ssh_config");
        let mut edited = loaded;
        edited.ssh_config_file = "after/ssh_config".into();
        store.unwrap().save(&edited).unwrap();

        let reloaded = config::load_or_create_at(&path).unwrap();
        assert_eq!(reloaded.ssh_config_file, "after/ssh_config");
        std::fs::remove_dir_all(root).unwrap();
    }
}
