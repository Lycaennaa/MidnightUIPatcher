use super::{
    FunctionStatementTarget, PatchArea, PatchOption, PatchRule, PatchSite, PatchSpec, PatchValue,
    TARGETS,
};

const NO_SIGNAL_ANIMATION_SITES: [PatchSite; 2] = [
    PatchSite {
        target: TARGETS[10],
        rule: PatchRule::Scalar {
            root_id: "element",
            scope: None,
            property: "isMoving",
            expected: "isDynamic && systemProperties.isAppActivated",
            replacement: PatchValue::Literal(
                "currentKey !== \"noSignal\" && isDynamic && systemProperties.isAppActivated",
            ),
        },
    },
    PatchSite {
        target: TARGETS[10],
        rule: PatchRule::Scalar {
            root_id: "animation",
            scope: None,
            property: "visible",
            expected: "element.isDynamic && (headerText.text || bodyText.text)",
            replacement: PatchValue::Literal(
                "currentKey !== \"noSignal\" && element.isDynamic && (headerText.text || bodyText.text)",
            ),
        },
    },
];

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

pub(super) const ANIMATION: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    rule: PatchRule::MultiSite {
        sites: &NO_SIGNAL_ANIMATION_SITES,
    },
    id: "no-signal-animation",
    label: "Stop No Signal screen movement and animation",
    target: TARGETS[10],
};

pub(super) const INSTRUCTIONS: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    rule: PatchRule::FunctionStatements {
        root_id: "screensaverRoot",
        targets: &NO_SIGNAL_INSTRUCTION_STATEMENTS,
    },
    id: "no-signal-instructions",
    label: "Hide No Signal connection instructions",
    target: TARGETS[10],
};

pub(super) const TEXT_COLOR: PatchSpec = PatchSpec {
    area: PatchArea::NoSignal,
    rule: PatchRule::Scalar {
        root_id: "root",
        scope: None,
        property: "color",
        expected: "\"#E6E6E6\"",
        replacement: PatchValue::Option(PatchOption::NoSignalTextColor),
    },
    id: "no-signal-text-color",
    label: "Set No Signal text color",
    target: TARGETS[11],
};
