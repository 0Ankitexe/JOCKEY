//! Iterative lexical preflight for bounded recovery.
//!
//! This module deliberately does not decide whether a source document is valid
//! JOCKY. The Pest grammar is the sole acceptance authority. Preflight records
//! opaque strings/comments, identifier boundaries, punctuation, and delimiter
//! depth so the parser can reject excessive nesting before recursive parsing
//! and recovery can synchronize without looking inside opaque regions.

use crate::source_map::Span;

pub(crate) const MAX_DELIMITER_DEPTH: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Keyword {
    Module,
    Target,
    Windows,
    Ubuntu,
    Fn,
    Let,
    If,
    Else,
    For,
    In,
    Return,
    Run,
    On,
    SelectedEndpoints,
    True,
    False,
    List,
    Option,
    Result,
}

impl Keyword {
    fn from_spelling(spelling: &str) -> Option<Self> {
        match spelling {
            "module" => Some(Self::Module),
            "target" => Some(Self::Target),
            "windows" => Some(Self::Windows),
            "ubuntu" => Some(Self::Ubuntu),
            "fn" => Some(Self::Fn),
            "let" => Some(Self::Let),
            "if" => Some(Self::If),
            "else" => Some(Self::Else),
            "for" => Some(Self::For),
            "in" => Some(Self::In),
            "return" => Some(Self::Return),
            "run" => Some(Self::Run),
            "on" => Some(Self::On),
            "selected_endpoints" => Some(Self::SelectedEndpoints),
            "true" => Some(Self::True),
            "false" => Some(Self::False),
            "list" => Some(Self::List),
            "option" => Some(Self::Option),
            "result" => Some(Self::Result),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DelimiterKind {
    Parenthesis,
    Brace,
    Angle,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DelimiterDepth {
    parentheses: usize,
    braces: usize,
    angles: usize,
}

impl DelimiterDepth {
    pub(crate) const fn angles(self) -> usize {
        self.angles
    }

    pub(crate) const fn total(self) -> usize {
        self.parentheses + self.braces + self.angles
    }

    fn push(&mut self, kind: DelimiterKind) {
        match kind {
            DelimiterKind::Parenthesis => self.parentheses += 1,
            DelimiterKind::Brace => self.braces += 1,
            DelimiterKind::Angle => self.angles += 1,
        }
    }

    fn pop(&mut self, kind: DelimiterKind) {
        let depth = match kind {
            DelimiterKind::Parenthesis => &mut self.parentheses,
            DelimiterKind::Brace => &mut self.braces,
            DelimiterKind::Angle => &mut self.angles,
        };
        debug_assert!(*depth > 0);
        *depth -= 1;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Punctuation {
    Comma,
    Colon,
    Equals,
    Dot,
    Pipe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LexicalTokenKind {
    Keyword(Keyword),
    Identifier,
    String,
    LineComment,
    BlockComment,
    OpenDelimiter(DelimiterKind),
    CloseDelimiter(DelimiterKind),
    Arrow,
    Punctuation(Punctuation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LexicalToken {
    kind: LexicalTokenKind,
    span: Span,
    depth_before: DelimiterDepth,
    depth_after: DelimiterDepth,
}

impl LexicalToken {
    pub(crate) const fn kind(&self) -> LexicalTokenKind {
        self.kind
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }

    pub(crate) const fn depth_before(&self) -> DelimiterDepth {
        self.depth_before
    }

    pub(crate) const fn depth_after(&self) -> DelimiterDepth {
        self.depth_after
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LexicalIssueKind {
    UnterminatedString,
    UnterminatedBlockComment,
    UnmatchedClosingDelimiter {
        found: DelimiterKind,
    },
    MismatchedClosingDelimiter {
        expected: DelimiterKind,
        found: DelimiterKind,
    },
    UnclosedDelimiter {
        opened: DelimiterKind,
    },
}

impl LexicalIssueKind {
    const fn rank(self) -> u8 {
        match self {
            Self::UnterminatedString => 10,
            Self::UnterminatedBlockComment => 20,
            Self::UnmatchedClosingDelimiter { .. } => 30,
            Self::MismatchedClosingDelimiter { .. } => 40,
            Self::UnclosedDelimiter { .. } => 50,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LexicalIssue {
    kind: LexicalIssueKind,
    span: Span,
}

impl LexicalIssue {
    pub(crate) const fn kind(&self) -> LexicalIssueKind {
        self.kind
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LexicalMap {
    tokens: Vec<LexicalToken>,
    issues: Vec<LexicalIssue>,
}

impl LexicalMap {
    pub(crate) fn tokens(&self) -> &[LexicalToken] {
        &self.tokens
    }

    pub(crate) fn issues(&self) -> &[LexicalIssue] {
        &self.issues
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LexicalError {
    ResourceLimit { span: Span, limit: usize },
}

#[derive(Clone, Copy, Debug)]
struct OpenDelimiter {
    kind: DelimiterKind,
    span: Span,
}

/// Scans lexical recovery facts without accepting or rejecting the grammar.
///
/// Ordinary lexical problems are returned in [`LexicalMap::issues`]. Exceeding
/// the fixed delimiter-depth guard is the sole fatal preflight error because
/// the recursive Pest grammar must not run on that input.
pub(crate) fn preflight(source: &str) -> Result<LexicalMap, LexicalError> {
    let mut cursor = 0;
    let mut depth = DelimiterDepth::default();
    let mut stack = Vec::<OpenDelimiter>::new();
    let mut tokens = Vec::new();
    let mut issues = Vec::new();

    while cursor < source.len() {
        if source[cursor..].starts_with("//") {
            let end = scan_line_comment(source, cursor);
            tokens.push(token(
                LexicalTokenKind::LineComment,
                cursor,
                end,
                depth,
                depth,
            ));
            cursor = end;
            continue;
        }

        if source[cursor..].starts_with("/*") {
            let (end, terminated) = scan_block_comment(source, cursor);
            tokens.push(token(
                LexicalTokenKind::BlockComment,
                cursor,
                end,
                depth,
                depth,
            ));
            if !terminated {
                issues.push(LexicalIssue {
                    kind: LexicalIssueKind::UnterminatedBlockComment,
                    span: Span::new(cursor, cursor + 2),
                });
            }
            cursor = end;
            continue;
        }

        if source.as_bytes()[cursor] == b'"' {
            let (end, terminated) = scan_string(source, cursor);
            tokens.push(token(LexicalTokenKind::String, cursor, end, depth, depth));
            if !terminated {
                issues.push(LexicalIssue {
                    kind: LexicalIssueKind::UnterminatedString,
                    span: Span::new(cursor, cursor + 1),
                });
            }
            cursor = end;
            continue;
        }

        if source[cursor..].starts_with("->") {
            tokens.push(token(
                LexicalTokenKind::Arrow,
                cursor,
                cursor + 2,
                depth,
                depth,
            ));
            cursor += 2;
            continue;
        }

        let byte = source.as_bytes()[cursor];
        if is_identifier_start(byte) {
            let mut end = cursor + 1;
            while end < source.len() && is_identifier_continue(source.as_bytes()[end]) {
                end += 1;
            }
            let kind = Keyword::from_spelling(&source[cursor..end])
                .map_or(LexicalTokenKind::Identifier, LexicalTokenKind::Keyword);
            tokens.push(token(kind, cursor, end, depth, depth));
            cursor = end;
            continue;
        }

        let delimiter = match byte {
            b'(' => Some((DelimiterKind::Parenthesis, true)),
            b')' => Some((DelimiterKind::Parenthesis, false)),
            b'{' => Some((DelimiterKind::Brace, true)),
            b'}' => Some((DelimiterKind::Brace, false)),
            b'<' => Some((DelimiterKind::Angle, true)),
            b'>' => Some((DelimiterKind::Angle, false)),
            _ => None,
        };

        if let Some((kind, is_open)) = delimiter {
            if is_open {
                if stack.len() == MAX_DELIMITER_DEPTH {
                    return Err(LexicalError::ResourceLimit {
                        span: Span::new(cursor, cursor + 1),
                        limit: MAX_DELIMITER_DEPTH,
                    });
                }
                let before = depth;
                stack.push(OpenDelimiter {
                    kind,
                    span: Span::new(cursor, cursor + 1),
                });
                depth.push(kind);
                tokens.push(token(
                    LexicalTokenKind::OpenDelimiter(kind),
                    cursor,
                    cursor + 1,
                    before,
                    depth,
                ));
            } else {
                let before = depth;
                match stack.last().copied() {
                    Some(open) if open.kind == kind => {
                        stack.pop();
                        depth.pop(kind);
                    }
                    Some(open) => issues.push(LexicalIssue {
                        kind: LexicalIssueKind::MismatchedClosingDelimiter {
                            expected: open.kind,
                            found: kind,
                        },
                        span: Span::new(cursor, cursor + 1),
                    }),
                    None => issues.push(LexicalIssue {
                        kind: LexicalIssueKind::UnmatchedClosingDelimiter { found: kind },
                        span: Span::new(cursor, cursor + 1),
                    }),
                }
                tokens.push(token(
                    LexicalTokenKind::CloseDelimiter(kind),
                    cursor,
                    cursor + 1,
                    before,
                    depth,
                ));
            }
            cursor += 1;
            continue;
        }

        let punctuation = match byte {
            b',' => Some(Punctuation::Comma),
            b':' => Some(Punctuation::Colon),
            b'=' => Some(Punctuation::Equals),
            b'.' => Some(Punctuation::Dot),
            b'|' => Some(Punctuation::Pipe),
            _ => None,
        };
        if let Some(kind) = punctuation {
            tokens.push(token(
                LexicalTokenKind::Punctuation(kind),
                cursor,
                cursor + 1,
                depth,
                depth,
            ));
            cursor += 1;
            continue;
        }

        // Unknown and trivia scalars are irrelevant to preflight, but advancing
        // by the complete UTF-8 scalar is essential for recovery termination.
        cursor += source[cursor..]
            .chars()
            .next()
            .expect("cursor is before source end")
            .len_utf8();
    }

    issues.extend(stack.into_iter().map(|open| LexicalIssue {
        kind: LexicalIssueKind::UnclosedDelimiter { opened: open.kind },
        span: open.span,
    }));
    issues.sort_by_key(|issue| (issue.span.start, issue.span.end, issue.kind.rank()));

    Ok(LexicalMap { tokens, issues })
}

fn token(
    kind: LexicalTokenKind,
    start: usize,
    end: usize,
    depth_before: DelimiterDepth,
    depth_after: DelimiterDepth,
) -> LexicalToken {
    LexicalToken {
        kind,
        span: Span::new(start, end),
        depth_before,
        depth_after,
    }
}

fn scan_line_comment(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = start + 2;
    while cursor < bytes.len() && !matches!(bytes[cursor], b'\r' | b'\n') {
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'\r') && bytes.get(cursor + 1) == Some(&b'\n') {
        cursor + 2
    } else if bytes.get(cursor) == Some(&b'\n') {
        cursor + 1
    } else {
        cursor
    }
}

fn scan_block_comment(source: &str, start: usize) -> (usize, bool) {
    let bytes = source.as_bytes();
    let mut cursor = start + 2;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return (cursor + 2, true);
        }
        cursor += 1;
    }
    (bytes.len(), false)
}

fn scan_string(source: &str, start: usize) -> (usize, bool) {
    let bytes = source.as_bytes();
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return (cursor + 1, true),
            b'\r' | b'\n' => return (cursor, false),
            b'\\' => {
                cursor += 1;
                if cursor < bytes.len() && !matches!(bytes[cursor], b'\r' | b'\n') {
                    cursor += source[cursor..]
                        .chars()
                        .next()
                        .expect("cursor is before source end")
                        .len_utf8();
                }
            }
            _ => {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is before source end")
                    .len_utf8();
            }
        }
    }
    (cursor, false)
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::{
        DelimiterKind, Keyword, LexicalError, LexicalIssueKind, LexicalTokenKind,
        MAX_DELIMITER_DEPTH, preflight,
    };
    use crate::source_map::Span;

    #[test]
    fn classifies_keywords_only_at_complete_identifier_boundaries() {
        let scan = preflight("module module_name true trueish _result result2").unwrap();
        let words = scan
            .tokens()
            .iter()
            .filter_map(|token| match token.kind() {
                LexicalTokenKind::Keyword(keyword) => Some(("keyword", Some(keyword))),
                LexicalTokenKind::Identifier => Some(("identifier", None)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            words,
            vec![
                ("keyword", Some(Keyword::Module)),
                ("identifier", None),
                ("keyword", Some(Keyword::True)),
                ("identifier", None),
                ("identifier", None),
                ("identifier", None),
            ]
        );
    }

    #[test]
    fn strings_and_comments_are_opaque_to_delimiter_and_keyword_scanning() {
        let source = "fn /* run { ( < */ f() { let x = \"} run // /*\" // target )\r\n}";
        let scan = preflight(source).unwrap();

        assert!(scan.issues().is_empty());
        assert_eq!(
            scan.tokens()
                .iter()
                .filter(|token| matches!(token.kind(), LexicalTokenKind::Keyword(_)))
                .count(),
            2
        );
        assert_eq!(
            scan.tokens()
                .iter()
                .filter(|token| matches!(token.kind(), LexicalTokenKind::String))
                .count(),
            1
        );
        assert_eq!(
            scan.tokens()
                .iter()
                .filter(|token| matches!(
                    token.kind(),
                    LexicalTokenKind::LineComment | LexicalTokenKind::BlockComment
                ))
                .count(),
            2
        );
    }

    #[test]
    fn records_depth_before_and_after_each_delimiter() {
        let scan = preflight("{(<x>)}").unwrap();
        let delimiters = scan
            .tokens()
            .iter()
            .filter(|token| {
                matches!(
                    token.kind(),
                    LexicalTokenKind::OpenDelimiter(_) | LexicalTokenKind::CloseDelimiter(_)
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(delimiters.len(), 6);
        assert_eq!(delimiters[2].depth_before().total(), 2);
        assert_eq!(delimiters[2].depth_after().total(), 3);
        assert_eq!(delimiters[3].depth_before().total(), 3);
        assert_eq!(delimiters[3].depth_after().total(), 2);
        assert_eq!(delimiters[5].depth_after().total(), 0);
    }

    #[test]
    fn treats_return_arrows_as_tokens_not_angle_closers() {
        let scan = preflight("fn f() -> result<ok, error> {}").unwrap();

        assert!(scan.issues().is_empty());
        assert_eq!(
            scan.tokens()
                .iter()
                .filter(|token| matches!(token.kind(), LexicalTokenKind::Arrow))
                .count(),
            1
        );
    }

    #[test]
    fn distinguishes_unterminated_opaque_regions_and_unmatched_delimiters() {
        let string_scan = preflight("\"not closed\nmodule next").unwrap();
        assert!(matches!(
            string_scan.issues()[0].kind(),
            LexicalIssueKind::UnterminatedString
        ));
        assert_eq!(string_scan.issues()[0].span(), Span::new(0, 1));

        let comment_scan = preflight("/* not closed").unwrap();
        assert!(matches!(
            comment_scan.issues()[0].kind(),
            LexicalIssueKind::UnterminatedBlockComment
        ));
        assert_eq!(comment_scan.issues()[0].span(), Span::new(0, 2));

        let delimiter_scan = preflight("})").unwrap();
        assert_eq!(delimiter_scan.issues().len(), 2);
        assert!(matches!(
            delimiter_scan.issues()[0].kind(),
            LexicalIssueKind::UnmatchedClosingDelimiter {
                found: DelimiterKind::Brace
            }
        ));
    }

    #[test]
    fn reports_unclosed_openers_in_source_order() {
        let scan = preflight("{(").unwrap();

        assert_eq!(scan.issues().len(), 2);
        assert_eq!(scan.issues()[0].span(), Span::new(0, 1));
        assert_eq!(scan.issues()[1].span(), Span::new(1, 2));
        assert!(
            scan.issues()
                .iter()
                .all(|issue| matches!(issue.kind(), LexicalIssueKind::UnclosedDelimiter { .. }))
        );
    }

    #[test]
    fn allows_exactly_the_documented_maximum_depth() {
        let source = format!(
            "{}{}",
            "(".repeat(MAX_DELIMITER_DEPTH),
            ")".repeat(MAX_DELIMITER_DEPTH)
        );

        let scan = preflight(&source).unwrap();
        assert!(scan.issues().is_empty());
    }

    #[test]
    fn rejects_the_delimiter_that_exceeds_the_depth_guard() {
        let source = "(".repeat(MAX_DELIMITER_DEPTH + 1);

        assert_eq!(
            preflight(&source),
            Err(LexicalError::ResourceLimit {
                span: Span::new(MAX_DELIMITER_DEPTH, MAX_DELIMITER_DEPTH + 1),
                limit: MAX_DELIMITER_DEPTH,
            })
        );
    }

    #[test]
    fn line_comments_consume_lf_and_crlf_but_not_a_bare_carriage_return() {
        let source = "// one\n// two\r\n// three\rmodule";
        let scan = preflight(source).unwrap();
        let comments = scan
            .tokens()
            .iter()
            .filter(|token| matches!(token.kind(), LexicalTokenKind::LineComment))
            .collect::<Vec<_>>();

        assert_eq!(comments.len(), 3);
        assert_eq!(comments[0].span(), Span::new(0, 7));
        assert_eq!(comments[1].span(), Span::new(7, 15));
        assert_eq!(comments[2].span(), Span::new(15, 23));
    }

    #[test]
    fn arbitrary_unicode_and_controls_always_advance() {
        let source = "💾\0é\u{7f} λ // fn\nidentifier";
        let scan = preflight(source).unwrap();

        assert!(scan.issues().is_empty());
        assert!(
            scan.tokens()
                .windows(2)
                .all(|pair| pair[0].span() != pair[1].span())
        );
    }
}
