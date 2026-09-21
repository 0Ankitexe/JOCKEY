//! Strict parser boundary. Successful whole-program recognition is the only
//! path that may construct an AST.

mod builder;
mod recovery;
mod v2;
pub(crate) use v2::parse_source;

use pest::Parser as _;
use pest::error::InputLocation;
use pest_derive::Parser;

use crate::ast::Program;
use crate::diagnostic::{
    Diagnostic, DiagnosticCategory, DiagnosticDetail, DiagnosticSet, ExpectedSyntax, MessageKey,
    ResourceLimitFailure, ResourceLimitKind,
};
use crate::lexer::{LexicalError, preflight};
use crate::{SourceFile, Span};

#[derive(Parser)]
#[grammar = "jocky.pest"]
struct JockyParser;

pub(crate) fn parse(source: SourceFile<'_>) -> Result<Program, DiagnosticSet> {
    let lexical = match preflight(source.text) {
        Ok(lexical) => lexical,
        Err(LexicalError::ResourceLimit { span, .. }) => {
            return Err(DiagnosticSet::from_one(
                ResourceLimitFailure::new(ResourceLimitKind::NestingDepth, span).into_diagnostic(),
            ));
        }
    };

    match JockyParser::parse(Rule::program, source.text) {
        Ok(mut pairs) => {
            let pair = pairs
                .next()
                .ok_or_else(|| internal_build_failure(Span::new(0, 0)))?;
            builder::build_program(pair, source.text.len())
                .map_err(|error| internal_build_failure(error.span))
        }
        Err(error) => {
            let offset = match error.location {
                InputLocation::Pos(position) => position,
                InputLocation::Span((start, _)) => start,
            };
            Err(recovery::recover(source, &lexical, offset))
        }
    }
}

fn internal_build_failure(span: Span) -> DiagnosticSet {
    DiagnosticSet::from_one(
        Diagnostic::new(
            DiagnosticCategory::UnexpectedToken,
            MessageKey::UnexpectedToken,
            span,
        )
        .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Program)),
    )
}

#[cfg(test)]
mod tests {
    use pest::Parser as _;

    use super::{JockyParser, Rule};

    fn accepts(rule: Rule, source: &str) -> bool {
        JockyParser::parse(rule, source).is_ok()
    }

    #[test]
    fn required_program_is_strictly_accepted() {
        let result = JockyParser::parse(
            Rule::program,
            include_str!("../../tests/fixtures/valid/triage.jky"),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn identifiers_reject_reserved_words_and_require_ascii_spelling() {
        assert!(accepts(Rule::identifier_entry, "valid_name2"));
        assert!(!accepts(Rule::identifier_entry, "fn"));
        assert!(!accepts(Rule::identifier_entry, "café"));
    }

    #[test]
    fn keywords_require_a_gap_and_identifier_boundary() {
        assert!(accepts(Rule::module_declaration_entry, "module demo"));
        assert!(!accepts(Rule::module_declaration_entry, "moduledemo"));
        assert!(!accepts(Rule::module_declaration_entry, "module_demo"));
    }

    #[test]
    fn supported_string_escapes_are_closed() {
        assert!(accepts(Rule::string_literal_entry, r#""a\n\r\t\"\\z""#));
        assert!(!accepts(Rule::string_literal_entry, r#""bad\q""#));
        assert!(!accepts(Rule::string_literal_entry, "\"line\nbreak\""));
    }

    #[test]
    fn integer_and_duration_lexemes_are_unbounded_but_canonical() {
        let huge = "9".repeat(2048);
        assert!(accepts(Rule::integer_literal_entry, &huge));
        assert!(accepts(Rule::integer_literal_entry, &format!("-{huge}")));
        assert!(accepts(Rule::duration_literal_entry, &format!("{huge}ms")));
        for invalid in ["+1", "01", "-0", "1.0"] {
            assert!(!accepts(Rule::integer_literal_entry, invalid), "{invalid}");
        }
    }

    #[test]
    fn comments_are_explicit_trivia_and_block_comments_do_not_nest() {
        assert!(accepts(Rule::trivia_entry, " // line\r\n/* block */\t"));
        assert!(!accepts(
            Rule::block_comment_entry,
            "/* outer /* inner */ tail */"
        ));
    }

    #[test]
    fn fragment_entries_are_fully_anchored() {
        assert!(accepts(
            Rule::type_reference_entry,
            "result<list<a>, option<b>>"
        ));
        assert!(!accepts(Rule::type_reference_entry, "item trailing"));
        assert!(!accepts(Rule::expression_entry, "call(), other"));
    }

    #[test]
    fn trailing_parameter_and_argument_commas_are_rejected() {
        assert!(!accepts(
            Rule::function_declaration_entry,
            "fn bad(value: item,) -> report {}"
        ));
        assert!(!accepts(Rule::expression_entry, "bad(value,)"));
    }

    #[test]
    fn only_canonical_target_selectors_are_accepted() {
        for valid in ["target windows", "target ubuntu", "target windows | ubuntu"] {
            assert!(accepts(Rule::target_declaration_entry, valid), "{valid}");
        }
        for invalid in [
            "target macos",
            "target ubuntu | windows",
            "target windows |",
            "target windows | ubuntu | windows",
        ] {
            assert!(
                !accepts(Rule::target_declaration_entry, invalid),
                "{invalid}"
            );
        }
    }
}
