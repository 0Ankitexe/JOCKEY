use jocky_language::{AstDocument, SourceFile, parse, parse_source};
use serde_json::Value;

fn source(profile: &str) -> String {
    format!(
        "module demo\ntarget ubuntu\n{profile}\nfn inspect(target: endpoint) -> forensic_result {{ return forensic.system.profile(target) }}\nrun inspect on selected_endpoints\n"
    )
}

#[test]
fn profile_spans_and_versioned_ast_keep_the_original_source() {
    for text in [
        source("profile lab"),
        source("profile /* comment */ lab").replace('\n', "\r\n"),
    ] {
        let parsed = parse_source(SourceFile::new("never-read.jky", &text)).unwrap();
        let profile = parsed.profile.as_ref().unwrap();
        assert_eq!(&text[profile.name_span.start..profile.name_span.end], "lab");
        assert_eq!(&text[profile.span.start..profile.span.start + 7], "profile");
        assert_eq!(parsed.program.span.end, text.len());
        let ast: Value = serde_json::from_slice(&parsed.to_pretty_json().unwrap()).unwrap();
        assert_eq!(ast["schema_version"], "2.0.0");
        assert!(parse(SourceFile::new("old-api", &text)).is_err());
    }
}

#[test]
fn contextual_words_comments_and_old_documents_are_unchanged() {
    for text in [
        source("// profile lab"),
        source("/* profile lab */"),
        source("")
            .replace("inspect", "profile")
            .replace("target: endpoint", "lab: endpoint")
            .replace("profile(target)", "profile(lab)"),
    ] {
        let parsed = parse_source(SourceFile::new("x", &text)).unwrap();
        assert!(parsed.profile.is_none());
        let old = parse(SourceFile::new("x", &text)).unwrap();
        assert_eq!(
            parsed.to_pretty_json().unwrap(),
            AstDocument::new(old).to_pretty_json().unwrap()
        );
    }
}

#[test]
fn bad_profiles_are_not_masked_and_recovery_finds_independent_errors() {
    for profile in [
        "profile standard",
        "profile",
        "profile lab profile lab",
        "profile @ lab",
        "profilelab",
        "profile lab @",
    ] {
        assert!(
            parse_source(SourceFile::new("bad.jky", &source(profile))).is_err(),
            "{profile}"
        );
    }
    let text = source("profile standard")
        .replace("target ubuntu", "target macos")
        .replace("return forensic.system.profile(target)", "return");
    let errors = parse_source(SourceFile::new("bad.jky", &text)).unwrap_err();
    assert!(
        errors
            .items()
            .iter()
            .any(|e| e.category.as_str() == "invalid-profile-declaration")
    );
    assert!(errors.len() > 1);
    for text in [
        source("").replace("target ubuntu", "profile lab target ubuntu"),
        format!("{}profile lab", source("")),
    ] {
        assert!(parse_source(SourceFile::new("bad", &text)).is_err());
    }
}

#[test]
fn deployed_ast_schemas_and_frozen_profile_fixture_match() {
    let text = include_str!("fixtures/profile/valid.jky");
    let parsed = parse_source(SourceFile::new("profile.jky", text)).unwrap();
    let emitted = parsed.to_pretty_json().unwrap();
    assert_eq!(emitted.last(), Some(&b'\n'));
    let actual: Value = serde_json::from_slice(&emitted).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/profile/valid.ast.json")).unwrap();
    assert_eq!(actual, expected);
    let original: Value = serde_json::from_str(include_str!(
        "../../shared/compiler-contracts/ast-v1.schema.json"
    ))
    .unwrap();
    let types: Value = serde_json::from_str(include_str!(
        "../../shared/compiler-contracts/compiler-types.schema.json"
    ))
    .unwrap();
    let profile: Value = serde_json::from_str(include_str!(
        "../../shared/compiler-contracts/profile-ast.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::draft202012::options()
        .with_resource(
            "https://jocky.dev/language/v1/ast-json.schema.json",
            jsonschema::Resource::from_contents(original.clone()).unwrap(),
        )
        .with_resource(
            "https://jocky.dev/compiler/v1/types.schema.json",
            jsonschema::Resource::from_contents(types).unwrap(),
        )
        .build(&profile)
        .unwrap();
    assert!(validator.is_valid(&actual));
    assert!(
        !jsonschema::validator_for(&original)
            .unwrap()
            .is_valid(&actual)
    );
    let mut invalid = actual;
    invalid["profile"]["name"] = serde_json::json!("standard");
    assert!(!validator.is_valid(&invalid));
}
