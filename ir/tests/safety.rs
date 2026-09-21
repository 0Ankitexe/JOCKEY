mod support;
use jocky_ir::{ExecutionLimits, decode_fixture, execute, prepare};
use serde_json::json;
use support::programs::{self, UBUNTU};

#[test]
fn paths_labels_and_command_looking_strings_remain_inert_authored_data() {
    let mut raw: serde_json::Value = serde_json::from_slice(UBUNTU).unwrap();
    let inert = "https://example.invalid/no-fetch ../no-read C:\\no-open --output no-write";
    for pointer in [
        "/providers/forensic.process.list/value/values/0/value/image_path",
        "/providers/forensic.indicator.correlate/expected/processes/values/0/value/image_path",
    ] {
        *raw.pointer_mut(pointer).unwrap() = json!(inert);
    }
    raw["endpoint"]["label"] = json!(inert);
    let mut d = programs::triage().document().clone();
    d.source.label = inert.into();
    support::reidentify(&mut d);
    let p = support::verify(d).unwrap();
    let limits = ExecutionLimits::default();
    let f = decode_fixture(&serde_json::to_vec(&raw).unwrap(), &limits).unwrap();
    let prepared = prepare(&p, &f, &limits).unwrap();
    assert_eq!(
        format!("{prepared:?}"),
        "PreparedInvocation { fixture_only: true }"
    );
    let report = execute(prepared);
    assert!(report.failure().is_none());
    for field in ["include", "provider", "plugin", "url", "path"] {
        let mut bad = raw.clone();
        bad[field] = json!(inert);
        assert!(decode_fixture(&serde_json::to_vec(&bad).unwrap(), &limits).is_err());
    }
}

#[test]
fn verifying_or_preparing_a_program_never_executes_its_reached_failure() {
    let mut raw: serde_json::Value = serde_json::from_slice(UBUNTU).unwrap();
    raw["providers"]["forensic.system.profile"] =
        json!({"status":"error","code":"PermissionDenied"});
    let p = programs::triage();
    let f = decode_fixture(
        &serde_json::to_vec(&raw).unwrap(),
        &ExecutionLimits::default(),
    )
    .unwrap();
    for _ in 0..10 {
        let verified = support::verify(p.document().clone()).unwrap();
        let prepared = prepare(&verified, &f, &ExecutionLimits::default()).unwrap();
        let report = execute(prepared);
        assert_eq!(report.failure().unwrap().code(), "PermissionDenied");
        assert_eq!(report.accounting().instructions, 1);
    }
}

#[test]
fn lab_profile_and_privilege_metadata_never_select_a_host_provider() {
    use jocky_ir::{
        identity,
        model::{Callee, ErrorPolicy, Metadata, Operation, Profile},
    };
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    for c in raw["functions"].as_array_mut().unwrap() {
        if c["module"] == "forensic.system" && c["name"] == "profile" {
            c["required_privilege"] = json!("lab_only");
            c["lab_only"] = json!(true);
        }
    }
    let registry = jocky_forensic::contracts::Registry::from_json(&raw.to_string()).unwrap();
    let mut c = registry.lookup("forensic.system.profile").unwrap().clone();
    c.supported_platforms.sort();
    c.possible_errors.sort_by_key(|e| e.code);
    let mut d = support::document();
    d.registry.fingerprint = identity::registry_fingerprint(&registry).unwrap();
    d.profile = Some(Profile::Lab);
    d.entry_metadata = Metadata {
        required_privilege: c.required_privilege,
        capabilities: vec![c.capability],
        supported_platforms: c.supported_platforms.clone(),
        unavailable_dependencies: vec![c.qualified_name()],
        read_only: true,
        lab_only: true,
    };
    d.functions[0].metadata = d.entry_metadata.clone();
    d.contracts = vec![c];
    d.functions[0].regions[0].instructions[0].operation = Operation::Call {
        destination: 1,
        callee: Callee::Contract { contract: 0 },
        arguments: vec![0],
        on_error: ErrorPolicy::Propagate,
    };
    support::reidentify(&mut d);
    let p = jocky_ir::verify(d, &registry).unwrap();
    let result = programs::run(&p, UBUNTU, ExecutionLimits::default());
    assert_eq!(result["outcome"]["error"]["code"], "Unsupported");
    assert_eq!(result["accounting"]["instructions"], 1);
}
