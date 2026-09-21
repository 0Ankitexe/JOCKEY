mod common;

use std::error::Error;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output};
use std::time::{Duration, Instant};

use jocky_language::cli::{
    CliError, ProcessOutcome, SourceReader, USAGE, args_os, run_with_reader,
};
use serde_json::Value;

fn run_binary(arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(ProcessCommand::new(env!("CARGO_BIN_EXE_jockyc"))
        .current_dir(common::repository_root())
        .args(arguments)
        .output()?)
}

#[test]
fn actual_binary_supports_the_three_success_modes() -> Result<(), Box<dyn Error>> {
    let path = "language/tests/fixtures/valid/triage.jky";

    let parsed = run_binary(&["parse", path])?;
    assert_eq!(parsed.status.code(), Some(0));
    assert_eq!(parsed.stdout, format!("parsed: {path}\n").as_bytes());
    assert!(parsed.stderr.is_empty());

    let checked = run_binary(&["check", path])?;
    assert_eq!(checked.status.code(), Some(0));
    assert!(checked.stdout.is_empty());
    assert!(checked.stderr.is_empty());

    let emitted = run_binary(&["parse", path, "--emit-ast", "json"])?;
    assert_eq!(emitted.status.code(), Some(0));
    assert!(emitted.stderr.is_empty());
    assert!(emitted.stdout.ends_with(b"\n"));
    let value: Value = serde_json::from_slice(&emitted.stdout)?;
    assert_eq!(value["schema_version"], "1.0.0");
    Ok(())
}

#[test]
fn invalid_argv_is_exact_usage_and_exit_two() -> Result<(), Box<dyn Error>> {
    assert!(USAGE.ends_with("  jockyc run-fixture <file> --fixture <fixture-directory>\n"));
    for arguments in [
        vec![],
        vec!["parse"],
        vec!["check", "file", "extra"],
        vec!["parse", "file", "json", "--emit-ast"],
        vec!["parse", "file", "--emit-ast", "json", "json"],
        vec!["execute", "file"],
    ] {
        let output = run_binary(&arguments)?;
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, USAGE.as_bytes(), "{arguments:?}");
    }
    Ok(())
}

#[test]
fn every_invalid_fixture_fails_all_modes_without_ast_output() -> Result<(), Box<dyn Error>> {
    for pair in common::discover_pairs("invalid", "stderr")? {
        let label = common::fixture_label(&pair.source)?;
        let expected = std::fs::read(&pair.oracle)?;
        for arguments in [
            vec!["parse", label.as_str()],
            vec!["parse", label.as_str(), "--emit-ast", "json"],
            vec!["check", label.as_str()],
        ] {
            let output = run_binary(&arguments)?;
            assert_eq!(output.status.code(), Some(1), "{} {arguments:?}", pair.name);
            assert!(
                output.stdout.is_empty(),
                "{} emitted false success",
                pair.name
            );
            assert_eq!(output.stderr, expected, "{} {arguments:?}", pair.name);
        }
    }
    Ok(())
}

#[test]
fn actual_input_failures_are_stable_and_hide_native_errors() -> Result<(), Box<dyn Error>> {
    let missing = run_binary(&["check", "does-not-exist.jky"])?;
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    assert_eq!(
        missing.stderr,
        b"error[input-not-found]: input file not found: does-not-exist.jky\n"
    );

    let directory = run_binary(&["check", "language"])?;
    assert_eq!(directory.status.code(), Some(2));
    assert!(directory.stdout.is_empty());
    assert_eq!(
        directory.stderr,
        b"error[input-unreadable]: input file is not readable: language\n"
    );

    let invalid_path =
        std::env::temp_dir().join(format!("jocky-invalid-utf8-{}.jky", std::process::id()));
    std::fs::write(&invalid_path, [0xff, 0xfe, 0x00])?;
    let invalid_label = invalid_path.to_str().expect("temporary path is UTF-8");
    let invalid = run_binary(&["check", invalid_label])?;
    std::fs::remove_file(&invalid_path)?;
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8(invalid.stderr)?.starts_with("error[invalid-utf8]:"));
    Ok(())
}

struct MemoryReader(Result<Vec<u8>, io::ErrorKind>);

impl SourceReader for MemoryReader {
    fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
        match &self.0 {
            Ok(bytes) => Ok(bytes.clone()),
            Err(kind) => Err(io::Error::from(*kind)),
        }
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn injected_resource_reader_and_output_failures_use_exit_two() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let outcome = run_with_reader(
        [OsString::from("check"), OsString::from("deep.jky")],
        &mut MemoryReader(Ok("(".repeat(257).into_bytes())),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(outcome, ProcessOutcome::CommandOrHostFailure);
    assert_eq!(outcome.exit_code(), 2);
    assert!(stdout.is_empty());
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("error[resource-limit]")
    );

    let valid = b"module m\ntarget windows\nfn f() -> r {}\n".to_vec();
    let mut stderr = Vec::new();
    let output_outcome = run_with_reader(
        [OsString::from("parse"), OsString::from("memory.jky")],
        &mut MemoryReader(Ok(valid)),
        &mut FailingWriter,
        &mut stderr,
    );
    assert_eq!(output_outcome.exit_code(), 2);
    assert_eq!(
        stderr,
        b"error[output-failure]: output could not be written\n"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_are_rejected_before_reading() {
    use std::os::unix::ffi::OsStringExt;

    let path = OsString::from_vec(vec![b'f', 0xff]);
    assert_eq!(
        args_os([OsString::from("check"), path]),
        Err(CliError::InvalidPathEncoding)
    );
}

#[test]
fn every_distributed_example_passes_all_modes_within_one_second() -> Result<(), Box<dyn Error>> {
    let root = common::repository_root();
    let mut examples = std::fs::read_dir(root.join("examples"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    examples.sort();
    examples.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("jky"));
    assert!(examples.len() >= 5);

    let schema = common::ast_schema()?;
    let validator = jsonschema::draft202012::options().build(&schema)?;
    for path in examples {
        let label = path
            .strip_prefix(&root)?
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let started = Instant::now();
        let parse_output = run_binary(&["parse", &label])?;
        assert_eq!(parse_output.status.code(), Some(0), "{label}");
        let check_output = run_binary(&["check", &label])?;
        assert_eq!(check_output.status.code(), Some(0), "{label}");
        let json_output = run_binary(&["parse", &label, "--emit-ast", "json"])?;
        assert_eq!(json_output.status.code(), Some(0), "{label}");
        let value: Value = serde_json::from_slice(&json_output.stdout)?;
        let errors = validator
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{label}: {errors:?}");
        assert!(started.elapsed() < Duration::from_secs(1), "{label}");
    }
    Ok(())
}
