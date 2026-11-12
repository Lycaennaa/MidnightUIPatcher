use super::*;
use crate::patches::{
    ChildObjectScope, FunctionStatementTarget, PATCH_OPTION_SPECS, PatchOption, PatchValue,
    ScalarTarget,
};
#[test]
fn scalar_rule_targets_unique_child_object_by_property() {
    const RULE: PatchRule = PatchRule::Scalar {
        root_id: "fixtureRoot",
        scope: Some(ChildObjectScope {
            class: "Rectangle",
            property: "visible",
            expected: "isBackdrop",
        }),
        property: "color",
        expected: "\"#112233\"",
        replacement: PatchValue::Literal("\"#000000\""),
    };
    let input = r##"Item {
            id: fixtureRoot
            Rectangle { visible: isBackdrop; color: "#112233" }
            Rectangle { visible: false; color: "#112233" }
        }"##;

    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let edit = locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default())
        .unwrap()
        .remove(0);
    let mut output = input.to_owned();
    output.replace_range(edit.start..edit.end, &edit.replacement);
    assert!(output.contains("visible: isBackdrop; color: \"#000000\""));
    assert!(output.contains("visible: false; color: \"#112233\""));

    for ambiguous in [
        input.replace("visible: false", "visible: isBackdrop"),
        input.replace("visible: isBackdrop", "visible: isDialog"),
    ] {
        let doc = QmlDoc::parse(&ambiguous, "fixture.qml").unwrap();
        assert!(matches!(
            locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
            Err(PatchError::Unsupported { .. })
        ));
    }
}

#[test]
fn dynamic_color_option_becomes_a_quoted_qml_literal() {
    const RULE: PatchRule = PatchRule::Scalar {
        root_id: "fixtureRoot",
        scope: None,
        property: "color",
        expected: "\"#112233\"",
        replacement: PatchValue::Option(PatchOption::InputTileTextColor),
    };
    let input = r##"Item { id: fixtureRoot; color: "#112233" }"##;
    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let mut options = PatchOptions::default();
    options
        .set_option("input-tile-text-color", "#8012abEF")
        .unwrap();

    let edit = locate_patch(&doc, RULE, "fixture.qml", &options)
        .unwrap()
        .remove(0);
    let mut output = input.to_owned();
    output.replace_range(edit.start..edit.end, &edit.replacement);
    assert_eq!(output, r##"Item { id: fixtureRoot; color: "#8012ABEF" }"##);
    assert!(matches!(
        options.set_option("input-tile-text-color", "\"; invalid"),
        Err(crate::patches::PatchOptionError::InvalidColor {
            name: "input-tile-text-color"
        })
    ));
}

#[test]
fn dynamic_opacity_and_selected_border_options_reach_qml_safely() {
    const TARGETS: [ScalarTarget; 3] = [
        ScalarTarget {
            scope: None,
            property: "opacity",
            expected: "active ? 1 : 0.5",
            replacement: PatchValue::Option(PatchOption::InputTileIconOpacity),
        },
        ScalarTarget {
            scope: None,
            property: "border.color",
            expected: "active ? \"#112233\" : \"#445566\"",
            replacement: PatchValue::Option(PatchOption::InputTileBorderColor),
        },
        ScalarTarget {
            scope: None,
            property: "border.width",
            expected: "active ? scale(1) : scale(0.5)",
            replacement: PatchValue::Option(PatchOption::InputTileBorderWidth),
        },
    ];
    const RULE: PatchRule = PatchRule::ScalarGroup {
        root_id: "fixtureRoot",
        targets: &TARGETS,
    };
    let input = r##"Rectangle { id: fixtureRoot; opacity: active ? 1 : 0.5; border.color: active ? "#112233" : "#445566"; border.width: active ? scale(1) : scale(0.5) }"##;
    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let mut options = PatchOptions::default();
    options
        .set_option("input-tile-icon-opacity", "0.75")
        .unwrap();
    options
        .set_option("input-tile-border-color", "#80a1b2c3")
        .unwrap();
    options
        .set_option("input-tile-border-width", "2.5")
        .unwrap();

    let mut edits = locate_patch(&doc, RULE, "fixture.qml", &options).unwrap();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut output = input.to_owned();
    for edit in edits {
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }
    assert_eq!(
        output,
        r##"Rectangle { id: fixtureRoot; opacity: 0.75; border.color: entry.currentInput ? "#80A1B2C3" : "transparent"; border.width: entry.currentInput ? styler.getRelativeValue(2.5) : 0 }"##
    );
}

#[test]
fn appearance_numeric_options_reject_values_outside_their_ranges() {
    let mut options = PatchOptions::default();
    options.set_option("main-panel-opacity", "0").unwrap();
    options.set_option("input-tile-icon-opacity", "1").unwrap();
    options
        .set_option("input-tile-border-width", "100")
        .unwrap();
    options.validate().unwrap();

    for (id, value, name) in [
        ("main-panel-opacity", "1.01", "main-panel-opacity"),
        ("input-tile-icon-opacity", "NaN", "input-tile-icon-opacity"),
        ("input-tile-border-width", "-0.1", "input-tile-border-width"),
        (
            "input-tile-border-width",
            "100.1",
            "input-tile-border-width",
        ),
    ] {
        assert!(matches!(
            options.set_option(id, value),
            Err(crate::patches::PatchOptionError::InvalidNumber {
                name: actual,
                ..
            }) if actual == name
        ));
    }
}

#[test]
fn option_specs_are_authoritative_for_defaults_and_value_access() {
    let mut options = PatchOptions::default();
    for spec in &PATCH_OPTION_SPECS {
        assert_eq!(
            options.value(spec.id).as_deref(),
            Some(spec.default),
            "default for `{}` must come from its option specification",
            spec.id
        );
        options.set_option(spec.id, spec.default).unwrap();
    }
    options.validate().unwrap();
}

#[test]
fn scalar_group_edits_each_uniquely_scoped_child() {
    const TARGETS: [ScalarTarget; 2] = [
        ScalarTarget {
            scope: Some(ChildObjectScope {
                class: "Image",
                property: "id",
                expected: "first",
            }),
            property: "opacity",
            expected: "1",
            replacement: PatchValue::Literal("0"),
        },
        ScalarTarget {
            scope: Some(ChildObjectScope {
                class: "Image",
                property: "id",
                expected: "second",
            }),
            property: "opacity",
            expected: "1",
            replacement: PatchValue::Literal("0"),
        },
    ];
    const RULE: PatchRule = PatchRule::ScalarGroup {
        root_id: "fixtureRoot",
        targets: &TARGETS,
    };
    let input = r#"Item {
            id: fixtureRoot
            Image { id: first; opacity: 1 }
            Image { id: second; opacity: 1 }
            Image { id: untouched; opacity: 1 }
        }"#;

    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let mut edits = locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()).unwrap();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut output = input.to_owned();
    for edit in edits {
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }
    assert!(output.contains("id: first; opacity: 0"));
    assert!(output.contains("id: second; opacity: 0"));
    assert!(output.contains("id: untouched; opacity: 1"));

    let ambiguous = input.replace("id: second", "id: first");
    let doc = QmlDoc::parse(&ambiguous, "fixture.qml").unwrap();
    assert!(matches!(
        locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
        Err(PatchError::Unsupported { .. })
    ));
}

#[test]
fn scalar_group_edits_a_qualified_root_property() {
    const TARGETS: [ScalarTarget; 1] = [ScalarTarget {
        scope: None,
        property: "border.width",
        expected: "entry.activate ? styler.getRelativeValue(1) : styler.getRelativeValue(0.5)",
        replacement: PatchValue::Literal("entry.currentInput ? styler.getRelativeValue(1) : 0"),
    }];
    const RULE: PatchRule = PatchRule::ScalarGroup {
        root_id: "fixtureRoot",
        targets: &TARGETS,
    };
    let input = r#"Rectangle {
            id: fixtureRoot
            border.width: entry.activate ? styler.getRelativeValue(1) : styler.getRelativeValue(0.5)
            Rectangle { border.width: 2 }
        }"#;

    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let edit = locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default())
        .unwrap()
        .remove(0);
    let mut output = input.to_owned();
    output.replace_range(edit.start..edit.end, &edit.replacement);
    assert!(output.contains("border.width: entry.currentInput ? styler.getRelativeValue(1) : 0"));
    assert!(output.contains("Rectangle { border.width: 2 }"));

    let ambiguous = input.replace(
        "            Rectangle { border.width: 2 }",
        "            border.width: 0\n            border.width: 1",
    );
    let doc = QmlDoc::parse(&ambiguous, "fixture.qml").unwrap();
    assert!(matches!(
        locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
        Err(PatchError::Unsupported { .. })
    ));
}

#[test]
fn if_condition_rule_is_scoped_and_requires_exact_count() {
    const RULE: PatchRule = PatchRule::IfCondition {
        root_id: "fixtureRoot",
        function: "getInputData",
        expected: "connected && !dongleConnected",
        replacement: "connected && active && !dongleConnected",
        expected_count: 1,
    };
    let input = r#"Item {
            id: fixtureRoot
            function getInputData() {
                if (connected && !dongleConnected) {
                    root.addInput();
                }
            }
            function unrelated() {
                if (connected && !dongleConnected) {
                    root.keep();
                }
            }
        }"#;

    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let edit = locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default())
        .unwrap()
        .remove(0);
    let mut output = input.to_owned();
    output.replace_range(edit.start..edit.end, &edit.replacement);
    assert!(output.contains("if (connected && active && !dongleConnected)"));
    assert_eq!(
        output.matches("if (connected && !dongleConnected)").count(),
        1
    );

    let duplicate = input.replace(
        "root.addInput();",
        "root.addInput();\n                if (connected && !dongleConnected) { root.keep(); }",
    );
    let doc = QmlDoc::parse(&duplicate, "fixture.qml").unwrap();
    assert!(matches!(
        locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
        Err(PatchError::Unsupported { .. })
    ));
}
#[test]
fn home_hub_patch_reindexes_input_updates_and_guards_empty_view() {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-home-hub")
        .unwrap();
    let app_interfaces = r#"Item {
        id: root
        function createInputPickerModel(contents) {
            createTitle();
        }
        function updateInputPickerModel(contents) {
            var startIdx = 1;
        }
    }"#;
    let main_view = r#"FocusScope {
        id: root
        Connections {
            target: interfaces.application
            onShowView: {
                if (systemProperties.audioGuidanceOn) {
                    var readText = interfaces.application.inputPickerModel.get(mainRibbon.currentIndex).entry;
                }
                uiController.goToState("opened");
            }
        }
    }"#;
    let docs = BTreeMap::from([
        (
            TARGETS[6].to_owned(),
            QmlDoc::parse(app_interfaces, TARGETS[6]).unwrap(),
        ),
        (
            TARGETS[5].to_owned(),
            QmlDoc::parse(main_view, TARGETS[5]).unwrap(),
        ),
    ]);
    let selected = BTreeSet::from([patch.id.to_owned()]);
    let mut edits = collect_patch_edits(
        &docs,
        std::slice::from_ref(patch),
        &selected,
        &PatchOptions::default(),
    )
    .unwrap();
    let mut app_edits = edits.remove(TARGETS[6]).unwrap();
    app_edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut app_output = app_interfaces.to_owned();
    for edit in app_edits {
        app_output.replace_range(edit.start..edit.end, &edit.replacement);
    }
    let mut view_edits = edits.remove(TARGETS[5]).unwrap();
    view_edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut view_output = main_view.to_owned();
    for edit in view_edits {
        view_output.replace_range(edit.start..edit.end, &edit.replacement);
    }

    assert!(!app_output.contains("createTitle();"));
    assert!(app_output.contains("var startIdx = 0;"));
    assert!(view_output.contains(
        "if (systemProperties.audioGuidanceOn && interfaces.application.inputPickerModel.count > 0)"
    ));
    assert!(view_output.contains("uiController.goToState(\"opened\");"));
}

#[test]
fn function_statements_are_scoped_and_require_exact_count() {
    const TARGETS: [FunctionStatementTarget; 1] = [FunctionStatementTarget {
        function: "build",
        expected: "fixtureRoot.removeThis();",
        replacement: "",
        expected_count: 2,
    }];
    const RULE: PatchRule = PatchRule::FunctionStatements {
        root_id: "fixtureRoot",
        targets: &TARGETS,
    };
    let input = r#"Item {
        id: fixtureRoot
        function build() {
            fixtureRoot.removeThis();
            fixtureRoot.removeThis();
            fixtureRoot.keep();
        }
        function other() {
            fixtureRoot.removeThis();
        }
    }"#;
    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let mut edits = locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()).unwrap();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    let mut output = input.to_owned();
    for edit in edits {
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }
    let build = output.split("function other()").next().unwrap();
    assert!(!build.contains("fixtureRoot.removeThis();"));
    assert!(build.contains("fixtureRoot.keep();"));
    assert!(
        output
            .split("function other()")
            .nth(1)
            .unwrap()
            .contains("fixtureRoot.removeThis();")
    );

    for unexpected_count in [
        input.replacen("fixtureRoot.removeThis();", "", 1),
        input.replace("fixtureRoot.keep();", "fixtureRoot.removeThis();"),
    ] {
        let doc = QmlDoc::parse(&unexpected_count, "fixture.qml").unwrap();
        assert!(matches!(
            locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
            Err(PatchError::Unsupported { .. })
        ));
    }
}

#[test]
fn function_statement_rule_rejects_calls_inside_expressions() {
    const TARGETS: [FunctionStatementTarget; 1] = [FunctionStatementTarget {
        function: "build",
        expected: "fixtureRoot.removeThis();",
        replacement: "",
        expected_count: 1,
    }];
    const RULE: PatchRule = PatchRule::FunctionStatements {
        root_id: "fixtureRoot",
        targets: &TARGETS,
    };
    let input = r#"Item {
        id: fixtureRoot
        function build() {
            var title = fixtureRoot.removeThis();
        }
    }"#;

    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    assert!(matches!(
        locate_patch(&doc, RULE, "fixture.qml", &PatchOptions::default()),
        Err(PatchError::Unsupported { .. })
    ));
}

#[test]
fn unselected_patch_sites_are_still_validated() {
    let input = r##"Item { id: fixtureRoot; color: "#112233" }"##;
    let doc = QmlDoc::parse(input, "fixture.qml").unwrap();
    let docs = BTreeMap::from([("fixture.qml".to_owned(), doc)]);
    let patches = [
        PatchSpec {
            rule: PatchRule::Scalar {
                root_id: "fixtureRoot",
                scope: None,
                property: "color",
                expected: "\"#112233\"",
                replacement: PatchValue::Literal("\"#000000\""),
            },
            id: "selected",
            label: "selected patch",
            target: "fixture.qml",
        },
        PatchSpec {
            rule: PatchRule::Scalar {
                root_id: "fixtureRoot",
                scope: None,
                property: "color",
                expected: "\"#445566\"",
                replacement: PatchValue::Literal("\"#000000\""),
            },
            id: "unselected",
            label: "unselected patch",
            target: "fixture.qml",
        },
    ];
    let selected = BTreeSet::from(["selected".to_owned()]);

    let error = collect_patch_edits(&docs, &patches, &selected, &PatchOptions::default())
        .err()
        .unwrap();
    assert_eq!(
        error,
        PatchError::Unsupported {
            file: "fixture.qml".to_owned(),
            detail: "unexpected value for color".to_owned(),
        }
    );
}
