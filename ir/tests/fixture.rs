mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, prepare};
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/valid/ubuntu/fixture.json")).unwrap()
}
fn decode(v: &Value) -> Result<jocky_ir::ValidatedFixture, jocky_ir::FixtureFailure> {
    decode_fixture(&serde_json::to_vec(v).unwrap(), &ExecutionLimits::default())
}

#[test]
fn both_complete_platforms_validate_and_missing_data_is_not_empty_data() {
    for bytes in [
        include_bytes!("fixtures/valid/ubuntu/fixture.json").as_slice(),
        include_bytes!("fixtures/valid/windows/fixture.json").as_slice(),
    ] {
        decode_fixture(bytes, &ExecutionLimits::default()).unwrap();
    }
    let mut empty = fixture();
    for pointer in [
        "/providers/forensic.process.list/value/values",
        "/providers/forensic.network.connections/value/values",
        "/providers/forensic.indicator.correlate/expected/processes/values",
        "/providers/forensic.indicator.correlate/expected/connections/values",
        "/providers/forensic.indicator.correlate/outcome/value/value/indicators",
    ] {
        *empty.pointer_mut(pointer).unwrap() = json!([]);
    }
    decode(&empty).unwrap();
    for key in [
        "forensic.system.profile",
        "forensic.process.list",
        "forensic.network.connections",
        "forensic.indicator.correlate",
    ] {
        let mut v = fixture();
        v["providers"].as_object_mut().unwrap().remove(key);
        assert!(decode(&v).is_err(), "{key}");
    }
}

#[test]
fn unused_outcomes_still_require_context_types_references_and_declared_errors() {
    let edits = [
        ("/schema_version", json!("9.0.0")),
        ("/endpoint/platform", json!("windows")),
        ("/observed_at", json!("2026-09-19T00:00:00Z")),
        (
            "/providers/forensic.system.profile/value/value/system",
            Value::Null,
        ),
        (
            "/providers/forensic.process.list/value/element_type/name",
            json!("bool"),
        ),
        (
            "/providers/forensic.network.connections/value/values/0/value/process_record_id",
            json!("00000000-0000-4000-8000-000000000099"),
        ),
        (
            "/providers/forensic.indicator.correlate/outcome/value/value/indicators/0/record_ids/0",
            json!("00000000-0000-4000-8000-000000000099"),
        ),
        (
            "/providers/forensic.indicator.correlate/expected/processes/values/0/value/name",
            json!("conflicting record"),
        ),
    ];
    for (path, value) in edits {
        let mut v = fixture();
        *v.pointer_mut(path).unwrap() = value;
        assert!(decode(&v).is_err(), "{path}");
    }
    let mut v = fixture();
    let p = &mut v["providers"]["forensic.process.list"]["value"]["values"];
    let duplicate = p[0].clone();
    p.as_array_mut().unwrap().push(duplicate);
    assert!(decode(&v).is_err());
    for slot in [
        "forensic.system.profile",
        "forensic.process.list",
        "forensic.network.connections",
    ] {
        for code in [
            "Unsupported",
            "PermissionDenied",
            "ResourceLimit",
            "CollectionFailed",
        ] {
            let mut v = fixture();
            v["providers"][slot] = json!({"status":"error","code":code});
            decode(&v).unwrap();
        }
        let mut v = fixture();
        v["providers"][slot] = json!({"status":"error","code":"InvalidInput"});
        assert!(decode(&v).is_err());
    }
    for code in ["NotImplemented", "InvalidInput", "ResourceLimit"] {
        let mut v = fixture();
        v["providers"]["forensic.indicator.correlate"]["outcome"] =
            json!({"status":"error","code":code});
        decode(&v).unwrap();
    }
}

#[test]
fn versions_json_and_effective_collection_limits_fail_before_preparation() {
    for bytes in [
        b"{".as_slice(),
        b"{} {}",
        b"{\"schema_version\":\"1.0.0\",\"schema_version\":\"1.0.0\"}",
        b"\xff",
    ] {
        assert!(decode_fixture(bytes, &ExecutionLimits::default()).is_err());
    }
    let mut v = fixture();
    v["include"] = json!("never-open");
    assert!(decode(&v).is_err());
    assert!(ExecutionLimits::new(0, 1, 1, 1).is_err());
    assert!(ExecutionLimits::new(1, 10_001, 1, 1).is_err());
    assert!(ExecutionLimits::new(1, 1, 10_000_001, 1).is_err());
    assert!(ExecutionLimits::new(1, 1, 1, 257).is_err());
    let fixture = decode(&fixture()).unwrap();
    let mut document = support::document();
    document.targets = vec![jocky_shared::compiler::Platform::Windows];
    assert!(
        prepare(
            &support::verify(document).unwrap(),
            &fixture,
            &ExecutionLimits::default()
        )
        .is_err()
    );
}

#[test]
fn stricter_preparation_rechecks_collections_and_returns_no_prepared_invocation() {
    // The authored indicator refers to two records. All lists are still checked,
    // including the unused provider data of this constant-only program.
    let wide = ExecutionLimits::default();
    let narrow = ExecutionLimits::new(100, 1, 100_000, 10).unwrap();
    let bytes = serde_json::to_vec(&fixture()).unwrap();
    assert!(matches!(
        decode_fixture(&bytes, &narrow),
        Err(jocky_ir::FixtureFailure::Resource)
    ));
    let fixture = decode_fixture(&bytes, &wide).unwrap();
    assert_eq!(fixture.fixture_id(), "00000000-0000-4000-8000-000000000002");
    assert_eq!(fixture.observed_at(), "2026-09-18T00:00:00Z");
    let program = support::verify(support::document()).unwrap();
    assert!(matches!(
        prepare(&program, &fixture, &narrow),
        Err(jocky_ir::FixtureFailure::Resource)
    ));
    assert!(prepare(&program, &fixture, &wide).is_ok());
}
