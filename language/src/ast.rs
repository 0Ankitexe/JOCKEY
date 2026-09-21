//! Owned, serializable syntax tree for the JOCKY Version 1 language surface.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Span;

pub const AST_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AstDocument {
    pub schema_version: String,
    pub program: Program,
}

impl AstDocument {
    #[must_use]
    pub fn new(program: Program) -> Self {
        Self {
            schema_version: AST_SCHEMA_VERSION.to_owned(),
            program,
        }
    }

    pub fn to_pretty_json(&self) -> Result<Vec<u8>, EmitError> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(EmitError::Serialize)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[derive(Debug)]
pub enum EmitError {
    Serialize(serde_json::Error),
}

impl fmt::Display for EmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AST JSON serialization failed")
    }
}

impl Error for EmitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Program {
    pub span: Span,
    pub module: ModuleDeclaration,
    pub target: TargetDeclaration,
    pub functions: Vec<FunctionDeclaration>,
    pub run: Option<RunStatement>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Identifier {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleDeclaration {
    pub name: Identifier,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TargetDeclaration {
    pub platforms: Vec<TargetPlatformNode>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetPlatformNode {
    Windows { span: Span },
    Ubuntu { span: Span },
}

impl TargetPlatformNode {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Windows { span } | Self::Ubuntu { span } => *span,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionDeclaration {
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeReference,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Parameter {
    pub name: Identifier,
    pub type_ref: TypeReference,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeReference {
    Named {
        name: Identifier,
        span: Span,
    },
    List {
        element: Box<TypeReference>,
        span: Span,
    },
    Option {
        element: Box<TypeReference>,
        span: Span,
    },
    Result {
        ok: Box<TypeReference>,
        error: Box<TypeReference>,
        span: Span,
    },
}

impl TypeReference {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Named { span, .. }
            | Self::List { span, .. }
            | Self::Option { span, .. }
            | Self::Result { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Statement {
    Let {
        name: Identifier,
        value: Expression,
        span: Span,
    },
    Call {
        callee: QualifiedName,
        arguments: Vec<Expression>,
        span: Span,
    },
    If {
        condition: Expression,
        then_branch: Block,
        else_branch: Option<Block>,
        span: Span,
    },
    For {
        binding: Identifier,
        collection: Expression,
        body: Block,
        span: Span,
    },
    Return {
        value: Expression,
        span: Span,
    },
}

impl Statement {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Let { span, .. }
            | Self::Call { span, .. }
            | Self::If { span, .. }
            | Self::For { span, .. }
            | Self::Return { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expression {
    Identifier {
        identifier: Identifier,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Integer {
        lexeme: String,
        span: Span,
    },
    Boolean {
        value: bool,
        span: Span,
    },
    Duration {
        magnitude: String,
        unit: DurationUnit,
        span: Span,
    },
    Call {
        callee: QualifiedName,
        arguments: Vec<Expression>,
        span: Span,
    },
}

impl Expression {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Identifier { span, .. }
            | Self::String { span, .. }
            | Self::Integer { span, .. }
            | Self::Boolean { span, .. }
            | Self::Duration { span, .. }
            | Self::Call { span, .. } => *span,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DurationUnit {
    #[serde(rename = "ms")]
    Milliseconds,
    #[serde(rename = "s")]
    Seconds,
    #[serde(rename = "m")]
    Minutes,
    #[serde(rename = "h")]
    Hours,
    #[serde(rename = "d")]
    Days,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualifiedName {
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunStatement {
    pub function: Identifier,
    pub destination: String,
    pub destination_span: Span,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::{AST_SCHEMA_VERSION, AstDocument, Identifier, ModuleDeclaration, Program};
    use crate::Span;

    #[test]
    fn ast_document_has_a_stable_version_and_final_lf() {
        let span = Span::new(0, 1);
        let document = AstDocument::new(Program {
            span,
            module: ModuleDeclaration {
                name: Identifier {
                    text: "m".to_owned(),
                    span,
                },
                span,
            },
            target: super::TargetDeclaration {
                platforms: vec![super::TargetPlatformNode::Windows { span }],
                span,
            },
            functions: vec![],
            run: None,
        });

        let bytes = document.to_pretty_json().expect("serializable AST");
        assert_eq!(document.schema_version, AST_SCHEMA_VERSION);
        assert!(bytes.ends_with(b"\n"));
        assert!(!bytes.ends_with(b"\n\n"));
    }
}
