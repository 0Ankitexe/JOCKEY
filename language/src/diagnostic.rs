//! Stable, project-owned diagnostics for the JOCKY syntax frontend.
//!
//! Parser-library errors are deliberately translated into the closed types in
//! this module before they cross the frontend boundary. Rendering is likewise
//! self-contained so it cannot inherit terminal, locale, or platform behavior.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{self, Write as _};

use crate::SourceFile;
use crate::source_map::{SourceMap, Span};

/// Maximum number of distinct diagnostics returned for one source file.
pub const MAX_DIAGNOSTICS: usize = 32;

/// Severity emitted by the Version 1 syntax frontend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
}

impl DiagnosticSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
        }
    }
}

/// Stable public diagnostic categories and their ordering rank.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiagnosticCategory {
    UnexpectedToken,
    MissingDelimiter,
    InvalidTargetDeclaration,
    MalformedType,
    DuplicateTopLevelRun,
    InvalidProfileDeclaration,
    ResourceLimit,
}

impl DiagnosticCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedToken => "unexpected-token",
            Self::MissingDelimiter => "missing-delimiter",
            Self::InvalidTargetDeclaration => "invalid-target-declaration",
            Self::MalformedType => "malformed-type",
            Self::DuplicateTopLevelRun => "duplicate-top-level-run",
            Self::InvalidProfileDeclaration => "invalid-profile-declaration",
            Self::ResourceLimit => "resource-limit",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            Self::UnexpectedToken => 10,
            Self::MissingDelimiter => 20,
            Self::InvalidTargetDeclaration => 30,
            Self::MalformedType => 40,
            Self::DuplicateTopLevelRun => 50,
            Self::InvalidProfileDeclaration => 60,
            Self::ResourceLimit => 90,
        }
    }

    /// Resource failures are typed separately from syntax judgments.
    pub const fn is_syntax(self) -> bool {
        !matches!(self, Self::ResourceLimit)
    }
}

/// Closed keys for all user-visible Version 1 diagnostic messages.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MessageKey {
    UnexpectedToken,
    IncompleteProgram,
    MissingDelimiter,
    InvalidTargetDeclaration,
    MalformedType,
    DuplicateTopLevelRun,
    InvalidProfileDeclaration,
    ResourceLimit,
}

impl MessageKey {
    pub const fn message(self) -> &'static str {
        match self {
            Self::UnexpectedToken => "unexpected token",
            Self::IncompleteProgram => "incomplete program",
            Self::MissingDelimiter => "missing delimiter",
            Self::InvalidTargetDeclaration => "target must be windows, ubuntu, or windows | ubuntu",
            Self::MalformedType => "malformed type",
            Self::DuplicateTopLevelRun => "duplicate top-level run statement",
            Self::InvalidProfileDeclaration => {
                "profile must be declared once as profile lab after target"
            }
            Self::ResourceLimit => "frontend nesting limit exceeded",
        }
    }

    const fn default_label(self) -> &'static str {
        match self {
            Self::UnexpectedToken => "unexpected token",
            Self::IncompleteProgram => "expected a complete program",
            Self::MissingDelimiter => "unclosed delimiter",
            Self::InvalidTargetDeclaration => "unsupported target",
            Self::MalformedType => "invalid type syntax",
            Self::DuplicateTopLevelRun => "additional run statement",
            Self::InvalidProfileDeclaration => "invalid lab profile declaration",
            Self::ResourceLimit => "nesting exceeds the frontend limit",
        }
    }
}

/// A delimiter whose absence can be diagnosed without dependency wording.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DelimiterKind {
    StringQuote,
    BlockComment,
    Parenthesis,
    Brace,
    TypeArgument,
}

impl DelimiterKind {
    const fn label(self) -> &'static str {
        match self {
            Self::StringQuote => "missing closing quote",
            Self::BlockComment => "missing closing */",
            Self::Parenthesis => "missing closing )",
            Self::Brace => "missing closing }",
            Self::TypeArgument => "missing closing >",
        }
    }
}

/// A grammar context named in a fixed actionable label.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExpectedSyntax {
    Program,
    Declaration,
    Statement,
    Expression,
    Type,
    Identifier,
}

impl ExpectedSyntax {
    const fn label(self) -> &'static str {
        match self {
            Self::Program => "expected a complete program",
            Self::Declaration => "expected a declaration",
            Self::Statement => "expected a statement",
            Self::Expression => "expected an expression",
            Self::Type => "expected a type",
            Self::Identifier => "expected an identifier",
        }
    }
}

/// The closed resource guard set for Version 1.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceLimitKind {
    NestingDepth,
}

impl ResourceLimitKind {
    const fn label(self) -> &'static str {
        match self {
            Self::NestingDepth => "nesting exceeds the frontend limit",
        }
    }
}

/// Optional structured detail used to select a fixed diagnostic label.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDetail {
    MissingDelimiter(DelimiterKind),
    Expected(ExpectedSyntax),
    UnsupportedTarget,
    DuplicateRun,
    ResourceLimit(ResourceLimitKind),
}

impl DiagnosticDetail {
    const fn label(self) -> &'static str {
        match self {
            Self::MissingDelimiter(delimiter) => delimiter.label(),
            Self::Expected(expected) => expected.label(),
            Self::UnsupportedTarget => "unsupported target",
            Self::DuplicateRun => "additional run statement",
            Self::ResourceLimit(limit) => limit.label(),
        }
    }
}

/// One stable frontend diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub category: DiagnosticCategory,
    pub message_key: MessageKey,
    pub span: Span,
    pub detail: Option<DiagnosticDetail>,
}

impl Diagnostic {
    pub const fn new(category: DiagnosticCategory, message_key: MessageKey, span: Span) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            category,
            message_key,
            span,
            detail: None,
        }
    }

    #[must_use]
    pub const fn with_detail(mut self, detail: DiagnosticDetail) -> Self {
        self.detail = Some(detail);
        self
    }

    pub const fn message(&self) -> &'static str {
        self.message_key.message()
    }

    pub const fn label(&self) -> &'static str {
        match self.detail {
            Some(detail) => detail.label(),
            None => self.message_key.default_label(),
        }
    }

    /// The required public ordering key, excluding only exact-dedup detail.
    pub const fn ordering_key(&self) -> (usize, u8, usize, MessageKey) {
        (
            self.span.start,
            self.category.rank(),
            self.span.end,
            self.message_key,
        )
    }

    fn cmp_stable(&self, other: &Self) -> Ordering {
        self.ordering_key()
            .cmp(&other.ordering_key())
            .then_with(|| self.detail.cmp(&other.detail))
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.category == other.category
            && self.span == other.span
            && self.message_key == other.message_key
            && self.detail == other.detail
    }
}

/// A typed resource failure before it is collected into a diagnostic set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceLimitFailure {
    pub kind: ResourceLimitKind,
    pub span: Span,
}

impl ResourceLimitFailure {
    pub const fn new(kind: ResourceLimitKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub const fn into_diagnostic(self) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCategory::ResourceLimit,
            MessageKey::ResourceLimit,
            self.span,
        )
        .with_detail(DiagnosticDetail::ResourceLimit(self.kind))
    }
}

impl From<ResourceLimitFailure> for Diagnostic {
    fn from(value: ResourceLimitFailure) -> Self {
        value.into_diagnostic()
    }
}

/// Error returned when construction would violate the non-empty invariant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmptyDiagnosticSet;

impl fmt::Display for EmptyDiagnosticSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a diagnostic set must contain at least one diagnostic")
    }
}

impl Error for EmptyDiagnosticSet {}

/// A sorted, exact-deduplicated, non-empty collection capped at 32 entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticSet {
    items: Vec<Diagnostic>,
    truncated: bool,
}

impl DiagnosticSet {
    pub fn new(mut diagnostics: Vec<Diagnostic>) -> Result<Self, EmptyDiagnosticSet> {
        if diagnostics.is_empty() {
            return Err(EmptyDiagnosticSet);
        }

        diagnostics.sort_by(Diagnostic::cmp_stable);
        diagnostics.dedup_by(|left, right| left.same_identity(right));

        let truncated = diagnostics.len() > MAX_DIAGNOSTICS;
        diagnostics.truncate(MAX_DIAGNOSTICS);

        Ok(Self {
            items: diagnostics,
            truncated,
        })
    }

    pub fn from_one(diagnostic: Diagnostic) -> Self {
        Self {
            items: vec![diagnostic],
            truncated: false,
        }
    }

    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    pub fn has_resource_limit(&self) -> bool {
        self.items
            .iter()
            .any(|diagnostic| diagnostic.category == DiagnosticCategory::ResourceLimit)
    }

    pub fn into_items(self) -> Vec<Diagnostic> {
        self.items
    }
}

impl<'a> IntoIterator for &'a DiagnosticSet {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Render a complete diagnostic set using stable LF-only output.
pub fn render_diagnostics(source: SourceFile<'_>, diagnostics: &DiagnosticSet) -> String {
    let source_map = SourceMap::new(source.text);
    let mut rendered = String::new();

    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        render_one(&mut rendered, &source, &source_map, diagnostic);
    }

    if diagnostics.is_truncated() {
        rendered.push('\n');
        rendered.push_str("note: additional diagnostics suppressed after 32 errors\n");
    }

    rendered
}

fn render_one(
    output: &mut String,
    source: &SourceFile<'_>,
    source_map: &SourceMap<'_>,
    diagnostic: &Diagnostic,
) {
    let start = floor_char_boundary(source.text, diagnostic.span.start.min(source.text.len()));
    let end = floor_char_boundary(
        source.text,
        diagnostic.span.end.max(start).min(source.text.len()),
    );

    let (line, column) = match source_map.location(start) {
        Ok(location) => (location.line, location.column),
        Err(_) => (1, 1),
    };
    let (line_start, line_end) = line_bounds(source.text, start);
    let raw_line = &source.text[line_start..line_end];
    let relative_start = start.saturating_sub(line_start).min(raw_line.len());
    let selected_end = end.min(line_end).max(start);
    let relative_end = selected_end.saturating_sub(line_start).min(raw_line.len());

    let (prefix, marker_start) = render_segment(&raw_line[..relative_start], 0);
    let (selected, marker_width) =
        render_segment(&raw_line[relative_start..relative_end], marker_start);
    let (suffix, _) = render_segment(
        &raw_line[relative_end..],
        marker_start.saturating_add(marker_width),
    );
    let mut snippet = prefix;
    snippet.push_str(&selected);
    snippet.push_str(&suffix);

    let escaped_name = escape_filename(source.name);
    let caret_count = marker_width.max(1);

    let _ = writeln!(
        output,
        "{}[{}]: {}",
        diagnostic.severity.as_str(),
        diagnostic.category.as_str(),
        diagnostic.message()
    );
    let _ = writeln!(output, "  --> {escaped_name}:{line}:{column}");
    output.push_str("   |\n");
    let _ = writeln!(output, "{line} | {snippet}");
    output.push_str("   | ");
    output.extend(std::iter::repeat_n(' ', marker_start));
    output.extend(std::iter::repeat_n('^', caret_count));
    output.push(' ');
    output.push_str(diagnostic.label());
    output.push('\n');
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn line_bounds(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let start = bytes[..offset]
        .iter()
        .rposition(|byte| matches!(*byte, b'\r' | b'\n'))
        .map_or(0, |newline| newline + 1);
    let end = bytes[offset..]
        .iter()
        .position(|byte| matches!(*byte, b'\r' | b'\n'))
        .map_or(text.len(), |relative| offset + relative);

    (start, end)
}

fn render_segment(segment: &str, initial_width: usize) -> (String, usize) {
    let mut rendered = String::new();
    let mut width = initial_width;

    for scalar in segment.chars() {
        if scalar == '\t' {
            let spaces = 4 - (width % 4);
            rendered.extend(std::iter::repeat_n(' ', spaces));
            width = width.saturating_add(spaces);
        } else if is_c0_or_c1(scalar) {
            let escape = format!("\\u{{{:04X}}}", scalar as u32);
            width = width.saturating_add(escape.len());
            rendered.push_str(&escape);
        } else {
            rendered.push(scalar);
            width = width.saturating_add(1);
        }
    }

    (rendered, width.saturating_sub(initial_width))
}

fn escape_filename(name: &str) -> String {
    let mut escaped = String::new();

    for scalar in name.chars() {
        match scalar {
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            scalar if is_c0_or_c1(scalar) => {
                let _ = write!(escaped, "\\u{{{:04X}}}", scalar as u32);
            }
            scalar => escaped.push(scalar),
        }
    }

    escaped
}

const fn is_c0_or_c1(scalar: char) -> bool {
    let codepoint = scalar as u32;
    codepoint <= 0x1f || (codepoint >= 0x7f && codepoint <= 0x9f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unexpected(span: Span) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCategory::UnexpectedToken,
            MessageKey::UnexpectedToken,
            span,
        )
    }

    #[test]
    fn category_identifiers_and_ranks_are_stable() {
        let expected = [
            (DiagnosticCategory::UnexpectedToken, "unexpected-token", 10),
            (
                DiagnosticCategory::MissingDelimiter,
                "missing-delimiter",
                20,
            ),
            (
                DiagnosticCategory::InvalidTargetDeclaration,
                "invalid-target-declaration",
                30,
            ),
            (DiagnosticCategory::MalformedType, "malformed-type", 40),
            (
                DiagnosticCategory::DuplicateTopLevelRun,
                "duplicate-top-level-run",
                50,
            ),
            (DiagnosticCategory::ResourceLimit, "resource-limit", 90),
        ];

        for (category, name, rank) in expected {
            assert_eq!(category.as_str(), name);
            assert_eq!(category.rank(), rank);
        }
        assert!(DiagnosticCategory::UnexpectedToken.is_syntax());
        assert!(!DiagnosticCategory::ResourceLimit.is_syntax());
    }

    #[test]
    fn set_is_non_empty_sorted_exact_deduplicated_and_capped() {
        assert_eq!(DiagnosticSet::new(Vec::new()), Err(EmptyDiagnosticSet));

        let duplicate = Diagnostic::new(
            DiagnosticCategory::MalformedType,
            MessageKey::MalformedType,
            Span::new(7, 8),
        );
        let same_location_lower_rank = unexpected(Span::new(7, 9));
        let mut input = vec![duplicate.clone(), duplicate, same_location_lower_rank];
        input.extend((0..40).map(|offset| unexpected(Span::new(100 + offset, 101 + offset))));

        let set = DiagnosticSet::new(input).expect("input is non-empty");
        assert_eq!(set.len(), MAX_DIAGNOSTICS);
        assert!(set.is_truncated());
        assert_eq!(set.items()[0].category, DiagnosticCategory::UnexpectedToken);
        assert_eq!(set.items()[1].category, DiagnosticCategory::MalformedType);
        assert_eq!(
            set.items()
                .iter()
                .filter(|item| item.span == Span::new(7, 8))
                .count(),
            1
        );
    }

    #[test]
    fn invalid_target_matches_the_contract_example() {
        let source = SourceFile::new("bad-target.jky", "module sample\ntarget macos\n");
        let diagnostic = Diagnostic::new(
            DiagnosticCategory::InvalidTargetDeclaration,
            MessageKey::InvalidTargetDeclaration,
            Span::new(21, 26),
        )
        .with_detail(DiagnosticDetail::UnsupportedTarget);

        assert_eq!(
            render_diagnostics(source, &DiagnosticSet::from_one(diagnostic)),
            concat!(
                "error[invalid-target-declaration]: target must be windows, ubuntu, or windows | ubuntu\n",
                "  --> bad-target.jky:2:8\n",
                "   |\n",
                "2 | target macos\n",
                "   |        ^^^^^ unsupported target\n",
            )
        );
    }

    #[test]
    fn rendering_uses_logical_unicode_columns_and_visible_control_widths() {
        let source = SourceFile::new("bad\t\u{7f}.jky", "\tlet café = \u{1}bad\n");
        let bad = source.text.find("bad").expect("fixture contains token");
        let diagnostic = unexpected(Span::new(bad, bad + 3));
        let rendered = render_diagnostics(source, &DiagnosticSet::from_one(diagnostic));

        assert!(rendered.contains("--> bad\\t\\u{007F}.jky:1:17\n"));
        assert!(rendered.contains("1 |     let café = \\u{0001}bad\n"));
        let expected_marker = format!("   | {}^^^ unexpected token\n", " ".repeat(23));
        assert!(rendered.contains(&expected_marker), "{rendered}");
    }

    #[test]
    fn crlf_is_one_line_break_and_carriage_return_is_not_rendered() {
        let text = "module sample\r\ntarget macos\r\n";
        let start = text.find("macos").expect("fixture contains target");
        let diagnostic = Diagnostic::new(
            DiagnosticCategory::InvalidTargetDeclaration,
            MessageKey::InvalidTargetDeclaration,
            Span::new(start, start + 5),
        );
        let rendered = render_diagnostics(
            SourceFile::new("crlf.jky", text),
            &DiagnosticSet::from_one(diagnostic),
        );

        assert!(rendered.contains("--> crlf.jky:2:8\n"));
        assert!(rendered.contains("2 | target macos\n"));
        assert!(!rendered.contains('\r'));
    }

    #[test]
    fn multiline_spans_mark_only_the_first_rendered_line() {
        let text = "let first\nsecond\n";
        let diagnostic = unexpected(Span::new(4, 16));
        let rendered = render_diagnostics(
            SourceFile::new("multi.jky", text),
            &DiagnosticSet::from_one(diagnostic),
        );

        assert!(rendered.contains("1 | let first\n"));
        assert!(rendered.contains("   |     ^^^^^ unexpected token\n"));
        assert!(!rendered.contains("second\n   |"));
    }

    #[test]
    fn zero_width_eof_span_has_one_caret() {
        let text = "module demo";
        let diagnostic = Diagnostic::new(
            DiagnosticCategory::MissingDelimiter,
            MessageKey::MissingDelimiter,
            Span::new(text.len(), text.len()),
        )
        .with_detail(DiagnosticDetail::MissingDelimiter(
            DelimiterKind::Parenthesis,
        ));
        let rendered = render_diagnostics(
            SourceFile::new("eof.jky", text),
            &DiagnosticSet::from_one(diagnostic),
        );

        assert!(rendered.contains("--> eof.jky:1:12\n"));
        assert!(rendered.contains("   |            ^ missing closing )\n"));
    }

    #[test]
    fn truncation_footer_is_blank_line_separated_and_lf_terminated() {
        let diagnostics = (0..=MAX_DIAGNOSTICS)
            .map(|offset| unexpected(Span::new(offset, offset)))
            .collect();
        let set = DiagnosticSet::new(diagnostics).expect("input is non-empty");
        let rendered = render_diagnostics(SourceFile::new("many.jky", ""), &set);

        assert!(rendered.ends_with("\nnote: additional diagnostics suppressed after 32 errors\n"));
        assert!(!rendered.contains('\r'));
    }

    #[test]
    fn resource_failure_remains_typed_through_collection() {
        let failure = ResourceLimitFailure::new(ResourceLimitKind::NestingDepth, Span::new(3, 4));
        let set = DiagnosticSet::from_one(failure.into());

        assert!(set.has_resource_limit());
        assert_eq!(set.items()[0].category, DiagnosticCategory::ResourceLimit);
        assert_eq!(
            set.items()[0].detail,
            Some(DiagnosticDetail::ResourceLimit(
                ResourceLimitKind::NestingDepth
            ))
        );
    }
}
