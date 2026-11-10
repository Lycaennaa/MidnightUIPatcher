pub const TARGETS: [&str; 5] = [
    "usr/lib/qml/QuickSettings/Utils/Style.qml",
    "usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml",
    "usr/lib/qml/SystemUI/Notification/NotificationStyle.qml",
    "usr/lib/qml/SystemUI/Notification/Toast.qml",
    "usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml",
];

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScopeStep {
    PropertyObject(&'static str),
    Member(&'static str),
    ChildObjectWithProperty {
        class: &'static str,
        property: &'static str,
        expected: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PatchRule {
    Scalar {
        root_id: &'static str,
        scope: &'static [ScopeStep],
        property: &'static str,
        expected: &'static str,
        replacement: &'static str,
    },
    ExpressionLiterals {
        root_id: &'static str,
        scope: &'static [ScopeStep],
        property: &'static str,
        expected_literal: &'static str,
        expected_count: usize,
        required_identifiers: &'static [&'static str],
        required_operators: &'static [&'static str],
        replacement: &'static str,
    },
    TokenSequence {
        expected: &'static str,
        replacement: &'static str,
        not_found: &'static str,
        ambiguous: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PatchSpec {
    pub(crate) rule: PatchRule,
    pub id: &'static str,
    pub label: &'static str,
    pub target: &'static str,
}

const MAIN_PANEL_SCOPE: &[ScopeStep] = &[ScopeStep::ChildObjectWithProperty {
    class: "Rectangle",
    property: "visible",
    expected: "interfaces.systemInfo.checkBackGround",
}];
const CANDIDATE_BACKGROUND_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("candidateListView"),
    ScopeStep::Member("BG_AREA"),
];
const MAIN_LAYOUT_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("quickSettingMainView"),
    ScopeStep::Member("MAINLAYOUT"),
];
const RECENT_HISTORY_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("quickSettingMainView"),
    ScopeStep::Member("RECENT_AREA"),
];
const EDIT_POPUP_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("quickSettingMainView"),
    ScopeStep::Member("EDITABLE_AREA"),
    ScopeStep::Member("BG_EDITBOX"),
];
const MENU_POPUP_AREA_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("quickSettingMainView"),
    ScopeStep::Member("MENU_AREA"),
];
const MENU_POPUP_BUTTON_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("quickSettingMainView"),
    ScopeStep::Member("MENU_AREA"),
    ScopeStep::Member("BUTTONS"),
];
const SLIDER_POPUP_SCOPE: &[ScopeStep] = &[
    ScopeStep::PropertyObject("barSliderPopup"),
    ScopeStep::Member("BG_AREA"),
];
const TOAST_SCOPE: &[ScopeStep] = &[ScopeStep::PropertyObject("toast")];

pub const PATCHES: [PatchSpec; 19] = [
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: MAIN_PANEL_SCOPE,
            property: "color",
            expected: "\"#323941\"",
            replacement: "\"#000000\"",
        },
        id: "main-panel-color",
        label: "Set panel backdrop color to black",
        target: TARGETS[1],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: MAIN_PANEL_SCOPE,
            property: "opacity",
            expected: "0.95",
            replacement: "1.0",
        },
        id: "main-panel-opacity",
        label: "Make panel backdrop fully opaque",
        target: TARGETS[1],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: MAIN_PANEL_SCOPE,
            property: "visible",
            expected: "interfaces.systemInfo.checkBackGround",
            replacement: "false",
        },
        id: "main-panel-visibility",
        label: "Hide the full-panel backdrop",
        target: TARGETS[1],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "menuPopupBg",
            scope: &[],
            property: "color",
            expected: "\"#2E3741\"",
            replacement: "\"#000000\"",
        },
        id: "menu-popup-color",
        label: "Set the menu popup background to black",
        target: TARGETS[1],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: &[],
            property: "bgColor",
            expected: "\"#575E66\"",
            replacement: "\"#000000\"",
        },
        id: "button-default-color",
        label: "Set the tile default fill to black",
        target: TARGETS[4],
    },
    PatchSpec {
        rule: PatchRule::ExpressionLiterals {
            root_id: "background",
            scope: &[],
            property: "color",
            expected_literal: "\"#575E66\"",
            expected_count: 2,
            required_identifiers: &["objDeactive", "hasFocus"],
            required_operators: &["?", ":"],
            replacement: "\"#000000\"",
        },
        id: "button-idle-fill",
        label: "Set idle tile fills to black",
        target: TARGETS[4],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: CANDIDATE_BACKGROUND_SCOPE,
            property: "BG_COLOR",
            expected: "\"#575E66\"",
            replacement: "\"#000000\"",
        },
        id: "candidate-background-color",
        label: "Set candidate-list background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: CANDIDATE_BACKGROUND_SCOPE,
            property: "BG_OPACITY",
            expected: "0.9",
            replacement: "1.0",
        },
        id: "candidate-background-opacity",
        label: "Make candidate-list background opaque",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: MAIN_LAYOUT_SCOPE,
            property: "COLOR",
            expected: "\"#575E66\"",
            replacement: "\"transparent\"",
        },
        id: "main-layout-color",
        label: "Make the main layout background transparent",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: MAIN_LAYOUT_SCOPE,
            property: "COLOR1",
            expected: "\"#2E3741\"",
            replacement: "\"transparent\"",
        },
        id: "main-layout-secondary-color",
        label: "Make the alternate main-layout background transparent",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: RECENT_HISTORY_SCOPE,
            property: "COLOR",
            expected: "\"#3E444D\"",
            replacement: "\"#000000\"",
        },
        id: "recent-history-color",
        label: "Set recent-history background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: EDIT_POPUP_SCOPE,
            property: "COLOR",
            expected: "\"#2E3741\"",
            replacement: "\"#000000\"",
        },
        id: "edit-popup-color",
        label: "Set edit-popup background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: MENU_POPUP_AREA_SCOPE,
            property: "BG_COLOR",
            expected: "\"#404040\"",
            replacement: "\"#000000\"",
        },
        id: "menu-popup-area-color",
        label: "Set the menu popup area to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: MENU_POPUP_BUTTON_SCOPE,
            property: "SETTING1_BG_COLOR",
            expected: "\"#575E66\"",
            replacement: "\"#000000\"",
        },
        id: "menu-popup-button-color",
        label: "Set menu popup buttons to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "styleRoot",
            scope: SLIDER_POPUP_SCOPE,
            property: "COLOR",
            expected: "\"#575E66\"",
            replacement: "\"#000000\"",
        },
        id: "slider-popup-color",
        label: "Set slider popup background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: TOAST_SCOPE,
            property: "activatedColor",
            expected: "\"#000000\"",
            replacement: "\"#FFFFFF\"",
        },
        id: "toast-activated-color",
        label: "Set activated toast color to white",
        target: TARGETS[2],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: TOAST_SCOPE,
            property: "backgroundColor",
            expected: "\"#787F86\"",
            replacement: "\"#000000\"",
        },
        id: "toast-background-color",
        label: "Set toast background to black",
        target: TARGETS[2],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: TOAST_SCOPE,
            property: "lightOpacity",
            expected: "0.5",
            replacement: "1.0",
        },
        id: "toast-light-opacity",
        label: "Make light-toast background opaque",
        target: TARGETS[2],
    },
    PatchSpec {
        rule: PatchRule::TokenSequence {
            expected: "return style.toast.textColor",
            replacement: "return style.toast.backgroundColor",
            not_found: "toast link color expression not found",
            ambiguous: "toast link color expression is ambiguous",
        },
        id: "toast-link-color",
        label: "Keep activated toast links white on black",
        target: TARGETS[3],
    },
];
