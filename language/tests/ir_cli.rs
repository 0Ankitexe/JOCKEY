#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use std::process::{Command, Output};
fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn build_is_silent_verified_and_never_overwrites_or_publishes_invalid_source() {
    let dir = OwnedDir::new("build-cli");
    let source = dir.write("triage.jky", include_bytes!("../../examples/triage.jky"));
    let out = dir.path("triage.json");
    let args = [
        "build-ir".as_ref(),
        source.as_os_str(),
        "--output".as_ref(),
        out.as_os_str(),
    ];
    let result = run(&args);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    let bytes = std::fs::read(&out).unwrap();
    jocky_ir::verify(
        jocky_ir::decode_ir(&bytes).unwrap(),
        jocky_forensic::contracts::builtin_registry().unwrap(),
    )
    .unwrap();
    assert_eq!(run(&args).status.code(), Some(2));
    assert_eq!(std::fs::read(&out).unwrap(), bytes);
    for (index,text) in ["module", "module bad target ubuntu fn main(t: endpoint) -> forensic_result { return missing } run main on selected_endpoints"].iter().enumerate() {
        let input=dir.write(&format!("invalid{index}.jky"),text);let out=dir.path(&format!("invalid{index}.json"));
        let r=run(&["build-ir".as_ref(),input.as_os_str(),"--output".as_ref(),out.as_os_str()]);
        assert_eq!(r.status.code(),Some(1));assert!(!out.exists());assert!(r.stdout.is_empty());assert!(!r.stderr.is_empty());
    }
    assert!(!dir.path(".jocky-ir-stage-0000.tmp").exists());
}

#[test]
fn build_accepts_only_exact_argv_and_no_later_commands() {
    for args in [
        vec!["build-ir"],
        vec!["build-ir", "x"],
        vec!["build-ir", "--output", "x", "y"],
        vec!["build-ir", "x", "--output", "y", "extra"],
        vec!["verify-ir", "x"],
        vec!["run-fixture", "x", "--fixture", "y"],
    ] {
        let r = Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
    }
}

#[test]
fn input_encoding_resource_warnings_and_failure_precedence_are_stable() {
    let dir = OwnedDir::new("build-errors");
    let out = dir.path("out.json");
    for (name, bytes, code) in [
        ("utf8", vec![255], "V3_INPUT"),
        (
            "oversize",
            vec![b' '; jocky_ir::limits::SOURCE_BYTES + 1],
            "V3_RESOURCE",
        ),
    ] {
        let input = dir.write(name, bytes);
        let r = run(&[
            "build-ir".as_ref(),
            input.as_os_str(),
            "--output".as_ref(),
            out.as_os_str(),
        ]);
        assert_eq!(r.status.code(), Some(2));
        assert!(r.stdout.is_empty());
        assert!(String::from_utf8(r.stderr).unwrap().contains(code));
        assert!(!out.exists());
    }
    let source = dir.write(
        "warnings.jky",
        include_bytes!("fixtures/ir/golden/early-return.jky"),
    );
    let r = run(&[
        "build-ir".as_ref(),
        source.as_os_str(),
        "--output".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(r.status.code(), Some(0));
    assert!(r.stdout.is_empty());
    assert!(
        String::from_utf8(r.stderr)
            .unwrap()
            .contains("warning[unreachable-code]")
    );
    let original = std::fs::read(&out).unwrap();
    let invalid=dir.write("invalid.jky","module bad target ubuntu fn main(t: endpoint) -> forensic_result { return missing } run main on selected_endpoints");
    let mut expected = None;
    for _ in 0..10 {
        let r = run(&[
            "build-ir".as_ref(),
            invalid.as_os_str(),
            "--output".as_ref(),
            out.as_os_str(),
        ]);
        assert_eq!(r.status.code(), Some(1));
        assert!(r.stdout.is_empty());
        if let Some(ref e) = expected {
            assert_eq!(&r.stderr, e);
        } else {
            expected = Some(r.stderr);
        }
    }
    assert_eq!(std::fs::read(&out).unwrap(), original);
    let r = run(&[
        "build-ir".as_ref(),
        source.as_os_str(),
        "--output".as_ref(),
        source.as_os_str(),
    ]);
    assert_eq!(r.status.code(), Some(2));
    assert_eq!(
        std::fs::read(source).unwrap(),
        include_bytes!("fixtures/ir/golden/early-return.jky")
    );
}

#[cfg(unix)]
#[test]
fn build_non_utf8_arguments_fail_before_any_file_access() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for args in [
        vec![
            OsString::from("build-ir"),
            OsString::from_vec(vec![255]),
            OsString::from("--output"),
            OsString::from("out"),
        ],
        vec![
            OsString::from("build-ir"),
            OsString::from("source"),
            OsString::from("--output"),
            OsString::from_vec(vec![255]),
        ],
    ] {
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
