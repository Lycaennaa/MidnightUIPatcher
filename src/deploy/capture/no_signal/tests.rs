use super::*;

fn input_status_for_tests(
    active: Option<HdmiInput>,
    states: [(bool, bool); HDMI_INPUTS.len()],
) -> InputStatus {
    input_status_for_device_order(active, states, HDMI_INPUTS)
}

fn input_status_for_device_order(
    active: Option<HdmiInput>,
    states: [(bool, bool); HDMI_INPUTS.len()],
    device_order: [HdmiInput; HDMI_INPUTS.len()],
) -> InputStatus {
    let mut properties = serde_json::Map::new();
    for (device_index, input) in device_order.into_iter().enumerate() {
        let (plugged, signal) = states[input.index()];
        properties.insert(
            format!("devices[{device_index}].appId"),
            serde_json::Value::String(input.app_id().to_owned()),
        );
        properties.insert(
            format!("devices[{device_index}].activate"),
            serde_json::Value::Bool(active == Some(input)),
        );
        properties.insert(
            format!("devices[{device_index}].hdmiPlugIn"),
            serde_json::Value::Bool(plugged),
        );
        properties.insert(
            format!("devices[{device_index}].hdmiSignalExist"),
            serde_json::Value::Bool(signal),
        );
    }
    parse_input_status(&serde_json::to_vec(&serde_json::Value::Object(properties)).unwrap())
        .unwrap()
}

#[test]
fn no_signal_capture_selects_first_disconnected_input_and_preserves_active_source() {
    let hdmi2_active = input_status_for_tests(
        Some(HdmiInput::Hdmi2),
        [(true, false), (true, true), (false, false), (false, true)],
    );
    assert_eq!(
        choose_no_signal_capture(&hdmi2_active).unwrap(),
        (HdmiInput::Hdmi3, HdmiInput::Hdmi2)
    );

    let hdmi1_active = input_status_for_tests(
        Some(HdmiInput::Hdmi1),
        [(true, true), (true, false), (true, true), (false, false)],
    );
    assert_eq!(
        choose_no_signal_capture(&hdmi1_active).unwrap(),
        (HdmiInput::Hdmi4, HdmiInput::Hdmi1)
    );
}
#[test]
fn no_signal_capture_maps_hdmi_identifiers_independent_of_device_order() {
    let status = input_status_for_device_order(
        Some(HdmiInput::Hdmi2),
        [(true, false), (true, true), (false, false), (false, true)],
        [
            HdmiInput::Hdmi4,
            HdmiInput::Hdmi2,
            HdmiInput::Hdmi1,
            HdmiInput::Hdmi3,
        ],
    );
    assert_eq!(
        choose_no_signal_capture(&status).unwrap(),
        (HdmiInput::Hdmi3, HdmiInput::Hdmi2)
    );
    assert_eq!(status.plugged, [true, true, false, false]);
    assert_eq!(status.signal, [false, true, false, true]);
}

#[test]
fn no_signal_capture_refuses_when_no_inactive_input_is_disconnected_or_source_cannot_be_restored() {
    let connected = input_status_for_tests(
        Some(HdmiInput::Hdmi2),
        [(true, false), (true, true), (true, true), (true, false)],
    );
    let connected_error = choose_no_signal_capture(&connected)
        .unwrap_err()
        .to_string();
    assert!(connected_error.contains("no inactive HDMI input"));

    let signal_without_cable = input_status_for_tests(
        Some(HdmiInput::Hdmi2),
        [(false, true), (true, true), (true, false), (false, true)],
    );
    let signal_error = choose_no_signal_capture(&signal_without_cable)
        .unwrap_err()
        .to_string();
    assert!(signal_error.contains("no inactive HDMI input"));

    let no_active_input = input_status_for_tests(None, [(false, false); HDMI_INPUTS.len()]);
    let restore_error = choose_no_signal_capture(&no_active_input)
        .unwrap_err()
        .to_string();
    assert!(restore_error.contains("cannot be restored"));
}

#[test]
fn no_signal_capture_requires_owner_approval_before_validating_connection_settings() {
    let error = capture_no_signal(&UserConfig::default(), false).unwrap_err();
    assert!(matches!(error, DeployError::NoSignalCaptureNotConfirmed));
}

#[test]
fn no_signal_capture_verifies_the_original_input_after_success() {
    let capture_path = Path::new("capture.png");
    let restored = input_status_for_tests(
        Some(HdmiInput::Hdmi2),
        [(false, false), (true, true), (false, false), (false, false)],
    );
    assert!(verify_restored_input(&restored, HdmiInput::Hdmi2, capture_path).is_ok());

    let wrong_input = input_status_for_tests(
        Some(HdmiInput::Hdmi1),
        [(false, false), (true, true), (false, false), (false, false)],
    );
    let error = verify_restored_input(&wrong_input, HdmiInput::Hdmi2, capture_path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("active input is hdmi1 instead of restored hdmi2"));
}
