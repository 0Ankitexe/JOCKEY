mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, decode_ir, execute, prepare};
#[test]
fn reports_are_closed_exclusive_context_consistent_and_lf_terminated() {
    let p = support::verify(
        decode_ir(include_bytes!(
            "../../language/tests/fixtures/ir/golden/triage.ir.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let f = decode_fixture(
        include_bytes!("fixtures/valid/ubuntu/fixture.json"),
        &ExecutionLimits::default(),
    )
    .unwrap();
    for limits in [
        ExecutionLimits::default(),
        ExecutionLimits::new(1, 10_000, 1_000_000, 128).unwrap(),
    ] {
        let r = execute(prepare(&p, &f, &limits).unwrap());
        let bytes = r.to_json().unwrap();
        assert!(bytes.ends_with(b"\n"));
        let mut raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(support::validator("fixture-report.schema.json").is_valid(&raw));
        assert_eq!(raw["endpoint_id"], f.endpoint().endpoint_id);
        assert_eq!(raw["fixture_id"], f.fixture_id());
        assert!(
            raw["accounting"]["instructions"].as_u64().unwrap()
                <= raw["limits"]["instructions"].as_u64().unwrap()
        );
        let mut mixed = raw.clone();
        if mixed["outcome"]["status"] == "success" {
            mixed["outcome"]["error"] = serde_json::json!({"category":"interpreter","code":"EXEC_WORK_LIMIT","origin":null});
        } else {
            mixed["outcome"]["value"] = serde_json::json!({"kind":"bool","value":false});
        }
        assert!(!support::validator("fixture-report.schema.json").is_valid(&mixed));
        raw["outcome"]["extra"] = true.into();
        assert!(!support::validator("fixture-report.schema.json").is_valid(&raw));
    }
}

#[test]
fn measured_success_and_failure_goldens_have_reviewed_context_counters_and_origin() {
    let p = support::verify(
        decode_ir(include_bytes!(
            "../../language/tests/fixtures/ir/golden/triage.ir.json"
        ))
        .unwrap(),
    )
    .unwrap();
    for (fixture, golden, failure) in [
        (
            include_bytes!("fixtures/valid/ubuntu/fixture.json").as_slice(),
            include_bytes!("fixtures/golden/triage-ubuntu.report.json").as_slice(),
            false,
        ),
        (
            include_bytes!("fixtures/valid/windows/fixture.json").as_slice(),
            include_bytes!("fixtures/golden/triage-windows.report.json").as_slice(),
            false,
        ),
        (
            include_bytes!("fixtures/valid/ubuntu/fixture.json").as_slice(),
            include_bytes!("fixtures/golden/provider-failure.report.json").as_slice(),
            true,
        ),
    ] {
        let mut raw: serde_json::Value = serde_json::from_slice(fixture).unwrap();
        if failure {
            raw["providers"]["forensic.system.profile"] =
                serde_json::json!({"status":"error","code":"PermissionDenied"});
        }
        let f = decode_fixture(
            &serde_json::to_vec(&raw).unwrap(),
            &ExecutionLimits::default(),
        )
        .unwrap();
        let r = execute(prepare(&p, &f, &ExecutionLimits::default()).unwrap());
        assert_eq!(r.to_json().unwrap(), golden);
        let before = *r.accounting();
        assert_eq!(r.to_json().unwrap(), golden);
        assert_eq!(*r.accounting(), before);
        assert_eq!(r.failure().is_some(), failure);
        let raw: serde_json::Value = serde_json::from_slice(golden).unwrap();
        assert!(support::validator("fixture-report.schema.json").is_valid(&raw));
        if let Some(error) = r.failure() {
            let origin = &p.document().functions[0].regions[0].instructions[0];
            assert_eq!(error.origin().unwrap().instruction_id, origin.id);
            assert_eq!(error.origin().unwrap().span, origin.span);
        } else {
            assert_eq!(
                raw["outcome"]["value"]["value"]["endpoint_id"],
                f.endpoint().endpoint_id
            );
            assert_eq!(
                raw["outcome"]["value"]["value"]["observed_at"],
                f.observed_at()
            );
            assert_eq!(
                raw["outcome"]["value"]["value"]["system"]["platform"],
                f.endpoint().platform.as_str()
            );
        }
    }
}
