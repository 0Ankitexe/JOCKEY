use jocky_forensic::contracts::{
    FunctionContract, ImplementationAvailability, Registry, RegistryErrorKind, builtin_registry,
};
use jocky_shared::compiler::{Capability, NamedType, Platform, Privilege, Type};
use serde_json::Value;
use std::collections::BTreeSet;

const CATALOGUE: &str = include_str!("../contracts/catalogue.v1.json");

#[test]
fn every_record_has_the_exact_planned_signature_flags_and_typed_errors() {
    // Independent expectations: a catalogue edit must not silently redefine a call.
    let expected = [
        (
            "forensic.driver.list",
            "target: endpoint",
            "list<driver_record>",
            "drivers",
            "elevated",
            "Unsupported,PermissionDenied,ResourceLimit,CollectionFailed",
        ),
        (
            "forensic.event.list",
            "target: endpoint, since: timestamp, until: timestamp, limit: int",
            "list<event_record>",
            "events",
            "elevated",
            "Unsupported,PermissionDenied,InvalidInput,ResourceLimit,Timeout,CollectionFailed",
        ),
        (
            "forensic.event.ubuntu_journal",
            "target: endpoint, since: timestamp, until: timestamp, limit: int",
            "list<event_record>",
            "events",
            "elevated",
            "Unsupported,PermissionDenied,InvalidInput,ResourceLimit,Timeout,CollectionFailed",
        ),
        (
            "forensic.event.windows_log",
            "target: endpoint, channel: string, since: timestamp, until: timestamp, limit: int",
            "list<event_record>",
            "events",
            "elevated",
            "Unsupported,PermissionDenied,InvalidInput,NotFound,ResourceLimit,Timeout,CollectionFailed",
        ),
        (
            "forensic.file.inspect",
            "target: endpoint, location: path",
            "file_record",
            "files",
            "user",
            "Unsupported,PermissionDenied,InvalidInput,NotFound,ResourceLimit",
        ),
        (
            "forensic.indicator.correlate",
            "system: forensic_result, processes: list<process_record>, connections: list<connection_record>",
            "forensic_result",
            "indicators",
            "user",
            "NotImplemented,InvalidInput,ResourceLimit",
        ),
        (
            "forensic.network.connections",
            "target: endpoint",
            "list<connection_record>",
            "network",
            "user",
            "Unsupported,PermissionDenied,ResourceLimit,CollectionFailed",
        ),
        (
            "forensic.persistence.list",
            "target: endpoint",
            "list<persistence_record>",
            "persistence",
            "user",
            "Unsupported,PermissionDenied,ResourceLimit,CollectionFailed",
        ),
        (
            "forensic.process.list",
            "target: endpoint",
            "list<process_record>",
            "processes",
            "user",
            "Unsupported,PermissionDenied,ResourceLimit,CollectionFailed",
        ),
        (
            "forensic.system.profile",
            "target: endpoint",
            "forensic_result",
            "system",
            "user",
            "Unsupported,PermissionDenied,ResourceLimit,CollectionFailed",
        ),
    ];
    let registry = builtin_registry().unwrap();
    assert_eq!(registry.functions().len(), expected.len());
    for (record, (name, parameters, result, capability, privilege, errors)) in
        registry.functions().zip(expected)
    {
        assert_eq!(record.qualified_name(), name);
        let (module, function) = name.rsplit_once('.').unwrap();
        assert_eq!(record.module, module);
        assert_eq!(record.name, function);
        assert_eq!(record.version, "0.1.0");
        assert_eq!(
            record
                .parameters
                .iter()
                .map(|p| format!("{}: {}", p.name, p.r#type))
                .collect::<Vec<_>>()
                .join(", "),
            parameters
        );
        assert_eq!(record.result.to_string(), result);
        assert_eq!(
            record.capability,
            Capability::from_name(capability).unwrap()
        );
        assert_eq!(
            record.required_privilege,
            Privilege::from_name(privilege).unwrap()
        );
        assert!(record.read_only && !record.lab_only);
        assert_eq!(record.availability, ImplementationAvailability::Unavailable);
        let platforms = match function {
            "windows_log" => vec![Platform::Windows],
            "ubuntu_journal" => vec![Platform::Ubuntu],
            _ => Platform::ALL.to_vec(),
        };
        assert_eq!(record.supported_platforms, platforms);
        assert_eq!(
            record
                .possible_errors
                .iter()
                .map(|e| format!("{:?}", e.code))
                .collect::<Vec<_>>()
                .join(","),
            errors
        );
        assert!(record.possible_errors.iter().all(|e| e.r#type
            == Type::Named {
                name: NamedType::DiagnosticError
            }));
    }
    assert_eq!(
        registry
            .functions()
            .map(|f| f.capability)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        Capability::ALL
    );
}

#[test]
fn each_record_is_schema_valid_and_round_trips_without_hidden_fields() {
    let document: Value = serde_json::from_str(CATALOGUE).unwrap();
    assert_eq!(document["schema_version"], "1.0.0");
    assert_eq!(document.as_object().unwrap().len(), 2);
    for record in document["functions"].as_array().unwrap() {
        assert_eq!(record.as_object().unwrap().len(), 12);
        // Registry::from_json always applies the deployed schema AND consistency pass.
        let single = serde_json::json!({"schema_version":"1.0.0", "functions":[record]});
        let validated = Registry::from_json(&single.to_string()).unwrap();
        let typed: FunctionContract = serde_json::from_value(record.clone()).unwrap();
        assert_eq!(validated.lookup(&typed.qualified_name()).unwrap(), &typed);
        assert_eq!(serde_json::to_value(&typed).unwrap(), *record);
    }
}

#[test]
fn catalogue_permutation_is_inert_and_executable_fields_or_new_statuses_are_rejected() {
    let mut document: Value = serde_json::from_str(CATALOGUE).unwrap();
    document["functions"].as_array_mut().unwrap().reverse();
    let reordered = Registry::from_json(&document.to_string()).unwrap();
    assert_eq!(
        reordered.functions().collect::<Vec<_>>(),
        builtin_registry().unwrap().functions().collect::<Vec<_>>()
    );
    for field in [
        "implementation",
        "command",
        "payload",
        "entrypoint",
        "collector",
    ] {
        let mut invalid = document.clone();
        invalid["functions"][0][field] = "not executable".into();
        assert_eq!(
            Registry::from_json(&invalid.to_string()).unwrap_err().kind,
            RegistryErrorKind::SchemaViolation
        );
    }
    document["functions"][0]["availability"] = "implemented".into();
    assert_eq!(
        Registry::from_json(&document.to_string()).unwrap_err().kind,
        RegistryErrorKind::SchemaViolation
    );
}
