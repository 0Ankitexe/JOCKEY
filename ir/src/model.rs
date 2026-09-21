//! Untrusted, separately versioned IR data. Construction is not verification.
use crate::value::Value;
use jocky_forensic::contracts::FunctionContract;
use jocky_shared::compiler::{Capability, Platform, Privilege, Type};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "1.0.0";
pub const LANGUAGE_VERSION: &str = "2.0.0";
pub const REGISTRY_VERSION: &str = "1.0.0";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub label: String,
    pub byte_length: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryIdentity {
    pub schema_version: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub name: String,
    pub span: Option<Span>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Lab,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub required_privilege: Privilege,
    pub capabilities: Vec<Capability>,
    pub supported_platforms: Vec<Platform>,
    pub unavailable_dependencies: Vec<String>,
    pub read_only: bool,
    pub lab_only: bool,
}
impl Default for Metadata {
    fn default() -> Self {
        Self {
            required_privilege: Privilege::User,
            capabilities: Vec::new(),
            supported_platforms: vec![Platform::Windows, Platform::Ubuntu],
            unavailable_dependencies: Vec::new(),
            read_only: true,
            lab_only: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IrDocument {
    pub schema_version: String,
    pub language_version: String,
    pub build_seed: String,
    pub source: SourceIdentity,
    pub registry: RegistryIdentity,
    pub module: Module,
    pub targets: Vec<Platform>,
    pub profile: Option<Profile>,
    pub entry: usize,
    pub entry_metadata: Metadata,
    pub contracts: Vec<FunctionContract>,
    pub constants: Vec<Value>,
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: String,
    pub span: Option<Span>,
    pub parameters: Vec<usize>,
    pub result: Type,
    pub slots: Vec<Slot>,
    pub root_region: usize,
    pub regions: Vec<Region>,
    pub metadata: Metadata,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    Parameter,
    Local,
    Temporary,
    LoopBinding,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub r#type: Type,
    pub region: usize,
    pub kind: SlotKind,
    pub name: Option<String>,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Instruction {
    pub id: String,
    pub span: Option<Span>,
    #[serde(flatten)]
    pub operation: Operation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Const {
        destination: usize,
        constant: usize,
    },
    Copy {
        destination: usize,
        source: usize,
    },
    Call {
        destination: usize,
        callee: Callee,
        arguments: Vec<usize>,
        on_error: ErrorPolicy,
    },
    If {
        condition: usize,
        then_region: usize,
        else_region: Option<usize>,
    },
    ForEach {
        collection: usize,
        item: usize,
        body_region: usize,
    },
    Return {
        value: usize,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Callee {
    Source { function: usize },
    Contract { contract: usize },
    Composition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPolicy {
    Propagate,
}

impl Operation {
    pub(crate) fn destination(&self) -> Option<usize> {
        match self {
            Self::Const { destination, .. }
            | Self::Copy { destination, .. }
            | Self::Call { destination, .. } => Some(*destination),
            _ => None,
        }
    }
    pub(crate) fn reads(&self) -> &[usize] {
        match self {
            Self::Const { .. } => &[],
            Self::Copy { source, .. } => std::slice::from_ref(source),
            Self::Call { arguments, .. } => arguments,
            Self::If { condition, .. } => std::slice::from_ref(condition),
            Self::ForEach { collection, .. } => std::slice::from_ref(collection),
            Self::Return { value } => std::slice::from_ref(value),
        }
    }
    pub(crate) fn children(&self) -> [Option<usize>; 2] {
        match self {
            Self::If {
                then_region,
                else_region,
                ..
            } => [Some(*then_region), *else_region],
            Self::ForEach { body_region, .. } => [Some(*body_region), None],
            _ => [None, None],
        }
    }
}

// Untrusted programmatic trees can exceed wire limits. Release their recursive
// values/types iteratively even when verification rejects them immediately.
impl Drop for IrDocument {
    fn drop(&mut self) {
        crate::value::drain_values(&mut self.constants);
        for f in &mut self.functions {
            crate::value::drain_type(&mut f.result);
            for slot in &mut f.slots {
                crate::value::drain_type(&mut slot.r#type);
            }
        }
        for c in &mut self.contracts {
            crate::value::drain_type(&mut c.result);
            for p in &mut c.parameters {
                crate::value::drain_type(&mut p.r#type);
            }
            for e in &mut c.possible_errors {
                crate::value::drain_type(&mut e.r#type);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opcode_shapes_and_metadata_defaults_are_closed() {
        for op in [
            Operation::Const {
                destination: 1,
                constant: 0,
            },
            Operation::Copy {
                destination: 1,
                source: 0,
            },
            Operation::Call {
                destination: 1,
                callee: Callee::Composition,
                arguments: vec![0, 0],
                on_error: ErrorPolicy::Propagate,
            },
            Operation::If {
                condition: 0,
                then_region: 1,
                else_region: None,
            },
            Operation::ForEach {
                collection: 0,
                item: 1,
                body_region: 1,
            },
            Operation::Return { value: 1 },
        ] {
            let bytes = serde_json::to_vec(&op).unwrap();
            assert_eq!(serde_json::from_slice::<Operation>(&bytes).unwrap(), op);
        }
        assert!(serde_json::from_str::<Operation>(r#"{"op":"while"}"#).is_err());
        assert!(serde_json::from_str::<ErrorPolicy>(r#""retry""#).is_err());
        assert_eq!(
            Metadata::default().supported_platforms,
            vec![Platform::Windows, Platform::Ubuntu]
        );
    }
}
