#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use std::{
    path::Path,
    process::{Command, Output},
};
fn run(source: &Path, fixture: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args([
            "run-fixture".as_ref(),
            source.as_os_str(),
            "--fixture".as_ref(),
            fixture.as_os_str(),
        ])
        .output()
        .unwrap()
}
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

struct InterruptedOutput {
    bytes: Vec<u8>,
    flush_only: bool,
}

#[test]
fn failed_runtime_stderr_delivery_overrides_provider_exit_without_rewriting_stdout() {
    struct LateFailure {
        flushes: usize,
    }
    impl std::io::Write for LateFailure {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.flushes == 1 {
                Ok(())
            } else {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
    }
    let d = OwnedDir::new("late-stderr");
    let mut f: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../examples/fixtures/triage/ubuntu/fixture.json"
    ))
    .unwrap();
    f["providers"]["forensic.system.profile"] =
        serde_json::json!({"status":"error","code":"PermissionDenied"});
    d.write("fixture.json", serde_json::to_vec(&f).unwrap());
    let mut stdout = Vec::new();
    let status = jocky_language::cli::run(
        [
            "run-fixture".into(),
            root().join("examples/triage.jky").into_os_string(),
            "--fixture".into(),
            d.0.as_os_str().to_owned(),
        ],
        &mut stdout,
        &mut LateFailure { flushes: 0 },
    );
    assert_eq!(status.exit_code(), 2);
    let report: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(report["outcome"]["error"]["code"], "PermissionDenied");
}
impl std::io::Write for InterruptedOutput {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if !self.flush_only && !self.bytes.is_empty() {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        let n = if self.flush_only {
            input.len()
        } else {
            input.len().min(17)
        };
        self.bytes.extend_from_slice(&input[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn partial_stdout_and_flush_failures_override_success_and_provider_status_without_second_json() {
    let dir = OwnedDir::new("fixture-streams");
    let source = root().join("examples/triage.jky");
    for code in [None, Some("PermissionDenied"), Some("ResourceLimit")] {
        let mut f: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../examples/fixtures/triage/ubuntu/fixture.json"
        ))
        .unwrap();
        if let Some(code) = code {
            f["providers"]["forensic.system.profile"] =
                serde_json::json!({"status":"error","code":code});
        }
        dir.write("fixture.json", serde_json::to_vec(&f).unwrap());
        for flush_only in [false, true] {
            let mut out = InterruptedOutput {
                bytes: vec![],
                flush_only,
            };
            let mut err = Vec::new();
            let result = jocky_language::cli::run(
                [
                    "run-fixture".into(),
                    source.as_os_str().to_owned(),
                    "--fixture".into(),
                    dir.0.as_os_str().to_owned(),
                ],
                &mut out,
                &mut err,
            );
            assert_eq!(result.exit_code(), 2);
            assert!(String::from_utf8(err).unwrap().contains("V3_OUTPUT"));
            if flush_only {
                serde_json::from_slice::<serde_json::Value>(&out.bytes).unwrap();
            } else {
                assert_eq!(out.bytes.len(), 17);
            }
        }
    }
}

#[test]
fn inert_fixture_paths_and_labels_cannot_change_test_owned_files_or_choose_providers() {
    let dir = OwnedDir::new("fixture-inert");
    let marker = dir.write("marker", "untouched");
    let mut f: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../examples/fixtures/triage/ubuntu/fixture.json"
    ))
    .unwrap();
    let inert = format!(
        "{} https://example.invalid/no-fetch --output {}",
        marker.display(),
        dir.path("never-created").display()
    );
    for pointer in [
        "/providers/forensic.process.list/value/values/0/value/image_path",
        "/providers/forensic.indicator.correlate/expected/processes/values/0/value/image_path",
    ] {
        *f.pointer_mut(pointer).unwrap() = inert.clone().into();
    }
    dir.write("fixture.json", serde_json::to_vec(&f).unwrap());
    let r = run(&root().join("examples/triage.jky"), &dir.0);
    assert!(r.status.success());
    assert!(r.stderr.is_empty());
    assert_eq!(std::fs::read(marker).unwrap(), b"untouched");
    assert!(!dir.path("never-created").exists());
    f["provider"] = inert.into();
    dir.write("fixture.json", serde_json::to_vec(&f).unwrap());
    let r = run(&root().join("examples/triage.jky"), &dir.0);
    assert_eq!(r.status.code(), Some(2));
    assert!(r.stdout.is_empty());
}

#[test]
fn teacher_demo_uses_only_fixture_json_and_keeps_real_modules_unavailable() {
    let catalogue = || {
        Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(["modules", "list"])
            .output()
            .unwrap()
            .stdout
    };
    let before = catalogue();
    for platform in ["ubuntu", "windows"] {
        let dir = OwnedDir::new("fixture-cli");
        let base = root().join("ir/tests/fixtures/valid").join(platform);
        dir.write(
            "fixture.json",
            std::fs::read(base.join("fixture.json")).unwrap(),
        );
        dir.write("expected-result.json", b"this must never be read");
        let r = run(&root().join("examples/triage.jky"), &dir.0);
        assert_eq!(
            r.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        assert!(r.stderr.is_empty());
        let report: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(base.join("expected-result.json")).unwrap())
                .unwrap();
        assert_eq!(report["outcome"]["value"], expected);
        assert_eq!(report["mode"], "fixture");
        assert_eq!(
            r.stdout,
            std::fs::read(root().join(format!(
                "ir/tests/fixtures/golden/triage-{platform}.report.json"
            )))
            .unwrap()
        );
    }
    assert_eq!(catalogue(), before);
}

#[test]
fn setup_source_and_runtime_failures_have_distinct_output_channels() {
    let dir = OwnedDir::new("fixture-errors");
    let source = root().join("examples/triage.jky");
    for bytes in [b"{}".as_slice(), b"\xff", br#"{"schema_version":"99.0.0"}"#] {
        let file = dir.path("fixture.json");
        std::fs::write(file, bytes).unwrap();
        let r = run(&source, &dir.0);
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(!r.stderr.is_empty());
    }
    let invalid = dir.write("invalid.jky", b"module");
    let r = run(&invalid, &dir.path("missing"));
    assert_eq!(r.status.code(), Some(1));
    assert!(r.stdout.is_empty());
    let ir = dir.write(
        "source.jky",
        include_bytes!("fixtures/ir/golden/triage.ir.json"),
    );
    let r = run(&ir, &dir.0);
    assert_eq!(r.status.code(), Some(1));
    assert!(r.stdout.is_empty());
    for (code, exit) in [("PermissionDenied", 1), ("ResourceLimit", 2)] {
        let mut fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../ir/tests/fixtures/valid/ubuntu/fixture.json"
        ))
        .unwrap();
        fixture["providers"]["forensic.system.profile"] =
            serde_json::json!({"status":"error","code":code});
        std::fs::write(
            dir.path("fixture.json"),
            serde_json::to_vec(&fixture).unwrap(),
        )
        .unwrap();
        let r = run(&source, &dir.0);
        assert_eq!(r.status.code(), Some(exit));
        assert!(!r.stderr.is_empty());
        let human = String::from_utf8_lossy(&r.stderr);
        assert!(human.contains("fixture execution stopped here"));
        assert!(!human.contains("compiler rejected this construct"));
        let report: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(report["outcome"]["error"]["code"], code);
        assert!(report["outcome"].get("value").is_none());
    }
}

#[test]
fn only_exact_fixture_arguments_are_accepted() {
    for args in [
        vec!["run-fixture"],
        vec!["run-fixture", "x"],
        vec!["run-fixture", "x", "--fixture", "y", "--live"],
        vec!["run-fixture", "--fixture", "y", "x"],
        vec!["run-fixture", "x", "--fixture", ""],
    ] {
        let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(r.stderr.starts_with(b"Usage:"));
    }
}

#[test]
fn static_commands_do_not_execute_and_fixture_selection_is_not_host_platform() {
    let dir = OwnedDir::new("fixture-static");
    let source = dir.write("unavailable.jky", b"module demo target windows | ubuntu fn main(t: endpoint) -> forensic_result { forensic.driver.list(t) return forensic.system.profile(t) } run main on selected_endpoints");
    for command in ["parse", "check"] {
        let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .arg(command)
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(0));
        if command == "parse" {
            assert_eq!(
                r.stdout,
                format!("parsed: {}\n", source.display()).as_bytes()
            );
        } else {
            assert!(r.stdout.is_empty());
        }
    }
    let fixture = root().join("examples/fixtures/triage/ubuntu");
    let r = run(&source, &fixture);
    assert_eq!(r.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(report["outcome"]["error"]["code"], "Unsupported");
    for (target, platform) in [("windows", "ubuntu"), ("ubuntu", "windows")] {
        let source = dir.write("target.jky", format!("module demo target {target} fn main(t: endpoint) -> forensic_result {{ return forensic.system.profile(t) }} run main on selected_endpoints"));
        let r = run(
            &source,
            &root().join("examples/fixtures/triage").join(platform),
        );
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(String::from_utf8(r.stderr).unwrap().contains("V3_FIXTURE"));
    }
    let source = dir.write("recursive.jky", b"module demo target ubuntu fn main(t: endpoint) -> forensic_result { return main(t) } run main on selected_endpoints");
    let r = run(&source, &fixture);
    assert_eq!(r.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(report["outcome"]["error"]["code"], "EXEC_CALL_DEPTH");
    assert_eq!(report["accounting"]["peak_call_depth"], 128);
    assert_eq!(report["accounting"]["instructions"], 128);
}

#[test]
#[cfg(unix)]
fn non_utf8_source_or_fixture_argument_is_rejected_without_lossy_path_access() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for index in [1, 3] {
        let mut args = ["run-fixture", "source.jky", "--fixture", "fixtures"].map(OsString::from);
        args[index] = OsString::from_vec(vec![0xff]);
        let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(
            String::from_utf8(r.stderr)
                .unwrap()
                .contains("invalid-path-encoding")
        );
    }
}
