//! Deterministic, bounded lowering of an actually checked program. No file APIs.
mod context;
mod expressions;
mod identity;
mod statements;
use crate::{Span, semantic::CheckedProgram};
use jocky_ir::{VerifiedProgram, VerifyFailure};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildOptions {
    pub seed: u64,
}

#[derive(Debug)]
pub enum LoweringFailure {
    Invariant(Option<Span>),
    Resource(Option<Span>),
    Verification(VerifyFailure),
}
impl LoweringFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invariant(_) => "LOWER_INVARIANT",
            Self::Resource(_) => "LOWER_RESOURCE",
            Self::Verification(e) => e.code().as_str(),
        }
    }
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Invariant(s) | Self::Resource(s) => *s,
            Self::Verification(e) => e
                .diagnostics()
                .first()
                .and_then(|d| d.span)
                .map(|s| Span::new(s.start, s.end)),
        }
    }
}
impl From<VerifyFailure> for LoweringFailure {
    fn from(e: VerifyFailure) -> Self {
        if e.code() == jocky_ir::DiagnosticCode::Resource {
            Self::Resource(None)
        } else {
            Self::Verification(e)
        }
    }
}
type Result<T> = std::result::Result<T, LoweringFailure>;
fn need<T>(value: Option<T>, span: Option<Span>) -> Result<T> {
    value.ok_or(LoweringFailure::Invariant(span))
}
fn span(s: Span) -> Option<jocky_ir::model::Span> {
    Some(jocky_ir::model::Span {
        start: s.start,
        end: s.end,
    })
}

/// The registry is captured by analysis. There is deliberately no replacement
/// registry or unchecked AST overload at this boundary.
pub fn lower(checked: &CheckedProgram, options: BuildOptions) -> Result<VerifiedProgram> {
    let mut context = context::Context::new(checked, options)?;
    for (index, function) in checked.syntax().program.functions.iter().enumerate() {
        context.function(index, function)?;
    }
    jocky_ir::identity::assign_instruction_ids(&mut context.document)?;
    jocky_ir::verify(context.document, checked.registry_snapshot()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_internal_semantic_facts_are_typed_not_defaults() {
        assert!(matches!(
            need::<usize>(None, Some(Span::new(0, 1))),
            Err(LoweringFailure::Invariant(Some(_)))
        ));
        assert_eq!(LoweringFailure::Resource(None).code(), "LOWER_RESOURCE");
        let text = include_str!("../../../examples/triage.jky");
        let checked = crate::semantic::analyze(
            crate::SourceFile::new("x", text),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        let mut c = context::Context::new(&checked, BuildOptions::default()).unwrap();
        c.functions.clear();
        assert!(matches!(
            c.function(0, &checked.syntax().program.functions[0]),
            Err(LoweringFailure::Invariant(_))
        ));
    }
}
