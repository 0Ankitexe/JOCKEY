//! Project-owned causes and bounded, source-ordered diagnostics.
use crate::{Span, escape_filename};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Severity {
    Error,
    Warning,
}
impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

macro_rules! codes {
    ($($variant:ident=$rank:literal=>$name:literal,$label:literal;)+) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
        pub enum DiagnosticCode {$($variant=$rank,)+}
        impl DiagnosticCode {
            pub const fn as_str(self)->&'static str {match self {$(Self::$variant=>$name,)+}}
            pub const fn label(self)->&'static str {match self {$(Self::$variant=>$label,)+}}
        }
    }
}
codes! {
    DuplicateDefinition=100=>"duplicate-definition","conflicting definition";
    UnknownName=110=>"unknown-name","name is not defined";
    UnknownType=120=>"unknown-type","type is not defined";
    NotCallable=130=>"not-callable","expected a function";
    ArgumentCountMismatch=140=>"argument-count-mismatch","argument count mismatch";
    ArgumentTypeMismatch=150=>"argument-type-mismatch","argument type mismatch";
    ReturnTypeMismatch=160=>"return-type-mismatch","return type mismatch";
    MissingReturn=170=>"missing-return","return value required";
    ConditionNotBool=180=>"condition-not-bool","expected bool";
    NotACollection=190=>"not-a-collection","expected a list";
    MissingRun=200=>"missing-run","run statement required";
    InvalidEntryPoint=210=>"invalid-entry-point","invalid run entry point";
    UnsupportedTarget=220=>"unsupported-target","target support mismatch";
    LabProfileRequired=230=>"lab-profile-required","lab profile required";
    UnreachableCode=300=>"unreachable-code","unreachable statement";
    UnusedBinding=310=>"unused-binding","unused binding";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryFailure {
    UnknownFunction,
    WrongSignature,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cause {
    DuplicateDefinition(String),
    UnknownName(String),
    UnknownType(String),
    NotCallable(String),
    ArgumentCount { expected: usize, actual: usize },
    ArgumentType { expected: String, actual: String },
    ReturnType { expected: String, actual: String },
    MissingReturn(String),
    ConditionNotBool(String),
    NotACollection(String),
    MissingRun,
    InvalidEntryPoint(EntryFailure),
    UnsupportedTarget(String),
    LabProfileRequired(String),
    UnreachableCode,
    UnusedBinding(String),
}
impl Cause {
    pub const fn code(&self) -> DiagnosticCode {
        use DiagnosticCode as C;
        match self {
            Self::DuplicateDefinition(_) => C::DuplicateDefinition,
            Self::UnknownName(_) => C::UnknownName,
            Self::UnknownType(_) => C::UnknownType,
            Self::NotCallable(_) => C::NotCallable,
            Self::ArgumentCount { .. } => C::ArgumentCountMismatch,
            Self::ArgumentType { .. } => C::ArgumentTypeMismatch,
            Self::ReturnType { .. } => C::ReturnTypeMismatch,
            Self::MissingReturn(_) => C::MissingReturn,
            Self::ConditionNotBool(_) => C::ConditionNotBool,
            Self::NotACollection(_) => C::NotACollection,
            Self::MissingRun => C::MissingRun,
            Self::InvalidEntryPoint(_) => C::InvalidEntryPoint,
            Self::UnsupportedTarget(_) => C::UnsupportedTarget,
            Self::LabProfileRequired(_) => C::LabProfileRequired,
            Self::UnreachableCode => C::UnreachableCode,
            Self::UnusedBinding(_) => C::UnusedBinding,
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::DuplicateDefinition(n) => {
                format!("duplicate or reserved definition: {}", escape_filename(n))
            }
            Self::UnknownName(n) => format!("unknown name: {}", escape_filename(n)),
            Self::UnknownType(n) => format!("unknown type: {}", escape_filename(n)),
            Self::NotCallable(n) => format!("value is not callable: {}", escape_filename(n)),
            Self::ArgumentCount { expected, actual } => {
                format!("wrong argument count: expected {expected}, found {actual}")
            }
            Self::ArgumentType { expected, actual } => {
                format!("argument type mismatch: expected {expected}, found {actual}")
            }
            Self::ReturnType { expected, actual } => {
                format!("return type mismatch: expected {expected}, found {actual}")
            }
            Self::MissingReturn(t) => format!("function may finish without returning {t}"),
            Self::ConditionNotBool(t) => format!("condition must have type bool; found {t}"),
            Self::NotACollection(t) => format!("for requires list<T>; found {t}"),
            Self::MissingRun => "program requires one top-level run statement".into(),
            Self::InvalidEntryPoint(_) => {
                "run entry must name a source function with signature (endpoint) -> forensic_result"
                    .into()
            }
            Self::UnsupportedTarget(n) => format!(
                "function does not support every declared target: {}",
                escape_filename(n)
            ),
            Self::LabProfileRequired(n) => format!(
                "function requires an explicit profile lab declaration: {}",
                escape_filename(n)
            ),
            Self::UnreachableCode => "statement is unreachable".into(),
            Self::UnusedBinding(n) => {
                format!("local binding is never used: {}", escape_filename(n))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticDiagnostic {
    pub span: Span,
    pub cause: Cause,
}
impl SemanticDiagnostic {
    pub const fn code(&self) -> DiagnosticCode {
        self.cause.code()
    }
    pub const fn severity(&self) -> Severity {
        match self.cause {
            Cause::UnreachableCode | Cause::UnusedBinding(_) => Severity::Warning,
            _ => Severity::Error,
        }
    }
    pub fn message(&self) -> String {
        self.cause.message()
    }
    pub const fn label(&self) -> &'static str {
        self.code().label()
    }
    fn compare(&self, other: &Self) -> Ordering {
        (self.span.start, self.severity(), self.code(), self.span.end)
            .cmp(&(
                other.span.start,
                other.severity(),
                other.code(),
                other.span.end,
            ))
            .then_with(|| self.message().cmp(&other.message()))
            .then_with(|| self.label().cmp(other.label()))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticDiagnostics {
    items: Vec<SemanticDiagnostic>,
    errors_truncated: bool,
    warnings_truncated: bool,
    saw_error: bool,
}
impl SemanticDiagnostics {
    pub fn items(&self) -> &[SemanticDiagnostic] {
        &self.items
    }
    pub const fn has_errors(&self) -> bool {
        self.saw_error
    }
    pub const fn errors_truncated(&self) -> bool {
        self.errors_truncated
    }
    pub const fn warnings_truncated(&self) -> bool {
        self.warnings_truncated
    }
    pub(super) fn push(&mut self, span: Span, cause: Cause) {
        let diagnostic = SemanticDiagnostic { span, cause };
        let severity = diagnostic.severity();
        self.saw_error |= severity == Severity::Error;
        match self
            .items
            .binary_search_by(|item| item.compare(&diagnostic))
        {
            Ok(_) => return,
            Err(index) => self.items.insert(index, diagnostic),
        }
        if self
            .items
            .iter()
            .filter(|d| d.severity() == severity)
            .count()
            > 32
        {
            if let Some(index) = self.items.iter().rposition(|d| d.severity() == severity) {
                self.items.remove(index);
            }
            match severity {
                Severity::Error => self.errors_truncated = true,
                Severity::Warning => self.warnings_truncated = true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn causes_have_closed_unique_codes_messages_and_labels() {
        let causes = [
            Cause::DuplicateDefinition("x".into()),
            Cause::UnknownName("x".into()),
            Cause::UnknownType("x".into()),
            Cause::NotCallable("x".into()),
            Cause::ArgumentCount {
                expected: 1,
                actual: 2,
            },
            Cause::ArgumentType {
                expected: "int".into(),
                actual: "bool".into(),
            },
            Cause::ReturnType {
                expected: "int".into(),
                actual: "bool".into(),
            },
            Cause::MissingReturn("int".into()),
            Cause::ConditionNotBool("int".into()),
            Cause::NotACollection("bytes".into()),
            Cause::MissingRun,
            Cause::InvalidEntryPoint(EntryFailure::UnknownFunction),
            Cause::UnsupportedTarget("f".into()),
            Cause::LabProfileRequired("f".into()),
            Cause::UnreachableCode,
            Cause::UnusedBinding("x".into()),
        ];
        assert_eq!(
            causes
                .iter()
                .map(Cause::code)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            16
        );
        for cause in causes {
            assert!(!cause.message().is_empty());
            assert!(!cause.code().label().is_empty());
        }
    }
    #[test]
    fn dedup_and_capping_keep_first_by_source_not_emission_order() {
        let mut set = SemanticDiagnostics::default();
        for index in (0..40).rev() {
            for _ in 0..2 {
                set.push(Span::new(index, index), Cause::MissingRun);
                set.push(Span::new(index, index), Cause::UnreachableCode);
            }
        }
        assert_eq!(set.items.len(), 64);
        assert!(set.has_errors());
        assert!(set.errors_truncated && set.warnings_truncated);
        assert_eq!(set.items.last().unwrap().span.start, 31);
        let mut exact = SemanticDiagnostics::default();
        for _ in 0..100 {
            exact.push(Span::new(0, 0), Cause::MissingRun);
        }
        assert_eq!(exact.items.len(), 1);
        assert!(!exact.errors_truncated);
    }
}
