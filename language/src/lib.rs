//! Pure, syntax-only frontend for the JOCKY language.
//!
//! The library consumes caller-supplied UTF-8 text and never interprets the
//! diagnostic filename label as a path. Host file and stream access belongs to
//! the separate [`cli`] adapter.

#![forbid(unsafe_code)]

pub mod ast;
pub mod cli;
mod diagnostic;
mod lexer;
pub mod lowering;
mod parser;
pub mod report;
pub mod semantic;
pub mod source;
mod source_map;

pub use ast::{AST_SCHEMA_VERSION, AstDocument, EmitError, Program};
pub use diagnostic::{
    DelimiterKind, Diagnostic, DiagnosticCategory, DiagnosticDetail, DiagnosticSet,
    DiagnosticSeverity, EmptyDiagnosticSet, ExpectedSyntax, MessageKey, ResourceLimitFailure,
    ResourceLimitKind, render_diagnostics,
};
pub use source::{LabProfileDeclaration, ParsedSource};
pub use source_map::{
    Location as SourceLocation, SourceMap, Span, escape_filename, escape_source_line,
};

/// Parse the profile-aware syntax envelope, without semantic analysis or I/O.
pub fn parse_source(source: SourceFile<'_>) -> Result<ParsedSource, DiagnosticSet> {
    parser::parse_source(source)
}

/// A borrowed input supplied to the pure syntax frontend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFile<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

impl<'a> SourceFile<'a> {
    #[must_use]
    pub const fn new(name: &'a str, text: &'a str) -> Self {
        Self { name, text }
    }
}

/// Parse one complete Version 1 program without performing host I/O or
/// semantic analysis.
pub fn parse(source: SourceFile<'_>) -> Result<Program, DiagnosticSet> {
    parser::parse(source)
}

/// Check Version 1 syntax and discard only a successfully built syntax tree.
pub fn check(source: SourceFile<'_>) -> Result<(), DiagnosticSet> {
    parse(source).map(|_| ())
}
