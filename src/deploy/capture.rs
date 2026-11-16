use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, UserConfig};
use crate::patches::PatchArea;

use super::{DeployError, ssh_input};

const MAX_GRAPHIC_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CAPTURE_NAME_COMPONENT_BYTES: usize = 48;
const CAPTURE_DEVICE_METADATA_PREFIX: &str = "CAPTURE_DEVICE_METADATA=";
const GRAPHIC_CAPTURE_SCRIPT: &str = r#"set -eu
umask 077
capture_dir="$(mktemp -d /tmp/systemui-graphic-XXXXXX)"
trap 'rm -rf "$capture_dir"' EXIT
capture_file="$capture_dir/capture.png"
metadata_file="$capture_dir/device-info.json"
node - "$metadata_file" <<'INFO_NODE'
var fs = require('fs');
var palmbus = require('/usr/lib/nodejs/palmbus.js');
var bus = new palmbus.Handle('', true);
var finished = false;
var timeout;
function finish(status, error) {
    if (finished) {
        return;
    }
    finished = true;
    clearTimeout(timeout);
    if (error) {
        process.stderr.write(error + '\n', function () {
            process.exit(status);
        });
    } else {
        process.exit(status);
    }
}
timeout = setTimeout(function () {
    finish(124, 'TV information service response timeout');
}, 15000);
try {
    var call = bus.call(
        'luna://com.webos.service.tv.systemproperty/getSystemInfo',
        JSON.stringify({ keys: ['modelName', 'firmwareVersion'] })
    );
    call.setResponseTimeout(15000);
    call.addListener('response', function (message) {
        if (finished) {
            return;
        }
        var result;
        try {
            result = JSON.parse(message.payload());
        } catch (error) {
            finish(1, 'TV information service returned an invalid response');
            return;
        }
        if (
            !result ||
            result.returnValue !== true ||
            typeof result.modelName !== 'string' ||
            !result.modelName.trim() ||
            typeof result.firmwareVersion !== 'string' ||
            !result.firmwareVersion.trim()
        ) {
            finish(1, 'TV did not return model and firmware information');
            return;
        }
        try {
            fs.writeFileSync(
                process.argv[2],
                JSON.stringify({
                    modelName: result.modelName,
                    firmwareVersion: result.firmwareVersion
                }),
                { mode: 0o600 }
            );
        } catch (error) {
            finish(1, 'Could not save TV metadata temporarily');
            return;
        }
        finish(0, null);
    });
} catch (error) {
    finish(1, 'Could not request TV model and firmware information');
}
INFO_NODE
response_status=0
response="$(node -e '
var palmbus = require("/usr/lib/nodejs/palmbus.js");
var bus = new palmbus.Handle("", true);
var request = JSON.stringify({
    path: process.argv[1],
    method: "GRAPHIC",
    format: "PNG",
    width: 1920,
    height: 1080
});
var finished = false;
var timeout = setTimeout(function () {
    process.stderr.write("Capture service response timeout\n");
    process.exit(124);
}, 31000);
var call = bus.call("luna://com.webos.service.capture/executeOneShot", request);
call.setResponseTimeout(30000);
call.addListener("response", function (message) {
    var payload = message.payload();
    var status = 0;
    try {
        var result = JSON.parse(payload);
        if (result.returnValue !== true) {
            status = 1;
        }
    } catch (error) {
        status = 2;
    }
    if (finished) {
        return;
    }
    finished = true;
    clearTimeout(timeout);
    process.stdout.write(payload + "\n", function () {
        process.exit(status);
    });
});
' "$capture_file")" || response_status=$?
if [ "$response_status" -ne 0 ]; then
    service_error="$(printf '%s\n' "$response" | node -e '
var fs = require("fs");
var result;
try {
    result = JSON.parse(fs.readFileSync(0, "utf8"));
} catch (error) {
    process.exit(0);
}
var details = [];
["errorCode", "errorText"].forEach(function (key) {
    var value = result && result[key];
    if (typeof value === "string" || typeof value === "number") {
        value = String(value).replace(/[\u0000-\u001f\u007f]/g, " ").trim();
        if (value) {
            details.push(key + "=" + value.slice(0, 256));
        }
    }
});
process.stdout.write(details.join("; ").slice(0, 512));
')"
    if [ -n "$service_error" ]; then
        printf 'Capture service request failed (status %s): %s\n' "$response_status" "$service_error" >&2
    else
        printf 'Capture service request failed (status %s).\n' "$response_status" >&2
    fi
    exit 1
fi
attempt=0
while [ "$attempt" -lt 20 ] && [ ! -s "$capture_file" ]; do
    sleep 0.5
    attempt=$((attempt + 1))
done
if [ ! -s "$capture_file" ]; then
    echo 'GRAPHIC capture produced no PNG' >&2
    exit 1
fi
capture_size="$(wc -c < "$capture_file" | tr -d '[:space:]')"
case "$capture_size" in
    ''|*[!0-9]*)
        echo 'GRAPHIC capture returned an invalid byte count' >&2
        exit 1
        ;;
esac
if [ "$capture_size" -gt "$1" ]; then
    echo 'GRAPHIC capture exceeded the size limit' >&2
    exit 1
fi
if [ ! -s "$metadata_file" ]; then
    echo 'TV returned no model or firmware metadata' >&2
    exit 1
fi
metadata_size="$(wc -c < "$metadata_file" | tr -d '[:space:]')"
case "$metadata_size" in
    ''|*[!0-9]*)
        echo 'TV metadata had an invalid byte count' >&2
        exit 1
        ;;
esac
if [ "$metadata_size" -gt 4096 ]; then
    echo 'TV metadata exceeded the size limit' >&2
    exit 1
fi
metadata_json="$(cat "$metadata_file")"
printf 'CAPTURE_DEVICE_METADATA=%s\n' "$metadata_json" >&2
cat "$capture_file"
"#;

pub fn capture_system_ui(config: &UserConfig, area_id: &str) -> Result<PathBuf, DeployError> {
    let area = capture_area_by_id(area_id)?;
    config::validate(config)?;
    let timestamp = capture_timestamp()?;
    let remote_command = format!("sh -s -- {MAX_GRAPHIC_CAPTURE_BYTES}");
    let output = ssh_input(
        config,
        &remote_command,
        GRAPHIC_CAPTURE_SCRIPT.as_bytes(),
        "GRAPHIC-only System UI capture",
    )?;
    if output.stdout.is_empty() {
        return Err(DeployError::Capture(
            "the TV returned an empty GRAPHIC capture".into(),
        ));
    }
    if output.stdout.len() > MAX_GRAPHIC_CAPTURE_BYTES {
        return Err(DeployError::Capture(
            "the TV capture exceeded the size limit".into(),
        ));
    }
    let (model, firmware) = capture_metadata_from_stderr(&output.stderr)?;
    let filename_stem = capture_file_stem(&model, &firmware, &timestamp, area)?;
    let screenshot_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    fs::create_dir_all(&screenshot_directory).map_err(|error| {
        DeployError::Capture(format!(
            "could not create the project screenshots folder: {error}"
        ))
    })?;
    let (path, mut file) = reserve_capture_file(&screenshot_directory, &filename_stem)?;
    if let Err(error) = file.write_all(&output.stdout) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(DeployError::Capture(format!(
            "could not write the local PNG: {error}"
        )));
    }
    Ok(path)
}

fn capture_area_by_id(area_id: &str) -> Result<PatchArea, DeployError> {
    PatchArea::ALL
        .into_iter()
        .find(|area| area.id() == area_id)
        .ok_or_else(|| {
            let supported = PatchArea::ALL
                .into_iter()
                .map(PatchArea::id)
                .collect::<Vec<_>>()
                .join(", ");
            DeployError::Capture(format!("unknown test area; choose one of: {supported}"))
        })
}

fn capture_metadata_from_stderr(stderr: &[u8]) -> Result<(String, String), DeployError> {
    let stderr = std::str::from_utf8(stderr)
        .map_err(|_| DeployError::Capture("TV metadata was not valid text".into()))?;
    let metadata = stderr
        .lines()
        .find_map(|line| line.strip_prefix(CAPTURE_DEVICE_METADATA_PREFIX))
        .ok_or_else(|| DeployError::Capture("TV returned no model or firmware metadata".into()))?;
    let metadata: serde_json::Value = serde_json::from_str(metadata)
        .map_err(|_| DeployError::Capture("TV metadata was not valid JSON".into()))?;
    let model = metadata
        .get("modelName")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| DeployError::Capture("TV returned no model name".into()))?;
    let firmware = metadata
        .get("firmwareVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| DeployError::Capture("TV returned no firmware version".into()))?;
    Ok((model.to_owned(), firmware.to_owned()))
}

fn capture_file_stem(
    model: &str,
    firmware: &str,
    timestamp: &str,
    area: PatchArea,
) -> Result<String, DeployError> {
    let model = model.trim();
    let model = model.rsplit_once('.').map_or(model, |(model, _)| model);
    let model = capture_filename_component(model, "TV model")?;
    let firmware = capture_filename_component(firmware, "firmware version")?;
    Ok(format!("{model}_v{firmware}_{timestamp}_{}", area.id()))
}

fn capture_filename_component(value: &str, label: &str) -> Result<String, DeployError> {
    if value.trim().is_empty() {
        return Err(DeployError::Capture(format!(
            "TV did not provide its {label}"
        )));
    }
    let mut component = String::with_capacity(MAX_CAPTURE_NAME_COMPONENT_BYTES);
    for character in value.trim().chars() {
        if component.len() >= MAX_CAPTURE_NAME_COMPONENT_BYTES {
            break;
        }
        if character.is_ascii_alphanumeric() {
            component.push(character);
        } else if !component.is_empty() && !component.ends_with('-') {
            component.push('-');
        }
    }
    let component = component.trim_matches('-');
    if component.is_empty() {
        return Err(DeployError::Capture(format!(
            "the {label} must contain ASCII letters or numbers to name the capture"
        )));
    }
    Ok(component.to_owned())
}

fn capture_timestamp() -> Result<String, DeployError> {
    let output = Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map_err(|error| {
            DeployError::Capture(format!("could not get the local capture time: {error}"))
        })?;
    if !output.status.success() {
        return Err(DeployError::Capture(
            "could not get the local capture time".into(),
        ));
    }
    let timestamp = std::str::from_utf8(&output.stdout)
        .map_err(|_| DeployError::Capture("local capture time was not valid text".into()))?
        .trim();
    if timestamp.len() != 15
        || !timestamp.bytes().enumerate().all(|(index, byte)| {
            if index == 8 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(DeployError::Capture(
            "local capture time had an unexpected format".into(),
        ));
    }
    Ok(timestamp.to_owned())
}

fn reserve_capture_file(
    directory: &Path,
    filename_stem: &str,
) -> Result<(PathBuf, File), DeployError> {
    for suffix in 0..100 {
        let filename = if suffix == 0 {
            format!("{filename_stem}.png")
        } else {
            format!("{filename_stem}-{suffix}.png")
        };
        let path = directory.join(filename);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(DeployError::Capture(format!(
                    "could not create screenshot file {}: {error}",
                    path.display()
                )));
            }
        }
    }
    Err(DeployError::Capture(
        "could not create a unique screenshot filename".into(),
    ))
}

#[cfg(test)]
mod tests;
