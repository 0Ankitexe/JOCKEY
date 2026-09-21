//! Deterministic recovery for diagnostics after strict whole-source failure.
//!
//! Recovery only classifies malformed islands. It never constructs an AST and
//! therefore cannot widen the language accepted by the strict Pest grammar.

use pest::Parser as _;

use super::{JockyParser, Rule};
use crate::diagnostic::{
    DelimiterKind as DiagnosticDelimiter, Diagnostic, DiagnosticCategory, DiagnosticDetail,
    DiagnosticSet, ExpectedSyntax, MessageKey,
};
use crate::lexer::{
    DelimiterKind, Keyword, LexicalIssueKind, LexicalMap, LexicalToken, LexicalTokenKind,
    Punctuation,
};
use crate::{SourceFile, Span};

pub(super) fn recover(
    source: SourceFile<'_>,
    lexical: &LexicalMap,
    strict_offset: usize,
) -> DiagnosticSet {
    let mut diagnostics = lexical_diagnostics(lexical);
    diagnose_targets(source.text, lexical.tokens(), &mut diagnostics);
    diagnose_generic_types(source.text, lexical.tokens(), &mut diagnostics);
    diagnose_duplicate_runs(source.text, lexical.tokens(), &mut diagnostics);
    diagnose_incomplete_statements(lexical.tokens(), &mut diagnostics);

    if diagnostics.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCategory::UnexpectedToken,
                if source.text.trim().is_empty() {
                    MessageKey::IncompleteProgram
                } else {
                    MessageKey::UnexpectedToken
                },
                token_span_at(source.text, strict_offset),
            )
            .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Program)),
        );
    }

    DiagnosticSet::new(diagnostics).unwrap_or_else(|_| {
        DiagnosticSet::from_one(Diagnostic::new(
            DiagnosticCategory::UnexpectedToken,
            MessageKey::UnexpectedToken,
            token_span_at(source.text, strict_offset),
        ))
    })
}

fn lexical_diagnostics(lexical: &LexicalMap) -> Vec<Diagnostic> {
    lexical
        .issues()
        .iter()
        .map(|issue| match issue.kind() {
            LexicalIssueKind::UnterminatedString => {
                missing_delimiter(issue.span(), DiagnosticDelimiter::StringQuote)
            }
            LexicalIssueKind::UnterminatedBlockComment => {
                missing_delimiter(issue.span(), DiagnosticDelimiter::BlockComment)
            }
            LexicalIssueKind::UnclosedDelimiter {
                opened: DelimiterKind::Angle,
            } => malformed_missing_angle(issue.span()),
            LexicalIssueKind::UnclosedDelimiter { opened } => {
                missing_delimiter(issue.span(), diagnostic_delimiter(opened))
            }
            LexicalIssueKind::MismatchedClosingDelimiter {
                expected: DelimiterKind::Angle,
                ..
            } => malformed_missing_angle(issue.span()),
            LexicalIssueKind::MismatchedClosingDelimiter { expected, .. } => {
                missing_delimiter(issue.span(), diagnostic_delimiter(expected))
            }
            LexicalIssueKind::UnmatchedClosingDelimiter { .. } => Diagnostic::new(
                DiagnosticCategory::UnexpectedToken,
                MessageKey::UnexpectedToken,
                issue.span(),
            ),
        })
        .collect()
}

fn diagnose_targets(source: &str, tokens: &[LexicalToken], diagnostics: &mut Vec<Diagnostic>) {
    diagnose_targets_with_profiles(source, tokens, diagnostics, &[]);
}

fn diagnose_targets_with_profiles(
    source: &str,
    tokens: &[LexicalToken],
    diagnostics: &mut Vec<Diagnostic>,
    profiles: &[usize],
) {
    let targets = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
            token.kind() == LexicalTokenKind::Keyword(Keyword::Target)
                && token.depth_before().total() == 0
        })
        .filter(|(index, _)| is_target_declaration_token(tokens, *index))
        .collect::<Vec<_>>();

    for (ordinal, (index, target)) in targets.into_iter().enumerate() {
        let boundary = next_top_level_boundary(tokens, index + 1).unwrap_or(source.len());
        let boundary = profiles
            .iter()
            .filter(|p| **p > index)
            .map(|p| tokens[*p].span().start)
            .min()
            .map_or(boundary, |p| p.min(boundary));
        let island_end = trim_trivia_end(source, boundary);
        let island = &source[target.span().start..island_end.max(target.span().end)];
        let valid = JockyParser::parse(Rule::target_declaration_entry, island).is_ok();
        if ordinal > 0 || !valid {
            let selector = target_selector_span(source, target.span(), island_end);
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCategory::InvalidTargetDeclaration,
                    MessageKey::InvalidTargetDeclaration,
                    selector,
                )
                .with_detail(DiagnosticDetail::UnsupportedTarget),
            );
        }
    }
}

// Only contextual top-level tokens are candidates; scanner adjacency alone
// never proves a declaration. Strict original slices are checked below.
pub(super) fn profile_candidates(source: &str, tokens: &[LexicalToken]) -> Vec<usize> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            if token.kind() != LexicalTokenKind::Identifier
                || token.depth_before().total() != 0
                || &source[token.span().start..token.span().end] != "profile"
            {
                return None;
            }
            let previous = tokens[..index].iter().rev().find(|t| {
                !matches!(
                    t.kind(),
                    LexicalTokenKind::LineComment | LexicalTokenKind::BlockComment
                )
            });
            if previous.is_some_and(|t| {
                matches!(
                    t.kind(),
                    LexicalTokenKind::Keyword(
                        Keyword::Module
                            | Keyword::Target
                            | Keyword::Fn
                            | Keyword::Run
                            | Keyword::On
                    ) | LexicalTokenKind::Arrow
                        | LexicalTokenKind::Punctuation(Punctuation::Dot)
                )
            }) {
                return None;
            }
            let next = tokens[index + 1..].iter().find(|t| {
                !matches!(
                    t.kind(),
                    LexicalTokenKind::LineComment | LexicalTokenKind::BlockComment
                )
            });
            if next.is_some_and(|t| {
                matches!(
                    t.kind(),
                    LexicalTokenKind::OpenDelimiter(DelimiterKind::Parenthesis)
                )
            }) {
                return None;
            }
            Some(index)
        })
        .collect()
}

pub(super) fn recover_profiles(
    source: SourceFile<'_>,
    lexical: &LexicalMap,
    strict_offset: usize,
    candidates: &[usize],
) -> DiagnosticSet {
    let tokens = lexical.tokens();
    let mut diagnostics = lexical_diagnostics(lexical);
    diagnose_targets_with_profiles(source.text, tokens, &mut diagnostics, candidates);
    diagnose_generic_types(source.text, tokens, &mut diagnostics);
    diagnose_duplicate_runs(source.text, tokens, &mut diagnostics);
    diagnose_incomplete_statements(tokens, &mut diagnostics);
    let target = tokens
        .iter()
        .enumerate()
        .find(|(i, t)| {
            t.depth_before().total() == 0
                && t.kind() == LexicalTokenKind::Keyword(Keyword::Target)
                && is_target_declaration_token(tokens, *i)
        })
        .map(|(_, t)| t.span().start);
    let function = tokens
        .iter()
        .find(|t| {
            t.depth_before().total() == 0 && t.kind() == LexicalTokenKind::Keyword(Keyword::Fn)
        })
        .map(|t| t.span().start);
    for (ordinal, index) in candidates.iter().enumerate() {
        let start = tokens[*index].span().start;
        let boundary = next_top_level_boundary(tokens, *index + 1).unwrap_or(source.text.len());
        let end = candidates
            .get(ordinal + 1)
            .map(|i| tokens[*i].span().start)
            .unwrap_or(boundary)
            .min(boundary);
        let end = trim_trivia_end(source.text, end).max(tokens[*index].span().end);
        let correctly_placed =
            target.is_some_and(|p| p < start) && function.is_some_and(|p| start < p);
        if ordinal > 0
            || !correctly_placed
            || JockyParser::parse(Rule::lab_profile_entry, &source.text[start..end]).is_err()
        {
            diagnostics.push(Diagnostic::new(
                DiagnosticCategory::InvalidProfileDeclaration,
                MessageKey::InvalidProfileDeclaration,
                Span::new(start, end),
            ));
        }
    }
    // A correct profile does not hide an independent strict recognition error.
    if diagnostics.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCategory::UnexpectedToken,
                MessageKey::UnexpectedToken,
                token_span_at(source.text, strict_offset),
            )
            .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Program)),
        );
    }
    DiagnosticSet::new(diagnostics)
        .unwrap_or_else(|_| super::internal_build_failure(Span::new(0, 0)))
}

fn diagnose_generic_types(
    source: &str,
    tokens: &[LexicalToken],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(
            token.kind(),
            LexicalTokenKind::Keyword(Keyword::List | Keyword::Option | Keyword::Result)
        ) {
            continue;
        }

        let Some(next) = tokens.get(index + 1) else {
            if follows_type_introducer(tokens, index) {
                diagnostics.push(malformed_type(token.span()));
            }
            continue;
        };
        if next.kind() != LexicalTokenKind::OpenDelimiter(DelimiterKind::Angle) {
            if follows_type_introducer(tokens, index) {
                diagnostics.push(malformed_type(token.span()));
            }
            continue;
        }

        let Some(close_index) = matching_angle(tokens, index + 1) else {
            // Lexical recovery already reports the unmatched opener.
            continue;
        };
        let end = tokens[close_index].span().end;
        let candidate = &source[token.span().start..end];
        if JockyParser::parse(Rule::type_reference_entry, candidate).is_err() {
            diagnostics.push(malformed_type(Span::new(token.span().start, end)));
        }
    }
}

fn diagnose_duplicate_runs(
    source: &str,
    tokens: &[LexicalToken],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut valid_runs = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind() != LexicalTokenKind::Keyword(Keyword::Run)
            || token.depth_before().total() != 0
        {
            continue;
        }
        let boundary = next_top_level_boundary(tokens, index + 1).unwrap_or(source.len());
        let end = trim_trivia_end(source, boundary).max(token.span().end);
        if JockyParser::parse(Rule::run_statement_entry, &source[token.span().start..end]).is_ok() {
            if valid_runs > 0 {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCategory::DuplicateTopLevelRun,
                        MessageKey::DuplicateTopLevelRun,
                        token.span(),
                    )
                    .with_detail(DiagnosticDetail::DuplicateRun),
                );
            }
            valid_runs += 1;
        }
    }
}

fn diagnose_incomplete_statements(tokens: &[LexicalToken], diagnostics: &mut Vec<Diagnostic>) {
    for (index, token) in tokens.iter().enumerate() {
        match token.kind() {
            LexicalTokenKind::Keyword(Keyword::Return)
                if tokens.get(index + 1).is_none_or(|next| {
                    matches!(
                        next.kind(),
                        LexicalTokenKind::CloseDelimiter(DelimiterKind::Brace)
                    )
                }) =>
            {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCategory::UnexpectedToken,
                        MessageKey::UnexpectedToken,
                        token.span(),
                    )
                    .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Expression)),
                );
            }
            LexicalTokenKind::Keyword(Keyword::In)
                if tokens.get(index + 1).is_none_or(|next| {
                    matches!(
                        next.kind(),
                        LexicalTokenKind::OpenDelimiter(DelimiterKind::Brace)
                    )
                }) =>
            {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCategory::UnexpectedToken,
                        MessageKey::UnexpectedToken,
                        token.span(),
                    )
                    .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Expression)),
                );
            }
            _ => {}
        }
    }
}

fn follows_type_introducer(tokens: &[LexicalToken], index: usize) -> bool {
    tokens.get(index.wrapping_sub(1)).is_some_and(|previous| {
        matches!(
            previous.kind(),
            LexicalTokenKind::Arrow | LexicalTokenKind::Punctuation(Punctuation::Colon)
        )
    })
}

fn matching_angle(tokens: &[LexicalToken], opener_index: usize) -> Option<usize> {
    let opener_depth = tokens.get(opener_index)?.depth_before().angles();
    tokens
        .iter()
        .enumerate()
        .skip(opener_index + 1)
        .find(|(_, token)| {
            token.kind() == LexicalTokenKind::CloseDelimiter(DelimiterKind::Angle)
                && token.depth_after().angles() == opener_depth
        })
        .map(|(index, _)| index)
}

fn next_top_level_boundary(tokens: &[LexicalToken], start: usize) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, token)| {
            if token.depth_before().total() == 0
                && (matches!(
                    token.kind(),
                    LexicalTokenKind::Keyword(Keyword::Module | Keyword::Fn | Keyword::Run)
                ) || (token.kind() == LexicalTokenKind::Keyword(Keyword::Target)
                    && is_target_declaration_token(tokens, index)))
                && token.depth_after().total() == 0
            {
                Some(token.span().start)
            } else {
                None
            }
        })
}

fn is_target_declaration_token(tokens: &[LexicalToken], index: usize) -> bool {
    let previous = tokens[..index].iter().rev().find(|token| {
        !matches!(
            token.kind(),
            LexicalTokenKind::LineComment | LexicalTokenKind::BlockComment
        )
    });
    !previous.is_some_and(|token| {
        matches!(
            token.kind(),
            LexicalTokenKind::Keyword(
                Keyword::Module | Keyword::Target | Keyword::Fn | Keyword::Run | Keyword::On
            ) | LexicalTokenKind::Punctuation(Punctuation::Dot)
        )
    })
}

fn target_selector_span(source: &str, target: Span, end: usize) -> Span {
    let mut start = target.end;
    while start < end {
        let character = source[start..]
            .chars()
            .next()
            .expect("bounded source position");
        if !character.is_whitespace() {
            break;
        }
        start += character.len_utf8();
    }
    if start == end {
        Span::new(target.end, target.end)
    } else {
        Span::new(start, end)
    }
}

fn trim_trivia_end(source: &str, end: usize) -> usize {
    let line_end = source.as_bytes()[..end]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(end, |newline| {
            let after = newline + 1;
            if source[after..end].trim().is_empty() {
                newline
            } else {
                end
            }
        });
    source[..line_end]
        .trim_end_matches(char::is_whitespace)
        .len()
}

fn malformed_type(span: Span) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCategory::MalformedType,
        MessageKey::MalformedType,
        span,
    )
    .with_detail(DiagnosticDetail::Expected(ExpectedSyntax::Type))
}

fn malformed_missing_angle(span: Span) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCategory::MalformedType,
        MessageKey::MalformedType,
        span,
    )
    .with_detail(DiagnosticDetail::MissingDelimiter(
        DiagnosticDelimiter::TypeArgument,
    ))
}

fn missing_delimiter(span: Span, delimiter: DiagnosticDelimiter) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCategory::MissingDelimiter,
        MessageKey::MissingDelimiter,
        span,
    )
    .with_detail(DiagnosticDetail::MissingDelimiter(delimiter))
}

const fn diagnostic_delimiter(delimiter: DelimiterKind) -> DiagnosticDelimiter {
    match delimiter {
        DelimiterKind::Parenthesis => DiagnosticDelimiter::Parenthesis,
        DelimiterKind::Brace => DiagnosticDelimiter::Brace,
        DelimiterKind::Angle => DiagnosticDelimiter::TypeArgument,
    }
}

fn token_span_at(source: &str, offset: usize) -> Span {
    let start = offset.min(source.len());
    source[start..].chars().next().map_or_else(
        || Span::new(start, start),
        |character| Span::new(start, start + character.len_utf8()),
    )
}

#[cfg(test)]
mod tests {
    use super::recover;
    use crate::SourceFile;
    use crate::diagnostic::DiagnosticCategory;
    use crate::lexer::preflight;

    fn categories(source: &str) -> Vec<DiagnosticCategory> {
        let lexical = preflight(source).expect("within resource bound");
        recover(SourceFile::new("test.jky", source), &lexical, 0)
            .iter()
            .map(|diagnostic| diagnostic.category)
            .collect()
    }

    #[test]
    fn classifies_specific_recovery_islands() {
        assert_eq!(
            categories("module m\ntarget macos\nfn f() -> r {}"),
            vec![DiagnosticCategory::InvalidTargetDeclaration]
        );
        assert!(
            categories("module m\ntarget windows\nfn f() -> list<> {}")
                .contains(&DiagnosticCategory::MalformedType)
        );
    }

    #[test]
    fn duplicate_runs_are_reported_without_accepting_a_program() {
        let source = "module m\ntarget windows\nfn f() -> r {}\nrun f on selected_endpoints\nrun f on selected_endpoints\n";
        assert_eq!(
            categories(source),
            vec![DiagnosticCategory::DuplicateTopLevelRun]
        );
    }

    #[test]
    fn top_level_recovery_ignores_comment_placement_and_contextual_identifiers() {
        let target =
            "module m\n/* trivia */ target macos\nfn f(target: endpoint) -> r { return target }";
        assert_eq!(
            categories(target),
            vec![DiagnosticCategory::InvalidTargetDeclaration]
        );

        let duplicate = "module m\ntarget windows\nfn target() -> r {}\n/* gap */ run target on selected_endpoints\n/* gap */ run target on selected_endpoints";
        assert_eq!(
            categories(duplicate),
            vec![DiagnosticCategory::DuplicateTopLevelRun]
        );
    }
}
