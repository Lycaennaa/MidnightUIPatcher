use super::*;
#[test]
fn input_source_banner_background_patch_clears_only_background_images() {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-banner-background")
        .unwrap();
    let input_banner = r#"Item {
        id: root
        Item {
            id: bannerBg
            Image { id: bgMiddle; source: root.visible ? styleSheet.banner.middleBg : "" }
            Image { id: bgRight; source: root.visible ? styleSheet.banner.rightBg : "" }
            Image { id: unrelated; source: "keep.png" }
        }
    }"#;
    let simple_banner = r#"Item {
        id: root
        Item {
            id: zappingBg
            Image { id: bgMiddle; source: root.visible ? (hasFocus? styleSheet.banner.middleFocusedBg : styleSheet.banner.middleBg) : "" }
            Image { id: bgRight; source: root.visible ? (hasFocus? styleSheet.banner.rightFocusedBg : styleSheet.banner.rightBg) : "" }
            Image { id: unrelated; source: "keep.png" }
        }
    }"#;
    let docs = BTreeMap::from([
        (
            TARGETS[8].to_owned(),
            QmlDoc::parse(input_banner, TARGETS[8]).unwrap(),
        ),
        (
            TARGETS[9].to_owned(),
            QmlDoc::parse(simple_banner, TARGETS[9]).unwrap(),
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
    let input_output = apply_edits(input_banner, edits.remove(TARGETS[8]).unwrap());
    let simple_output = apply_edits(simple_banner, edits.remove(TARGETS[9]).unwrap());

    assert_eq!(input_output.matches("source: \"\"").count(), 2);
    assert_eq!(simple_output.matches("source: \"\"").count(), 2);
    assert!(input_output.contains("id: unrelated; source: \"keep.png\""));
    assert!(simple_output.contains("id: unrelated; source: \"keep.png\""));
    assert_eq!(
        patch.affected_targets().collect::<Vec<_>>(),
        vec![TARGETS[8], TARGETS[9]]
    );

    let missing_background = simple_banner.replace("id: bgRight", "id: renamedBackground");
    let bad_docs = BTreeMap::from([
        (
            TARGETS[8].to_owned(),
            QmlDoc::parse(input_banner, TARGETS[8]).unwrap(),
        ),
        (
            TARGETS[9].to_owned(),
            QmlDoc::parse(&missing_background, TARGETS[9]).unwrap(),
        ),
    ]);
    assert!(matches!(
        collect_patch_edits(
            &bad_docs,
            std::slice::from_ref(patch),
            &selected,
            &PatchOptions::default(),
        ),
        Err(PatchError::Unsupported { .. })
    ));
}

#[test]
fn input_source_banner_text_color_updates_both_source_labels() {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-banner-text-color")
        .unwrap();
    let simple_banner = r##"Item {
        id: root
        Text {
            id: inputNameItem
            color: needFocusHandle ? styleSheet.banner.inputFocusedColor : styleSheet.banner.inputColor
        }
        ScrollText {
            id: inputLabelItem
            color: needFocusHandle ? styleSheet.banner.inputFocusedColor : styleSheet.banner.inputColor
        }
        Text { id: unrelated; color: "#E6E6E6" }
    }"##;
    let docs = BTreeMap::from([(
        TARGETS[9].to_owned(),
        QmlDoc::parse(simple_banner, TARGETS[9]).unwrap(),
    )]);
    let selected = BTreeSet::from([patch.id.to_owned()]);
    let apply_color = |options: &PatchOptions| {
        let mut edits =
            collect_patch_edits(&docs, std::slice::from_ref(patch), &selected, options).unwrap();
        apply_edits(simple_banner, edits.remove(TARGETS[9]).unwrap())
    };

    let default_output = apply_color(&PatchOptions::default());
    assert_eq!(default_output.matches("color: \"#FF0000\"").count(), 2);
    assert!(default_output.contains("id: unrelated; color: \"#E6E6E6\""));

    let mut options = PatchOptions::default();
    options
        .set_option("input-banner-text-color", "#12ABEF")
        .unwrap();
    let custom_output = apply_color(&options);
    assert_eq!(custom_output.matches("color: \"#12ABEF\"").count(), 2);
    assert!(custom_output.contains("id: unrelated; color: \"#E6E6E6\""));
}

#[test]
fn input_banner_menu_style_keeps_a_focus_outline_with_transparent_fill() {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-banner-menu-color")
        .unwrap();
    let source = r#"import QtQuick 2.12
import "../BannerItems"
Item {
    OptionButton {
        id: option
        baInterface: interfaces[C.NAME_INBANNER]
    }
    OptionButton {
        id: unrelatedOption
        baInterface: interfaces[C.NAME_INBANNER]
    }
}"#;
    let apply = |source: &str, options: &PatchOptions| -> Result<String, PatchError> {
        let docs = BTreeMap::from([(
            TARGETS[8].to_owned(),
            QmlDoc::parse(source, TARGETS[8]).unwrap(),
        )]);
        let selected = BTreeSet::from([patch.id.to_owned()]);
        let mut edits =
            collect_patch_edits(&docs, std::slice::from_ref(patch), &selected, options)?;
        Ok(apply_edits(source, edits.remove(TARGETS[8]).unwrap()))
    };

    let default_output = apply(source, &PatchOptions::default()).unwrap();
    assert_eq!(
        default_output
            .matches("import QtGraphicalEffects 1.12")
            .count(),
        1
    );
    assert!(default_output.contains("bgFocusedColor: \"#00000000\""));
    assert_eq!(default_output.matches("ColorOverlay {").count(), 1);
    assert!(default_output.contains("source: option.img"));
    assert!(default_output.contains("color: \"#FF0000\""));
    assert!(default_output.contains("border.color: \"#E6E6E6\""));
    assert!(default_output.contains("border.width: 3"));
    assert!(default_output.contains("visible: option.hasFocus"));
    assert!(!default_output.contains("radius:"));

    let mut options = PatchOptions::default();
    options
        .set_option("input-banner-menu-color", "#12ABEF")
        .unwrap();
    options
        .set_option("input-banner-menu-focus-fill-color", "#80112233")
        .unwrap();
    options
        .set_option("input-banner-menu-focus-border-color", "#00AACC")
        .unwrap();
    options
        .set_option("input-banner-menu-focus-border-width", "2.5")
        .unwrap();
    let custom_output = apply(source, &options).unwrap();
    assert!(custom_output.contains("bgFocusedColor: \"#80112233\""));
    assert!(custom_output.contains("color: \"#12ABEF\""));
    assert!(custom_output.contains("border.color: \"#00AACC\""));
    assert!(custom_output.contains("border.width: 2.5"));
    assert!(custom_output.contains("id: unrelatedOption"));
    assert_eq!(custom_output.matches("ColorOverlay {").count(), 1);

    let already_imported = source.replace(
        "import QtQuick 2.12\n",
        "import QtQuick 2.12\nimport QtGraphicalEffects 1.12\n",
    );
    let output = apply(&already_imported, &PatchOptions::default()).unwrap();
    assert_eq!(output.matches("import QtGraphicalEffects 1.12").count(), 1);
    let aliased_import = source.replace(
        "import QtQuick 2.12\n",
        "import QtQuick 2.12\nimport QtGraphicalEffects 1.12 as Effects\n",
    );
    assert!(matches!(
        apply(&aliased_import, &PatchOptions::default()),
        Err(PatchError::Unsupported { detail, .. }) if detail.contains("aliased import")
    ));
}

#[test]
fn input_banner_divider_patch_hides_only_the_menu_separator() {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == "input-banner-divider")
        .unwrap();
    let source = r#"Row {
    id: bannerRow
    Rectangle {
        width: styleSheet.banner.optionBarWidth
        visible: interfaces[C.NAME_INBANNER] && interfaces[C.NAME_INBANNER].isOkBanner
    }
    Rectangle {
        width: 3
        visible: true
    }
}"#;
    let docs = BTreeMap::from([(
        TARGETS[8].to_owned(),
        QmlDoc::parse(source, TARGETS[8]).unwrap(),
    )]);
    let selected = BTreeSet::from([patch.id.to_owned()]);
    let mut edits = collect_patch_edits(
        &docs,
        std::slice::from_ref(patch),
        &selected,
        &PatchOptions::default(),
    )
    .unwrap();
    let output = apply_edits(source, edits.remove(TARGETS[8]).unwrap());

    assert!(output.contains("width: styleSheet.banner.optionBarWidth\n        visible: false"));
    assert!(output.contains("width: 3\n        visible: true"));
}
