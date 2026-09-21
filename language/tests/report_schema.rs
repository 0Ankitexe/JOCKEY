mod support;
use serde_json::{Value, json};

#[test]
fn report_fixtures_and_required_closed_fields_follow_the_deployed_schema() {
    let validator = support::validator("check-report.schema.json");
    let root = support::root().join("language/tests/fixtures/reports");
    for (directory, valid) in [("valid", true), ("invalid", false)] {
        for file in std::fs::read_dir(root.join(directory)).unwrap() {
            let file = file.unwrap().path();
            let value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            assert_eq!(validator.is_valid(&value), valid, "{}", file.display());
            if !valid {
                let expected_pointer = match file.file_stem().unwrap().to_str().unwrap() {
                    "valid-without-metadata" => "/metadata",
                    "invalid-without-errors" | "failure-with-source-only-error" => "/diagnostics",
                    "unknown-status" => "/status",
                    "source-without-location" | "host-with-fake-location" => {
                        "/diagnostics/0/location"
                    }
                    "wrong-severity" => "/diagnostics/0/severity",
                    name => panic!("fixture missing an expected reason: {name}"),
                };
                assert!(
                    validator
                        .iter_errors(&value)
                        .any(|error| error.instance_path.to_string() == expected_pointer),
                    "{} should fail at {expected_pointer}",
                    file.display()
                );
            }
        }
    }
    let valid: Value =
        serde_json::from_str(include_str!("fixtures/reports/valid/valid.json")).unwrap();
    for key in valid.as_object().unwrap().keys() {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(!validator.is_valid(&missing), "missing {key}");
    }
    for pointer in [
        "",
        "/truncated",
        "/metadata",
        "/metadata/entry",
        "/metadata/functions/0",
    ] {
        let mut extra = valid.clone();
        extra
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(!validator.is_valid(&extra), "extra field {pointer}");
    }
}

#[test]
fn schema_retrieval_is_denied_and_status_severity_location_relations_are_enforced() {
    for uri in [
        "https://unregistered.invalid/schema",
        "file:///unregistered-schema",
        "urn:unregistered",
    ] {
        assert!(
            jsonschema::draft202012::options()
                .with_retriever(support::DenyRetrieval)
                .build(&json!({"$ref":uri}))
                .is_err()
        );
    }
    let validator = support::validator("check-report.schema.json");
    let base: Value =
        serde_json::from_str(include_str!("fixtures/reports/valid/invalid.json")).unwrap();
    for (pointer, value) in [
        ("/status", json!("unknown")),
        ("/metadata", json!({})),
        ("/diagnostics/0/severity", json!("warning")),
        ("/diagnostics/0/code", json!("made-up")),
        ("/diagnostics/0/location", Value::Null),
        ("/diagnostics/0/label", Value::Null),
        ("/diagnostics/0/location/start/line", json!(0)),
        ("/diagnostics/0/location/marker_width", json!(0)),
        ("/file", Value::Null),
        ("/schema_version", json!("2.0.0")),
    ] {
        let mut invalid = base.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(!validator.is_valid(&invalid), "{pointer}");
    }
}
