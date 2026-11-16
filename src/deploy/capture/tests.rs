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

#[cfg(unix)]
#[test]
fn graphic_capture_rejects_nested_success_from_failed_service_and_cleans_partial_file() {
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
    -e)
        if [ -n "${3:-}" ]; then
            printf 'partial capture' > "$3"
            printf '%s\n' '{"returnValue":false,"details":{"returnValue":true}}'
            exit 1
        fi
        ;;
    *)
        exit 2
        ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();

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
        .arg(MAX_GRAPHIC_CAPTURE_BYTES.to_string())
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
        .write_all(GRAPHIC_CAPTURE_SCRIPT.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "partial capture must not be returned"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Capture service request failed"),
        "unexpected capture error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !capture_directory.exists(),
        "remote temporary files must be cleaned"
    );
}
