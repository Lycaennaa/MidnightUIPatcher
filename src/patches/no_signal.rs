use super::{
    FunctionStatementTarget, PatchArea, PatchOption, PatchRule, PatchSpec, PatchValue,
    PatchVariant, TARGETS, WebOsMajor,
};

const ANIMATION_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[10],
    PatchRule::Scalar {
        root_id: "animation",
        scope: None,
        property: "visible",
        expected: "element.isDynamic && (headerText.text || bodyText.text)",
        replacement: PatchValue::Literal(
            "currentKey !== \"noSignal\" && element.isDynamic && (headerText.text || bodyText.text)",
        ),
    },
)];

pub(super) const ANIMATION: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    id: "no-signal-animation",
    label: "Hide the No Signal GIF",
    variants: &ANIMATION_VARIANTS,
};

const MOVEMENT_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[10],
    PatchRule::Scalar {
        root_id: "element",
        scope: None,
        property: "isMoving",
        expected: "isDynamic && systemProperties.isAppActivated",
        replacement: PatchValue::Literal(
            "currentKey !== \"noSignal\" && isDynamic && systemProperties.isAppActivated",
        ),
    },
)];

pub(super) const MOVEMENT: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    id: "no-signal-movement",
    label: "Stop No Signal screen movement",
    variants: &MOVEMENT_VARIANTS,
};

const NO_SIGNAL_INSTRUCTION_STATEMENTS: [FunctionStatementTarget; 2] = [
    FunctionStatementTarget {
        function: "_setMessage",
        expected: "bodyText.text = Qt.binding(function() { return screensaverInterfaces.getDynamicSubMessage(currentKey) || \"\" });",
        replacement: "bodyText.text = Qt.binding(function() { return currentKey !== \"noSignal\" ? (screensaverInterfaces.getDynamicSubMessage(currentKey) || \"\") : \"\" });",
        expected_count: 1,
    },
    FunctionStatementTarget {
        function: "_setMessage",
        expected: "bodyText.text = Qt.binding(function() { return screensaverInterfaces.getStaticBodyMessage(currentKey) });",
        replacement: "bodyText.text = Qt.binding(function() { return currentKey !== \"noSignal\" ? screensaverInterfaces.getStaticBodyMessage(currentKey) : \"\" });",
        expected_count: 1,
    },
];

const INSTRUCTIONS_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[10],
    PatchRule::FunctionStatements {
        root_id: "screensaverRoot",
        targets: &NO_SIGNAL_INSTRUCTION_STATEMENTS,
    },
)];

pub(super) const INSTRUCTIONS: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    id: "no-signal-instructions",
    label: "Hide No Signal connection instructions",
    variants: &INSTRUCTIONS_VARIANTS,
};

const TEXT_COLOR_VARIANTS: [PatchVariant; 1] = [PatchVariant::new(
    WebOsMajor::WEBOS_24,
    TARGETS[11],
    PatchRule::Scalar {
        root_id: "root",
        scope: None,
        property: "color",
        expected: "\"#E6E6E6\"",
        replacement: PatchValue::Option(PatchOption::NoSignalTextColor),
    },
)];

pub(super) const TEXT_COLOR: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    id: "no-signal-text-color",
    label: "Set No Signal text color",
    variants: &TEXT_COLOR_VARIANTS,
};
