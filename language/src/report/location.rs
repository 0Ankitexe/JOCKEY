//! Borrow escaped source lines rather than allocating repeated large snippets.
use super::ReportError;
use crate::{SourceFile, SourceMap, Span};
use serde::{Serialize, Serializer};
use std::fmt::{self, Write};

#[derive(Debug, Serialize)]
struct Position {
    line: usize,
    column: usize,
}
#[derive(Debug, Serialize)]
pub(super) struct ReportLocation<'a> {
    span: Span,
    start: Position,
    end: Position,
    snippet: Snippet<'a>,
    marker_start: usize,
    marker_width: usize,
}
impl<'a> ReportLocation<'a> {
    pub fn new(
        source: SourceFile<'a>,
        map: &SourceMap<'a>,
        span: Span,
        legacy: bool,
    ) -> Result<Self, ReportError> {
        map.validate_span(span)
            .map_err(|_| ReportError::InvalidSpan)?;
        let start = map
            .location(span.start)
            .map_err(|_| ReportError::InvalidSpan)?;
        let end = map
            .location(span.end)
            .map_err(|_| ReportError::InvalidSpan)?;
        let line = map
            .line_at(span.start)
            .map_err(|_| ReportError::InvalidSpan)?;
        // Legacy diagnostics historically split on either CR or LF, including
        // positions on a terminator. Preserve that rendering exactly for syntax.
        let (line_start, line_end) = if legacy {
            let bytes = source.text.as_bytes();
            (
                bytes[..span.start]
                    .iter()
                    .rposition(|b| matches!(b, b'\r' | b'\n'))
                    .map_or(0, |i| i + 1),
                bytes[span.start..]
                    .iter()
                    .position(|b| matches!(b, b'\r' | b'\n'))
                    .map_or(bytes.len(), |i| span.start + i),
            )
        } else {
            (line.start, line.end)
        };
        let marker_byte = span.start.min(line_end);
        let end_byte = span.end.min(line_end).max(marker_byte);
        let marker_start = super::human::width(&source.text[line_start..marker_byte], 0);
        let marker_width =
            super::human::width(&source.text[marker_byte..end_byte], marker_start).max(1);
        Ok(Self {
            span,
            start: Position {
                line: start.line,
                column: start.column,
            },
            end: Position {
                line: end.line,
                column: end.column,
            },
            snippet: Snippet(&source.text[line_start..line_end]),
            marker_start,
            marker_width,
        })
    }
}
#[derive(Debug)]
struct Snippet<'a>(&'a str);
impl fmt::Display for Snippet<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut column = 0;
        for ch in self.0.chars() {
            match ch {
                '\t' => {
                    let spaces = 4 - column % 4;
                    for _ in 0..spaces {
                        f.write_char(' ')?;
                    }
                    column += spaces;
                }
                ch if ch.is_control() => {
                    write!(f, "\\u{{{:04X}}}", ch as u32)?;
                    column += 8;
                }
                ch => {
                    f.write_char(ch)?;
                    column += 1;
                }
            }
        }
        Ok(())
    }
}
impl Serialize for Snippet<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
pub(super) fn write_filename(text: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for ch in text.chars() {
        match ch {
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            ch if ch.is_control() => write!(f, "\\u{{{:04X}}}", ch as u32)?,
            ch => f.write_char(ch)?,
        }
    }
    Ok(())
}
