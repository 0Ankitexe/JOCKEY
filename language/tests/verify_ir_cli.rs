#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

fn run(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args(args)
        .output()
        .unwrap()
}
fn verify(path: &Path) -> Output {
    run(&["verify-ir".as_ref(), path.as_os_str()])
}

#[test]
fn standalone_verification_reads_only_artifact_and_keeps_files_unchanged() {
    let dir = OwnedDir::new("verify-valid");
    for (name, bytes) in [
        (
            "hand.json",
            include_bytes!("../../ir/tests/fixtures/valid/hand-authored.ir.json").as_slice(),
        ),
        (
            "triage.json",
            include_bytes!("fixtures/ir/golden/triage.ir.json").as_slice(),
        ),
    ] {
        let mut document = jocky_ir::decode_ir(bytes).unwrap();
        document.source.label = "missing/source/never-open.jky".into();
        jocky_ir::identity::assign_instruction_ids(&mut document).unwrap();
        let bytes = serde_json::to_vec(&document).unwrap();
        let path = dir.write(name, &bytes);
        let out = verify(&path);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 2);
}

#[test]
fn every_frozen_rejection_has_correct_exit_and_no_output_document() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("ir/tests/fixtures/invalid/verifier");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for case in manifest.as_array().unwrap() {
        let out = verify(&root.join(case["file"].as_str().unwrap()));
        let code = case["code"].as_str().unwrap();
        assert_eq!(
            out.status.code(),
            Some(if matches!(code, "IR_VERSION" | "IR_RESOURCE") {
                2
            } else {
                1
            }),
            "{case}"
        );
        assert!(out.stdout.is_empty());
        let err = String::from_utf8(out.stderr).unwrap();
        let expected = match code {
            "IR_VERSION" => "V3_VERSION",
            "IR_RESOURCE" => "V3_RESOURCE",
            other => other,
        };
        assert!(err.contains(expected), "{case}: {err}");
        assert!(!err.contains("never-read"));
    }
}

#[test]
fn exact_arguments_encoding_size_and_version_precedence() {
    for args in [
        vec!["verify-ir"],
        vec!["verify-ir", "--help"],
        vec!["verify-ir", "x", "extra"],
        vec!["verify-ir", "x", "--diagnostic-format", "json"],
        vec!["verify-ir", ""],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(out.stderr.starts_with(b"Usage:"));
    }
    let dir = OwnedDir::new("verify-input");
    assert_eq!(verify(&dir.path("missing")).status.code(), Some(2));
    for (name, bytes, exit, code) in [
        ("utf8", vec![255], 2, "V3_INPUT"),
        (
            "oversize",
            vec![b' '; jocky_ir::limits::IR_BYTES + 1],
            2,
            "V3_RESOURCE",
        ),
        (
            "version",
            br#"{"schema_version":"9.0.0"}"#.to_vec(),
            2,
            "V3_VERSION",
        ),
        (
            "trailing",
            br#"{"schema_version":"9.0.0"} trailing"#.to_vec(),
            1,
            "IR_JSON",
        ),
    ] {
        let out = verify(&dir.write(name, bytes));
        assert_eq!(out.status.code(), Some(exit));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8(out.stderr).unwrap().contains(code));
    }
}

#[cfg(unix)]
#[test]
fn invalid_utf8_argument_is_rejected_before_io() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let arg = OsString::from_vec(vec![255]);
    let out = run(&["verify-ir".as_ref(), &arg]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("invalid-path-encoding")
    );
}

#[test]
fn old_source_reader_is_not_used_and_failed_diagnostic_delivery_is_exit_two() {
    struct NoSource;
    impl jocky_language::cli::SourceReader for NoSource {
        fn read(&mut self, _: &Path) -> std::io::Result<Vec<u8>> {
            panic!("standalone verification read source")
        }
    }
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("ir/tests/fixtures/invalid/verifier/wrong-id.ir.json");
    let mut out = Vec::new();
    assert_eq!(
        jocky_language::cli::run_with_reader(
            [std::ffi::OsString::from("verify-ir"), path.into_os_string()],
            &mut NoSource,
            &mut out,
            &mut Broken
        )
        .exit_code(),
        2
    );
    assert!(out.is_empty());
}
