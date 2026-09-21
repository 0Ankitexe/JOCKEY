mod support;
use jocky_language::{SourceFile, check, parse, parse_source, render_diagnostics};

#[test]
fn every_original_ast_oracle_and_binary_parse_mode_is_unchanged() {
    let directory = support::root().join("language/tests/fixtures/valid");
    let schema = support::validator("ast-v1.schema.json");
    let mut count = 0;
    for file in std::fs::read_dir(directory).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|e| e != "jky") {
            continue;
        }
        count += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let label = path
            .strip_prefix(support::root())
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let source = SourceFile::new(&label, &text);
        let parsed = parse_source(source).unwrap();
        assert_eq!(parsed.program, parse(source).unwrap());
        check(source).unwrap(); // Deliberately remains syntax-only even for anomalies.
        let expected = std::fs::read(path.with_extension("ast.json")).unwrap();
        assert_eq!(parsed.to_pretty_json().unwrap(), expected);
        let binary = support::binary(&["parse", &label, "--emit-ast", "json"]);
        assert_eq!(binary.status.code(), Some(0));
        assert_eq!(binary.stdout, expected);
        assert!(binary.stderr.is_empty());
        let value = serde_json::from_slice(&binary.stdout).unwrap();
        assert!(schema.is_valid(&value));
        let plain = support::binary(&["parse", &label]);
        assert_eq!(plain.status.code(), Some(0));
        assert_eq!(plain.stdout, format!("parsed: {label}\n").as_bytes());
    }
    assert_eq!(count, 19);
}
#[test]
fn original_syntax_failures_remain_exact_in_both_parse_entries() {
    let mut count = 0;
    for file in std::fs::read_dir(support::root().join("language/tests/fixtures/invalid")).unwrap()
    {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|e| e != "jky") {
            continue;
        }
        count += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let label = path
            .strip_prefix(support::root())
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let source = SourceFile::new(&label, &text);
        let old = parse(source).unwrap_err();
        let new = parse_source(source).unwrap_err();
        assert_eq!(old, new);
        assert_eq!(
            render_diagnostics(source, &new).as_bytes(),
            std::fs::read(path.with_extension("stderr")).unwrap()
        );
        for args in [
            vec!["parse", label.as_str()],
            vec!["parse", label.as_str(), "--emit-ast", "json"],
        ] {
            let output = support::binary(&args);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert_eq!(output.stderr, render_diagnostics(source, &old).as_bytes());
        }
    }
    assert_eq!(count, 20);
}
#[test]
fn profile_ast_uses_its_local_versioned_schema_and_legacy_api_still_rejects_it() {
    let path = "language/tests/fixtures/profile/valid.jky";
    let output = support::binary(&["parse", path, "--emit-ast", "json"]);
    assert_eq!(output.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(support::validator("profile-ast.schema.json").is_valid(&value));
    assert!(!support::validator("ast-v1.schema.json").is_valid(&value));
    let text = std::fs::read_to_string(support::root().join(path)).unwrap();
    assert!(check(SourceFile::new(path, &text)).is_err());
}
