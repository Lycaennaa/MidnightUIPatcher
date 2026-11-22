use std::path::{Path, PathBuf};

use crate::config::{self, UserConfig};
use crate::patches::PatchArea;

use super::{DeployError, MAX_CAPTURE_BYTES, capture_timestamp, ssh_input, write_capture_output};

const INPUT_STATUS_REMOTE_COMMAND: &str = r#"script -q -c 'luna-send -w 5000 -n 1 -f -q "devices[0].appId" -q "devices[0].activate" -q "devices[0].hdmiPlugIn" -q "devices[0].hdmiSignalExist" -q "devices[1].appId" -q "devices[1].activate" -q "devices[1].hdmiPlugIn" -q "devices[1].hdmiSignalExist" -q "devices[2].appId" -q "devices[2].activate" -q "devices[2].hdmiPlugIn" -q "devices[2].hdmiSignalExist" -q "devices[3].appId" -q "devices[3].activate" -q "devices[3].hdmiPlugIn" -q "devices[3].hdmiSignalExist" "luna://com.webos.service.eim/getAllInputStatus" "{}"' /dev/null"#;
pub(super) const NO_SIGNAL_CAPTURE_SCRIPT: &str = concat!(
    include_str!("no-signal-capture.sh"),
    include_str!("halgal-checkerboard-capture.sh"),
    "HALGAL_CAPTURE_SCRIPT\n",
);

const HDMI_INPUTS: [HdmiInput; 4] = [
    HdmiInput::Hdmi1,
    HdmiInput::Hdmi2,
    HdmiInput::Hdmi3,
    HdmiInput::Hdmi4,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdmiInput {
    Hdmi1,
    Hdmi2,
    Hdmi3,
    Hdmi4,
}

impl HdmiInput {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hdmi1 => "hdmi1",
            Self::Hdmi2 => "hdmi2",
            Self::Hdmi3 => "hdmi3",
            Self::Hdmi4 => "hdmi4",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Hdmi1 => 0,
            Self::Hdmi2 => 1,
            Self::Hdmi3 => 2,
            Self::Hdmi4 => 3,
        }
    }

    fn app_id(self) -> &'static str {
        match self {
            Self::Hdmi1 => "com.webos.app.hdmi1",
            Self::Hdmi2 => "com.webos.app.hdmi2",
            Self::Hdmi3 => "com.webos.app.hdmi3",
            Self::Hdmi4 => "com.webos.app.hdmi4",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InputStatus {
    active: Option<HdmiInput>,
    plugged: [bool; HDMI_INPUTS.len()],
    signal: [bool; HDMI_INPUTS.len()],
}

impl InputStatus {
    fn is_disconnected(&self, input: HdmiInput) -> bool {
        let index = input.index();
        !self.plugged[index] && !self.signal[index]
    }
}

pub fn capture_no_signal(
    config: &UserConfig,
    confirm_owner_approval: bool,
) -> Result<(PathBuf, HdmiInput), DeployError> {
    if !confirm_owner_approval {
        return Err(DeployError::NoSignalCaptureNotConfirmed);
    }
    config::validate(config)?;
    let initial_status = read_input_status(config)?;
    let (input, restore_input) = choose_no_signal_capture(&initial_status)?;
    let timestamp = capture_timestamp()?;
    let remote_command = format!(
        "sh -s -- {} {} {MAX_CAPTURE_BYTES}",
        input.as_str(),
        restore_input.as_str()
    );
    let output = ssh_input(
        config,
        &remote_command,
        NO_SIGNAL_CAPTURE_SCRIPT.as_bytes(),
        "No Signal input switch and capture",
    )?;
    let path = write_capture_output(output, &timestamp, PatchArea::NoSignal)?;
    let restored_status = match read_input_status(config) {
        Ok(status) => status,
        Err(error) => {
            return Err(DeployError::InputSwitch(format!(
                "No Signal capture was saved to {}, but the restored input could not be verified: {error}",
                path.display()
            )));
        }
    };
    verify_restored_input(&restored_status, restore_input, &path)?;
    Ok((path, input))
}

fn choose_no_signal_capture(status: &InputStatus) -> Result<(HdmiInput, HdmiInput), DeployError> {
    let restore_input = status.active.ok_or_else(|| {
        DeployError::InputSwitch(
            "the current source is not HDMI 1–4; refusing a switch that cannot be restored".into(),
        )
    })?;
    let input = HDMI_INPUTS
        .into_iter()
        .find(|input| *input != restore_input && status.is_disconnected(*input))
        .ok_or_else(|| {
            DeployError::InputSwitch(
                "no inactive HDMI input reports both no cable and no signal".into(),
            )
        })?;
    Ok((input, restore_input))
}

fn read_input_status(config: &UserConfig) -> Result<InputStatus, DeployError> {
    let output = ssh_input(
        config,
        INPUT_STATUS_REMOTE_COMMAND,
        &[],
        "TV input status query",
    )?;
    parse_input_status(&output.stdout)
}

fn parse_input_status(bytes: &[u8]) -> Result<InputStatus, DeployError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| DeployError::InputSwitch("TV returned invalid input status JSON".into()))?;
    let property = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                DeployError::InputSwitch(format!(
                    "TV input status omitted {key} or returned a non-boolean value"
                ))
            })
    };
    let string_property = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                DeployError::InputSwitch(format!(
                    "TV input status omitted {key} or returned a non-string value"
                ))
            })
    };
    let mut active = None;
    let mut plugged = [false; HDMI_INPUTS.len()];
    let mut signal = [false; HDMI_INPUTS.len()];
    let mut seen = [false; HDMI_INPUTS.len()];
    for device_index in 0..HDMI_INPUTS.len() {
        let device_key = |field: &str| format!("devices[{device_index}].{field}");
        let app_id_key = device_key("appId");
        let app_id = string_property(&app_id_key)?;
        let input = HDMI_INPUTS
            .into_iter()
            .find(|input| input.app_id() == app_id)
            .ok_or_else(|| {
                DeployError::InputSwitch(format!(
                    "TV input status returned unknown HDMI app ID {app_id}"
                ))
            })?;
        let index = input.index();
        if seen[index] {
            return Err(DeployError::InputSwitch(format!(
                "TV input status returned duplicate HDMI app ID {app_id}"
            )));
        }
        seen[index] = true;
        if property(&device_key("activate"))? && active.replace(input).is_some() {
            return Err(DeployError::InputSwitch(
                "TV reported multiple HDMI inputs active at the same time".into(),
            ));
        }
        plugged[index] = property(&device_key("hdmiPlugIn"))?;
        signal[index] = property(&device_key("hdmiSignalExist"))?;
    }
    if seen.contains(&false) {
        return Err(DeployError::InputSwitch(
            "TV input status omitted one or more HDMI inputs".into(),
        ));
    }
    Ok(InputStatus {
        active,
        plugged,
        signal,
    })
}

fn verify_restored_input(
    status: &InputStatus,
    restore_input: HdmiInput,
    capture_path: &Path,
) -> Result<(), DeployError> {
    if status.active != Some(restore_input) {
        let active = status.active.map_or("unknown", HdmiInput::as_str);
        return Err(DeployError::InputSwitch(format!(
            "No Signal capture was saved to {}, but the active input is {active} instead of restored {}",
            capture_path.display(),
            restore_input.as_str()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
