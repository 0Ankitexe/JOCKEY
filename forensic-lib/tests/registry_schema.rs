use jocky_forensic::contracts::{Registry, RegistryErrorKind};
use serde_json::Value;
use std::{fs, path::Path};

#[test]
fn valid_and_invalid_fixtures_fail_in_the_expected_layer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cases: Value =
        serde_json::from_str(&fs::read_to_string(root.join("schema-cases.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let input = fs::read_to_string(root.join(case["file"].as_str().unwrap())).unwrap();
        match case["error"].as_str() {
            None => {
                Registry::from_json(&input).unwrap();
            }
            Some(kind) => {
                let error = Registry::from_json(&input).unwrap_err();
                assert_eq!(format!("{:?}", error.kind), kind, "{}", case["file"]);
                assert!(!error.reason.is_empty());
            }
        }
    }
}

#[test]
fn schema_failures_never_yield_a_partial_catalogue() {
    let mut input: Value =
        serde_json::from_str(include_str!("../contracts/catalogue.v1.json")).unwrap();
    input["functions"][9]["availability"] = Value::String("implemented".into());
    assert_eq!(
        Registry::from_json(&input.to_string()).unwrap_err().kind,
        RegistryErrorKind::SchemaViolation
    );
}
