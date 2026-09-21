mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, decode_ir, execute, prepare};
use serde_json::{Value, json};
fn triage() -> jocky_ir::VerifiedProgram {
    support::verify(
        decode_ir(include_bytes!(
            "../../language/tests/fixtures/ir/golden/triage.ir.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn raw() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/valid/ubuntu/fixture.json")).unwrap()
}
fn run(program: &jocky_ir::VerifiedProgram, fixture: &Value, limits: ExecutionLimits) -> Value {
    let fixture = decode_fixture(&serde_json::to_vec(fixture).unwrap(), &limits).unwrap();
    let report = execute(prepare(program, &fixture, &limits).unwrap());
    serde_json::from_slice(&report.to_json().unwrap()).unwrap()
}

#[test]
fn exact_synthetic_triage_on_both_platforms() {
    let program = triage();
    for platform in ["ubuntu", "windows"] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/valid")
            .join(platform);
        let fixture: Value =
            serde_json::from_slice(&std::fs::read(root.join("fixture.json")).unwrap()).unwrap();
        let expected: Value =
            serde_json::from_slice(&std::fs::read(root.join("expected-result.json")).unwrap())
                .unwrap();
        let report = run(&program, &fixture, ExecutionLimits::default());
        assert_eq!(
            report["outcome"],
            json!({"status":"success","value":expected})
        );
        assert_eq!(report["platform"], platform);
        assert_eq!(report["accounting"]["instructions"], 10);
        assert_eq!(report["accounting"]["peak_call_depth"], 1);
    }
}

#[test]
fn provider_failures_stop_at_original_instruction_without_partial_success() {
    let program = triage();
    let origin = &program.document().functions[0].regions[0].instructions[0];
    for code in [
        "Unsupported",
        "PermissionDenied",
        "ResourceLimit",
        "CollectionFailed",
    ] {
        let mut fixture = raw();
        fixture["providers"]["forensic.system.profile"] = json!({"status":"error","code":code});
        let r = run(&program, &fixture, ExecutionLimits::default());
        assert_eq!(r["outcome"]["status"], "failure");
        assert!(r["outcome"].get("value").is_none());
        assert_eq!(r["outcome"]["error"]["code"], code);
        assert_eq!(r["outcome"]["error"]["origin"]["instruction_id"], origin.id);
        assert_eq!(r["accounting"]["instructions"], 1);
    }
}

#[test]
fn actual_arguments_are_matched_before_correlation_response() {
    let mut fixture = raw();
    fixture["providers"]["forensic.indicator.correlate"]["expected"]["system"]["value"]["system"]
        ["hostname"] = json!("different authored profile");
    let r = run(&triage(), &fixture, ExecutionLimits::default());
    assert_eq!(r["outcome"]["error"]["code"], "InvalidInput");
    assert_eq!(r["accounting"]["instructions"], 7);
}

#[test]
fn recursive_call_and_instruction_work_limits_already_guard_first_executor() {
    let mut d = support::document();
    d.functions[0].regions[0].instructions[0].operation = jocky_ir::model::Operation::Call {
        destination: 1,
        callee: jocky_ir::model::Callee::Source { function: 0 },
        arguments: vec![0],
        on_error: jocky_ir::model::ErrorPolicy::Propagate,
    };
    let p = support::verify(d).unwrap();
    for (limits, code, count) in [
        (
            ExecutionLimits::new(3, 10_000, 1_000_000, 128).unwrap(),
            "EXEC_INSTRUCTION_LIMIT",
            3,
        ),
        (
            ExecutionLimits::new(100, 10_000, 1_000_000, 2).unwrap(),
            "EXEC_CALL_DEPTH",
            2,
        ),
    ] {
        let r = run(&p, &raw(), limits);
        assert_eq!(r["outcome"]["error"]["code"], code);
        assert_eq!(r["accounting"]["instructions"], count);
    }
    let r = run(
        &triage(),
        &raw(),
        ExecutionLimits::new(100, 10_000, 1, 128).unwrap(),
    );
    assert_eq!(r["outcome"]["error"]["code"], "EXEC_WORK_LIMIT");
    assert!(r["accounting"]["work"].as_u64().unwrap() <= 1);
}

fn identity_function(ty: &Value) -> Value {
    json!({"name":"identity","span":null,"parameters":[0],"result":ty,
        "slots":[{"type":ty,"region":0,"kind":"parameter","name":"value","span":null}],
        "root_region":0,"regions":[{"instructions":[{"id":"","span":null,"op":"return","value":0}]}],
        "metadata":support::raw()["entry_metadata"]})
}
fn slot(ty: &Value, region: usize, kind: &str, name: Value) -> Value {
    json!({"type":ty,"region":region,"kind":kind,"name":name,"span":null})
}
fn verified_raw(raw: Value) -> jocky_ir::VerifiedProgram {
    let mut d: jocky_ir::IrDocument = serde_json::from_value(raw).unwrap();
    support::reidentify(&mut d);
    support::verify(d).unwrap()
}

#[test]
fn ordinary_option_result_and_domain_data_pass_through_typed_calls_without_unwrapping() {
    let named = |name| json!({"kind":"named","name":name});
    let int = named("int");
    let error = named("diagnostic_error");
    let mut values = vec![
        json!({"kind":"int","value":"123456789012345678901234567890"}),
        json!({"kind":"bytes","value":"00ff"}),
        json!({"kind":"path","value":"../../never-open"}),
        json!({"kind":"ip_address","value":"192.0.2.1"}),
        json!({"kind":"duration","magnitude":"123456789012345678901234567890","unit":"h"}),
        json!({"kind":"timestamp","value":"2026-09-18T00:00:00Z"}),
        json!({"kind":"option","element_type":int,"value":null}),
        json!({"kind":"option","element_type":int,"value":{"kind":"int","value":"1"}}),
        json!({"kind":"result","ok_type":int,"error_type":error,"variant":"ok","value":{"kind":"int","value":"2"}}),
        json!({"kind":"result","ok_type":int,"error_type":error,"variant":"error","value":{"kind":"diagnostic_error","code":"ResourceLimit"}}),
    ];
    for variant in ["ok", "error"] {
        let mut value = json!({"kind":"option","element_type":int,"value":null});
        let mut ty = json!({"kind":"option","element":int});
        for _ in 0..20 {
            let (ok, error) = if variant == "ok" {
                (ty.clone(), named("string"))
            } else {
                (named("string"), ty.clone())
            };
            value = json!({"kind":"result","ok_type":ok,"error_type":error,"variant":variant,"value":value});
            ty = json!({"kind":"result","ok":ok,"error":error});
        }
        values.push(value);
    }
    values.push(json!({"kind":"int","value":"9".repeat(10_000)}));
    values.push(json!({"kind":"duration","magnitude":"9".repeat(10_000),"unit":"d"}));
    values.push(json!({"kind":"option","element_type":{"kind":"option","element":int},"value":{"kind":"option","element_type":int,"value":{"kind":"int","value":"1"}}}));
    for value in values {
        let ty =
            jocky_ir::value::validate(&serde_json::from_value(value.clone()).unwrap()).unwrap();
        let ty = serde_json::to_value(ty).unwrap();
        let mut d = support::raw();
        d["constants"].as_array_mut().unwrap().push(value.clone());
        d["functions"]
            .as_array_mut()
            .unwrap()
            .push(identity_function(&ty));
        let f = &mut d["functions"][0];
        for _ in 0..3 {
            f["slots"]
                .as_array_mut()
                .unwrap()
                .push(slot(&ty, 0, "temporary", Value::Null));
        }
        let ops = f["regions"][0]["instructions"].as_array_mut().unwrap();
        ops.splice(0..0, [
            json!({"id":"","span":null,"op":"const","destination":2,"constant":1}),
            json!({"id":"","span":null,"op":"call","destination":3,"callee":{"kind":"source","function":1},"arguments":[2],"on_error":"propagate"}),
            json!({"id":"","span":null,"op":"copy","destination":4,"source":3}),
        ]);
        let program = verified_raw(d);
        assert_eq!(
            serde_json::to_value(&program.document().constants[1]).unwrap(),
            value
        );
        let r = run(&program, &raw(), ExecutionLimits::default());
        assert_eq!(r["outcome"]["status"], "success", "{value}");
        assert_eq!(r["accounting"]["instructions"], 6);
        assert_eq!(r["accounting"]["peak_call_depth"], 2);
    }
}

#[test]
fn hand_authored_collection_order_and_early_return_are_observable() {
    let ty = json!({"kind":"named","name":"forensic_result"});
    let list_ty = json!({"kind":"list","element":ty});
    let profile = raw()["providers"]["forensic.system.profile"]["value"].clone();
    let mut first = profile.clone();
    first["value"]["system"]["hostname"] = "first".into();
    let mut second = profile.clone();
    second["value"]["system"]["hostname"] = "second".into();
    for values in [
        vec![],
        vec![first.clone(), second.clone()],
        vec![second, first],
    ] {
        let expected = values.first().unwrap_or(&profile).clone();
        let empty = values.is_empty();
        let mut d = support::raw();
        d["constants"][0] = profile.clone();
        d["constants"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"list","element_type":ty,"values":values}));
        d["functions"]
            .as_array_mut()
            .unwrap()
            .push(identity_function(&ty));
        let f = &mut d["functions"][0];
        f["slots"].as_array_mut().unwrap().extend([
            slot(&list_ty, 0, "temporary", Value::Null),
            slot(&ty, 1, "loop_binding", json!("item")),
            slot(&ty, 1, "temporary", Value::Null),
            slot(&ty, 1, "temporary", Value::Null),
        ]);
        f["regions"][0]["instructions"].as_array_mut().unwrap().splice(0..0, [
            json!({"id":"","span":null,"op":"const","destination":2,"constant":1}),
            json!({"id":"","span":null,"op":"for_each","collection":2,"item":3,"body_region":1}),
        ]);
        f["regions"].as_array_mut().unwrap().push(json!({"instructions":[
            {"id":"","span":null,"op":"copy","destination":4,"source":3},
            {"id":"","span":null,"op":"call","destination":5,"callee":{"kind":"source","function":1},"arguments":[4],"on_error":"propagate"},
            {"id":"","span":null,"op":"return","value":5}
        ]}));
        let r = run(&verified_raw(d), &raw(), ExecutionLimits::default());
        assert_eq!(r["outcome"]["value"], expected);
        assert_eq!(r["accounting"]["instructions"], if empty { 5 } else { 7 });
    }
}

#[test]
fn return_of_wrong_fixture_context_is_typed_failure_not_evidence() {
    let mut d = support::document();
    let jocky_ir::value::Value::ForensicResult { value } = &mut d.constants[0] else {
        unreachable!()
    };
    value.endpoint_id = "00000000-0000-4000-8000-000000000099".into();
    let r = run(
        &support::verify(d).unwrap(),
        &raw(),
        ExecutionLimits::default(),
    );
    assert_eq!(r["outcome"]["error"]["code"], "EXEC_INVALID_VALUE");
    assert!(r["outcome"].get("value").is_none());
}
