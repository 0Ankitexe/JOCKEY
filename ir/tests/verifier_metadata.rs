mod support;
use jocky_forensic::contracts::{Registry, builtin_registry};
use jocky_ir::{DiagnosticCode as Code, IrDocument, identity, model::*};
use jocky_shared::compiler::{Platform, Privilege};

fn external(registry: &Registry) -> IrDocument {
    let mut d = support::document();
    let mut c = registry.lookup("forensic.system.profile").unwrap().clone();
    c.supported_platforms.sort();
    c.possible_errors.sort_by_key(|e| e.code);
    d.registry.fingerprint = identity::registry_fingerprint(registry).unwrap();
    d.functions[0].metadata = Metadata {
        required_privilege: c.required_privilege,
        capabilities: vec![c.capability],
        supported_platforms: c.supported_platforms.clone(),
        unavailable_dependencies: vec![c.qualified_name()],
        read_only: c.read_only,
        lab_only: c.lab_only,
    };
    d.entry_metadata = d.functions[0].metadata.clone();
    d.contracts.push(c);
    d.functions[0].regions[0].instructions[0].operation = Operation::Call {
        destination: 1,
        callee: Callee::Contract { contract: 0 },
        arguments: vec![0],
        on_error: ErrorPolicy::Propagate,
    };
    support::reidentify(&mut d);
    d
}
fn registry(platform: &str, lab: bool) -> Registry {
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    for c in raw["functions"].as_array_mut().unwrap() {
        if c["module"] == "forensic.system" && c["name"] == "profile" {
            c["supported_platforms"] = serde_json::json!([platform]);
            if lab {
                c["lab_only"] = true.into();
                c["required_privilege"] = "lab_only".into();
            }
        }
    }
    Registry::from_json(&raw.to_string()).unwrap()
}

#[test]
fn complete_descriptors_and_recursive_closure_cannot_be_forged() {
    let mut d = external(builtin_registry().unwrap());
    support::verify(d.clone()).unwrap();
    let mut helper = d.functions[0].clone();
    helper.name = "helper".into();
    // A source SCC includes the external dependency even in dead instructions.
    helper.regions[0].instructions[0].operation = Operation::Call {
        destination: 1,
        callee: Callee::Source { function: 0 },
        arguments: vec![0],
        on_error: ErrorPolicy::Propagate,
    };
    d.functions.push(helper);
    let mut s = d.functions[0].slots[1].clone();
    s.kind = SlotKind::Temporary;
    d.functions[0].slots.push(s);
    let mut i = d.functions[0].regions[0].instructions[0].clone();
    i.operation = Operation::Call {
        destination: 2,
        callee: Callee::Source { function: 1 },
        arguments: vec![0],
        on_error: ErrorPolicy::Propagate,
    };
    d.functions[0].regions[0].instructions.push(i);
    support::reidentify(&mut d);
    support::verify(d.clone()).unwrap();
    d.functions[1].metadata.capabilities.clear();
    assert_eq!(support::verify(d).unwrap_err().code(), Code::Metadata);
    let mut d = external(builtin_registry().unwrap());
    d.contracts[0].read_only = false;
    assert_eq!(support::verify(d).unwrap_err().code(), Code::Contract);
}

#[test]
fn opposite_platforms_and_lab_requirements_are_checked_without_execution() {
    for (name, allowed, forbidden) in [
        ("windows", Platform::Windows, Platform::Ubuntu),
        ("ubuntu", Platform::Ubuntu, Platform::Windows),
    ] {
        let registry = registry(name, false);
        let mut d = external(&registry);
        d.targets = vec![allowed];
        jocky_ir::verify(d.clone(), &registry).unwrap();
        d.targets = vec![forbidden];
        assert_eq!(
            jocky_ir::verify(d, &registry).unwrap_err().code(),
            Code::Metadata
        );
    }
    let registry = registry("ubuntu", true);
    let mut d = external(&registry);
    d.targets = vec![Platform::Ubuntu];
    assert_eq!(d.entry_metadata.required_privilege, Privilege::LabOnly);
    assert_eq!(
        jocky_ir::verify(d.clone(), &registry).unwrap_err().code(),
        Code::Metadata
    );
    d.profile = Some(Profile::Lab);
    jocky_ir::verify(d, &registry).unwrap();
}

#[test]
fn uncalled_helpers_do_not_inflate_entry_but_still_obey_platform_gates() {
    let registry = registry("windows", false);
    let mut d = external(&registry);
    let helper = d.functions[0].clone();
    d.functions[0] = support::document().functions[0].clone();
    d.entry_metadata = Metadata::default();
    d.functions.push(helper);
    d.functions[1].name = "helper".into();
    d.targets = vec![Platform::Windows];
    support::reidentify(&mut d);
    jocky_ir::verify(d.clone(), &registry).unwrap();
    d.targets = vec![Platform::Ubuntu];
    assert_eq!(
        jocky_ir::verify(d, &registry).unwrap_err().code(),
        Code::Metadata
    );
}

#[test]
fn uncalled_lab_helper_and_dead_source_scc_keep_static_obligations() {
    let registry = registry("ubuntu", true);
    let mut d = external(&registry);
    let mut helper = d.functions[0].clone();
    helper.name = "lab_helper".into();
    d.functions[0] = support::document().functions[0].clone();
    d.entry_metadata = Metadata::default();
    d.functions.push(helper);
    d.targets = vec![Platform::Ubuntu];
    support::reidentify(&mut d);
    assert_eq!(
        jocky_ir::verify(d.clone(), &registry).unwrap_err().code(),
        Code::Metadata
    );
    d.profile = Some(Profile::Lab);
    let verified = jocky_ir::verify(d.clone(), &registry).unwrap();
    assert_eq!(
        verified.document().entry_metadata.required_privilege,
        Privilege::User
    );
    d.functions[1].metadata.required_privilege = Privilege::User;
    assert_eq!(
        jocky_ir::verify(d, &registry).unwrap_err().code(),
        Code::Metadata
    );
    // The accepted wrapper owns its document, not this subsequently mutated clone.
    assert_eq!(
        verified.document().functions[1].metadata.required_privilege,
        Privilege::LabOnly
    );
}
