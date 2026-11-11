pub const TARGETS: [&str; 2] = [
    "usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml",
    "usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml",
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChildObjectScope {
    pub(crate) class: &'static str,
    pub(crate) property: &'static str,
    pub(crate) expected: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PatchRule {
    Scalar {
        root_id: &'static str,
        scope: Option<ChildObjectScope>,
        property: &'static str,
        expected: &'static str,
        replacement: &'static str,
    },
    ExpressionLiterals {
        root_id: &'static str,
        property: &'static str,
        expected_literal: &'static str,
        expected_count: usize,
        required_identifiers: &'static [&'static str],
        required_operators: &'static [&'static str],
        replacement: &'static str,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PatchSpec {
    pub(crate) rule: PatchRule,
    pub id: &'static str,
    pub label: &'static str,
    pub target: &'static str,
}

const MAIN_PANEL_CHILD: ChildObjectScope = ChildObjectScope {
    class: "Rectangle",
    property: "visible",
    expected: "interfaces.systemInfo.checkBackGround",
};

pub const PATCHES: [PatchSpec; 6] = [
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "color",
            expected: "\"#323941\"",
            replacement: "\"#000000\"",
        },
        id: "main-panel-color",
        label: "Set panel backdrop color to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "opacity",
            expected: "0.95",
            replacement: "1.0",
        },
        id: "main-panel-opacity",
        label: "Make panel backdrop fully opaque",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "mainLayout",
            scope: Some(MAIN_PANEL_CHILD),
            property: "visible",
            expected: "interfaces.systemInfo.checkBackGround",
            replacement: "false",
        },
        id: "main-panel-visibility",
        label: "Hide the full-panel backdrop",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "menuPopupBg",
            scope: None,
            property: "color",
            expected: "\"#2E3741\"",
            replacement: "\"#000000\"",
        },
        id: "menu-popup-color",
        label: "Set the menu popup background to black",
        target: TARGETS[0],
    },
    PatchSpec {
        rule: PatchRule::Scalar {
            root_id: "root",
            scope: None,
            property: "bgColor",
            expected: "\"#575E66\"",
            replacement: "\"#000000\"",
        },
        id: "button-default-color",
        label: "Set the tile default fill to black",
        target: TARGETS[1],
    },
    PatchSpec {
        rule: PatchRule::ExpressionLiterals {
            root_id: "background",
            property: "color",
            expected_literal: "\"#575E66\"",
            expected_count: 2,
            required_identifiers: &["objDeactive", "hasFocus"],
            required_operators: &["?", ":"],
            replacement: "\"#000000\"",
        },
        id: "button-idle-fill",
        label: "Set idle tile fills to black",
        target: TARGETS[1],
    },
];
