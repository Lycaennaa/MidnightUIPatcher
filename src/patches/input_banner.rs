use super::{
    ChildObjectScope, PatchArea, PatchOption, PatchRule, PatchSite, PatchSpec, PatchValue,
    PatchVariant, ScalarTarget, TARGETS, WebOsMajor,
};
const INPUT_BANNER_MAIN_BACKGROUND_EDITS: [ScalarTarget; 2] = [
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "bgMiddle",
        }),
        property: "source",
        expected: "root.visible ? styleSheet.banner.middleBg : \"\"",
        replacement: PatchValue::Literal("\"\""),
    },
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "bgRight",
        }),
        property: "source",
        expected: "root.visible ? styleSheet.banner.rightBg : \"\"",
        replacement: PatchValue::Literal("\"\""),
    },
];

const INPUT_BANNER_SIMPLE_BACKGROUND_EDITS: [ScalarTarget; 2] = [
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "bgMiddle",
        }),
        property: "source",
        expected: "root.visible ? (hasFocus? styleSheet.banner.middleFocusedBg : styleSheet.banner.middleBg) : \"\"",
        replacement: PatchValue::Literal("\"\""),
    },
    ScalarTarget {
        scope: Some(ChildObjectScope {
            class: "Image",
            property: "id",
            expected: "bgRight",
        }),
        property: "source",
        expected: "root.visible ? (hasFocus? styleSheet.banner.rightFocusedBg : styleSheet.banner.rightBg) : \"\"",
        replacement: PatchValue::Literal("\"\""),
    },
];

const INPUT_BANNER_BACKGROUND_SITES: [PatchSite; 2] = [
    PatchSite {
        target: TARGETS[8],
        rule: PatchRule::ScalarGroup {
            root_id: "bannerBg",
            targets: &INPUT_BANNER_MAIN_BACKGROUND_EDITS,
        },
    },
    PatchSite {
        target: TARGETS[9],
        rule: PatchRule::ScalarGroup {
            root_id: "zappingBg",
            targets: &INPUT_BANNER_SIMPLE_BACKGROUND_EDITS,
        },
    },
];
const INPUT_BANNER_TEXT_SITES: [PatchSite; 2] = [
    PatchSite {
        target: TARGETS[9],
        rule: PatchRule::Scalar {
            root_id: "inputNameItem",
            scope: None,
            property: "color",
            expected: "needFocusHandle ? styleSheet.banner.inputFocusedColor : styleSheet.banner.inputColor",
            replacement: PatchValue::Option(PatchOption::InputBannerTextColor),
        },
    },
    PatchSite {
        target: TARGETS[9],
        rule: PatchRule::Scalar {
            root_id: "inputLabelItem",
            scope: None,
            property: "color",
            expected: "needFocusHandle ? styleSheet.banner.inputFocusedColor : styleSheet.banner.inputColor",
            replacement: PatchValue::Option(PatchOption::InputBannerTextColor),
        },
    },
];
const INPUT_BANNER_DIVIDER: PatchRule = PatchRule::Scalar {
    root_id: "bannerRow",
    scope: Some(ChildObjectScope {
        class: "Rectangle",
        property: "width",
        expected: "styleSheet.banner.optionBarWidth",
    }),
    property: "visible",
    expected: "interfaces[C.NAME_INBANNER] && interfaces[C.NAME_INBANNER].isOkBanner",
    replacement: PatchValue::Literal("false"),
};
const INPUT_BANNER_MENU_STYLE_VALUES: [PatchValue; 4] = [
    PatchValue::Option(PatchOption::InputBannerMenuFocusFillColor),
    PatchValue::Option(PatchOption::InputBannerMenuColor),
    PatchValue::Option(PatchOption::InputBannerMenuFocusBorderColor),
    PatchValue::Option(PatchOption::InputBannerMenuFocusBorderWidth),
];

const INPUT_BANNER_MENU_SITES: [PatchSite; 2] = [
    PatchSite {
        target: TARGETS[8],
        rule: PatchRule::Import {
            module: "QtGraphicalEffects",
            version: "1.12",
        },
    },
    PatchSite {
        target: TARGETS[8],
        rule: PatchRule::AppendMembers {
            root_id: "option",
            anchor_property: "baInterface",
            expected: "interfaces[C.NAME_INBANNER]",
            child_types: &["ColorOverlay", "Rectangle"],
            member_template: r#"bgFocusedColor: {{0}}
ColorOverlay {
    anchors.fill: option.img
    source: option.img
    z: 1
    color: {{1}}
}
Rectangle {
    anchors.fill: parent
    color: "transparent"
    border.color: {{2}}
    border.width: {{3}}
    z: 2
    visible: option.hasFocus
}"#,
            replacements: &INPUT_BANNER_MENU_STYLE_VALUES,
        },
    },
];
const BACKGROUND_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[8],
    PatchRule::MultiSite {
        sites: &INPUT_BANNER_BACKGROUND_SITES,
    },
)];

const TEXT_COLOR_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[9],
    PatchRule::MultiSite {
        sites: &INPUT_BANNER_TEXT_SITES,
    },
)];

const DIVIDER_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[8],
    INPUT_BANNER_DIVIDER,
)];

const MENU_STYLE_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[8],
    PatchRule::MultiSite {
        sites: &INPUT_BANNER_MENU_SITES,
    },
)];

pub(super) const BACKGROUND: PatchSpec = PatchSpec {
    area: PatchArea::InputBanner,
    id: "input-banner-background",
    label: "Remove the input source banner background",
    variants: &BACKGROUND_VARIANTS,
};

pub(super) const TEXT_COLOR: PatchSpec = PatchSpec {
    area: PatchArea::InputBanner,
    id: "input-banner-text-color",
    label: "Set the input source banner text color",
    variants: &TEXT_COLOR_VARIANTS,
};

pub(super) const DIVIDER: PatchSpec = PatchSpec {
    area: PatchArea::InputBanner,
    id: "input-banner-divider",
    label: "Remove the input banner divider bar",
    variants: &DIVIDER_VARIANTS,
};

pub(super) const MENU_STYLE: PatchSpec = PatchSpec {
    area: PatchArea::InputBanner,
    id: "input-banner-menu-color",
    label: "Style the input banner menu icon and focus state",
    variants: &MENU_STYLE_VARIANTS,
};
