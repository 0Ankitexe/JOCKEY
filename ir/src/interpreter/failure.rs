use crate::model::{Instruction, Span};
use jocky_forensic::contracts::PossibleErrorCode;
use serde::Serialize;

pub(crate) fn provider_code(code: PossibleErrorCode) -> &'static str {
    match code {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ExecutionCode {
    #[serde(rename = "EXEC_INSTRUCTION_LIMIT")]
    InstructionLimit,
    #[serde(rename = "EXEC_WORK_LIMIT")]
    WorkLimit,
    #[serde(rename = "EXEC_COLLECTION_LIMIT")]
    CollectionLimit,
    #[serde(rename = "EXEC_CALL_DEPTH")]
    CallDepth,
    #[serde(rename = "EXEC_VALUE_DEPTH")]
    ValueDepth,
    #[serde(rename = "EXEC_MEMORY_LIMIT")]
    MemoryLimit,
    #[serde(rename = "EXEC_OUTPUT_LIMIT")]
    OutputLimit,
    #[serde(rename = "EXEC_INVALID_VALUE")]
    InvalidValue,
    #[serde(rename = "EXEC_INVALID_COMPOSITION")]
    InvalidComposition,
}
impl ExecutionCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InstructionLimit => "EXEC_INSTRUCTION_LIMIT",
            Self::WorkLimit => "EXEC_WORK_LIMIT",
            Self::CollectionLimit => "EXEC_COLLECTION_LIMIT",
            Self::CallDepth => "EXEC_CALL_DEPTH",
            Self::ValueDepth => "EXEC_VALUE_DEPTH",
            Self::MemoryLimit => "EXEC_MEMORY_LIMIT",
            Self::OutputLimit => "EXEC_OUTPUT_LIMIT",
            Self::InvalidValue => "EXEC_INVALID_VALUE",
            Self::InvalidComposition => "EXEC_INVALID_COMPOSITION",
        }
    }
    pub fn is_resource(self) -> bool {
        !matches!(self, Self::InvalidValue | Self::InvalidComposition)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticOrigin {
    pub instruction_id: String,
    pub span: Option<Span>,
}
impl From<&Instruction> for DiagnosticOrigin {
    fn from(i: &Instruction) -> Self {
        Self {
            instruction_id: i.id.clone(),
            span: i.span,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum ExecutionFailure {
    Provider {
        code: PossibleErrorCode,
        function: String,
        origin: Option<DiagnosticOrigin>,
    },
    Interpreter {
        code: ExecutionCode,
        origin: Option<DiagnosticOrigin>,
    },
}
impl ExecutionFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Provider { code, .. } => provider_code(*code),
            Self::Interpreter { code, .. } => code.as_str(),
        }
    }
    pub fn is_resource(&self) -> bool {
        match self {
            Self::Provider { code, .. } => *code == PossibleErrorCode::ResourceLimit,
            Self::Interpreter { code, .. } => code.is_resource(),
        }
    }
    pub fn origin(&self) -> Option<&DiagnosticOrigin> {
        match self {
            Self::Provider { origin, .. } | Self::Interpreter { origin, .. } => origin.as_ref(),
        }
    }
    pub(crate) fn interpreter(code: ExecutionCode, origin: Option<&Instruction>) -> Self {
        Self::Interpreter {
            code,
            origin: origin.map(Into::into),
        }
    }
}
