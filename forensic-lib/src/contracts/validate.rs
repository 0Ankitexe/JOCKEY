use super::{RegistryError, RegistryErrorKind, model::RegistryDocument};
use jsonschema::{Resource, Retrieve, Uri, Validator};
use serde_json::Value;
use std::{collections::BTreeSet, sync::OnceLock};

const TYPES: &str = include_str!("../../../shared/compiler-contracts/compiler-types.schema.json");
const SCHEMA: &str = include_str!("../../../shared/compiler-contracts/module-registry.schema.json");

struct DenyRetrieval;
impl Retrieve for DenyRetrieval {
    fn retrieve(&self, _: &Uri<String>) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err(Box::new(RegistryError::new(
            RegistryErrorKind::SchemaViolation,
            "unknown-schema-resource",
        )))
    }
}

fn validator() -> Result<&'static Validator, RegistryError> {
    static VALIDATOR: OnceLock<Result<Validator, RegistryError>> = OnceLock::new();
    VALIDATOR
        .get_or_init(|| {
            let invalid =
                || RegistryError::new(RegistryErrorKind::SchemaViolation, "invalid-bundled-schema");
            let types: Value = serde_json::from_str(TYPES).map_err(|_| invalid())?;
            let schema: Value = serde_json::from_str(SCHEMA).map_err(|_| invalid())?;
            jsonschema::draft202012::options()
                .with_retriever(DenyRetrieval)
                .with_resource(
                    "https://jocky.dev/compiler/v1/types.schema.json",
                    Resource::from_contents(types).map_err(|_| invalid())?,
                )
                .build(&schema)
                .map_err(|_| invalid())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) fn validate(value: Value) -> Result<RegistryDocument, RegistryError> {
    if value.get("schema_version").is_some_and(|v| v != "1.0.0") {
        return Err(RegistryError::new(
            RegistryErrorKind::UnsupportedVersion,
            "unsupported-version",
        )
        .at("/schema_version".into()));
    }
    precheck_limits(&value)?;
    if let Err(error) = validator()?.validate(&value) {
        return Err(
            RegistryError::new(RegistryErrorKind::SchemaViolation, "schema-violation")
                .at(error.instance_path.to_string()),
        );
    }
    let document: RegistryDocument = serde_json::from_value(value)
        .map_err(|_| RegistryError::new(RegistryErrorKind::SchemaViolation, "typed-decode"))?;
    if document.schema_version != "1.0.0" {
        return Err(RegistryError::new(
            RegistryErrorKind::UnsupportedVersion,
            "unsupported-version",
        ));
    }
    let mut identities = BTreeSet::new();
    for (index, function) in document.functions.iter().enumerate() {
        let pointer = format!("/functions/{index}");
        if !valid_qualified(&function.module) || !valid_identifier(&function.name) {
            return Err(RegistryError::new(
                RegistryErrorKind::InvalidIdentifier,
                "identifier-segment",
            )
            .at(pointer));
        }
        if !identities.insert(function.qualified_name()) {
            return Err(RegistryError::new(
                RegistryErrorKind::DuplicateFunction,
                "duplicate-function",
            )
            .at(pointer));
        }
        let mut names = BTreeSet::new();
        for (parameter_index, parameter) in function.parameters.iter().enumerate() {
            if !names.insert(&parameter.name) {
                return Err(RegistryError::new(
                    RegistryErrorKind::DuplicateParameter,
                    "duplicate-parameter",
                )
                .at(format!("{pointer}/parameters/{parameter_index}/name")));
            }
        }
        let mut codes = BTreeSet::new();
        for (error_index, error) in function.possible_errors.iter().enumerate() {
            if !codes.insert(error.code) {
                return Err(RegistryError::new(
                    RegistryErrorKind::DuplicateErrorCode,
                    "duplicate-error-code",
                )
                .at(format!("{pointer}/possible_errors/{error_index}/code")));
            }
        }
    }
    Ok(document)
}

fn precheck_limits(value: &Value) -> Result<(), RegistryError> {
    let limit = |reason| RegistryError::new(RegistryErrorKind::ResourceLimit, reason);
    let Some(functions) = value.get("functions").and_then(Value::as_array) else {
        return Ok(());
    };
    if functions.len() > 512 {
        return Err(limit("registry-functions"));
    }
    let mut stack = Vec::new();
    for function in functions {
        if let Some(parameters) = function.get("parameters").and_then(Value::as_array) {
            if parameters.len() > 64 {
                return Err(limit("registry-parameters"));
            }
            stack.extend(
                parameters
                    .iter()
                    .filter_map(|p| p.get("type"))
                    .map(|ty| (ty, 1usize)),
            );
        }
        if let Some(result) = function.get("result") {
            stack.push((result, 1));
        }
        if let Some(errors) = function.get("possible_errors").and_then(Value::as_array) {
            stack.extend(
                errors
                    .iter()
                    .filter_map(|e| e.get("type"))
                    .map(|ty| (ty, 1)),
            );
        }
    }
    let mut nodes = 0usize;
    while let Some((value, depth)) = stack.pop() {
        nodes = nodes
            .checked_add(1)
            .ok_or_else(|| limit("registry-type-nodes"))?;
        if nodes > 32_768 {
            return Err(limit("registry-type-nodes"));
        }
        if depth > 64 {
            return Err(limit("registry-type-depth"));
        }
        for key in ["element", "ok", "error"] {
            if let Some(child) = value.get(key) {
                stack.push((child, depth + 1));
            }
        }
    }
    Ok(())
}

pub(super) fn valid_identifier(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() <= 64
        && bytes
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        && !matches!(
            name,
            "module"
                | "windows"
                | "ubuntu"
                | "fn"
                | "let"
                | "if"
                | "else"
                | "for"
                | "in"
                | "return"
                | "run"
                | "on"
                | "selected_endpoints"
                | "true"
                | "false"
        )
}
pub(super) fn valid_qualified(name: &str) -> bool {
    name.len() <= 193 && name.split('.').all(valid_identifier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn retrieval_and_identifier_escape_hatches_are_closed() {
        for uri in [
            "https://example.invalid/schema",
            "file:///forbidden-schema",
            "urn:missing",
        ] {
            assert!(
                jsonschema::draft202012::options()
                    .with_retriever(DenyRetrieval)
                    .build(&json!({"$ref":uri}))
                    .is_err()
            );
        }
        for bad in ["", "windows", "hello\n", "a-b", "é", "a..b"] {
            assert!(!valid_identifier(bad));
        }
        for good in ["target", "list", "profile", "lab", "inspect"] {
            assert!(valid_identifier(good));
        }
    }

    #[test]
    fn type_occurrence_budget_counts_cache_independent_occurrences() {
        let ty = json!({"kind":"named","name":"int"});
        let functions=(0..512).map(|_|json!({"parameters":(0..63).map(|_|json!({"type":ty})).collect::<Vec<_>>(),"result":ty})).collect::<Vec<_>>();
        let mut document = json!({"functions":functions});
        assert!(precheck_limits(&document).is_ok());
        document["functions"][0]["parameters"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":ty}));
        assert_eq!(
            precheck_limits(&document).unwrap_err().reason,
            "registry-type-nodes"
        );
    }
}
