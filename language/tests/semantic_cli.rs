use jocky_language::{
    SourceFile,
    cli::{ProcessOutcome, SourceReader, run_with_reader},
    report::render_failure,
    semantic::analyze,
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn binary(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args(arguments)
        .current_dir(root())
        .output()
        .unwrap()
}

#[test]
fn human_check_handles_every_negative_and_warning_only_success() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/semantic/invalid/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let file = format!(
            "language/tests/fixtures/semantic/invalid/{}",
            case["file"].as_str().unwrap()
        );
        let text = std::fs::read_to_string(root().join(&file)).unwrap();
        let failure = analyze(
            SourceFile::new(&file, &text),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap_err();
        let expected = render_failure(SourceFile::new(&file, &text), &failure).unwrap();
        for arguments in [
            vec!["check", file.as_str()],
            vec!["check", file.as_str(), "--diagnostic-format", "human"],
        ] {
            let output = binary(&arguments);
            assert_eq!(output.status.code(), Some(1), "{file}");
            assert!(output.stdout.is_empty());
            assert_eq!(output.stderr, expected.as_bytes());
        }
    }
    let output = binary(&[
        "check",
        "language/tests/fixtures/semantic/warnings/unused.jky",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        include_bytes!("fixtures/semantic/warnings/unused.stderr")
    );
}

#[test]
fn golden_human_error_is_stable_across_ten_binary_invocations() {
    for _ in 0..10 {
        let output = binary(&[
            "check",
            "language/tests/fixtures/semantic/invalid/non-bool-condition.jky",
        ]);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            output.stderr,
            include_bytes!("fixtures/semantic/invalid/non-bool-condition.stderr")
        );
    }
}

#[test]
fn positives_profile_and_original_syntax_failures_follow_the_cli_contract() {
    for path in [
        "examples/triage.jky",
        "language/tests/fixtures/profile/valid.jky",
    ] {
        let output = binary(&["check", path, "--diagnostic-format", "human"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    let emitted = binary(&[
        "parse",
        "language/tests/fixtures/profile/valid.jky",
        "--emit-ast",
        "json",
    ]);
    assert_eq!(emitted.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&emitted.stdout).unwrap()["schema_version"],
        "2.0.0"
    );
    let syntax = binary(&["check", "language/tests/fixtures/invalid/duplicate-run.jky"]);
    assert_eq!(syntax.status.code(), Some(1));
    assert_eq!(
        syntax.stderr,
        include_bytes!("fixtures/invalid/duplicate-run.stderr")
    );
    for args in [
        vec!["check", "examples/triage.jky", "--diagnostic-format", "xml"],
        vec![
            "check",
            "--diagnostic-format",
            "human",
            "examples/triage.jky",
        ],
    ] {
        let output = binary(&args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, jocky_language::cli::USAGE.as_bytes());
    }
}

struct Memory(Vec<u8>);
impl SourceReader for Memory {
    fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}
struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn semantic_resource_and_writer_failures_never_report_success() {
    let arguments = || [OsString::from("check"), OsString::from("memory")];
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(
        run_with_reader(
            arguments(),
            &mut Memory(vec![b' '; 4_194_305]),
            &mut out,
            &mut err
        ),
        ProcessOutcome::CommandOrHostFailure
    );
    assert!(out.is_empty());
    assert!(String::from_utf8(err).unwrap().contains("resource-limit"));
    let warnings = include_bytes!("fixtures/semantic/warnings/unused.jky").to_vec();
    assert_eq!(
        run_with_reader(
            arguments(),
            &mut Memory(warnings),
            &mut out,
            &mut FailedWriter
        ),
        ProcessOutcome::CommandOrHostFailure
    );
}

#[test]
fn oversized_human_report_is_replaced_not_partially_written() {
    let body = (0..40)
        .map(|i| format!("let unused{i} = 1 "))
        .collect::<String>();
    let source = format!(
        "module demo target ubuntu fn main(target: endpoint) -> forensic_result {{ let padding = \"{}\" {body} return forensic.system.profile(target) }} run main on selected_endpoints",
        "x".repeat(500_000)
    );
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = run_with_reader(
        [OsString::from("check"), OsString::from("large-line")],
        &mut Memory(source.into_bytes()),
        &mut out,
        &mut err,
    );
    assert_eq!(result, ProcessOutcome::CommandOrHostFailure);
    assert!(out.is_empty());
    assert_eq!(err, b"error[resource-limit]: report bytes limit exceeded\n");
}

#[test]
fn json_matches_every_semantic_negative_and_all_examples_pass_both_formats() {
    let schema = {
        #[path = "support/mod.rs"]
        mod report_support;
        report_support::validator("check-report.schema.json")
    };
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/semantic/invalid/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let path = format!(
            "language/tests/fixtures/semantic/invalid/{}",
            case["file"].as_str().unwrap()
        );
        let output = binary(&["check", &path, "--diagnostic-format", "json"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(schema.is_valid(&report));
        assert!(report["metadata"].is_null());
        assert!(
            report["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == case["code"] && d["location"]["span"] == case["span"])
        );
    }
    let mut count = 0;
    for file in std::fs::read_dir(root().join("examples")).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|e| e != "jky") {
            continue;
        }
        count += 1;
        let label = format!("examples/{}", path.file_name().unwrap().to_str().unwrap());
        for format in ["human", "json"] {
            let output = binary(&["check", &label, "--diagnostic-format", format]);
            assert_eq!(output.status.code(), Some(0), "{label}");
            assert!(output.stderr.is_empty());
            if format == "json" {
                let report: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert!(schema.is_valid(&report));
                assert_eq!(report["status"], "valid");
            } else {
                assert!(output.stdout.is_empty());
            }
        }
    }
    assert_eq!(count, 8);
}
