//! Pure text views of validated, inert contracts; no source or registry file I/O.
use jocky_forensic::contracts::{
    ImplementationAvailability, LookupError, PossibleErrorCode, Registry,
};
use jocky_shared::compiler::Platform;

pub(super) fn list(registry: &Registry) -> String {
    registry
        .functions()
        .map(|contract| {
            format!(
                "{} {} {}\n",
                contract.qualified_name(),
                contract.version,
                availability(contract.availability)
            )
        })
        .collect()
}

pub(super) fn describe(registry: &Registry, name: &str) -> Result<String, LookupError> {
    let contract = registry.lookup(name)?;
    let parameters = contract
        .parameters
        .iter()
        .map(|p| format!("{}: {}", p.name, p.r#type))
        .collect::<Vec<_>>()
        .join(", ");
    let platforms = Platform::ALL
        .iter()
        .filter(|p| contract.supported_platforms.contains(p))
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let errors = contract
        .possible_errors
        .iter()
        .map(|e| format!("{}: {}", error_code(e.code), e.r#type))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        concat!(
            "module: {}\nname: {}\nversion: {}\nparameters: ({})\nresult: {}\n",
            "supported_platforms: {}\nrequired_privilege: {}\ncapability: {}\n",
            "read_only: {}\nlab_only: {}\npossible_errors: {}\navailability: {}\n",
        ),
        contract.module,
        contract.name,
        contract.version,
        parameters,
        contract.result,
        platforms,
        contract.required_privilege,
        contract.capability,
        contract.read_only,
        contract.lab_only,
        errors,
        availability(contract.availability)
    ))
}

fn availability(value: ImplementationAvailability) -> &'static str {
    match value {
        ImplementationAvailability::Unavailable => "unavailable",
    }
}
fn error_code(value: PossibleErrorCode) -> &'static str {
    match value {
        PossibleErrorCode::NotImplemented => "NotImplemented",
        PossibleErrorCode::Unsupported => "Unsupported",
        PossibleErrorCode::PermissionDenied => "PermissionDenied",
        PossibleErrorCode::InvalidInput => "InvalidInput",
        PossibleErrorCode::NotFound => "NotFound",
        PossibleErrorCode::ResourceLimit => "ResourceLimit",
        PossibleErrorCode::Timeout => "Timeout",
        PossibleErrorCode::CollectionFailed => "CollectionFailed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{CliError, render_cli_error};
    use jocky_forensic::contracts::{LookupError, Registry, builtin_registry};
    use serde_json::Value;

    const LIST: &str = concat!(
        "forensic.driver.list 0.1.0 unavailable\n",
        "forensic.event.list 0.1.0 unavailable\n",
        "forensic.event.ubuntu_journal 0.1.0 unavailable\n",
        "forensic.event.windows_log 0.1.0 unavailable\n",
        "forensic.file.inspect 0.1.0 unavailable\n",
        "forensic.indicator.correlate 0.1.0 unavailable\n",
        "forensic.network.connections 0.1.0 unavailable\n",
        "forensic.persistence.list 0.1.0 unavailable\n",
        "forensic.process.list 0.1.0 unavailable\n",
        "forensic.system.profile 0.1.0 unavailable\n",
    );
    const PROCESS: &str = concat!(
        "module: forensic.process\nname: list\nversion: 0.1.0\n",
        "parameters: (target: endpoint)\nresult: list<process_record>\n",
        "supported_platforms: windows, ubuntu\nrequired_privilege: user\n",
        "capability: processes\nread_only: true\nlab_only: false\n",
        "possible_errors: Unsupported: diagnostic_error, PermissionDenied: diagnostic_error, ResourceLimit: diagnostic_error, CollectionFailed: diagnostic_error\n",
        "availability: unavailable\n",
    );

    #[test]
    fn sorted_list_and_twelve_description_lines_match_the_contract() {
        let registry = builtin_registry().unwrap();
        assert_eq!(list(registry), LIST);
        assert_eq!(
            describe(registry, "forensic.process.list").unwrap(),
            PROCESS
        );
        assert_eq!(PROCESS.lines().count(), 12);
    }

    #[test]
    fn input_order_does_not_change_list_or_platform_order_but_errors_keep_declaration_order() {
        let mut document: Value = serde_json::from_str(include_str!(
            "../../../forensic-lib/contracts/catalogue.v1.json"
        ))
        .unwrap();
        document["functions"].as_array_mut().unwrap().reverse();
        for f in document["functions"].as_array_mut().unwrap() {
            f["supported_platforms"].as_array_mut().unwrap().reverse();
            f["possible_errors"].as_array_mut().unwrap().reverse();
        }
        let registry = Registry::from_json(&document.to_string()).unwrap();
        assert_eq!(list(&registry), LIST);
        assert_eq!(describe(&registry, "forensic.process.list").unwrap(), PROCESS.replace(
            "Unsupported: diagnostic_error, PermissionDenied: diagnostic_error, ResourceLimit: diagnostic_error, CollectionFailed: diagnostic_error",
            "CollectionFailed: diagnostic_error, ResourceLimit: diagnostic_error, PermissionDenied: diagnostic_error, Unsupported: diagnostic_error"));
    }

    #[test]
    fn zero_parameters_nested_types_and_lab_metadata_are_rendered_without_execution() {
        let registry = Registry::from_json(include_str!(
            "../../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
        ))
        .unwrap();
        assert_eq!(
            describe(&registry, "forensic.lab.inspect").unwrap(),
            concat!(
                "module: forensic.lab\nname: inspect\nversion: 0.1.0\nparameters: ()\n",
                "result: list<driver_record>\nsupported_platforms: windows, ubuntu\n",
                "required_privilege: lab_only\ncapability: drivers\nread_only: true\n",
                "lab_only: true\npossible_errors: Unsupported: diagnostic_error, PermissionDenied: diagnostic_error, ResourceLimit: diagnostic_error, CollectionFailed: diagnostic_error\navailability: unavailable\n"
            )
        );
        let mut doc: Value = serde_json::from_str(include_str!(
            "../../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
        ))
        .unwrap();
        doc["functions"][0]["result"] = serde_json::json!({"kind":"result", "ok":{"kind":"option","element":{"kind":"named","name":"bytes"}}, "error":{"kind":"named","name":"diagnostic_error"}});
        let registry = Registry::from_json(&doc.to_string()).unwrap();
        assert!(
            describe(&registry, "forensic.lab.inspect")
                .unwrap()
                .contains("result: result<option<bytes>, diagnostic_error>\n")
        );
    }

    #[test]
    fn typed_lookup_and_registry_failures_have_stable_escaped_output() {
        let registry = builtin_registry().unwrap();
        for (name, kind) in [
            ("forensic.missing.function", LookupError::UnknownFunction),
            ("forensic_result", LookupError::UnknownFunction),
            ("bad\n\t\u{1b}\u{85}", LookupError::InvalidName),
            ("", LookupError::InvalidName),
        ] {
            assert_eq!(describe(registry, name), Err(kind));
        }
        assert_eq!(
            render_cli_error(CliError::ModuleNotFound, Some("bad\n\t\u{1b}\u{85}")),
            "error[module-not-found]: module function not found: bad\\n\\t\\u{001B}\\u{0085}\n"
        );
        assert_eq!(CliError::ModuleNotFound.id(), "module-not-found");
        assert!(Registry::from_json("{}").is_err());
        assert_eq!(
            render_cli_error(CliError::InvalidRegistry, None),
            "error[invalid-registry]: module registry is invalid\n"
        );
    }
}
