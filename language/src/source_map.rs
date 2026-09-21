//! Source spans, logical locations, and deterministic display escaping.
//!
//! Byte offsets always refer to the caller's original UTF-8 source. Newlines
//! are normalized only for logical line lookup: both LF and CRLF advance one
//! line, while their original bytes remain available through spans.

use std::error::Error;
use std::fmt;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// The number of display columns between tab stops.
pub const TAB_WIDTH: usize = 4;

/// A half-open byte range in the original UTF-8 source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Span {
    /// Inclusive zero-based UTF-8 byte offset.
    pub start: usize,
    /// Exclusive zero-based UTF-8 byte offset.
    pub end: usize,
}

impl Span {
    /// Construct a half-open span from trusted, ordered parser offsets.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "span start must not exceed span end");
        Self { start, end }
    }

    /// Return the length of the span in source bytes.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Return whether the span selects no source bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// A one-based logical source location.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Location {
    /// One-based source line.
    pub line: usize,
    /// One-based Unicode-scalar display column.
    pub column: usize,
}

/// One logical source line without its LF or CRLF terminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLine<'a> {
    /// One-based line number.
    pub number: usize,
    /// Inclusive byte offset in the original source.
    pub start: usize,
    /// Exclusive byte offset before the newline terminator.
    pub end: usize,
    /// Original source text for this line, excluding its terminator.
    pub text: &'a str,
}

/// A rejected byte offset or span at the source-map boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMapError {
    /// An offset was greater than the original source byte length.
    OffsetOutOfBounds { offset: usize, source_len: usize },
    /// An offset split a UTF-8 encoded Unicode scalar.
    OffsetNotCharBoundary { offset: usize },
    /// A span's inclusive start followed its exclusive end.
    SpanStartAfterEnd { start: usize, end: usize },
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::OffsetOutOfBounds { offset, source_len } => {
                write!(
                    formatter,
                    "byte offset {offset} exceeds source length {source_len}"
                )
            }
            Self::OffsetNotCharBoundary { offset } => {
                write!(
                    formatter,
                    "byte offset {offset} is not a UTF-8 character boundary"
                )
            }
            Self::SpanStartAfterEnd { start, end } => {
                write!(formatter, "span start {start} exceeds span end {end}")
            }
        }
    }
}

impl Error for SourceMapError {}

/// An immutable logical-line index over original source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMap<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    /// Index all logical line starts in `source`.
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = Vec::with_capacity(source.lines().count().saturating_add(1));
        line_starts.push(0);
        line_starts.extend(
            source
                .as_bytes()
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        );

        Self {
            source,
            line_starts,
        }
    }

    /// Return the original source byte length.
    #[must_use]
    pub const fn source_len(&self) -> usize {
        self.source.len()
    }

    /// Return the ordered zero-based byte offsets of all logical line starts.
    #[must_use]
    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    /// Convert a valid UTF-8 byte offset, including EOF, to a logical location.
    pub fn location(&self, byte_offset: usize) -> Result<Location, SourceMapError> {
        self.validate_offset(byte_offset)?;
        let line_index = self.line_index(byte_offset);
        let line = self.line_by_index(line_index);
        let prefix_end = byte_offset.min(line.end);
        let column = display_column(&self.source[line.start..prefix_end]);

        Ok(Location {
            line: line.number,
            column,
        })
    }

    /// Return the logical line containing a valid byte offset, including EOF.
    pub fn line_at(&self, byte_offset: usize) -> Result<SourceLine<'a>, SourceMapError> {
        self.validate_offset(byte_offset)?;
        Ok(self.line_by_index(self.line_index(byte_offset)))
    }

    /// Validate ordering, bounds, and UTF-8 boundaries for a source span.
    pub fn validate_span(&self, span: Span) -> Result<(), SourceMapError> {
        if span.start > span.end {
            return Err(SourceMapError::SpanStartAfterEnd {
                start: span.start,
                end: span.end,
            });
        }
        self.validate_offset(span.start)?;
        self.validate_offset(span.end)
    }

    /// Recover the exact original text selected by a validated span.
    pub fn slice(&self, span: Span) -> Result<&'a str, SourceMapError> {
        self.validate_span(span)?;
        Ok(&self.source[span.start..span.end])
    }

    fn validate_offset(&self, byte_offset: usize) -> Result<(), SourceMapError> {
        if byte_offset > self.source.len() {
            return Err(SourceMapError::OffsetOutOfBounds {
                offset: byte_offset,
                source_len: self.source.len(),
            });
        }
        if !self.source.is_char_boundary(byte_offset) {
            return Err(SourceMapError::OffsetNotCharBoundary {
                offset: byte_offset,
            });
        }
        Ok(())
    }

    fn line_index(&self, byte_offset: usize) -> usize {
        self.line_starts
            .partition_point(|line_start| *line_start <= byte_offset)
            .saturating_sub(1)
    }

    fn line_by_index(&self, line_index: usize) -> SourceLine<'a> {
        let start = self.line_starts[line_index];
        let raw_end = self
            .line_starts
            .get(line_index + 1)
            .copied()
            .unwrap_or(self.source.len());
        let mut end = raw_end;

        if self.line_starts.get(line_index + 1).is_some() {
            debug_assert_eq!(self.source.as_bytes().get(end - 1), Some(&b'\n'));
            end -= 1;
            if end > start && self.source.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
        }

        SourceLine {
            number: line_index + 1,
            start,
            end,
            text: &self.source[start..end],
        }
    }
}

/// Render one source line without allowing C0/C1 controls into terminal output.
#[must_use]
pub fn escape_source_line(source_line: &str) -> String {
    let mut escaped = String::with_capacity(source_line.len());
    let mut column = 1;

    for character in source_line.chars() {
        match character {
            '\r' | '\n' => break,
            '\t' => {
                let next_column = next_tab_stop(column);
                escaped.extend(std::iter::repeat_n(' ', next_column - column));
                column = next_column;
            }
            character if is_c0_or_c1(character) => {
                write_control_escape(&mut escaped, character);
                column += control_escape_width();
            }
            character => {
                escaped.push(character);
                column += 1;
            }
        }
    }

    escaped
}

/// Escape a caller-supplied filename label without resolving or opening it.
#[must_use]
pub fn escape_filename(filename: &str) -> String {
    let mut escaped = String::with_capacity(filename.len());

    for character in filename.chars() {
        match character {
            '\t' => escaped.push_str(r"\t"),
            '\n' => escaped.push_str(r"\n"),
            '\r' => escaped.push_str(r"\r"),
            character if is_c0_or_c1(character) => {
                write_control_escape(&mut escaped, character);
            }
            character => escaped.push(character),
        }
    }

    escaped
}

const fn next_tab_stop(column: usize) -> usize {
    column + TAB_WIDTH - ((column - 1) % TAB_WIDTH)
}

fn display_column(prefix: &str) -> usize {
    prefix.chars().fold(1, |column, character| {
        if character == '\t' {
            next_tab_stop(column)
        } else {
            column + 1
        }
    })
}

const fn is_c0_or_c1(character: char) -> bool {
    matches!(character as u32, 0x00..=0x1f | 0x7f..=0x9f)
}

const fn control_escape_width() -> usize {
    r"\u{0000}".len()
}

fn write_control_escape(output: &mut String, character: char) {
    write!(output, "\\u{{{:04X}}}", character as u32).expect("writing to a String cannot fail");
}

#[cfg(test)]
mod tests {
    use super::{Location, SourceMap, SourceMapError, Span, escape_filename, escape_source_line};

    #[test]
    fn span_is_half_open_and_serde_ready() {
        let span = Span::new(2, 7);

        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert!(Span::new(4, 4).is_empty());
        assert_eq!(
            serde_json::to_string(&span).unwrap(),
            r#"{"start":2,"end":7}"#
        );
        assert_eq!(
            serde_json::from_str::<Span>(r#"{"start":2,"end":7}"#).unwrap(),
            span
        );
    }

    #[test]
    #[should_panic(expected = "span start must not exceed span end")]
    fn span_constructor_rejects_reversed_bounds() {
        let _ = Span::new(2, 1);
    }

    #[test]
    fn indexes_lf_crlf_and_final_empty_lines_without_changing_bytes() {
        let source = "first\r\nsecond\nthird\r\nfourth\n";
        let map = SourceMap::new(source);

        assert_eq!(map.source_len(), source.len());
        assert_eq!(map.line_starts(), &[0, 7, 14, 21, 28]);
        assert_eq!(map.slice(Span::new(0, 7)).unwrap(), "first\r\n");
        assert_eq!(map.line_at(8).unwrap().text, "second");
        assert_eq!(map.line_at(source.len()).unwrap().text, "");
    }

    #[test]
    fn maps_unicode_scalar_byte_offsets_to_one_based_columns() {
        let map = SourceMap::new("aé中z");

        assert_eq!(map.location(0), Ok(Location { line: 1, column: 1 }));
        assert_eq!(map.location(1), Ok(Location { line: 1, column: 2 }));
        assert_eq!(map.location(3), Ok(Location { line: 1, column: 3 }));
        assert_eq!(map.location(6), Ok(Location { line: 1, column: 4 }));
        assert_eq!(map.location(7), Ok(Location { line: 1, column: 5 }));
    }

    #[test]
    fn maps_tabs_to_columns_one_five_nine_and_thirteen() {
        let map = SourceMap::new("\ta\tb\t");

        assert_eq!(map.location(0).unwrap().column, 1);
        assert_eq!(map.location(1).unwrap().column, 5);
        assert_eq!(map.location(2).unwrap().column, 6);
        assert_eq!(map.location(3).unwrap().column, 9);
        assert_eq!(map.location(4).unwrap().column, 10);
        assert_eq!(map.location(5).unwrap().column, 13);
    }

    #[test]
    fn maps_lf_and_crlf_to_the_same_logical_locations() {
        let lf = SourceMap::new("α\tX\n\tY");
        let crlf = SourceMap::new("α\tX\r\n\tY");

        assert_eq!(lf.location(5), Ok(Location { line: 2, column: 1 }));
        assert_eq!(crlf.location(6), Ok(Location { line: 2, column: 1 }));
        assert_eq!(lf.location(6), Ok(Location { line: 2, column: 5 }));
        assert_eq!(crlf.location(7), Ok(Location { line: 2, column: 5 }));
        assert_eq!(lf.location(7), Ok(Location { line: 2, column: 6 }));
        assert_eq!(crlf.location(8), Ok(Location { line: 2, column: 6 }));
    }

    #[test]
    fn newline_bytes_map_to_the_preceding_line_end() {
        let lf = SourceMap::new("x\ny");
        let crlf = SourceMap::new("x\r\ny");

        assert_eq!(lf.location(1), Ok(Location { line: 1, column: 2 }));
        assert_eq!(crlf.location(1), Ok(Location { line: 1, column: 2 }));
        assert_eq!(crlf.location(2), Ok(Location { line: 1, column: 2 }));
    }

    #[test]
    fn reports_empty_and_trailing_newline_eof_locations() {
        assert_eq!(
            SourceMap::new("").location(0),
            Ok(Location { line: 1, column: 1 })
        );
        assert_eq!(
            SourceMap::new("x").location(1),
            Ok(Location { line: 1, column: 2 })
        );
        assert_eq!(
            SourceMap::new("x\n").location(2),
            Ok(Location { line: 2, column: 1 })
        );
        assert_eq!(
            SourceMap::new("x\r\n").location(3),
            Ok(Location { line: 2, column: 1 })
        );
    }

    #[test]
    fn rejects_out_of_bounds_and_non_boundary_offsets() {
        let map = SourceMap::new("é");

        assert_eq!(
            map.location(3),
            Err(SourceMapError::OffsetOutOfBounds {
                offset: 3,
                source_len: 2,
            })
        );
        assert_eq!(
            map.location(1),
            Err(SourceMapError::OffsetNotCharBoundary { offset: 1 })
        );
    }

    #[test]
    fn rejects_invalid_spans_and_recovers_exact_original_text() {
        let map = SourceMap::new("aé\r\n");

        assert_eq!(map.slice(Span::new(1, 3)).unwrap(), "é");
        assert_eq!(
            map.validate_span(Span { start: 3, end: 2 }),
            Err(SourceMapError::SpanStartAfterEnd { start: 3, end: 2 })
        );
        assert_eq!(
            map.validate_span(Span { start: 1, end: 2 }),
            Err(SourceMapError::OffsetNotCharBoundary { offset: 2 })
        );
    }

    #[test]
    fn source_line_escaping_expands_tabs_and_escapes_other_controls() {
        assert_eq!(escape_source_line("a\tb"), "a   b");
        assert_eq!(
            escape_source_line("\0\u{001f}\u{007f}\u{0085}é\\"),
            r"\u{0000}\u{001F}\u{007F}\u{0085}é\"
        );
        assert_eq!(escape_source_line("visible\rnot rendered"), "visible");
        assert_eq!(escape_source_line("visible\nnot rendered"), "visible");
    }

    #[test]
    fn filename_escaping_uses_short_whitespace_escapes_and_uppercase_control_hex() {
        assert_eq!(
            escape_filename("a\tb\nc\rd\0\u{001f}\u{007f}\u{009f}é\\"),
            r"a\tb\nc\rd\u{0000}\u{001F}\u{007F}\u{009F}é\"
        );
    }

    #[test]
    fn escaping_emits_no_raw_c0_or_c1_controls() {
        let mut controls = String::new();
        controls.extend((0..=0x1f).filter_map(char::from_u32));
        controls.extend((0x7f..=0x9f).filter_map(char::from_u32));

        let escaped_filename = escape_filename(&controls);
        assert!(!escaped_filename.chars().any(is_c0_or_c1));

        let escaped_source = escape_source_line(&controls);
        assert!(!escaped_source.chars().any(is_c0_or_c1));
    }

    fn is_c0_or_c1(character: char) -> bool {
        matches!(character as u32, 0x00..=0x1f | 0x7f..=0x9f)
    }
}
