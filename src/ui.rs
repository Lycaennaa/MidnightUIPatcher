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
use crate::patches::{
    PATCH_OPTION_SPECS, PATCHES, PatchArea, PatchOptionKind, PatchOptionSpec, PatchOptions,
    PatchSpec, TARGETS,
};
use crate::text_input::{self, LineInput};
const DEPLOY_SUCCESS: &str = "Installed/updated; all target mounts and backup manifest verified";
#[cfg(target_os = "macos")]
const AUTHENTICATION_GUIDANCE: &str = "Authentication uses your SSH agent/key or a macOS password prompt when OpenSSH requests one. Passwords and private-key contents are never saved.";
#[cfg(not(target_os = "macos"))]
const AUTHENTICATION_GUIDANCE: &str = "Authentication uses your SSH agent/key. Interactive password prompts are supported only on macOS. Passwords and private-key contents are never saved.";

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
    fn location_description(&self) -> String {
        match self {
            Self::Managed(path) => format!("Default settings file: {}", path.display()),
            Self::Explicit(path) => format!("Custom settings file: {}", path.display()),
        }
    }
}
struct AppView {
    settings_file: Option<SettingsFile>,
    config: UserConfig,
    settings_fields: SettingsFields,
    settings_open: bool,
    settings_load_failed: bool,
    selected_ids: BTreeSet<String>,
    expanded_patch_ids: BTreeSet<String>,
    expanded_area_ids: BTreeSet<&'static str>,
    plan: Option<Arc<DeploymentPlan>>,
    busy: bool,
    error: Option<String>,
    status: String,
    show_diff: bool,
    confirm_unverified: bool,
    confirm_root_recovery: bool,
    confirm_refresh: bool,
    capture_area: Option<PatchArea>,
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
    appearance: AppearanceFields,
}

struct AppearanceFields {
    inputs: Vec<(&'static str, Entity<LineInput>)>,
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
            appearance: AppearanceFields::new(&config.patch_options, cx),
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
        config.patch_options = self.appearance.to_options(cx)?;
        Ok(config)
    }
}
impl AppearanceFields {
    fn new(options: &PatchOptions, cx: &mut Context<AppView>) -> Self {
        let inputs = PATCH_OPTION_SPECS
            .iter()
            .map(|option| {
                let value = options
                    .value(option.id)
                    .expect("option metadata must map to a value");
                let placeholder = match option.kind {
                    PatchOptionKind::Color => "#RRGGBB or #AARRGGBB",
                    PatchOptionKind::Opacity => "0 to 1",
                    PatchOptionKind::RelativeWidth => "0 to 100",
                    PatchOptionKind::BorderWidth => "0 to 10",
                };
                (
                    option.id,
                    cx.new(|cx| LineInput::new(cx, value, placeholder)),
                )
            })
            .collect();
        Self { inputs }
    }

    fn to_options(&self, cx: &mut Context<AppView>) -> Result<PatchOptions, String> {
        let mut options = PatchOptions::default();
        for (id, input) in &self.inputs {
            let value = input.read_with(cx, |input, _| input.text());
            options
                .set_option(id, &value)
                .map_err(|error| error.to_string())?;
        }
        options.validate().map_err(|error| error.to_string())?;
        Ok(options)
    }

    fn input(&self, id: &str) -> Entity<LineInput> {
        self.inputs
            .iter()
            .find(|(option_id, _)| *option_id == id)
            .map(|(_, input)| input.clone())
            .expect("appearance input must have option metadata")
    }

    fn value(&self, id: &str, cx: &Context<AppView>) -> String {
        self.input(id).read_with(cx, |input, _| input.text())
    }

    fn summary(&self, patch_id: &str, cx: &Context<AppView>) -> String {
        let mut summary = String::new();
        for option in PATCH_OPTION_SPECS
            .iter()
            .filter(|option| option.patch_id == patch_id)
        {
            if !summary.is_empty() {
                summary.push_str(" · ");
            }
            summary.push_str(option.label);
            summary.push_str(": ");
            summary.push_str(&self.value(option.id, cx));
        }
        summary
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
            settings_load_failed: error.is_some(),
            selected_ids: PATCHES.iter().map(|patch| patch.id.to_owned()).collect(),
            expanded_area_ids: BTreeSet::new(),
            expanded_patch_ids: BTreeSet::new(),
            plan: None,
            busy: false,
            error,
            status: "Edit connection settings, choose tweaks, then inspect the TV before applying any changes."
                .into(),
            show_diff: false,
            confirm_unverified: false,
            confirm_root_recovery: false,
            confirm_refresh: false,
            capture_area: None,
            deployment_status: "Not applied".into(),
            refresh_status: "Not restarted".into(),
            visual_result: "Not checked".into(),
        }
    }

    fn edit_settings(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.settings_open = true;
        self.error = None;
        cx.notify();
    }

    fn discard_settings(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.settings_fields = SettingsFields::new(&self.config, cx);
        self.settings_open = false;
        self.expanded_patch_ids.clear();
        self.invalidate_plan("Unsaved connection and appearance settings discarded.");
        cx.notify();
    }

    fn reset_appearance_options(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let defaults = PatchOptions::default();
        for (id, input) in &self.settings_fields.appearance.inputs {
            let value = defaults
                .value(id)
                .expect("option metadata must map to a value");
            input.update(cx, |field, cx| field.set_text(value, cx));
        }
        self.invalidate_plan(
            "Appearance choices reset to defaults; inspect to preview or save them.",
        );
        cx.notify();
    }

    fn toggle_patch_options(&mut self, patch_id: &'static str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.plan.is_some() {
            self.invalidate_plan("Option editor opened; inspect again before applying.");
        }
        if !self.expanded_patch_ids.remove(patch_id) {
            self.expanded_patch_ids.insert(patch_id.to_owned());
        }
        cx.notify();
    }

    fn toggle_patch_selection(&mut self, patch_id: &'static str, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if !self.selected_ids.remove(patch_id) {
            self.selected_ids.insert(patch_id.to_owned());
        }
        self.invalidate_plan(&format!(
            "{}/{} tweaks selected; inspect the TV again to validate and preview this selection.",
            self.selected_ids.len(),
            PATCHES.len(),
        ));
        cx.notify();
    }
    fn invalidate_plan(&mut self, status: &str) {
        self.plan = None;
        self.show_diff = false;
        self.confirm_unverified = false;
        self.confirm_root_recovery = false;
        self.confirm_refresh = false;
        self.deployment_status = "Not applied".into();
        self.visual_result = "Not checked".into();
        self.error = None;
        self.status = status.to_owned();
    }

    fn save_appearance_options(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.settings_load_failed {
            self.error = Some(
                "Saved settings could not be loaded; save connection settings before saving appearance choices."
                    .into(),
            );
            self.status =
                "Appearance choices were not saved because the current settings could not be loaded."
                    .into();
            cx.notify();
            return;
        }
        let patch_options = match self.settings_fields.appearance.to_options(cx) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error);
                self.status = "Appearance options are invalid; correct them before saving.".into();
                cx.notify();
                return;
            }
        };
        let Some(settings_file) = self.settings_file.as_ref() else {
            self.error = Some("User-local settings path is unavailable.".into());
            cx.notify();
            return;
        };
        let mut settings = self.config.clone();
        settings.patch_options = patch_options;
        match settings_file.save(&settings) {
            Ok(()) => {
                self.config = settings;
                self.invalidate_plan(
                    "Appearance choices saved locally. Inspect the TV to preview them.",
                );
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.status = "Could not save appearance choices.".into();
            }
        }
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
                self.settings_load_failed = false;
                self.settings_open = false;
                self.plan = None;
                self.deployment_status = "Not applied".into();
                self.refresh_status = "Not restarted".into();
                self.visual_result = "Not checked".into();
                self.confirm_unverified = false;
                self.confirm_root_recovery = false;
                self.confirm_refresh = false;
                self.error = None;
                self.status = "Connection and appearance settings saved locally. Inspect the TV to fetch and verify current QML.".into();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.status = "Could not save connection and appearance settings.".into();
            }
        }
        cx.notify();
    }

    fn inspect(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.settings_open {
            return;
        }
        let mut config = self.config.clone();
        config.patch_options = match self.settings_fields.appearance.to_options(cx) {
            Ok(options) => options,
            Err(error) => {
                self.invalidate_plan(
                    "Appearance options are invalid; correct them before inspecting.",
                );
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let selected = self.selected_ids.clone();
        self.plan = None;
        self.deployment_status = "Not applied".into();
        self.refresh_status = "Not restarted".into();
        self.visual_result = "Not checked".into();
        self.confirm_unverified = false;
        self.confirm_root_recovery = false;
        self.confirm_refresh = false;
        self.error = None;
        self.show_diff = false;
        self.expanded_patch_ids.clear();
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
        let deployment_summary = deploy::deployment_success_summary(plan.selected_labels.len());
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
                        this.status = format!(
                            "Deployment succeeded. {deployment_summary}. Retired targets are unmounted; original hashes verified. Display refresh is still pending."
                        );
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
            || self.settings_load_failed
            || self.settings_file.is_none()
            || self.config.ssh_target.is_empty()
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
                        this.status = "Restart complete. Visually check Quick Settings, the Input Picker, the Input Banner, and the No Signal screen on the TV.".into();
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

    fn select_capture_area(&mut self, area: PatchArea, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.capture_area = Some(area);
        self.error = None;
        self.status = format!(
            "Selected {}. Open that overlay on the TV before capturing.",
            area.label()
        );
        cx.notify();
    }

    fn capture_system_ui(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.settings_open
            || self.settings_load_failed
            || self.settings_file.is_none()
            || self.config.ssh_target.is_empty()
        {
            return;
        }
        let Some(area) = self.capture_area else {
            return;
        };
        let config = self.config.clone();
        self.busy = true;
        self.error = None;
        self.status = format!(
            "Capturing the {} HAL_GAL surface with an offscreen checkerboard over root SSH…",
            area.label()
        );
        cx.notify();
        cx.spawn(async move |weak, async_cx| {
            let result = async_cx
                .background_spawn(async move { deploy::capture_system_ui(&config, area.id()) })
                .await;
            let _ = weak.update(async_cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(path) => {
                        this.status = format!(
                            "Saved checkerboard PNG for {} to {}.",
                            area.label(),
                            path.display()
                        );
                    }
                    Err(error) => {
                        this.status =
                            "Checkerboard System UI capture failed; no local screenshot was kept."
                                .into();
                        this.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn report(&self) -> String {
        match &self.plan {
            Some(plan) => {
                let deployment = if self.deployment_status == DEPLOY_SUCCESS {
                    deploy::deployment_success_summary(plan.selected_labels.len())
                } else {
                    self.deployment_status.clone()
                };
                deploy::manual_report(
                    plan,
                    &deployment,
                    &self.refresh_status,
                    &self.visual_result,
                )
            }
            None => "Midnight UI Patcher report\nNo device inspection completed.\nVisual check: Not checked\n".into(),
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_count = self.selected_ids.len();
        let all_areas_expanded = PatchArea::ALL
            .into_iter()
            .all(|area| self.expanded_area_ids.contains(area.id()));
        let all_tweaks_selected = PATCHES
            .iter()
            .all(|patch| self.selected_ids.contains(patch.id));
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
        let can_restart_service = !self.busy
            && !self.settings_open
            && !self.settings_load_failed
            && self.settings_file.is_some()
            && !self.config.ssh_target.is_empty();
        let can_capture = !self.busy
            && !self.settings_open
            && !self.settings_load_failed
            && self.settings_file.is_some()
            && !self.config.ssh_target.is_empty()
            && self.capture_area.is_some();
        let can_apply = plan_ready
            && !self.busy
            && !self.settings_open
            && self.confirm_root_recovery
            && (!unverified || self.confirm_unverified)
            && self.deployment_status != DEPLOY_SUCCESS;
        let report = self.report();
        let deployment_summary = self
            .plan
            .as_ref()
            .map(|plan| deploy::deployment_success_summary(plan.selected_labels.len()));

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
            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Patches selected LG webOS Quick Settings and Input Picker QML files locally, then transfers the result directly over root SSH. No QML, logs, or device details are sent anywhere else."))
            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Independent, unofficial project; not affiliated with or endorsed by LG Electronics."))
            .child(
                div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                    .when(self.settings_open, |card| {
                        let settings_location = self
                            .settings_file
                            .as_ref()
                            .map(SettingsFile::location_description)
                            .unwrap_or_else(|| "Settings file location unavailable".to_owned());
                        card.child(
                            div().flex().flex_wrap().items_center().gap_2()
                                .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Connection settings"))
                                .child(div().debug_selector(|| "connection-settings-explainer".to_owned()).flex_1().min_w_0().text_sm().text_color(rgb(0xb7bbc2)).child(format!("{settings_location}. Use an OpenSSH alias (including ProxyJump) or root@TV_IP. {AUTHENTICATION_GUIDANCE}")))
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
                                    )
                            )
                    })
                    .when(!self.settings_open, |card| {
                        card.child(div().child(format!("SSH target: {target}")))
                            .child(div().child(format!("Model: {}", if self.config.model.is_empty() { "(not supplied)" } else { &self.config.model })))
                            .child(div().child(format!("Firmware label: {firmware}")))
                            .child(div().text_sm().text_color(rgb(0xb7bbc2)).child(AUTHENTICATION_GUIDANCE))
                            .child(
                                div().px_3().py_2().bg(rgb(0x343944)).rounded_md().cursor_pointer()
                                    .id("edit-settings")
                                    .on_click(cx.listener(|this, _, _, cx| this.edit_settings(cx)))
                                    .child("Edit connection settings"),
                            )
                    }),
            )
            .child(
                div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                    .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Display service"))
                    .child(div().text_sm().text_color(rgb(0xf0d28a)).child("Remote checks require an active overlay and verify its backups and mounts before restart."))
                    .child(
                        div().flex().items_center().gap_2()
                            .id("confirm-display-refresh")
                            .debug_selector(|| "confirm-display-refresh".to_owned())
                            .when(can_restart_service, |control| {
                                control.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_refresh = !this.confirm_refresh;
                                    cx.notify();
                                }))
                            })
                            .child(if self.confirm_refresh { "[x]" } else { "[ ]" })
                            .child("I approve this restart; it may interrupt the TV UI."),
                    )
                    .child(
                        div().px_3().py_2().bg(if can_restart_service && self.confirm_refresh { rgb(0x8a4f20) } else { rgb(0x383a3d) }).rounded_md()
                            .id("restart-display")
                            .debug_selector(|| "restart-display".to_owned())
                            .when(can_restart_service && self.confirm_refresh, |button| {
                                button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.refresh_display(cx)))
                            })
                            .child(if self.busy { "Working…" } else { "Restart display service" }),
                    ),
            )
            .child(
                div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                    .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("System UI capture"))
                    .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Open the overlay on the TV, select its area, then capture. Transparent pixels show a checkerboard in the saved PNG only; the TV display is not changed. Captures stay in the project-root screenshots/ folder."))
                    .child(
                        div().flex().flex_wrap().gap_2()
                            .children(PatchArea::ALL.into_iter().map(|area| {
                                let selected = self.capture_area == Some(area);
                                div().flex().items_center().gap_1().px_3().py_2()
                                    .bg(if selected { rgb(0x315a88) } else { rgb(0x343944) })
                                    .rounded_md()
                                    .id(SharedString::from(format!("capture-area-{}", area.id())))
                                    .debug_selector(move || format!(
                                        "capture-area-{}-{}",
                                        if selected { "selected" } else { "unselected" },
                                        area.id()
                                    ))
                                    .when(!self.busy, |button| {
                                        button.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                                            this.select_capture_area(area, cx)
                                        }))
                                    })
                                    .child(if selected { "[x]" } else { "[ ]" })
                                    .child(area.label())
                            }))
                    )
                    .child(
                        div().px_3().py_2()
                            .bg(if can_capture { rgb(0x315a88) } else { rgb(0x383a3d) })
                            .rounded_md()
                            .id("capture-system-ui")
                            .debug_selector(move || if can_capture {
                                "capture-system-ui-ready".to_owned()
                            } else {
                                "capture-system-ui-disabled".to_owned()
                            })
                            .when(can_capture, |button| {
                                button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                                    this.capture_system_ui(cx)
                                }))
                            })
                            .child(if self.busy { "Working…" } else { "Capture current System UI" }),
                    ),
            )
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::BOLD)
                    .debug_selector(|| {
                        if all_tweaks_selected {
                            "tweak-selection-all".to_owned()
                        } else if selected_count == 0 {
                            "tweak-selection-none".to_owned()
                        } else {
                            "tweak-selection-partial".to_owned()
                        }
                    })
                    .child(format!(
                        "Choose tweaks ({selected_count}/{} selected)",
                        PATCHES.len()
                    )),
            )
            .child(
                div().flex().gap_2().child(
                    div()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x343944))
                        .rounded_md()
                        .cursor_pointer()
                        .id("toggle-all-areas")
                        .debug_selector(|| {
                            if all_areas_expanded {
                                "close-all-areas".to_owned()
                            } else {
                                "expand-all-areas".to_owned()
                            }
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if PatchArea::ALL
                                .into_iter()
                                .all(|area| this.expanded_area_ids.contains(area.id()))
                            {
                                this.expanded_area_ids.clear();
                            } else {
                                this.expanded_area_ids
                                    .extend(PatchArea::ALL.into_iter().map(|area| area.id()));
                            }
                            cx.notify();
                        }))
                        .child(if all_areas_expanded {
                            "Close all areas"
                        } else {
                            "Expand all areas"
                        }),
                )
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .bg(rgb(0x343944))
                        .rounded_md()
                        .cursor_pointer()
                        .id("toggle-all-selection")
                        .debug_selector(|| {
                            if all_tweaks_selected {
                                "deselect-all-tweaks".to_owned()
                            } else {
                                "select-all-tweaks".to_owned()
                            }
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.busy {
                                return;
                            }
                            if PATCHES
                                .iter()
                                .all(|patch| this.selected_ids.contains(patch.id))
                            {
                                this.selected_ids.clear();
                                this.invalidate_plan(&format!(
                                    "{}/{} tweaks selected. Select at least one before inspection.",
                                    this.selected_ids.len(),
                                    PATCHES.len(),
                                ));
                            } else {
                                this.selected_ids = PATCHES
                                    .iter()
                                    .map(|patch| patch.id.to_owned())
                                    .collect();
                                this.invalidate_plan(&format!(
                                    "All {} tweaks selected. Inspect the TV to validate the current source.",
                                    PATCHES.len()
                                ));
                            }
                            cx.notify();
                        }))
                        .child(if all_tweaks_selected {
                            "Deselect all tweaks"
                        } else {
                            "Select all tweaks"
                        }),
                ),
            )
            .children(PatchArea::ALL.into_iter().map(|area| {
                let expanded = self.expanded_area_ids.contains(area.id());
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .debug_selector(|| format!("patch-area-{}", area.id()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .id(area.id())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.expanded_area_ids.contains(area.id()) {
                                    this.expanded_area_ids.remove(area.id());
                                } else {
                                    this.expanded_area_ids.insert(area.id());
                                }
                                cx.notify();
                            }))
                            .child(if expanded { "▼" } else { "▶" })
                            .child(
                                div()
                                    .debug_selector(|| {
                                        format!("patch-area-toggle-{}", area.id())
                                    })
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(area.label()),
                            ),
                    )
                    .when(expanded, |section| {
                        section.children(
                            PATCHES
                                .iter()
                                .filter(move |patch| patch.area == area)
                                .map(|patch| {
                                    patch_row(
                                        patch,
                                        self.selected_ids.contains(patch.id),
                                        self.expanded_patch_ids.contains(patch.id),
                                        self.busy,
                                        &self.settings_fields.appearance,
                                        cx,
                                    )
                                }),
                        )
                    })
            }))
            .child(
                div().flex().flex_col().gap_2()
                    .child(div().text_sm().text_color(rgb(0xb7bbc2)).child("Option editors stay hidden until opened. Inspections use current values; save appearance choices to keep them."))
                    .child(
                        div().flex().gap_2()
                            .child(
                                div().px_3().py_2().bg(if self.busy { rgb(0x383a3d) } else { rgb(0x315a88) }).rounded_md()
                                    .id("save-appearance-options")
                                    .debug_selector(|| "save-appearance-options".to_owned())
                                    .when(!self.busy, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.save_appearance_options(cx))))
                                    .child("Save appearance choices"),
                            )
                            .child(
                                div().px_3().py_2().bg(if self.busy { rgb(0x383a3d) } else { rgb(0x343944) }).rounded_md()
                                    .id("reset-appearance-defaults")
                                    .debug_selector(|| "reset-appearance-defaults".to_owned())
                                    .when(!self.busy, |button| button.cursor_pointer().on_click(cx.listener(|this, _, _, cx| this.reset_appearance_options(cx))))
                                    .child("Reset appearance defaults"),
                            ),
                    ),
            )
            .child(div().p_2().bg(rgb(0x22252b)).rounded_md().child(self.status.clone()))
            .when_some(self.error.clone(), |element, error| {
                element.child(div().p_2().bg(rgb(0x512b2b)).rounded_md().text_color(rgb(0xffdede)).child(error))
            })
            .when_some(self.plan.as_ref(), |element, plan| {
                let selected_list = plan.selected_labels.iter().map(|label| format!("• {label}")).collect::<Vec<_>>().join("\n");
                let apply_label = if self.busy {
                    "Working…".to_owned()
                } else {
                    format!("Apply selected edits ({})", plan.selected_labels.len())
                };
                element.child(
                    div().flex().flex_col().gap_2().p_3().bg(rgb(0x202a24)).rounded_md()
                        .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Pre-install summary"))
                        .child(div().child(format!("Action: {}", plan.mode.label())))
                        .child(div().child(format!("Source check: all {} fetched files parsed; all {} structural patch sites matched.", TARGETS.len(), plan.structurally_verified)))
                        .child(div().child(format!("Selected edits ({}/{}):\n{selected_list}", plan.selected_labels.len(), PATCHES.len())))
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
                                .child(apply_label),
                        ),
                )
            })
            .when(self.deployment_status == DEPLOY_SUCCESS, |element| {
                element.child(
                    div().flex().flex_col().gap_2().p_3().bg(rgb(0x22252b)).rounded_md()
                        .child(div().text_lg().font_weight(gpui::FontWeight::BOLD).child("Deployment report"))
                        .child(div().child(format!("Deployment: {}", deployment_summary.as_deref().unwrap_or(self.deployment_status.as_str()))))
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

fn patch_row(
    patch: &'static PatchSpec,
    selected: bool,
    expanded: bool,
    busy: bool,
    appearance: &AppearanceFields,
    cx: &Context<AppView>,
) -> impl IntoElement {
    let has_options = PATCH_OPTION_SPECS
        .iter()
        .any(|option| option.patch_id == patch.id);
    let summary = appearance.summary(patch.id, cx);

    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .bg(rgb(0x22252b))
        .rounded_md()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .id(SharedString::from(format!("patch-select-{}", patch.id)))
                        .debug_selector(|| format!("patch-select-{}", patch.id))
                        .when(!busy, |checkbox| {
                            checkbox.cursor_pointer().on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.toggle_patch_selection(patch.id, cx);
                                },
                            ))
                        })
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!(
                                        "patch-selection-state-{}-{}",
                                        patch.id,
                                        if selected { "selected" } else { "unselected" }
                                    )
                                })
                                .child(if selected { "[x]" } else { "[ ]" }),
                        )
                        .child(patch.label),
                )
                .child(div().flex_1())
                .when(has_options, |row| {
                    row.child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x343944))
                            .rounded_md()
                            .id(SharedString::from(format!(
                                "patch-options-toggle-{}",
                                patch.id
                            )))
                            .debug_selector(|| format!("patch-options-toggle-{}", patch.id))
                            .when(!busy, |button| {
                                button.cursor_pointer().on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.toggle_patch_options(patch.id, cx);
                                    },
                                ))
                            })
                            .child(if expanded {
                                "Options ▾"
                            } else {
                                "Options ▸"
                            }),
                    )
                }),
        )
        .when(!summary.is_empty(), |row| {
            row.child(
                div()
                    .ml_8()
                    .text_sm()
                    .text_color(rgb(0xb7bbc2))
                    .debug_selector(|| format!("patch-option-summary-{}", patch.id))
                    .child(if selected {
                        format!("Will apply: {summary}")
                    } else {
                        format!("Not selected · if selected: {summary}")
                    }),
            )
        })
        .when(expanded && has_options, |row| {
            row.child(
                div()
                    .ml_8()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .bg(rgb(0x17191d))
                    .rounded_md()
                    .debug_selector(|| format!("patch-option-editor-{}", patch.id))
                    .children(
                        PATCH_OPTION_SPECS
                            .iter()
                            .filter(|option| option.patch_id == patch.id)
                            .map(|option| {
                                patch_option_field(option, appearance.input(option.id), busy, cx)
                            }),
                    ),
            )
        })
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

fn patch_option_field(
    option: &PatchOptionSpec,
    input: Entity<LineInput>,
    busy: bool,
    cx: &Context<AppView>,
) -> impl IntoElement {
    let input_row = if option.kind == PatchOptionKind::Color {
        let current_value = input.read_with(cx, |input, _| input.text());
        let color = hex_color_rgb(&current_value).map_or(0x41454d, |[red, green, blue]| {
            ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
        });
        let picker_input = input.clone();
        let option_id = option.id;
        let option_label = option.label;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x41454d))
                    .bg(rgb(color)),
            )
            .child(div().flex_1().min_w_0().child(input))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .bg(if busy {
                        rgb(0x383a3d)
                    } else {
                        rgb(0x343944)
                    })
                    .rounded_md()
                    .id(SharedString::from(format!(
                        "patch-option-color-picker-{option_id}"
                    )))
                    .debug_selector(move || format!("patch-option-color-picker-{option_id}"))
                    .when(!busy, |button| {
                        let picker_input = picker_input.clone();
                        button.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                            if this.busy {
                                return;
                            }
                            let current = picker_input.read_with(cx, |input, _| input.text());
                            let initial_rgb = hex_color_rgb(&current).unwrap_or([0, 0, 0]);
                            let title = format!("Choose {option_label}");
                            this.busy = true;
                            this.status = format!(
                                "Choose a color for {option_label} or cancel the color dialog."
                            );
                            cx.notify();
                            cx.spawn(async move |weak, async_cx| {
                                let result = async_cx
                                    .background_spawn(async move {
                                        tinyfiledialogs::color_chooser_dialog(
                                            &title,
                                            tinyfiledialogs::DefaultColorValue::RGB(&initial_rgb),
                                        )
                                    })
                                    .await;
                                let _ = weak.update(async_cx, |this, cx| {
                                    this.busy = false;
                                    if let Some((_, selected_rgb)) = result {
                                        let selected =
                                            color_with_picked_rgb(&current, selected_rgb);
                                        let input =
                                            this.settings_fields.appearance.input(option_id);
                                        let value_at_return =
                                            input.read_with(cx, |input, _| input.text());
                                        if value_at_return != current {
                                            this.status = "Color was not applied because the hex value changed while the picker was open.".into();
                                        } else {
                                            input.update(cx, |input, input_cx| {
                                                input.set_text(selected.clone(), input_cx)
                                            });
                                            if !current.eq_ignore_ascii_case(&selected) {
                                                this.invalidate_plan(
                                                    "Appearance colors changed; inspect again before applying.",
                                                );
                                            } else {
                                                this.status =
                                                    "Color selection left this value unchanged."
                                                        .into();
                                            }
                                        }
                                    } else {
                                        this.status =
                                            "Color picker closed without a selection; no changes made."
                                                .into();
                                    }
                                    cx.notify();
                                });
                            })
                            .detach();
                        }))
                    })
                    .child(if busy { "Please wait" } else { "Choose color" }),
            )
    } else {
        div().flex().items_center().gap_2().child(input)
    };

    div()
        .flex()
        .flex_col()
        .gap_1()
        .debug_selector(|| format!("patch-option-field-{}", option.id))
        .child(div().text_sm().child(option.label))
        .child(input_row)
        .child(div().text_sm().text_color(rgb(0xb7bbc2)).child(option.help))
        .when(option.kind == PatchOptionKind::Color, |field| {
            field.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xb7bbc2))
                    .child("Picker changes RGB and preserves alpha from 8-digit hex; edit the hex value to change alpha."),
            )
        })
}

fn hex_color_rgb(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = if hex.len() == 8 { &hex[2..] } else { hex };
    Some([
        u8::from_str_radix(&rgb[0..2], 16).ok()?,
        u8::from_str_radix(&rgb[2..4], 16).ok()?,
        u8::from_str_radix(&rgb[4..6], 16).ok()?,
    ])
}

fn color_with_picked_rgb(current: &str, [red, green, blue]: [u8; 3]) -> String {
    let alpha = current
        .strip_prefix('#')
        .filter(|hex| hex.len() == 8 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .and_then(|hex| u8::from_str_radix(&hex[..2], 16).ok());
    if let Some(alpha) = alpha {
        format!("#{alpha:02X}{red:02X}{green:02X}{blue:02X}")
    } else {
        format!("#{red:02X}{green:02X}{blue:02X}")
    }
}

#[cfg(test)]
mod color_picker_tests {
    use super::{color_with_picked_rgb, hex_color_rgb};

    #[test]
    fn picker_uses_qt_rgb_and_preserves_alpha() {
        assert_eq!(hex_color_rgb("#112233"), Some([0x11, 0x22, 0x33]));
        assert_eq!(hex_color_rgb("#80112233"), Some([0x11, 0x22, 0x33]));
        assert_eq!(hex_color_rgb("#1234🙂"), None);
        assert_eq!(
            color_with_picked_rgb("#80112233", [0xaa, 0xbb, 0xcc]),
            "#80AABBCC"
        );
        assert_eq!(
            color_with_picked_rgb("#112233", [0xaa, 0xbb, 0xcc]),
            "#AABBCC"
        );
        assert_eq!(
            color_with_picked_rgb("#GG112233", [0xaa, 0xbb, 0xcc]),
            "#AABBCC"
        );
    }
}

#[cfg(test)]
mod color_picker_ui_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn every_color_option_renders_a_picker(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let config = UserConfig::default();
            AppView {
                settings_file: None,
                settings_fields: SettingsFields::new(&config, cx),
                config,
                settings_open: false,
                settings_load_failed: false,
                selected_ids: BTreeSet::new(),
                expanded_area_ids: PatchArea::ALL.into_iter().map(PatchArea::id).collect(),
                expanded_patch_ids: PATCHES.iter().map(|patch| patch.id.to_owned()).collect(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(4000.0)));
        window_cx.run_until_parked();

        for selector in [
            "patch-option-color-picker-main-panel-color",
            "patch-option-color-picker-menu-popup-color",
            "patch-option-color-picker-button-default-color",
            "patch-option-color-picker-button-idle-fill",
            "patch-option-color-picker-input-tile-background-color",
            "patch-option-color-picker-input-tile-text-color",
            "patch-option-color-picker-input-tile-border-color",
        ] {
            assert!(
                window_cx.debug_bounds(selector).is_some(),
                "{selector} should be rendered",
            );
        }
    }
}

#[cfg(test)]
mod visual_layout_tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn tweak_rows_show_option_summaries_and_collapsed_editors(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let config = UserConfig::default();
            let settings_fields = SettingsFields::new(&config, cx);
            assert_eq!(
                settings_fields.appearance.summary("main-panel-color", cx),
                "Panel backdrop color: #000000"
            );
            assert_eq!(
                settings_fields
                    .appearance
                    .summary("input-tile-red-text", cx),
                "Input tile text color: #FF0000"
            );
            AppView {
                settings_file: None,
                config,
                settings_fields,
                settings_open: true,
                settings_load_failed: false,
                selected_ids: BTreeSet::from(["main-panel-color".to_owned()]),
                expanded_patch_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(1800.0)));
        window_cx.run_until_parked();

        let bounds = window_cx
            .debug_bounds("connection-settings-explainer")
            .expect("connection settings explainer should be rendered");
        assert!(bounds.size.width > px(0.0));
        assert!(bounds.origin.x >= px(0.0));
        assert!(bounds.origin.x + bounds.size.width <= px(1568.0));
        let collapsed_height = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should be rendered")
            .size
            .height;
        assert!(
            window_cx
                .debug_bounds("patch-select-main-panel-color")
                .is_none(),
            "Quick Settings should start collapsed",
        );
        assert!(
            window_cx
                .debug_bounds("patch-select-input-screen-backdrop")
                .is_none(),
            "Input Picker should start collapsed",
        );
        let quick_settings_toggle = window_cx
            .debug_bounds("patch-area-toggle-quick-settings")
            .expect("Quick Settings heading should expand its area");
        window_cx.simulate_click(quick_settings_toggle.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(
            window_cx
                .debug_bounds("patch-select-main-panel-color")
                .is_some(),
            "opening Quick Settings should show its tweaks",
        );
        let expanded_height = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should remain rendered")
            .size
            .height;
        assert!(
            expanded_height > collapsed_height,
            "opening Quick Settings should increase its height",
        );
        let quick_settings_toggle = window_cx
            .debug_bounds("patch-area-toggle-quick-settings")
            .expect("Quick Settings heading should collapse its area");
        window_cx.simulate_click(quick_settings_toggle.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        let collapsed_height_after = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should remain rendered")
            .size
            .height;
        assert!(
            collapsed_height_after < expanded_height,
            "closing Quick Settings should reduce its height",
        );
        let quick_settings_toggle = window_cx
            .debug_bounds("patch-area-toggle-quick-settings")
            .expect("Quick Settings heading should expand its area again");
        window_cx.simulate_click(quick_settings_toggle.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(
            window_cx
                .debug_bounds("patch-select-main-panel-color")
                .is_some(),
            "reopening Quick Settings should restore its tweaks",
        );
        assert!(
            window_cx
                .debug_bounds("patch-selection-state-main-panel-color-selected")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("patch-option-summary-main-panel-color")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("patch-options-toggle-main-panel-color")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("patch-option-editor-main-panel-color")
                .is_none()
        );

        let option_toggle = window_cx
            .debug_bounds("patch-options-toggle-main-panel-color")
            .expect("option dropdown should have a clickable control");
        window_cx.simulate_click(option_toggle.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();

        assert!(
            window_cx
                .debug_bounds("patch-option-editor-main-panel-color")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("patch-option-field-main-panel-color")
                .is_some()
        );
        let selection_toggle = window_cx
            .debug_bounds("patch-select-main-panel-color")
            .expect("tweak should have a clickable selection checkbox");
        window_cx.simulate_click(selection_toggle.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(
            window_cx
                .debug_bounds("patch-selection-state-main-panel-color-unselected")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("input-tile-text-color-Red")
                .is_none()
        );
        assert!(
            window_cx
                .debug_bounds("reset-appearance-defaults")
                .is_some()
        );
        assert!(
            window_cx
                .debug_bounds("patch-option-editor-main-panel-color")
                .is_some()
        );
    }

    #[gpui::test]
    async fn display_service_restart_control_is_visible_before_deployment(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let mut config = UserConfig::default();
            config.ssh_target = "root@tv.local".into();
            let settings_fields = SettingsFields::new(&config, cx);
            AppView {
                settings_file: Some(SettingsFile::Explicit(PathBuf::from(
                    "target/gui-restart/config.toml",
                ))),
                config,
                settings_fields,
                settings_open: false,
                settings_load_failed: false,
                selected_ids: BTreeSet::new(),
                expanded_patch_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(1491.0)));
        window_cx.run_until_parked();

        let restart_button = window_cx
            .debug_bounds("restart-display")
            .expect("display-service restart button should be visible before deployment");
        let approval_control = window_cx
            .debug_bounds("confirm-display-refresh")
            .expect("display-service restart should require owner approval");
        let quick_settings = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should be rendered");
        assert!(approval_control.size.height > px(0.0));
        assert!(restart_button.origin.y < quick_settings.origin.y);
        assert!(
            restart_button.origin.y + restart_button.size.height <= px(1491.0),
            "display-service restart control should fit in the initial window viewport",
        );
    }
    #[gpui::test]
    async fn system_ui_capture_requires_area_and_marks_selected_area(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let mut config = UserConfig::default();
            config.ssh_target = "root@tv.local".into();
            let settings_fields = SettingsFields::new(&config, cx);
            AppView {
                settings_file: Some(SettingsFile::Explicit(PathBuf::from(
                    "target/gui-capture/config.toml",
                ))),
                config,
                settings_fields,
                settings_open: false,
                settings_load_failed: false,
                selected_ids: BTreeSet::new(),
                expanded_patch_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(2400.0)));
        window_cx.run_until_parked();

        assert!(
            window_cx
                .debug_bounds("capture-system-ui-disabled")
                .is_some()
        );
        let input_picker = window_cx
            .debug_bounds("capture-area-unselected-input-picker")
            .expect("Input Picker should be a selectable capture area");
        window_cx.simulate_click(input_picker.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(
            window_cx
                .debug_bounds("capture-area-selected-input-picker")
                .is_some()
        );
        assert!(window_cx.debug_bounds("capture-system-ui-ready").is_some());
    }

    #[gpui::test]
    async fn bulk_controls_track_area_and_tweak_state(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let config = UserConfig::default();
            AppView {
                settings_file: None,
                settings_fields: SettingsFields::new(&config, cx),
                config,
                settings_open: false,
                settings_load_failed: false,
                selected_ids: BTreeSet::from(["main-panel-color".to_owned()]),
                expanded_patch_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(2400.0)));
        window_cx.run_until_parked();

        assert!(window_cx.debug_bounds("expand-all-areas").is_some());
        assert!(window_cx.debug_bounds("select-all-tweaks").is_some());
        assert!(window_cx.debug_bounds("tweak-selection-partial").is_some());
        let quick_settings_top = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should be rendered")
            .origin
            .y;
        for selector in ["expand-all-areas", "select-all-tweaks"] {
            let button = window_cx
                .debug_bounds(selector)
                .expect("bulk action button should be rendered");
            assert!(
                button.origin.y + button.size.height <= quick_settings_top,
                "bulk action buttons should appear above the tweak groupings",
            );
        }
        let quick_settings_collapsed_height = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should be rendered")
            .size
            .height;
        let input_picker_collapsed_height = window_cx
            .debug_bounds("patch-area-input-picker")
            .expect("Input Picker area should be rendered")
            .size
            .height;

        let expand_all = window_cx
            .debug_bounds("expand-all-areas")
            .expect("Expand all areas button should be available");
        window_cx.simulate_click(expand_all.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(window_cx.debug_bounds("close-all-areas").is_some());
        assert!(
            window_cx
                .debug_bounds("patch-select-input-screen-backdrop")
                .is_some(),
            "Expand all should open the Input Picker area",
        );
        let quick_settings_expanded_height = window_cx
            .debug_bounds("patch-area-quick-settings")
            .expect("Quick Settings area should remain rendered")
            .size
            .height;
        let input_picker_expanded_height = window_cx
            .debug_bounds("patch-area-input-picker")
            .expect("Input Picker area should remain rendered")
            .size
            .height;
        assert!(quick_settings_expanded_height > quick_settings_collapsed_height);
        assert!(input_picker_expanded_height > input_picker_collapsed_height);

        let close_all = window_cx
            .debug_bounds("close-all-areas")
            .expect("Close all areas button should be available");
        window_cx.simulate_click(close_all.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(
            window_cx
                .debug_bounds("patch-area-quick-settings")
                .expect("Quick Settings area should remain rendered")
                .size
                .height
                < quick_settings_expanded_height,
        );
        assert!(
            window_cx
                .debug_bounds("patch-area-input-picker")
                .expect("Input Picker area should remain rendered")
                .size
                .height
                < input_picker_expanded_height,
        );

        let select_all = window_cx
            .debug_bounds("select-all-tweaks")
            .expect("Select all tweaks button should be available");
        window_cx.simulate_click(select_all.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(window_cx.debug_bounds("deselect-all-tweaks").is_some());
        assert!(window_cx.debug_bounds("tweak-selection-all").is_some(),);

        let deselect_all = window_cx
            .debug_bounds("deselect-all-tweaks")
            .expect("Deselect all tweaks button should be available");
        window_cx.simulate_click(deselect_all.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();
        assert!(window_cx.debug_bounds("tweak-selection-none").is_some());
    }
    #[gpui::test]
    async fn appearance_settings_save_with_connection_port(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let (_view, window_cx) = app_cx.add_window_view(|_, cx| {
            let mut config = UserConfig::default();
            config.port = 1111;
            let settings_fields = SettingsFields::new(&config, cx);
            settings_fields
                .port
                .update(cx, |field, cx| field.set_text("2222".into(), cx));
            let text_color = settings_fields
                .appearance
                .inputs
                .iter()
                .find(|(id, _)| *id == "input-tile-text-color")
                .unwrap()
                .1
                .clone();
            text_color.update(cx, |field, cx| field.set_text("#00AACC".into(), cx));

            let saved = settings_fields.to_config(&config, cx).unwrap();
            assert_eq!(saved.port, 2222);
            assert_eq!(saved.patch_options.input_tile_text_color, "#00AACC");

            AppView {
                settings_file: None,
                config: saved,
                settings_fields,
                settings_open: true,
                settings_load_failed: false,
                selected_ids: BTreeSet::new(),
                expanded_patch_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.run_until_parked();
    }

    #[gpui::test]
    async fn save_appearance_button_persists_options_without_overwriting_connection_settings(
        cx: &TestAppContext,
    ) {
        let mut app_cx = (*cx).clone();
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "gui-appearance-save-{}-{}",
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
        initial.patch_options.main_panel_color = "#112233".into();
        config::save_at(&path, &initial).unwrap();

        let app_path = path.clone();
        let (_view, window_cx) = app_cx.add_window_view(move |_, cx| {
            let config = initial.clone();
            let settings_fields = SettingsFields::new(&config, cx);
            settings_fields
                .appearance
                .input("main-panel-color")
                .update(cx, |field, cx| field.set_text("#445566".into(), cx));
            AppView {
                settings_file: Some(SettingsFile::Explicit(app_path)),
                config,
                settings_fields,
                settings_open: false,
                settings_load_failed: false,
                selected_ids: BTreeSet::new(),
                expanded_area_ids: BTreeSet::new(),
                expanded_patch_ids: BTreeSet::new(),
                plan: None,
                busy: false,
                error: None,
                status: "Test".into(),
                show_diff: false,
                confirm_unverified: false,
                confirm_root_recovery: false,
                confirm_refresh: false,
                capture_area: None,
                deployment_status: "Not applied".into(),
                refresh_status: "Not restarted".into(),
                visual_result: "Not checked".into(),
            }
        });
        window_cx.simulate_resize(size(px(1568.0), px(2400.0)));
        window_cx.run_until_parked();
        let save_button = window_cx
            .debug_bounds("save-appearance-options")
            .expect("appearance save action should be rendered");
        window_cx.simulate_click(save_button.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();

        let saved = config::load_or_create_at(&path).unwrap();
        assert_eq!(saved.patch_options.main_panel_color, "#445566");
        assert_eq!(saved.ssh_target, "root@tv.local");
        assert_eq!(saved.ssh_config_file, "before/ssh_config");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui::test]
    async fn appearance_save_refuses_to_overwrite_settings_after_load_failure(cx: &TestAppContext) {
        let mut app_cx = (*cx).clone();
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "gui-appearance-load-failure-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let path = root.join("config.toml");
        let original =
            "ssh_target = \"root@tv.local\"\n[patch_options]\nmain_panel_opacity = 2.0\n";
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&path, original).unwrap();

        let app_path = path.clone();
        let (_view, window_cx) =
            app_cx.add_window_view(move |_, cx| AppView::new(Some(app_path), cx));
        window_cx.simulate_resize(size(px(1568.0), px(2400.0)));
        window_cx.run_until_parked();
        let save_button = window_cx
            .debug_bounds("save-appearance-options")
            .expect("appearance save action should be rendered");
        window_cx.simulate_click(save_button.center(), gpui::Modifiers::default());
        window_cx.run_until_parked();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
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
        assert_eq!(
            store.as_ref().unwrap().location_description(),
            format!("Custom settings file: {}", path.display())
        );
        assert_eq!(
            SettingsFile::Managed(PathBuf::from("default/config.toml")).location_description(),
            "Default settings file: default/config.toml"
        );
        assert_eq!(loaded.ssh_config_file, "before/ssh_config");
        let mut edited = loaded;
        edited.ssh_config_file = "after/ssh_config".into();
        store.unwrap().save(&edited).unwrap();

        let reloaded = config::load_or_create_at(&path).unwrap();
        assert_eq!(reloaded.ssh_config_file, "after/ssh_config");
        std::fs::remove_dir_all(root).unwrap();
    }
}
