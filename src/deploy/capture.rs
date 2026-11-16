use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{self, UserConfig};
use crate::patches::PatchArea;

use super::{DeployError, ssh_input};

const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CAPTURE_NAME_COMPONENT_BYTES: usize = 48;
const CAPTURE_DEVICE_METADATA_PREFIX: &str = "CAPTURE_DEVICE_METADATA=";
const CAPTURE_SURFACE_METADATA_PREFIX: &str = "CAPTURE_SURFACE_METADATA=";
const CAPTURE_WIDTH: u32 = 1920;
const CAPTURE_HEIGHT: u32 = 1080;
const CHECKERBOARD_TILE_SIZE: usize = 48;
const CHECKERBOARD_DARK: u8 = 56;
const CHECKERBOARD_LIGHT: u8 = 176;
const HALGAL_PIXEL_FORMAT_BGRA32: u16 = 0;
const HALGAL_CHECKERBOARD_CAPTURE_SCRIPT: &str =
    include_str!("capture/halgal-checkerboard-capture.sh");

pub fn capture_system_ui(config: &UserConfig, area_id: &str) -> Result<PathBuf, DeployError> {
    let area = capture_area_by_id(area_id)?;
    config::validate(config)?;
    let timestamp = capture_timestamp()?;
    let remote_command = format!("sh -s -- {MAX_CAPTURE_BYTES}");
    let output = ssh_input(
        config,
        &remote_command,
        HALGAL_CHECKERBOARD_CAPTURE_SCRIPT.as_bytes(),
        "checkerboard System UI capture",
    )?;
    if output.stdout.is_empty() {
        return Err(DeployError::Capture(
            "the TV returned an empty HAL_GAL capture".into(),
        ));
    }
    if output.stdout.len() > MAX_CAPTURE_BYTES {
        return Err(DeployError::Capture(
            "the TV capture exceeded the size limit".into(),
        ));
    }
    let (model, firmware) = capture_metadata_from_stderr(&output.stderr)?;
    let surface = capture_surface_metadata_from_stderr(&output.stderr)?;
    let png = checkerboard_png(&output.stdout, &surface)?;
    let filename_stem = capture_file_stem(&model, &firmware, &timestamp, area)?;
    let screenshot_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    fs::create_dir_all(&screenshot_directory).map_err(|error| {
        DeployError::Capture(format!(
            "could not create the project screenshots folder: {error}"
        ))
    })?;
    let (path, mut file) = reserve_capture_file(&screenshot_directory, &filename_stem)?;
    if let Err(error) = file.write_all(&png) {
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
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
struct CaptureSurfaceMetadata {
    width: u32,
    height: u32,
    pitch: usize,
    bpp: u16,
    #[serde(rename = "pixelFormat")]
    pixel_format: u16,
    length: usize,
}

fn capture_surface_metadata_from_stderr(
    stderr: &[u8],
) -> Result<CaptureSurfaceMetadata, DeployError> {
    let stderr = std::str::from_utf8(stderr)
        .map_err(|_| DeployError::Capture("HAL_GAL metadata was not valid text".into()))?;
    let metadata = stderr
        .lines()
        .find_map(|line| line.strip_prefix(CAPTURE_SURFACE_METADATA_PREFIX))
        .ok_or_else(|| DeployError::Capture("TV returned no HAL_GAL surface metadata".into()))?;
    serde_json::from_str(metadata)
        .map_err(|_| DeployError::Capture("HAL_GAL surface metadata was invalid JSON".into()))
}

fn checkerboard_png(bgra: &[u8], surface: &CaptureSurfaceMetadata) -> Result<Vec<u8>, DeployError> {
    if surface.width == 0
        || surface.height == 0
        || surface.width > CAPTURE_WIDTH
        || surface.height > CAPTURE_HEIGHT
        || surface.bpp != 32
        || surface.pixel_format != HALGAL_PIXEL_FORMAT_BGRA32
    {
        return Err(DeployError::Capture(
            "TV returned an unsupported HAL_GAL surface format".into(),
        ));
    }
    let width = surface.width as usize;
    let height = surface.height as usize;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| DeployError::Capture("HAL_GAL row size overflowed".into()))?;
    let frame_bytes = surface
        .pitch
        .checked_mul(height)
        .ok_or_else(|| DeployError::Capture("HAL_GAL frame size overflowed".into()))?;
    if surface.pitch < row_bytes
        || surface.length < frame_bytes
        || surface.length > MAX_CAPTURE_BYTES
        || bgra.len() != surface.length
    {
        return Err(DeployError::Capture(
            "TV returned an incomplete or oversized HAL_GAL surface".into(),
        ));
    }

    let mut png_bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, surface.width, surface.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| {
        DeployError::Capture(format!("could not encode checkerboard PNG: {error}"))
    })?;
    {
        let mut stream = writer.stream_writer().map_err(|error| {
            DeployError::Capture(format!("could not encode checkerboard PNG: {error}"))
        })?;
        let mut row = vec![0; width * 3];
        for y in 0..height {
            let source_start = y * surface.pitch;
            let source_row = &bgra[source_start..source_start + row_bytes];
            for (x, pixel) in source_row.chunks_exact(4).enumerate() {
                let [blue, green, red, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
                let matte = if ((x / CHECKERBOARD_TILE_SIZE + y / CHECKERBOARD_TILE_SIZE) & 1) == 0
                {
                    CHECKERBOARD_DARK
                } else {
                    CHECKERBOARD_LIGHT
                };
                let inverse_alpha = 255 - u32::from(alpha);
                let source_alpha = u32::from(alpha);
                let offset = x * 3;
                row[offset] = ((u32::from(red) * source_alpha
                    + u32::from(matte) * inverse_alpha
                    + 127)
                    / 255) as u8;
                row[offset + 1] =
                    ((u32::from(green) * source_alpha + u32::from(matte) * inverse_alpha + 127)
                        / 255) as u8;
                row[offset + 2] =
                    ((u32::from(blue) * source_alpha + u32::from(matte) * inverse_alpha + 127)
                        / 255) as u8;
            }
            stream.write_all(&row).map_err(|error| {
                DeployError::Capture(format!("could not encode checkerboard PNG: {error}"))
            })?;
        }
        stream.finish().map_err(|error| {
            DeployError::Capture(format!("could not encode checkerboard PNG: {error}"))
        })?;
    }
    writer.finish().map_err(|error| {
        DeployError::Capture(format!("could not encode checkerboard PNG: {error}"))
    })?;
    if png_bytes.len() > MAX_CAPTURE_BYTES {
        return Err(DeployError::Capture(
            "the checkerboard PNG exceeded the size limit".into(),
        ));
    }
    Ok(png_bytes)
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
