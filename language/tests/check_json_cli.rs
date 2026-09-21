mod support;
use jocky_language::cli::{SourceReader, run_with_reader};
use std::{ffi::OsString, io, path::Path};

#[test]
fn real_json_checks_have_one_schema_valid_document_and_correct_status() {
    for (file, exit, status) in [
        ("examples/triage.jky", 0, "valid"),
        (
            "language/tests/fixtures/semantic/warnings/unused.jky",
            0,
            "valid",
        ),
        (
            "language/tests/fixtures/semantic/invalid/non-bool-condition.jky",
            1,
            "invalid",
        ),
        (
            "language/tests/fixtures/invalid/duplicate-run.jky",
            1,
            "invalid",
        ),
        ("target/nonexistent-report-input.jky", 2, "failure"),
        ("language", 2, "failure"),
    ] {
        let output = support::binary(&["check", file, "--diagnostic-format", "json"]);
        assert_eq!(output.status.code(), Some(exit), "{file}");
        assert!(output.stderr.is_empty(), "{:?}", output.stderr);
        let value = support::assert_report(&output.stdout);
        assert_eq!(value["status"], status);
        assert_eq!(value["file"], file);
        if exit == 2 {
            assert!(value["diagnostics"][0]["location"].is_null());
        }
        if exit != 0 {
            assert!(value["metadata"].is_null());
        }
    }
}
struct Bytes(Result<Vec<u8>, io::ErrorKind>);
impl SourceReader for Bytes {
    fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
        self.0.clone().map_err(io::Error::from)
    }
}
#[test]
fn injected_read_encoding_and_resource_failures_are_structured_and_locationless() {
    for (input, code) in [
        (Ok(vec![0xff]), "invalid-utf8"),
        (Err(io::ErrorKind::PermissionDenied), "input-unreadable"),
        (Err(io::ErrorKind::NotFound), "input-not-found"),
        (Ok(vec![b' '; 4_194_305]), "resource-limit"),
    ] {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let outcome = run_with_reader(
            ["check", "fake\nname", "--diagnostic-format", "json"].map(OsString::from),
            &mut Bytes(input),
            &mut out,
            &mut err,
        );
        assert_eq!(outcome.exit_code(), 2);
        assert!(err.is_empty());
        let value = support::assert_report(&out);
        assert_eq!(value["diagnostics"][0]["code"], code);
        assert!(value["diagnostics"][0]["location"].is_null());
        assert!(value["diagnostics"][0]["label"].is_null());
        assert_eq!(value["file"], "fake\nname");
    }
}
#[test]
fn malformed_options_never_guess_a_json_request() {
    for args in [
        vec!["check", "--diagnostic-format", "json", "a"],
        vec!["check", "a", "--diagnostic-format", "xml"],
        vec!["check", "a", "--diagnostic-format", "json", "extra"],
        vec![
            "check",
            "a",
            "--diagnostic-format",
            "json",
            "--diagnostic-format",
            "json",
        ],
    ] {
        let output = support::binary(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, jocky_language::cli::USAGE.as_bytes());
    }
}
#[test]
fn platform_non_utf8_path_is_recognized_before_decoding_without_reading() {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    };
    #[cfg(any(unix, windows))]
    {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args([
                OsString::from("check"),
                path,
                OsString::from("--diagnostic-format"),
                OsString::from("json"),
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stderr.is_empty());
        let value = support::assert_report(&output.stdout);
        assert!(value["file"].is_null());
        assert_eq!(value["diagnostics"][0]["code"], "invalid-path-encoding");
    }
}

#[test]
fn actual_invalid_utf8_file_is_not_lossily_decoded() {
    let dir = support::root().join("target/check-json-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("invalid-{}.bin", std::process::id()));
    let mut file = std::fs::File::create_new(&path).unwrap();
    std::io::Write::write_all(&mut file, &[0xff, 0xfe, 0]).unwrap();
    drop(file);
    let output = support::binary(&[
        "check",
        path.to_str().unwrap(),
        "--diagnostic-format",
        "json",
    ]);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value = support::assert_report(&output.stdout);
    assert_eq!(value["diagnostics"][0]["code"], "invalid-utf8");
    assert!(value["diagnostics"][0]["location"].is_null());
}
