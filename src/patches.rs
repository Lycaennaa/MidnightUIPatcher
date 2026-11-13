pub(crate) use crate::patch_options::PatchOption;
pub use crate::patch_options::{
    PATCH_OPTION_SPECS, PatchOptionError, PatchOptionKind, PatchOptionSpec, PatchOptions,
};
mod input_banner;
mod no_signal;

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
pub struct PatchSpec {
    pub(crate) rule: PatchRule,
    pub(crate) area: PatchArea,
    pub id: &'static str,
    pub label: &'static str,
    pub target: &'static str,
}
impl PatchSpec {
    pub fn affected_targets(&self) -> impl Iterator<Item = &'static str> + '_ {
        let sites: &[PatchSite] = match self.rule {
            PatchRule::MultiSite { sites } => sites,
            _ => &[],
        };
        std::iter::once(self.target).chain(
            sites
                .iter()
                .map(|site| site.target)
                .filter(move |target| *target != self.target),
        )
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

pub const PATCHES: [PatchSpec; 25] = [
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "color",
            expected: "\"#323941\"",
            replacement: PatchValue::Option(PatchOption::MainPanelColor),
        },
        id: "main-panel-color",
        label: "Set panel backdrop color",
        target: TARGETS[0],
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "opacity",
            expected: "0.95",
            replacement: PatchValue::Option(PatchOption::MainPanelOpacity),
        },
        id: "main-panel-opacity",
        label: "Set panel backdrop opacity",
        target: TARGETS[0],
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "visible",
            expected: "interfaces.systemInfo.checkBackGround",
            replacement: PatchValue::Literal("false"),
        },
        id: "main-panel-visibility",
        label: "Hide the full-panel backdrop",
        target: TARGETS[0],
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::Scalar {
            root_id: "menuPopupBg",
            scope: None,
            property: "color",
            expected: "\"#2E3741\"",
            replacement: PatchValue::Option(PatchOption::MenuPopupColor),
        },
        id: "menu-popup-color",
        label: "Set the menu popup background color",
        target: TARGETS[0],
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: None,
            property: "bgColor",
            expected: "\"#575E66\"",
            replacement: PatchValue::Option(PatchOption::ButtonDefaultColor),
        },
        id: "button-default-color",
        label: "Set the tile default fill color",
        target: TARGETS[1],
    },
    PatchSpec {
        area: PatchArea::QuickSettings,
        rule: PatchRule::ExpressionLiterals {
            root_id: "background",
            property: "color",
            expected_literal: "\"#575E66\"",
            expected_count: 2,
            required_identifiers: &["objDeactive", "hasFocus"],
            required_operators: &["?", ":"],
            replacement: PatchValue::Option(PatchOption::ButtonIdleFill),
        },
        id: "button-idle-fill",
        label: "Set idle tile fill color",
        target: TARGETS[1],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::Scalar {
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
        id: "input-screen-backdrop",
        label: "Remove the input screen background",
        target: TARGETS[2],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::Scalar {
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
        id: "input-tile-shadow",
        label: "Remove the shadow behind input tiles",
        target: TARGETS[3],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::MultiSite {
            sites: &INPUT_HOME_HUB_SITES,
        },
        id: "input-home-hub",
        label: "Remove the Home Hub tile",
        target: TARGETS[6],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::FunctionStatements {
            root_id: "root",
            targets: &INPUT_LIVE_TV_STATEMENTS,
        },
        id: "input-live-tv",
        label: "Remove the Live TV tile",
        target: TARGETS[7],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::IfCondition {
            root_id: "root",
            function: "getInputData",
            expected: "eimData.connected && (eimData.hdmiPlugIn === undefined || eimData.hdmiPlugIn) && !eimData.dongleConnected",
            replacement: "eimData.connected && eimData.activate && (eimData.hdmiPlugIn === undefined || eimData.hdmiPlugIn) && !eimData.dongleConnected",
            expected_count: 1,
        },
        id: "input-active-signal",
        label: "Hide inputs without an active signal",
        target: TARGETS[7],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::Scalar {
            root_id: "categoryText",
            scope: None,
            property: "visible",
            expected: "entry && entry.showCategory ? entry.showCategory : false",
            replacement: PatchValue::Literal(
                "entry && entry.showCategory && entry.category !== stringSheet.input && entry.category !== stringSheet.inputMNT ? entry.showCategory : false",
            ),
        },
        id: "input-category-text",
        label: "Hide the TV / Inputs category text",
        target: TARGETS[5],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::Scalar {
            root_id: "addOnButton",
            scope: None,
            property: "visible",
            expected: "(entry.hasOwnProperty('addOnInfo') && entry.addOnInfo) &&",
            replacement: PatchValue::Literal(
                "(entry.hasOwnProperty('addOnInfo') && entry.addOnInfo && entry.addOnInfo.type !== 'deviceControl') &&",
            ),
        },
        id: "input-universal-control",
        label: "Hide the Universal Control settings button",
        target: TARGETS[5],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::Scalar {
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
        id: "input-tile-background",
        label: "Set the input tile background color",
        target: TARGETS[3],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::ScalarGroup {
            root_id: "root",
            targets: &INPUT_TILE_ICON_EDITS,
        },
        id: "input-tile-icons",
        label: "Set input tile icon opacity",
        target: TARGETS[4],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::ScalarGroup {
            root_id: "root",
            targets: &INPUT_TILE_TEXT_EDITS,
        },
        id: "input-tile-red-text",
        label: "Set input tile text color",
        target: TARGETS[4],
    },
    PatchSpec {
        area: PatchArea::InputPicker,
        rule: PatchRule::ScalarGroup {
            root_id: "root",
            targets: &INPUT_TILE_BORDER_EDITS,
        },
        id: "input-tile-border",
        label: "Show a border only on the selected input",
        target: TARGETS[4],
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

#[cfg(test)]
mod tests;
