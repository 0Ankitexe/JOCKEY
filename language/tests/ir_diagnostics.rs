use std::{path::Path, process::Command};
#[test]
fn standalone_diagnostics_are_source_honest_snapshots_repeated_ten_times() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for name in ["wrong-id", "span-outside-source", "argument-count"] {
        let expected =
            std::fs::read(root.join(format!("language/tests/fixtures/ir/diagnostics/{name}.txt")))
                .unwrap();
        for _ in 0..10 {
            let result = Command::new(env!("CARGO_BIN_EXE_jockyc"))
                .current_dir(root)
                .args([
                    "verify-ir",
                    &format!("ir/tests/fixtures/invalid/verifier/{name}.ir.json"),
                ])
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(1));
            assert!(result.stdout.is_empty());
            assert_eq!(result.stderr, expected);
        }
    }
}
