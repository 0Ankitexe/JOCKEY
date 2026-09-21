mod support;

use jsonschema::error::ValidationErrorKind;
use serde_json::{Value, json};

#[test]
fn all_deployed_wire_examples_validate_offline() {
    for (schema, text) in [
        (
            "ir.schema.json",
            include_str!("fixtures/valid/hand-authored.ir.json"),
        ),
        (
            "fixture-bundle.schema.json",
            include_str!("fixtures/valid/ubuntu/fixture.json"),
        ),
        (
            "fixture-bundle.schema.json",
            include_str!("fixtures/valid/windows/fixture.json"),
        ),
        (
            "fixture-values.schema.json",
            include_str!("fixtures/valid/ubuntu/expected-result.json"),
        ),
        (
            "fixture-values.schema.json",
            include_str!("fixtures/valid/windows/expected-result.json"),
        ),
        (
            "fixture-report.schema.json",
            include_str!("fixtures/valid/success.report.json"),
        ),
        (
            "fixture-report.schema.json",
            include_str!("fixtures/valid/failure.report.json"),
        ),
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        assert!(support::validator(schema).is_valid(&value), "{schema}");
    }
}

#[test]
fn negative_manifest_rejects_at_the_expected_validator_and_pointer() {
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/invalid/schema/manifest.json")).unwrap();
    let sources = [
        (
            "missing-endpoint.fixture.json",
            include_str!("fixtures/invalid/schema/missing-endpoint.fixture.json"),
        ),
        (
            "unknown-value.json",
            include_str!("fixtures/invalid/schema/unknown-value.json"),
        ),
        (
            "unknown-opcode.ir.json",
            include_str!("fixtures/invalid/schema/unknown-opcode.ir.json"),
        ),
        (
            "partial-success.report.json",
            include_str!("fixtures/invalid/schema/partial-success.report.json"),
        ),
    ];
    for case in manifest.as_array().unwrap() {
        let text = sources
            .iter()
            .find(|(name, _)| *name == case["file"])
            .unwrap()
            .1;
        let value: Value = serde_json::from_str(text).unwrap();
        let errors: Vec<_> = support::validator(case["schema"].as_str().unwrap())
            .iter_errors(&value)
            .collect();
        assert!(
            errors.iter().any(|error| {
                error.instance_path.to_string() == case["path"].as_str().unwrap()
                    && matches!(
                        (&error.kind, case["validator"].as_str().unwrap()),
                        (ValidationErrorKind::Required { .. }, "required")
                            | (ValidationErrorKind::OneOfNotValid { .. }, "oneOf")
                    )
            }),
            "{}: {errors:?}",
            case["file"]
        );
    }
}

#[test]
fn fields_enums_formats_identifiers_and_declared_errors_are_closed() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/valid/ubuntu/fixture.json")).unwrap();
    for (pointer, bad) in [
        ("/endpoint/endpoint_id", json!("missing")),
        ("/endpoint/platform", json!("macos")),
        ("/observed_at", json!("2026-02-30T00:00:00Z")),
        ("/observed_at", json!("2026-09-18T00:00:00+00:00")),
        (
            "/providers/forensic.system.profile",
            json!({"status":"error","code":"InvalidInput"}),
        ),
    ] {
        let mut bad_fixture = fixture.clone();
        *bad_fixture.pointer_mut(pointer).unwrap() = bad;
        assert!(
            !support::validator("fixture-bundle.schema.json").is_valid(&bad_fixture),
            "{pointer}"
        );
    }
    let mut unknown = fixture;
    unknown["include"] = json!("https://example.invalid/fixture.json");
    assert!(!support::validator("fixture-bundle.schema.json").is_valid(&unknown));
    let external = json!({"$ref":"https://example.invalid/schema.json"});
    assert!(
        jsonschema::draft202012::options()
            .with_retriever(support::Deny)
            .build(&external)
            .is_err()
    );
}

#[test]
fn strict_ir_decode_rejects_duplicate_trailing_version_and_encoding() {
    for input in [b"{\"a\":1,\"a\":2}".as_slice(), b"{} {}", &[0xff], b"{"] {
        assert!(jocky_ir::decode_ir(input).is_err());
    }
    let mut unknown = support::raw();
    unknown["schema_version"] = json!("99.0.0");
    assert_eq!(
        jocky_ir::decode_ir(&serde_json::to_vec(&unknown).unwrap())
            .unwrap_err()
            .code(),
        jocky_ir::DiagnosticCode::Version
    );
    let mut missing = support::raw();
    missing.as_object_mut().unwrap().remove("entry");
    assert!(jocky_ir::decode_ir(&serde_json::to_vec(&missing).unwrap()).is_err());
    let valid = support::verify(support::document()).unwrap();
    let bytes = valid.to_json().unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(
        support::verify(jocky_ir::decode_ir(&bytes).unwrap())
            .unwrap()
            .to_json()
            .unwrap(),
        bytes
    );
}
#[test]
fn verified_serialization_remains_closed_and_reverifies_without_source() {
    let verified = support::verify(support::document()).unwrap();
    let bytes = verified.to_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(support::validator("ir.schema.json").is_valid(&parsed));
    assert_eq!(
        support::verify(jocky_ir::decode_ir(&bytes).unwrap())
            .unwrap()
            .to_json()
            .unwrap(),
        bytes
    );
    let mut extra = parsed;
    extra["include"] = serde_json::json!("never-open");
    assert_eq!(
        jocky_ir::decode_ir(&serde_json::to_vec(&extra).unwrap())
            .unwrap_err()
            .code(),
        jocky_ir::DiagnosticCode::Schema
    );
}
