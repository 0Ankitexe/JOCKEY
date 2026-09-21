use serde::{Deserialize, Serialize};
use std::fmt;

vocabulary!(NamedType {
    Bool => "bool", Int => "int", String => "string", Bytes => "bytes",
    Duration => "duration", Timestamp => "timestamp", Endpoint => "endpoint",
    Platform => "platform", IpAddress => "ip_address", Path => "path",
    ProcessRecord => "process_record", ConnectionRecord => "connection_record",
    FileRecord => "file_record", EventRecord => "event_record",
    PersistenceRecord => "persistence_record", DriverRecord => "driver_record",
    ForensicResult => "forensic_result", DiagnosticError => "diagnostic_error",
});

/// Exact, span-free contract type. Untrusted documents are bounded by their reader.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Type {
    Named { name: NamedType },
    List { element: Box<Type> },
    Option { element: Box<Type> },
    Result { ok: Box<Type>, error: Box<Type> },
}

impl Type {
    #[must_use]
    pub const fn named(name: NamedType) -> Self {
        Self::Named { name }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named { name } => write!(f, "{name}"),
            Self::List { element } => write!(f, "list<{element}>"),
            Self::Option { element } => write!(f, "option<{element}>"),
            Self::Result { ok, error } => write!(f, "result<{ok}, {error}>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vocabulary_is_bijective_and_exact() {
        for name in NamedType::ALL {
            assert_eq!(NamedType::from_name(name.as_str()), Some(name));
        }
        assert_eq!(NamedType::from_name("any"), None);
        assert_eq!(Type::named(NamedType::IpAddress).to_string(), "ip_address");
    }
}
