use super::*;

fn apply_patch(
    patch_id: &str,
    target: &str,
    source: &str,
    options: &PatchOptions,
) -> Result<String, PatchError> {
    let patch = PATCHES
        .iter()
        .find(|patch| patch.id == patch_id)
        .expect("patch id must exist");
    let docs = BTreeMap::from([(target.to_owned(), QmlDoc::parse(source, target)?)]);
    let selected = BTreeSet::from([patch_id.to_owned()]);
    let mut edits = collect_patch_edits(&docs, std::slice::from_ref(patch), &selected, options)?;
    Ok(apply_edits(source, edits.remove(target).unwrap()))
}

#[test]
fn no_signal_animation_patch_guards_motion_and_indicator_only_for_no_signal() {
    let source = r#"Item {
        id: screensaverRoot
        RandomMoveItem {
            id: element
            isMoving: isDynamic && systemProperties.isAppActivated
            isRepeatedMoving: !isPhotoRunning
        }
        AnimatedImage {
            id: animation
            visible: element.isDynamic && (headerText.text || bodyText.text)
        }
        Text { id: unrelated; visible: true }
    }"#;

    let output = apply_patch(
        "no-signal-animation",
        TARGETS[10],
        source,
        &PatchOptions::default(),
    )
    .unwrap();

    assert!(output.contains(
        "isMoving: currentKey !== \"noSignal\" && isDynamic && systemProperties.isAppActivated"
    ));
    assert!(output.contains(
        "visible: currentKey !== \"noSignal\" && element.isDynamic && (headerText.text || bodyText.text)"
    ));
    assert!(output.contains("isRepeatedMoving: !isPhotoRunning"));
    assert!(output.contains("id: unrelated; visible: true"));
}

#[test]
fn no_signal_instructions_patch_clears_both_message_modes_only_for_no_signal() {
    let source = r#"Item {
        id: screensaverRoot
        function _setMessage() {
            if (currentViewType === ViewModel.viewType.withImage) {
                headerText.text = Qt.binding(function() { return screensaverInterfaces.getDynamicMessage(currentKey) });
                bodyText.text = Qt.binding(function() { return screensaverInterfaces.getDynamicSubMessage(currentKey) || "" });
            } else if (currentViewType === ViewModel.viewType.textOnly) {
                headerText.text = Qt.binding(function() { return screensaverInterfaces.getStaticHeaderMessage(currentKey) });
                bodyText.text = Qt.binding(function() { return screensaverInterfaces.getStaticBodyMessage(currentKey) });
            }
        }
    }"#;

    let output = apply_patch(
        "no-signal-instructions",
        TARGETS[10],
        source,
        &PatchOptions::default(),
    )
    .unwrap();

    assert!(output.contains(
        "bodyText.text = Qt.binding(function() { return currentKey !== \"noSignal\" ? (screensaverInterfaces.getDynamicSubMessage(currentKey) || \"\") : \"\" });"
    ));
    assert!(output.contains(
        "bodyText.text = Qt.binding(function() { return currentKey !== \"noSignal\" ? screensaverInterfaces.getStaticBodyMessage(currentKey) : \"\" });"
    ));
    assert!(output.contains("getDynamicMessage(currentKey)"));
    assert!(output.contains("getStaticHeaderMessage(currentKey)"));
}

#[test]
fn no_signal_text_color_patch_uses_validated_color_and_preserves_other_text() {
    let source = r##"Text {
        id: root
        color: "#E6E6E6"
        text: ""
    }
    Text { id: unrelated; color: "#E6E6E6"; text: "Other" }"##;

    let default_output = apply_patch(
        "no-signal-text-color",
        TARGETS[11],
        source,
        &PatchOptions::default(),
    )
    .unwrap();
    assert!(default_output.contains(
        "color: (text === stringSheet.screensaver.strings.noSignal.full || text === stringSheet.screensaver.strings.noSignal.header) ? \"#FF0000\" : \"#E6E6E6\""
    ));
    assert!(default_output.contains("id: unrelated; color: \"#E6E6E6\"; text: \"Other\""));

    let mut options = PatchOptions::default();
    options
        .set_option("no-signal-text-color", "#12ABEF")
        .unwrap();
    let custom_output = apply_patch("no-signal-text-color", TARGETS[11], source, &options).unwrap();
    assert!(custom_output.contains(
        "color: (text === stringSheet.screensaver.strings.noSignal.full || text === stringSheet.screensaver.strings.noSignal.header) ? \"#12ABEF\" : \"#E6E6E6\""
    ));
    assert!(custom_output.contains("id: unrelated; color: \"#E6E6E6\"; text: \"Other\""));
}
