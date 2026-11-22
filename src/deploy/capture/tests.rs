use super::*;

#[cfg(unix)]
use std::process::Stdio;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::current_dir()
            .unwrap()
            .join(".build")
            .join(format!("{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn screenshot_names_include_sanitized_device_metadata_and_area() {
    assert_eq!(
        capture_file_stem(
            " OLED 55/C3 ",
            "webOS 24.0/1 ",
            "20261116-140102",
            PatchArea::InputPicker,
        )
        .unwrap(),
        "OLED-55-C3_vwebOS-24-0-1_20261116-140102_input-picker"
    );
    assert!(
        capture_file_stem(
            "",
            "webOS 24.0",
            "20261116-140102",
            PatchArea::QuickSettings
        )
        .is_err()
    );
}

#[test]
fn screenshot_names_strip_model_suffix_and_prefix_firmware_version() {
    assert_eq!(
        capture_file_stem(
            "OLED55G45LW.REGION",
            "33.22.75",
            "20261116-140102",
            PatchArea::InputPicker,
        )
        .unwrap(),
        "OLED55G45LW_v33-22-75_20261116-140102_input-picker"
    );
}

#[test]
fn screenshot_area_rejects_unknown_identifiers() {
    assert_eq!(
        capture_area_by_id("input-picker").unwrap(),
        PatchArea::InputPicker
    );
    assert!(capture_area_by_id("../../outside").is_err());
}

#[test]
fn screenshot_metadata_uses_the_tv_model_and_firmware_response() {
    assert_eq!(
        capture_metadata_from_stderr(
            b"CAPTURE_DEVICE_METADATA={\"modelName\":\"OLED55G45LW.REGION\",\"firmwareVersion\":\"33.22.75\"}\n"
        )
        .unwrap(),
        (
            "OLED55G45LW.REGION".to_owned(),
            "33.22.75".to_owned()
        )
    );
}

#[test]
fn screenshot_metadata_requires_both_tv_fields() {
    assert!(capture_metadata_from_stderr(b"").is_err());
    assert!(
        capture_metadata_from_stderr(b"CAPTURE_DEVICE_METADATA={\"modelName\":\"OLED55G45LW\"}\n")
            .is_err()
    );
}

#[test]
fn screenshot_reservation_never_overwrites_and_keeps_files_private() {
    let test_directory = TestDirectory::new("capture-file-test");
    let directory = test_directory.path().join("captures");
    fs::create_dir_all(&directory).unwrap();

    let (first_path, mut first_file) =
        reserve_capture_file(&directory, "OLED55_webOS24_20261116-140102_input-picker").unwrap();
    first_file.write_all(b"existing capture").unwrap();
    drop(first_file);
    let (second_path, second_file) =
        reserve_capture_file(&directory, "OLED55_webOS24_20261116-140102_input-picker").unwrap();
    drop(second_file);

    assert_ne!(first_path, second_path);
    assert_eq!(fs::read(&first_path).unwrap(), b"existing capture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(first_path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}

#[test]
fn checkerboard_png_composites_transparency_over_alternating_tiles() {
    let width = 49_u32;
    let mut bgra = vec![0; width as usize * 4];
    bgra[4..8].copy_from_slice(&[20, 10, 200, 255]);
    bgra[8..12].copy_from_slice(&[0, 0, 255, 128]);
    let surface = CaptureSurfaceMetadata {
        width,
        height: 1,
        pitch: bgra.len(),
        bpp: 32,
        pixel_format: HALGAL_PIXEL_FORMAT_BGRA32,
        length: bgra.len(),
    };

    let png = checkerboard_png(&bgra, &surface).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut decoded).unwrap();
    decoded.truncate(info.buffer_size());

    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(&decoded[0..3], &[56, 56, 56]);
    assert_eq!(&decoded[3..6], &[200, 10, 20]);
    assert_eq!(&decoded[6..9], &[156, 28, 28]);
    assert_eq!(&decoded[48 * 3..49 * 3], &[176, 176, 176]);
}

#[test]
fn checkerboard_png_rejects_unsupported_or_incomplete_surfaces() {
    let surface = CaptureSurfaceMetadata {
        width: 1,
        height: 1,
        pitch: 4,
        bpp: 32,
        pixel_format: 1,
        length: 4,
    };
    let error = checkerboard_png(&[0, 0, 0, 255], &surface)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("unsupported HAL_GAL surface format"),
        "{error}"
    );

    let surface = CaptureSurfaceMetadata {
        pixel_format: HALGAL_PIXEL_FORMAT_BGRA32,
        length: 8,
        ..surface
    };
    let error = checkerboard_png(&[0, 0, 0, 255], &surface)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("incomplete or oversized HAL_GAL surface"),
        "{error}"
    );
}

#[test]
fn halgal_surface_metadata_requires_the_complete_layout() {
    assert_eq!(
        capture_surface_metadata_from_stderr(
            b"noise\nCAPTURE_SURFACE_METADATA={\"width\":1920,\"height\":1080,\"pitch\":7680,\"bpp\":32,\"pixelFormat\":0,\"length\":8294400}\n"
        )
        .unwrap(),
        CaptureSurfaceMetadata {
            width: 1920,
            height: 1080,
            pitch: 7680,
            bpp: 32,
            pixel_format: 0,
            length: 8294400,
        }
    );
    assert!(
        capture_surface_metadata_from_stderr(
            b"CAPTURE_SURFACE_METADATA={\"width\":1920,\"height\":1080,\"pitch\":7680,\"bpp\":32,\"pixelFormat\":0}\n"
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn halgal_capture_does_not_return_partial_surface_on_failure() {
    use std::os::unix::fs::PermissionsExt;

    let test_directory = TestDirectory::new("capture-response-test");
    let bin = test_directory.path().join("bin");
    fs::create_dir(&bin).unwrap();

    let node = bin.join("node");
    fs::write(
        &node,
        r#"#!/bin/sh
set -eu
case "${1:-}" in
    -)
        printf '%s\n' '{"modelName":"Test TV","firmwareVersion":"1"}' > "$2"
        ;;
    *)
        exit 2
        ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();

    let python = bin.join("python3");
    fs::write(
        &python,
        r#"#!/bin/sh
set -eu
printf 'partial surface' > "$2"
printf '%s\n' 'HAL_GAL_CaptureFrameBuffer failed' >&2
exit 1
"#,
    )
    .unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o700)).unwrap();

    let mktemp = bin.join("mktemp");
    fs::write(
        &mktemp,
        "#!/bin/sh\nset -eu\nmkdir -p \"$CAPTURE_TEST_DIR\"\nprintf '%s\\n' \"$CAPTURE_TEST_DIR\"\n",
    )
    .unwrap();
    fs::set_permissions(&mktemp, fs::Permissions::from_mode(0o700)).unwrap();

    let capture_directory = test_directory.path().join("tv-capture");
    let mut path_entries = vec![bin];
    path_entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(path_entries).unwrap();

    let mut child = Command::new("/bin/sh")
        .arg("-s")
        .arg("--")
        .arg(MAX_CAPTURE_BYTES.to_string())
        .current_dir(test_directory.path())
        .env("PATH", path)
        .env("CAPTURE_TEST_DIR", &capture_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(HALGAL_CHECKERBOARD_CAPTURE_SCRIPT.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "partial surface must not be returned"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("HAL_GAL_CaptureFrameBuffer failed"),
        "unexpected capture error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !capture_directory.exists(),
        "remote temporary files must be cleaned"
    );
}

#[cfg(unix)]
#[test]
fn no_signal_remote_capture_restores_after_success_and_failure_and_checks_hdmi4_status() {
    use std::os::unix::fs::PermissionsExt;

    let test_directory = TestDirectory::new("no-signal-restore-test");
    let bin = test_directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let state_file = test_directory.path().join("active-input");
    fs::write(&state_file, "hdmi2").unwrap();
    let events_file = test_directory.path().join("input-events");
    let write_executable = |name: &str, source: &str| {
        let path = bin.join(name);
        fs::write(&path, source).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    };

    write_executable(
        "script",
        r#"#!/bin/sh
set -eu
case "$3" in
    *getAllInputStatus*)
        case "$3" in
            *"luna-send -w 5000 -n 1"*) ;;
            *) exit 4 ;;
        esac
        active=$(cat "$NO_SIGNAL_TEST_STATE")
        if [ "$NO_SIGNAL_TEST_LUNA_TIMEOUT" = status ] && [ "$active" = hdmi4 ]; then
            printf '%s\n' status-timeout >> "$NO_SIGNAL_TEST_EVENTS"
            exit 124
        fi
        if [ "$active" = hdmi4 ]; then
            if [ "$NO_SIGNAL_TEST_BAD_HDMI4_SIGNAL" = 1 ]; then
                printf '%s\n' '{"devices[0].appId":"com.webos.app.hdmi3","devices[0].activate":false,"devices[0].hdmiPlugIn":false,"devices[0].hdmiSignalExist":false,"devices[1].appId":"com.webos.app.hdmi2","devices[1].activate":false,"devices[1].hdmiPlugIn":true,"devices[1].hdmiSignalExist":true,"devices[2].appId":"com.webos.app.hdmi4","devices[2].activate":true,"devices[2].hdmiPlugIn":false,"devices[2]XhdmiSignalExist":false,"devices[3].appId":"com.webos.app.hdmi1","devices[3].activate":false,"devices[3].hdmiPlugIn":false,"devices[3].hdmiSignalExist":false}'
            else
                printf '%s\n' '{"devices[0].appId":"com.webos.app.hdmi3","devices[0].activate":false,"devices[0].hdmiPlugIn":false,"devices[0].hdmiSignalExist":false,"devices[1].appId":"com.webos.app.hdmi2","devices[1].activate":false,"devices[1].hdmiPlugIn":true,"devices[1].hdmiSignalExist":true,"devices[2].appId":"com.webos.app.hdmi4","devices[2].activate":true,"devices[2].hdmiPlugIn":false,"devices[2].hdmiSignalExist":false,"devices[3].appId":"com.webos.app.hdmi1","devices[3].activate":false,"devices[3].hdmiPlugIn":false,"devices[3].hdmiSignalExist":false}'
            fi
        else
            printf '%s\n' '{"devices[0].appId":"com.webos.app.hdmi3","devices[0].activate":false,"devices[0].hdmiPlugIn":false,"devices[0].hdmiSignalExist":false,"devices[1].appId":"com.webos.app.hdmi2","devices[1].activate":true,"devices[1].hdmiPlugIn":true,"devices[1].hdmiSignalExist":true,"devices[2].appId":"com.webos.app.hdmi4","devices[2].activate":false,"devices[2].hdmiPlugIn":false,"devices[2].hdmiSignalExist":false,"devices[3].appId":"com.webos.app.hdmi1","devices[3].activate":false,"devices[3].hdmiPlugIn":false,"devices[3].hdmiSignalExist":false}'
        fi
        ;;
    *applicationManager/launch*)
        case "$3" in
            *"luna-send -w 5000 -n 1"*) ;;
            *) exit 5 ;;
        esac
        case "$3" in
            *com.webos.app.hdmi4*)
                if [ "$NO_SIGNAL_TEST_LUNA_TIMEOUT" = launch ]; then
                    printf '%s\n' hdmi4-timeout >> "$NO_SIGNAL_TEST_EVENTS"
                    exit 124
                fi
                if [ "$NO_SIGNAL_TEST_NO_SWITCH" != 1 ]; then
                    printf '%s' hdmi4 > "$NO_SIGNAL_TEST_STATE"
                fi
                printf '%s\n' hdmi4 >> "$NO_SIGNAL_TEST_EVENTS"
                ;;
            *com.webos.app.hdmi2*)
                if [ "$NO_SIGNAL_TEST_NO_RESTORE" != 1 ]; then
                    printf '%s' hdmi2 > "$NO_SIGNAL_TEST_STATE"
                fi
                printf '%s\n' hdmi2 >> "$NO_SIGNAL_TEST_EVENTS"
                ;;
            *) exit 2 ;;
        esac
        printf '%s\n' '{"returnValue": true}'
        ;;
    *) exit 3 ;;
esac
"#,
    );
    write_executable(
        "node",
        "#!/bin/sh\nset -eu\nprintf '%s\\n' '{\"modelName\":\"Test TV\",\"firmwareVersion\":\"1\"}' > \"$2\"\n",
    );
    let capture_calls_file = test_directory.path().join("capture-calls");
    write_executable(
        "python3",
        r#"#!/bin/sh
set -eu
printf '%s\n' called >> "$NO_SIGNAL_TEST_CAPTURE_CALLS"
if [ "$NO_SIGNAL_TEST_CAPTURE_FAIL" = 1 ]; then
    printf partial > "$2"
    printf '%s\n' 'HAL_GAL_CaptureFrameBuffer failed' >&2
    exit 1
fi
head -c 4 /dev/zero > "$2"
printf '%s\n' 'CAPTURE_SURFACE_METADATA={"width":1,"height":1,"pitch":4,"bpp":32,"pixelFormat":0,"length":4}' >&2
"#,
    );
    write_executable(
        "mktemp",
        "#!/bin/sh\nset -eu\nmkdir -p \"$CAPTURE_TEST_DIR\"\nprintf '%s\\n' \"$CAPTURE_TEST_DIR\"\n",
    );
    write_executable("sleep", "#!/bin/sh\nexit 0\n");

    let mut path_entries = vec![bin];
    path_entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(path_entries).unwrap();
    let capture_directory = test_directory.path().join("tv-capture");
    let run_capture = |no_switch: bool,
                       fail_capture: bool,
                       bad_hdmi4_signal: bool,
                       luna_timeout: &str,
                       no_restore: bool| {
        let mut child = Command::new("/bin/sh")
            .arg("-s")
            .arg("--")
            .arg("hdmi4")
            .arg("hdmi2")
            .arg(MAX_CAPTURE_BYTES.to_string())
            .current_dir(test_directory.path())
            .env("PATH", &path)
            .env("NO_SIGNAL_TEST_STATE", &state_file)
            .env("CAPTURE_TEST_DIR", &capture_directory)
            .env("NO_SIGNAL_TEST_EVENTS", &events_file)
            .env("NO_SIGNAL_TEST_CAPTURE_CALLS", &capture_calls_file)
            .env(
                "NO_SIGNAL_TEST_NO_SWITCH",
                if no_switch { "1" } else { "0" },
            )
            .env(
                "NO_SIGNAL_TEST_CAPTURE_FAIL",
                if fail_capture { "1" } else { "0" },
            )
            .env(
                "NO_SIGNAL_TEST_BAD_HDMI4_SIGNAL",
                if bad_hdmi4_signal { "1" } else { "0" },
            )
            .env("NO_SIGNAL_TEST_LUNA_TIMEOUT", luna_timeout)
            .env(
                "NO_SIGNAL_TEST_NO_RESTORE",
                if no_restore { "1" } else { "0" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(super::no_signal::NO_SIGNAL_CAPTURE_SCRIPT.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };

    let output = run_capture(false, true, false, "none", false);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&events_file).unwrap(), "hdmi4\nhdmi2\n");
    assert_eq!(fs::read_to_string(&capture_calls_file).unwrap(), "called\n");
    assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi2");

    fs::write(&state_file, "hdmi2").unwrap();
    fs::remove_file(&events_file).unwrap();
    fs::remove_file(&capture_calls_file).unwrap();
    let output = run_capture(true, false, false, "none", false);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("is not active"),
        "expected inactive input refusal, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&events_file).unwrap(), "hdmi4\nhdmi2\n");
    assert!(
        !capture_calls_file.exists(),
        "must not capture the wrong input"
    );
    assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi2");

    fs::write(&state_file, "hdmi2").unwrap();
    fs::remove_file(&events_file).unwrap();
    let output = run_capture(false, false, false, "none", false);
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 4);
    assert_eq!(fs::read_to_string(&events_file).unwrap(), "hdmi4\nhdmi2\n");
    assert_eq!(fs::read_to_string(&capture_calls_file).unwrap(), "called\n");
    assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi2");

    fs::write(&state_file, "hdmi2").unwrap();
    fs::remove_file(&events_file).unwrap();
    fs::remove_file(&capture_calls_file).unwrap();
    let output = run_capture(false, false, true, "none", false);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("connected or has signal"),
        "expected missing HDMI 4 property refusal, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&events_file).unwrap(), "hdmi4\nhdmi2\n");
    assert!(
        !capture_calls_file.exists(),
        "must not accept a mismatched EIM key"
    );
    assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi2");
    for (timeout, expected_events) in [
        ("launch", "hdmi4-timeout\nhdmi2\n"),
        ("status", "hdmi4\nstatus-timeout\nhdmi2\n"),
    ] {
        fs::write(&state_file, "hdmi2").unwrap();
        fs::remove_file(&events_file).unwrap();
        if capture_calls_file.exists() {
            fs::remove_file(&capture_calls_file).unwrap();
        }
        let output = run_capture(false, false, false, timeout, false);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(fs::read_to_string(&events_file).unwrap(), expected_events);
        assert!(
            !capture_calls_file.exists(),
            "must not capture after a timed-out Luna request"
        );
        assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi2");
    }
    for fail_capture in [false, true] {
        fs::write(&state_file, "hdmi2").unwrap();
        fs::remove_file(&events_file).unwrap();
        if capture_calls_file.exists() {
            fs::remove_file(&capture_calls_file).unwrap();
        }
        let output = run_capture(false, fail_capture, false, "none", true);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Could not verify restoration of the previously active HDMI input"),
            "the launch reply must not substitute for restoration verification: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&events_file).unwrap(), "hdmi4\nhdmi2\n");
        assert_eq!(fs::read_to_string(&state_file).unwrap(), "hdmi4");
        assert_eq!(fs::read_to_string(&capture_calls_file).unwrap(), "called\n");
    }
}
