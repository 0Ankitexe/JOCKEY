use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use jocky_shared::TaskState;
use serde_json::Value;

const CONTRACTS: [&str; 6] = [
    "endpoint-identity",
    "task-envelope",
    "task-status-event",
    "forensic-record",
    "evidence-manifest",
    "build-manifest",
];

const INVALID_EXPECTATIONS: [(&str, &str); 10] = [
    ("endpoint-identity--missing-id.json", "endpoint_id"),
    ("endpoint-identity--malformed-id.json", "endpoint-1"),
    ("task-envelope--missing-id.json", "task_id"),
    (
        "task-envelope--non-utc-timestamp.json",
        "2026-08-30T17:30:00+05:30",
    ),
    (
        "task-envelope--invalid-timestamp.json",
        "2026-99-99T99:99:99Z",
    ),
    ("task-status-event--missing-id.json", "event_id"),
    ("task-status-event--unknown-state.json", "paused"),
    ("forensic-record--missing-id.json", "record_id"),
    ("evidence-manifest--missing-id.json", "manifest_id"),
    ("build-manifest--missing-id.json", "build_id"),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("shared crate must be directly below the repository root")
        .to_path_buf()
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&source)?)
}

fn schema_for(contract: &str) -> Result<Value, Box<dyn Error>> {
    read_json(
        &repository_root()
            .join("shared/schemas")
            .join(format!("{contract}.schema.json")),
    )
}

fn validator_for(schema: &Value) -> Result<jsonschema::Validator, Box<dyn Error>> {
    Ok(jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(schema)?)
}

#[test]
fn every_valid_fixture_satisfies_its_schema() -> Result<(), Box<dyn Error>> {
    for contract in CONTRACTS {
        let schema = schema_for(contract)?;
        let validator = validator_for(&schema)?;
        let fixture = read_json(
            &repository_root()
                .join("fixtures/valid")
                .join(format!("{contract}.json")),
        )?;

        let errors: Vec<String> = validator
            .iter_errors(&fixture)
            .map(|error| error.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "valid {contract} fixture failed: {errors:?}"
        );
    }

    Ok(())
}

#[test]
fn every_invalid_fixture_fails_for_the_expected_reason() -> Result<(), Box<dyn Error>> {
    for (fixture_name, expected_reason) in INVALID_EXPECTATIONS {
        let contract = fixture_name
            .split_once("--")
            .expect("invalid fixtures use <contract>--<reason>.json")
            .0;
        let schema = schema_for(contract)?;
        let validator = validator_for(&schema)?;
        let fixture = read_json(
            &repository_root()
                .join("fixtures/invalid")
                .join(fixture_name),
        )?;

        let errors: Vec<String> = validator
            .iter_errors(&fixture)
            .map(|error| error.to_string())
            .collect();
        assert!(!errors.is_empty(), "invalid fixture {fixture_name} passed");
        assert!(
            errors.iter().any(|error| error.contains(expected_reason)),
            "{fixture_name} did not fail for {expected_reason:?}: {errors:?}"
        );
    }

    Ok(())
}

#[test]
fn rust_task_states_match_the_schema_source_of_truth() -> Result<(), Box<dyn Error>> {
    let schema = schema_for("task-status-event")?;
    let schema_states = schema["properties"]["state"]["enum"]
        .as_array()
        .expect("task state must be an enum")
        .iter()
        .map(|value| value.as_str().expect("task states must be strings"))
        .collect::<Vec<_>>();
    let rust_states = TaskState::ALL.map(TaskState::as_str);

    assert_eq!(schema_states, rust_states);
    Ok(())
}
