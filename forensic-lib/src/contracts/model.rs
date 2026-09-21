use jocky_shared::compiler::{Capability, Platform, Privilege, Type};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationAvailability {
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PossibleErrorCode {
    NotImplemented,
    Unsupported,
    PermissionDenied,
    InvalidInput,
    NotFound,
    ResourceLimit,
    Timeout,
    CollectionFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub name: String,
    pub r#type: Type,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PossibleError {
    pub code: PossibleErrorCode,
    pub r#type: Type,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionContract {
    pub module: String,
    pub name: String,
    pub version: String,
    pub parameters: Vec<Parameter>,
    pub result: Type,
    pub supported_platforms: Vec<Platform>,
    pub required_privilege: Privilege,
    pub capability: Capability,
    pub read_only: bool,
    pub lab_only: bool,
    pub possible_errors: Vec<PossibleError>,
    pub availability: ImplementationAvailability,
}
impl FunctionContract {
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistryDocument {
    pub schema_version: String,
    pub functions: Vec<FunctionContract>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryErrorKind {
    InvalidJson,
    DuplicateJsonKey,
    UnsupportedVersion,
    SchemaViolation,
    DuplicateFunction,
    DuplicateParameter,
    DuplicateErrorCode,
    InvalidIdentifier,
    ResourceLimit,
}

/// Project-owned error details. `reason` is a fixed key, never input/error prose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryError {
    pub kind: RegistryErrorKind,
    pub pointer: Option<String>,
    pub reason: &'static str,
}
impl RegistryError {
    pub(super) fn new(kind: RegistryErrorKind, reason: &'static str) -> Self {
        Self {
            kind,
            pointer: None,
            reason,
        }
    }
    pub(super) fn at(mut self, pointer: String) -> Self {
        self.pointer = Some(pointer);
        self
    }
}
impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "registry error: {}", self.reason)
    }
}
impl Error for RegistryError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupError {
    InvalidName,
    UnknownFunction,
}
impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidName => "invalid module function name",
            Self::UnknownFunction => "unknown module function",
        })
    }
}
impl Error for LookupError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enums_and_errors_never_claim_availability_or_leak_payloads() {
        assert!(serde_json::from_str::<ImplementationAvailability>("\"implemented\"").is_err());
        assert!(serde_json::from_str::<PossibleErrorCode>("\"Anything\"").is_err());
        assert_eq!(
            RegistryError::new(RegistryErrorKind::InvalidJson, "invalid-json").to_string(),
            "registry error: invalid-json"
        );
        assert_eq!(
            LookupError::InvalidName.to_string(),
            "invalid module function name"
        );
    }
}
