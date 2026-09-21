use jocky_forensic::contracts::builtin_registry;
mod support;
use jocky_language::{SourceFile, report::render_failure, semantic::analyze};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn binary(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

#[test]
fn human_binary_checks_matching_opposite_dual_branch_and_dead_platform_cases() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/semantic/platforms/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let path = format!(
            "language/tests/fixtures/semantic/platforms/{}",
            case["file"].as_str().unwrap()
        );
        let text = std::fs::read_to_string(root().join(&path)).unwrap();
        let valid = case["errors"].as_array().unwrap().is_empty();
        let expected = if valid {
            String::new()
        } else {
            render_failure(
                SourceFile::new(&path, &text),
                &analyze(SourceFile::new(&path, &text), builtin_registry().unwrap()).unwrap_err(),
            )
            .unwrap()
        };
        for args in [
            vec!["check", path.as_str()],
            vec!["check", path.as_str(), "--diagnostic-format", "human"],
        ] {
            let output = binary(&args);
            assert_eq!(
                output.status.code(),
                Some(if valid { 0 } else { 1 }),
                "{path}"
            );
            assert!(output.stdout.is_empty());
            assert_eq!(output.stderr, expected.as_bytes(), "{path}");
            if !valid {
                assert!(expected.contains(&format!("--> {path}:")));
            }
        }
        let output = binary(&["check", &path, "--diagnostic-format", "json"]);
        assert_eq!(output.status.code(), Some(if valid { 0 } else { 1 }));
        assert!(output.stderr.is_empty());
        let report = support::assert_report(&output.stdout);
        assert_eq!(report["status"], if valid { "valid" } else { "invalid" });
        for error in case["errors"].as_array().unwrap() {
            assert!(
                report["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["code"] == error["code"] && d["location"]["span"] == error["span"])
            );
        }
    }
}

#[test]
fn actual_binary_accepts_profile_syntax_but_never_ships_a_lab_operation() {
    let output = binary(&["check", "language/tests/fixtures/profile/valid.jky"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    for file in [
        "unknown-profile.jky",
        "repeated-profile.jky",
        "misplaced-profile.jky",
    ] {
        let path = format!("language/tests/fixtures/semantic/lab/{file}");
        let output = binary(&["check", &path]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("error[invalid-profile-declaration]")
        );
    }
    for file in ["lab-without-profile.jky", "opted-in.jky"] {
        let path = format!("language/tests/fixtures/semantic/lab/{file}");
        let output = binary(&["check", &path]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("error[unknown-name]: unknown name: forensic.lab.inspect"));
        assert!(!stderr.contains("lab-profile-required"));
    }
}
