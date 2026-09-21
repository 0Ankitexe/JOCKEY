//! Closed, bounded failures: no native error text or evidence payloads.
use crate::model::Span;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiagnosticCode {
    Json,
    Schema,
    Version,
    Identity,
    Reference,
    Type,
    Initialization,
    Control,
    Return,
    Contract,
    Metadata,
    Resource,
}
impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "IR_JSON",
            Self::Schema => "IR_SCHEMA",
            Self::Version => "IR_VERSION",
            Self::Identity => "IR_IDENTITY",
            Self::Reference => "IR_REFERENCE",
            Self::Type => "IR_TYPE",
            Self::Initialization => "IR_INITIALIZATION",
            Self::Control => "IR_CONTROL",
            Self::Return => "IR_RETURN",
            Self::Contract => "IR_CONTRACT",
            Self::Metadata => "IR_METADATA",
            Self::Resource => "IR_RESOURCE",
        }
    }
    pub const fn message(self) -> &'static str {
        match self {
            Self::Json => "invalid complete UTF-8 JSON or duplicate key",
            Self::Schema => "IR contract shape or value format is invalid",
            Self::Version => "unsupported compiler contract version",
            Self::Identity => "inconsistent instruction identity",
            Self::Reference => "invalid reference, ownership or source span",
            Self::Type => "incompatible exact IR types",
            Self::Initialization => "invalid slot initialization",
            Self::Control => "invalid structured control flow",
            Self::Return => "invalid entry or function return",
            Self::Contract => "external registry contract mismatch",
            Self::Metadata => "inconsistent static requirements",
            Self::Resource => "IR resource limit exceeded",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub pointer: Option<String>,
    pub instruction_id: Option<String>,
    pub span: Option<Span>,
    pub(crate) position: Option<(usize, Option<usize>, Option<usize>)>,
}
impl Diagnostic {
    pub fn message(&self) -> &'static str {
        self.code.message()
    }
    pub(crate) fn at(code: DiagnosticCode, pointer: impl Into<String>) -> Self {
        let pointer = pointer.into();
        let fields: Vec<_> = pointer.split('/').collect();
        let position = if fields.get(1) == Some(&"functions") {
            fields.get(2).and_then(|f| f.parse().ok()).map(|f| {
                let region = if fields.get(3) == Some(&"regions") {
                    fields.get(4).and_then(|s| s.parse().ok())
                } else {
                    None
                };
                let instruction = if fields.get(5) == Some(&"instructions") {
                    fields.get(6).and_then(|s| s.parse().ok())
                } else {
                    None
                };
                (f, region, instruction)
            })
        } else {
            None
        };
        Self {
            code,
            pointer: Some(pointer),
            instruction_id: None,
            span: None,
            position,
        }
    }
    pub(crate) fn instruction(
        code: DiagnosticCode,
        f: usize,
        r: usize,
        i: usize,
        ins: &crate::model::Instruction,
        field: &str,
    ) -> Self {
        Self {
            code,
            pointer: Some(format!(
                "/functions/{f}/regions/{r}/instructions/{i}{field}"
            )),
            instruction_id: Some(ins.id.clone()),
            span: ins.span,
            position: Some((f, Some(r), Some(i))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failure {
    diagnostics: Vec<Diagnostic>,
    truncated: bool,
}
pub type IrReadFailure = Failure;
pub type IrWriteFailure = Failure;
pub type VerifyFailure = Failure;
impl Failure {
    pub fn code(&self) -> DiagnosticCode {
        self.diagnostics[0].code
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
    pub(crate) fn at(code: DiagnosticCode, pointer: impl Into<String>) -> Self {
        Self {
            diagnostics: vec![Diagnostic::at(code, pointer)],
            truncated: false,
        }
    }
    pub(crate) fn resource() -> Self {
        Self::at(DiagnosticCode::Resource, "")
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code().as_str(), self.code().message())
    }
}
impl Error for Failure {}

#[derive(Default)]
pub(crate) struct Diagnostics {
    items: Vec<Diagnostic>,
    truncated: bool,
}
impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        if self.items.contains(&diagnostic) {
            return;
        }
        self.items.push(diagnostic);
        self.items.sort_by(|a, b| {
            (a.position, a.code.as_str(), &a.pointer).cmp(&(
                b.position,
                b.code.as_str(),
                &b.pointer,
            ))
        });
        if self.items.len() > 20 {
            self.items.pop();
            self.truncated = true;
        }
    }
    pub fn finish(self) -> Result<(), Failure> {
        if self.items.is_empty() {
            Ok(())
        } else {
            Err(Failure {
                diagnostics: self.items,
                truncated: self.truncated,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_are_closed_fixed_and_sorted_capped_without_payloads() {
        let codes = [
            DiagnosticCode::Json,
            DiagnosticCode::Schema,
            DiagnosticCode::Version,
            DiagnosticCode::Identity,
            DiagnosticCode::Reference,
            DiagnosticCode::Type,
            DiagnosticCode::Initialization,
            DiagnosticCode::Control,
            DiagnosticCode::Return,
            DiagnosticCode::Contract,
            DiagnosticCode::Metadata,
            DiagnosticCode::Resource,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for code in codes {
            assert!(seen.insert(code.as_str()));
            assert!(!code.message().is_empty());
        }
        let mut diagnostics = Diagnostics::default();
        for i in (0..25).rev() {
            let d = Diagnostic::at(DiagnosticCode::Type, format!("/{i:02}"));
            diagnostics.push(d.clone());
            diagnostics.push(d);
        }
        let failure = diagnostics.finish().unwrap_err();
        assert_eq!(failure.diagnostics().len(), 20);
        assert!(failure.truncated());
        assert_eq!(failure.diagnostics()[0].pointer.as_deref(), Some("/00"));
        assert!(!Failure::resource().truncated());
        assert_eq!(Failure::resource().diagnostics().len(), 1);
    }
}
