use super::*;
use crate::{ExecutionLimits, decode_fixture, decode_ir};
use jocky_forensic::contracts::{FunctionContract, PossibleError};
use jocky_shared::compiler::{NamedType, Privilege, Type};

fn fixture() -> ValidatedFixture {
    decode_fixture(
        include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
        &ExecutionLimits::default(),
    )
    .unwrap()
}
fn document() -> IrDocument {
    decode_ir(include_bytes!(
        "../../../language/tests/fixtures/ir/golden/triage.ir.json"
    ))
    .unwrap()
}
fn canonical(name: &str) -> FunctionContract {
    let mut c = builtin_registry().unwrap().lookup(name).unwrap().clone();
    c.supported_platforms.sort();
    c.possible_errors.sort_by_key(|e| e.code);
    c
}

#[test]
fn changed_descriptors_and_lab_only_functions_never_select_a_mock() {
    let f = fixture();
    let mut d = document();
    for change in 0..9 {
        let mut c = canonical("forensic.system.profile");
        match change {
            0 => c.version = "2.0.0".into(),
            1 => c.parameters[0].name = "other".into(),
            2 => c.result = Type::named(NamedType::Bool),
            3 => c.supported_platforms.pop().map(|_| ()).unwrap(),
            4 => c.required_privilege = Privilege::Elevated,
            5 => c.read_only = false,
            6 => {
                c.lab_only = true;
                c.required_privilege = Privilege::LabOnly;
            }
            7 => c.capability = jocky_shared::compiler::Capability::Drivers,
            _ => c.module = "unmocked".into(),
        }
        d.contracts = vec![c];
        let mut arena = ValueArena::default();
        let mut budget = Budget::new(ExecutionLimits::default(), 0).unwrap();
        let p = Providers::prepare(&f, &d, &mut arena, &mut budget).unwrap();
        let e = p
            .invoke(
                0,
                &[p.endpoint()],
                &d,
                &arena,
                &mut budget,
                &d.functions[0].regions[0].instructions[0],
            )
            .unwrap_err();
        assert_eq!(e.code(), "Unsupported", "change {change}");
        assert_eq!(budget.accounting.work, 0);
    }
}

#[test]
fn unavailable_uses_only_declared_errors_in_priority_order() {
    let f = fixture();
    let mut d = document();
    for (codes, expected) in [
        (
            vec![Error::NotImplemented, Error::Unsupported],
            "Unsupported",
        ),
        (vec![Error::NotImplemented], "NotImplemented"),
        (vec![Error::PermissionDenied], "EXEC_INVALID_VALUE"),
    ] {
        let mut c = canonical("forensic.driver.list");
        c.possible_errors = codes
            .into_iter()
            .map(|code| PossibleError {
                code,
                r#type: Type::named(NamedType::DiagnosticError),
            })
            .collect();
        d.contracts = vec![c];
        let mut arena = ValueArena::default();
        let mut budget = Budget::new(ExecutionLimits::default(), 0).unwrap();
        let p = Providers::prepare(&f, &d, &mut arena, &mut budget).unwrap();
        let e = p
            .invoke(
                0,
                &[p.endpoint()],
                &d,
                &arena,
                &mut budget,
                &d.functions[0].regions[0].instructions[0],
            )
            .unwrap_err();
        assert_eq!(e.code(), expected);
    }
}

#[test]
fn every_mock_error_propagates_unchanged_and_mismatched_endpoint_is_not_provider_success() {
    let mut d = document();
    for (index, name) in [
        "forensic.system.profile",
        "forensic.process.list",
        "forensic.network.connections",
        "forensic.indicator.correlate",
    ]
    .iter()
    .enumerate()
    {
        d.contracts = vec![canonical(name)];
        for error in &d.contracts[0].possible_errors {
            let mut raw: serde_json::Value = serde_json::from_slice(include_bytes!(
                "../../tests/fixtures/valid/ubuntu/fixture.json"
            ))
            .unwrap();
            let outcome = serde_json::json!({"status":"error","code":error.code});
            if index == 3 {
                raw["providers"][name]["outcome"] = outcome;
            } else {
                raw["providers"][name] = outcome;
            }
            let f = decode_fixture(
                &serde_json::to_vec(&raw).unwrap(),
                &ExecutionLimits::default(),
            )
            .unwrap();
            let mut arena = ValueArena::default();
            let mut budget = Budget::new(ExecutionLimits::default(), 0).unwrap();
            let p = Providers::prepare(&f, &d, &mut arena, &mut budget).unwrap();
            let args = if index == 3 {
                p.expected.to_vec()
            } else {
                vec![p.endpoint()]
            };
            let origin = &d.functions[0].regions[0].instructions[0];
            let e = p
                .invoke(0, &args, &d, &arena, &mut budget, origin)
                .unwrap_err();
            assert!(matches!(e, ExecutionFailure::Provider { code, .. } if code == error.code));
            assert_eq!(e.origin().unwrap().instruction_id, origin.id);
            if index < 3 {
                let mut endpoint = f.endpoint().clone();
                endpoint.label = "different synthetic identity".into();
                let h = retain(
                    &Value::Endpoint { value: endpoint },
                    &mut arena,
                    &mut budget,
                )
                .unwrap();
                let e = p
                    .invoke(0, &[h], &d, &arena, &mut budget, origin)
                    .unwrap_err();
                assert_eq!(e.code(), "EXEC_INVALID_VALUE");
            }
        }
    }
}

#[test]
fn composition_checks_roles_context_and_references_and_shares_children() {
    let f = fixture();
    let d = document();
    let mut arena = ValueArena::default();
    let mut budget = Budget::new(ExecutionLimits::default(), 0).unwrap();
    let p = Providers::prepare(&f, &d, &mut arena, &mut budget).unwrap();
    let StoredOutcome::Ok(profile) = p.outcomes[0] else {
        unreachable!()
    };
    let StoredOutcome::Ok(indicators) = p.outcomes[3] else {
        unreachable!()
    };
    let before = budget.accounting;
    let h = compose(&[profile, indicators], &mut arena, &f, &mut budget).unwrap();
    let (combined, a, b) = arena.forensic(h).unwrap();
    assert_eq!((a, b), (profile, indicators));
    assert!(combined.system.is_some() && !combined.indicators.is_empty());
    assert_eq!(
        budget.accounting.peak_storage_bytes - before.peak_storage_bytes,
        80
    );
    assert!(budget.accounting.work > before.work);
    for args in [
        vec![],
        vec![profile],
        vec![indicators, profile],
        vec![profile, profile],
    ] {
        assert_eq!(
            compose(&args, &mut arena, &f, &mut budget),
            Err(ExecutionCode::InvalidComposition)
        );
    }
    for change in 0..4 {
        let mut bad = f
            .bundle
            .providers
            .correlation
            .outcome
            .value()
            .unwrap()
            .clone();
        let Value::ForensicResult { value } = &mut bad else {
            unreachable!()
        };
        match change {
            0 => value.endpoint_id = "00000000-0000-4000-8000-000000000099".into(),
            1 => value.observed_at = "2026-09-19T00:00:00Z".into(),
            2 => value.indicators[0].record_ids[0] = "00000000-0000-4000-8000-000000000099".into(),
            _ => value.indicators.push(value.indicators[0].clone()),
        }
        let bad = retain(&bad, &mut arena, &mut budget).unwrap();
        assert_eq!(
            compose(&[profile, bad], &mut arena, &f, &mut budget),
            Err(ExecutionCode::InvalidComposition)
        );
    }
}
