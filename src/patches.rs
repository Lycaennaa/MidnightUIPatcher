pub(crate) use crate::patch_options::PatchOption;
pub use crate::patch_options::{
    PATCH_OPTION_SPECS, PatchOptionError, PatchOptionKind, PatchOptionSpec, PatchOptions,
};
mod compatibility;
mod input_banner;
mod no_signal;

pub use self::compatibility::*;
pub mod rule_revisions {
    pub const SCALAR: u32 = 1;
    pub const EXPRESSION_LITERALS: u32 = 1;
    pub const SCALAR_GROUP: u32 = 1;
    pub const FUNCTION_STATEMENTS: u32 = 1;
    pub const IF_CONDITION: u32 = 1;
    pub const IMPORT: u32 = 1;
    pub const APPEND_MEMBERS: u32 = 1;
    pub const MULTI_SITE: u32 = 1;
}
pub const TARGETS: [&str; 12] = [
    "usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml",
    "usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml",
    "usr/lib/qml/InputPicker/Containers/RootView.qml",
    "usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml",
    "usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml",
    "usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml",
    "usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml",
    "usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml",
    "usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml",
    "usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml",
    "usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/Screensaver.qml",
    "usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/ScreensaverMainText.qml",
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChildObjectScope {
    pub(crate) class: &'static str,
    pub(crate) property: &'static str,
    pub(crate) expected: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScalarTarget {
    pub(crate) scope: Option<ChildObjectScope>,
    pub(crate) property: &'static str,
    pub(crate) expected: &'static str,
    pub(crate) replacement: PatchValue,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FunctionStatementTarget {
    pub(crate) function: &'static str,
    pub(crate) expected: &'static str,
    pub(crate) replacement: &'static str,
    pub(crate) expected_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PatchRule {
    Scalar {
        root_id: &'static str,
        scope: Option<ChildObjectScope>,
        property: &'static str,
        expected: &'static str,
        replacement: PatchValue,
    },
    ExpressionLiterals {
        root_id: &'static str,
        property: &'static str,
        expected_literal: &'static str,
        expected_count: usize,
        required_identifiers: &'static [&'static str],
        required_operators: &'static [&'static str],
        replacement: PatchValue,
    },
    ScalarGroup {
        root_id: &'static str,
        targets: &'static [ScalarTarget],
    },
    FunctionStatements {
        root_id: &'static str,
        targets: &'static [FunctionStatementTarget],
    },
    IfCondition {
        root_id: &'static str,
        function: &'static str,
        expected: &'static str,
        replacement: &'static str,
        expected_count: usize,
    },
    Import {
        module: &'static str,
        version: &'static str,
    },
    AppendMembers {
        root_id: &'static str,
        anchor_property: &'static str,
        expected: &'static str,
        child_types: &'static [&'static str],
        member_template: &'static str,
        replacements: &'static [PatchValue],
    },
    MultiSite {
        sites: &'static [PatchSite],
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PatchSite {
    pub(crate) target: &'static str,
    pub(crate) rule: PatchRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchArea {
    QuickSettings,
    InputPicker,
    InputBanner,
    NoSignal,
}

impl PatchArea {
    pub(crate) const ALL: [Self; 4] = [
        Self::QuickSettings,
        Self::InputPicker,
        Self::InputBanner,
        Self::NoSignal,
    ];

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::QuickSettings => "quick-settings",
            Self::InputPicker => "input-picker",
            Self::InputBanner => "input-banner",
            Self::NoSignal => "no-signal",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::QuickSettings => "Quick Settings",
            Self::InputPicker => "Input Picker",
            Self::InputBanner => "Input Banner",
            Self::NoSignal => "No Signal",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PatchVariant {
    pub major: WebOsMajor,
    pub target: &'static str,
    pub(crate) rule: PatchRule,
}

impl PatchVariant {
    pub(crate) const fn new(major: WebOsMajor, target: &'static str, rule: PatchRule) -> Self {
        Self {
            major,
            target,
            rule,
        }
    }

    pub fn affected_targets(&self) -> impl Iterator<Item = &'static str> + '_ {
        let (single, sites): (Option<&'static str>, &[PatchSite]) = match self.rule {
            PatchRule::MultiSite { sites } => (None, sites),
            _ => (Some(self.target), &[]),
        };
        let mut seen = Vec::new();
        single
            .into_iter()
            .chain(sites.iter().map(|site| site.target))
            .filter(move |target| {
                if seen.contains(target) {
                    false
                } else {
                    seen.push(*target);
                    true
                }
            })
    }

    pub const fn rule_kind(&self) -> PatchRuleKind {
        self.rule.kind()
    }

    pub fn implementation_revision(&self, patch_id: &str) -> ImplementationRevision {
        self.implementation_revision_with_revisions(patch_id, RuleRevisions::CURRENT)
    }

    pub fn implementation_revision_with_revisions(
        &self,
        patch_id: &str,
        revisions: RuleRevisions,
    ) -> ImplementationRevision {
        compute_implementation_revision(patch_id, self, revisions)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PatchSpec {
    pub(crate) area: PatchArea,
    pub id: &'static str,
    pub label: &'static str,
    pub variants: &'static [PatchVariant],
}

impl PatchSpec {
    pub fn primary_variant(&self) -> &'static PatchVariant {
        self.variant(WebOsMajor::WEBOS_24)
            .or_else(|| self.variants.first())
            .expect("every logical tweak must declare at least one implementation variant")
    }

    pub fn primary_target(&self) -> &'static str {
        self.primary_variant().target
    }

    pub fn affected_targets(&self) -> impl Iterator<Item = &'static str> + '_ {
        let mut seen = Vec::new();
        self.variants
            .iter()
            .flat_map(|variant| variant.affected_targets())
            .filter(move |target| {
                if seen.contains(target) {
                    false
                } else {
                    seen.push(*target);
                    true
                }
            })
    }

    pub fn variant(&self, major: WebOsMajor) -> Option<&'static PatchVariant> {
        self.variants.iter().find(|variant| variant.major == major)
    }
    /// Identifies the direct variant or closest-earlier upward-fallback candidate for `major`.
    /// Returning an `UpwardFallback` candidate does not authorize applying it without
    /// explicit per-tweak opt-in and compatibility-gate checks.
    pub fn resolve_variant(
        &self,
        major: WebOsMajor,
    ) -> Option<(&'static PatchVariant, VariantSelectionKind)> {
        if let Some(direct) = self.variant(major) {
            return Some((direct, VariantSelectionKind::Direct));
        }
        self.variants
            .iter()
            .filter(|variant| variant.major < major)
            .max_by_key(|variant| variant.major)
            .map(|fallback| {
                (
                    fallback,
                    VariantSelectionKind::UpwardFallback {
                        origin_major: fallback.major,
                    },
                )
            })
    }

    pub fn resolve(&self, identity: &TvIdentity) -> ResolvedPatch {
        self.resolve_with(identity, &COMPATIBILITY_EVIDENCE, RuleRevisions::CURRENT)
    }

    pub fn resolve_with(
        &self,
        identity: &TvIdentity,
        evidence: &[EvidenceRecord],
        revisions: RuleRevisions,
    ) -> ResolvedPatch {
        compatibility::resolve_patch_with(self, identity, evidence, revisions)
    }

    pub fn evidence(&self) -> impl Iterator<Item = &'static EvidenceRecord> + '_ {
        COMPATIBILITY_EVIDENCE
            .iter()
            .filter(move |record| record.patch_id == self.id)
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum PatchValue {
    Literal(&'static str),
    Option(PatchOption),
}

impl PatchOptions {
    pub(crate) fn replacement(&self, value: PatchValue) -> String {
        match value {
            PatchValue::Literal(text) => text.to_owned(),
            PatchValue::Option(option) => self.option_replacement(option),
        }
    }
}

const MAIN_PANEL_CHILD: ChildObjectScope = ChildObjectScope {
    class: "Rectangle",
    property: "visible",
    expected: "interfaces.systemInfo.checkBackGround",
};

const INPUT_TILE_ICON_EDITS: [ScalarTarget; 3] = [
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "inputSourceImage",
        }),
        property: "opacity",
        expected: "entry.activate ? 1 : 0.5",
        replacement: PatchValue::Option(PatchOption::InputTileIconOpacity),
    },
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "deviceControlImg",
        }),
        property: "opacity",
        expected: "entry.activate ? 1 : 0.5",
        replacement: PatchValue::Option(PatchOption::InputTileIconOpacity),
    },
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "curInputMark",
        }),
        property: "visible",
        expected: "entry.currentInput ? entry.currentInput : false",
        replacement: PatchValue::Literal("false"),
    },
];

const INPUT_TILE_TEXT_EDITS: [ScalarTarget; 2] = [
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "ScrollText",
            property: "id",
            expected: "deviceName",
        }),
        property: "color",
        expected: "entry.activate && (entry.status === \"connected\" || entry.status === undefined) ? \"#000000\" : \"#808080\"",
        replacement: PatchValue::Option(PatchOption::InputTileTextColor),
    },
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "ScrollText",
            property: "id",
            expected: "portName",
        }),
        property: "color",
        expected: "entry.activate ? \"#4D4D4D\" : \"#808080\"",
        replacement: PatchValue::Option(PatchOption::InputTileTextColor),
    },
];

const INPUT_TILE_BORDER_EDITS: [ScalarTarget; 2] = [
    ScalarTarget {
        scope: None,
        property: "border.color",
        expected: "entry.activate ? \"#FFFFFFFF\" : \"#26FFFFFF\"",
        replacement: PatchValue::Option(PatchOption::InputTileBorderColor),
    },
    ScalarTarget {
        scope: None,
        property: "border.width",
        expected: "entry.activate ? styler.getRelativeValue(1) : styler.getRelativeValue(0.5)",
        replacement: PatchValue::Option(PatchOption::InputTileBorderWidth),
    },
];
const INPUT_HOME_HUB_STATEMENTS: [FunctionStatementTarget; 1] = [FunctionStatementTarget {
    function: "createInputPickerModel",
    expected: "createTitle();",
    replacement: "",
    expected_count: 1,
}];

const INPUT_LIVE_TV_STATEMENTS: [FunctionStatementTarget; 1] = [FunctionStatementTarget {
    function: "getInputData",
    expected: "root.addTVItem();",
    replacement: "",
    expected_count: 2,
}];
const INPUT_HOME_HUB_SITES: [PatchSite; 1] = [PatchSite {
    target: TARGETS[6],
    rule: PatchRule::FunctionStatements {
        root_id: "root",
        targets: &INPUT_HOME_HUB_STATEMENTS,
    },
}];

const MAIN_PANEL_COLOR_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[0],
    PatchRule::Scalar {
        root_id: "mainLayout",
        scope: Some(MAIN_PANEL_CHILD),
        property: "color",
        expected: "\"#323941\"",
        replacement: PatchValue::Option(PatchOption::MainPanelColor),
    },
)];

const MAIN_PANEL_OPACITY_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[0],
    PatchRule::Scalar {
        root_id: "mainLayout",
        scope: Some(MAIN_PANEL_CHILD),
        property: "opacity",
        expected: "0.95",
        replacement: PatchValue::Option(PatchOption::MainPanelOpacity),
    },
)];

const MAIN_PANEL_VISIBILITY_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[0],
    PatchRule::Scalar {
        root_id: "mainLayout",
        scope: Some(MAIN_PANEL_CHILD),
        property: "visible",
        expected: "interfaces.systemInfo.checkBackGround",
        replacement: PatchValue::Literal("false"),
    },
)];

const MENU_POPUP_COLOR_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[0],
    PatchRule::Scalar {
        root_id: "menuPopupBg",
        scope: None,
        property: "color",
        expected: "\"#2E3741\"",
        replacement: PatchValue::Option(PatchOption::MenuPopupColor),
    },
)];

const BUTTON_DEFAULT_COLOR_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[1],
    PatchRule::Scalar {
        root_id: "root",
        scope: None,
        property: "bgColor",
        expected: "\"#575E66\"",
        replacement: PatchValue::Option(PatchOption::ButtonDefaultColor),
    },
)];

const BUTTON_IDLE_FILL_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[1],
    PatchRule::ExpressionLiterals {
        root_id: "background",
        property: "color",
        expected_literal: "\"#575E66\"",
        expected_count: 2,
        required_identifiers: &["objDeactive", "hasFocus"],
        required_operators: &["?", ":"],
        replacement: PatchValue::Option(PatchOption::ButtonIdleFill),
    },
)];

const INPUT_SCREEN_BACKDROP_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[2],
    PatchRule::Scalar {
        root_id: "root",
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "gradationBG",
        }),
        property: "source",
        expected: "styler.gradationBG",
        replacement: PatchValue::Literal("\"\""),
    },
)];

const INPUT_TILE_SHADOW_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[3],
    PatchRule::Scalar {
        root_id: "mainRibbonItem",
        scope: Some(ChildObjectScope {
            class: "Loader",
            property: "id",
            expected: "shadowLoader",
        }),
        property: "active",
        expected: "hasFocus",
        replacement: PatchValue::Literal("false"),
    },
)];

const INPUT_HOME_HUB_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[6],
    PatchRule::MultiSite {
        sites: &INPUT_HOME_HUB_SITES,
    },
)];

const INPUT_LIVE_TV_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[7],
    PatchRule::FunctionStatements {
        root_id: "root",
        targets: &INPUT_LIVE_TV_STATEMENTS,
    },
)];

const INPUT_ACTIVE_SIGNAL_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[7],
    PatchRule::IfCondition {
        root_id: "root",
        function: "getInputData",
        expected: "eimData.connected && (eimData.hdmiPlugIn === undefined || eimData.hdmiPlugIn) && !eimData.dongleConnected",
        replacement: "eimData.connected && eimData.activate && (eimData.hdmiPlugIn === undefined || eimData.hdmiPlugIn) && !eimData.dongleConnected",
        expected_count: 1,
    },
)];

const INPUT_CATEGORY_TEXT_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[5],
    PatchRule::Scalar {
        root_id: "categoryText",
        scope: None,
        property: "visible",
        expected: "entry && entry.showCategory ? entry.showCategory : false",
        replacement: PatchValue::Literal(
            "entry && entry.showCategory && entry.category !== stringSheet.input && entry.category !== stringSheet.inputMNT ? entry.showCategory : false",
        ),
    },
)];

const INPUT_UNIVERSAL_CONTROL_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[5],
    PatchRule::Scalar {
        root_id: "addOnButton",
        scope: None,
        property: "visible",
        expected: "(entry.hasOwnProperty('addOnInfo') && entry.addOnInfo) &&",
        replacement: PatchValue::Literal(
            "(entry.hasOwnProperty('addOnInfo') && entry.addOnInfo && entry.addOnInfo.type !== 'deviceControl') &&",
        ),
    },
)];

const INPUT_TILE_BACKGROUND_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[3],
    PatchRule::Scalar {
        root_id: "mainRibbonItem",
        scope: Some(ChildObjectScope {
            class: "Rectangle",
            property: "id",
            expected: "bg",
        }),
        property: "color",
        expected: "entry.activate ? \"#E6E6E6\" : entry.type === \"title\" ? \"#575E66\" : \"#A6A6A6\"",
        replacement: PatchValue::Option(PatchOption::InputTileBackgroundColor),
    },
)];

const INPUT_TILE_ICONS_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[4],
    PatchRule::ScalarGroup {
        root_id: "root",
        targets: &INPUT_TILE_ICON_EDITS,
    },
)];

const INPUT_TILE_TEXT_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[4],
    PatchRule::ScalarGroup {
        root_id: "root",
        targets: &INPUT_TILE_TEXT_EDITS,
    },
)];

const INPUT_TILE_BORDER_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[4],
    PatchRule::ScalarGroup {
        root_id: "root",
        targets: &INPUT_TILE_BORDER_EDITS,
    },
)];

pub const PATCHES: [PatchSpec; 25] = [
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "main-panel-color",
        label: "Set panel backdrop color",
        variants: &MAIN_PANEL_COLOR_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "main-panel-opacity",
        label: "Set panel backdrop opacity",
        variants: &MAIN_PANEL_OPACITY_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "main-panel-visibility",
        label: "Hide the full-panel backdrop",
        variants: &MAIN_PANEL_VISIBILITY_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "menu-popup-color",
        label: "Set the menu popup background color",
        variants: &MENU_POPUP_COLOR_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "button-default-color",
        label: "Set the tile default fill color",
        variants: &BUTTON_DEFAULT_COLOR_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        id: "button-idle-fill",
        label: "Set idle tile fill color",
        variants: &BUTTON_IDLE_FILL_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-screen-backdrop",
        label: "Remove the input screen background",
        variants: &INPUT_SCREEN_BACKDROP_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-tile-shadow",
        label: "Remove the shadow behind input tiles",
        variants: &INPUT_TILE_SHADOW_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-home-hub",
        label: "Remove the Home Hub tile",
        variants: &INPUT_HOME_HUB_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-live-tv",
        label: "Remove the Live TV tile",
        variants: &INPUT_LIVE_TV_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-active-signal",
        label: "Hide inputs without an active signal",
        variants: &INPUT_ACTIVE_SIGNAL_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-category-text",
        label: "Hide the TV / Inputs category text",
        variants: &INPUT_CATEGORY_TEXT_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-universal-control",
        label: "Hide the Universal Control settings button",
        variants: &INPUT_UNIVERSAL_CONTROL_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-tile-background",
        label: "Set the input tile background color",
        variants: &INPUT_TILE_BACKGROUND_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-tile-icons",
        label: "Set input tile icon opacity",
        variants: &INPUT_TILE_ICONS_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-tile-red-text",
        label: "Set input tile text color",
        variants: &INPUT_TILE_TEXT_VARIANTS,
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        id: "input-tile-border",
        label: "Show a border only on the selected input",
        variants: &INPUT_TILE_BORDER_VARIANTS,
    },
    input_banner::BACKGROUND,
    input_banner::TEXT_COLOR,
    input_banner::DIVIDER,
    input_banner::MENU_STYLE,
    no_signal::ANIMATION,
    no_signal::MOVEMENT,
    no_signal::INSTRUCTIONS,
    no_signal::TEXT_COLOR,
];

pub fn resolve_patches(identity: &TvIdentity) -> Vec<ResolvedPatch> {
    PATCHES
        .iter()
        .map(|patch| patch.resolve(identity))
        .collect()
}

pub fn resolve_patch(id: &str, identity: &TvIdentity) -> Option<ResolvedPatch> {
    PATCHES
        .iter()
        .find(|patch| patch.id == id)
        .map(|patch| patch.resolve(identity))
}

pub fn targets_for_major(major: WebOsMajor) -> Vec<&'static str> {
    let mut collected = Vec::new();
    for patch in &PATCHES {
        let Some((variant, _)) = patch.resolve_variant(major) else {
            continue;
        };
        for target in variant.affected_targets() {
            if !collected.contains(&target) {
                collected.push(target);
            }
        }
    }

    let mut ordered = Vec::with_capacity(collected.len());
    for target in TARGETS {
        if collected.contains(&target) {
            ordered.push(target);
        }
    }
    for target in collected {
        if !ordered.contains(&target) {
            ordered.push(target);
        }
    }
    ordered
}

const BASELINE_REPORT_REF: &str = "owner-baseline-webos24-33.22.75";

const fn baseline_evidence(
    patch_id: &'static str,
    implementation_revision: &'static str,
) -> EvidenceRecord {
    EvidenceRecord {
        patch_id,
        implementation_revision,
        major: WebOsMajor::WEBOS_24,
        firmware_version: "33.22.75",
        model: None,
        machine: Some("O24"),
        region: "GLOBAL",
        status: EvidenceStatus::MaintainerVerified,
        visual_scope: VisualConfirmationScope::NormalVisualStateOnly,
        report_ref: BASELINE_REPORT_REF,
        superseded: false,
    }
}

pub const COMPATIBILITY_EVIDENCE: [EvidenceRecord; 25] = [
    baseline_evidence(
        "main-panel-color",
        "sha256:3defab7b64fc1152a07dbdcff9f7f6f66f61ef3f45a5c4174468e5198430c067",
    ),
    baseline_evidence(
        "main-panel-opacity",
        "sha256:67913acc9b565f4c2ee10d4590e52b4b3362514f28e8142c25dfd633c60956da",
    ),
    baseline_evidence(
        "main-panel-visibility",
        "sha256:a5f90d6198915f1808174721859bb737bd3d06f15f77d4fd5b29b058f03c4f2b",
    ),
    baseline_evidence(
        "menu-popup-color",
        "sha256:afd8ad3f528a9e8ecf8996c063682ba478b9e3445f5b340aaa8d8a161b5e18a1",
    ),
    baseline_evidence(
        "button-default-color",
        "sha256:13c8d44e9d4265615ea721892f80a15b3e7259ebb6fcd497b6e6b66ffa6ddf65",
    ),
    baseline_evidence(
        "button-idle-fill",
        "sha256:d0af41efa5def505d720e10d481e38bf34a0853d23653a9f2c39e2cc4c5e59d4",
    ),
    baseline_evidence(
        "input-screen-backdrop",
        "sha256:594b178584e24753883e2a4e8c19eafdabca2abff26cb2613be638cedc0a6e72",
    ),
    baseline_evidence(
        "input-tile-shadow",
        "sha256:6d9b6035ab2ea2467c08dca3bf606bbade4952fe2f2a33e2809416662357b353",
    ),
    baseline_evidence(
        "input-home-hub",
        "sha256:728899ccf3e9d177a6f34b243837664d1eb61ac3e1fe3213a2970095f16a6ea8",
    ),
    baseline_evidence(
        "input-live-tv",
        "sha256:2984c50414bafc78c7f47274aa6f82f8f636cc6e1f1106768c1ab79ee12f19ed",
    ),
    baseline_evidence(
        "input-active-signal",
        "sha256:e231c60a0ba4f38706aa1eaa492e481851c81fd9ce9ff372f25d1e43a526e2b3",
    ),
    baseline_evidence(
        "input-category-text",
        "sha256:4a55c7ae03f394a8c7b6a66e680586066c7bceae474285bc2b6ace69d886dfaf",
    ),
    baseline_evidence(
        "input-universal-control",
        "sha256:2a38f684f1865a53f5dfee148729134f974b50c716c06c44c12955f1039337fe",
    ),
    baseline_evidence(
        "input-tile-background",
        "sha256:8665ed691ccae23b20b58fc0a39fa88970744e7bc5ac15b3519642a26635aba1",
    ),
    baseline_evidence(
        "input-tile-icons",
        "sha256:8ae9b8aefcb69d75dbaaf51941885aa5815516965d357e2c22402374986a72e2",
    ),
    baseline_evidence(
        "input-tile-red-text",
        "sha256:702cfa3b97c223f0d5d6336bdbcb3bb6c33fbd37f5c41dd337ad46d681676688",
    ),
    baseline_evidence(
        "input-tile-border",
        "sha256:e523b9e0818fbc6eb8175b320c51b976bd6d5cee9c2e74033390e578f159b34b",
    ),
    baseline_evidence(
        "input-banner-background",
        "sha256:6f3faa0bb3053d577df1aeae29b247125cb271c8b6fd0cf56a8e01a22754f217",
    ),
    baseline_evidence(
        "input-banner-text-color",
        "sha256:1688da3ed7ed7475871fd88627be823ebe3da3bfadd5b838ab92e341ddff9897",
    ),
    baseline_evidence(
        "input-banner-divider",
        "sha256:e5b4b2787d51aedf2eeea1d124f451331bbb51200d8f87943b1b84d683a6623b",
    ),
    baseline_evidence(
        "input-banner-menu-color",
        "sha256:62ac021b8f37b3bf3f97757718a0cbaeaf677c0ea798a48d9cd72a497bb9769a",
    ),
    baseline_evidence(
        "no-signal-animation",
        "sha256:77253648eb5a165518776cd987b3e0d5068a54b7dfdf8228aa08d8c06c4196e1",
    ),
    baseline_evidence(
        "no-signal-movement",
        "sha256:7ab5b286f7483db11550b6caef36ccfcf35427d22cf5346d93b40d9aaae6bca7",
    ),
    baseline_evidence(
        "no-signal-instructions",
        "sha256:b662663ae1c0130abe375c07128d6c260a9f0d07c49f5af0092053fc5052cdf1",
    ),
    baseline_evidence(
        "no-signal-text-color",
        "sha256:80a0a6721dcda4f4bfe5dc6a96ebdb2c1865e5568a686d9206af22d549856e97",
    ),
];

#[cfg(test)]
mod tests;
