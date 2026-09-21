//! Exact allocation preflight for the unchanged Version 1 renderer.
use super::human::RenderError;
use crate::{
    DiagnosticSet, SourceFile, SourceMap, render_diagnostics, semantic::limits::MAX_REPORT_BYTES,
};

fn metrics(text: &str, mut column: usize) -> (usize, usize) {
    let (mut bytes, start) = (0usize, column);
    for ch in text.chars() {
        let (encoded, visual) = match ch {
            '\t' => {
                let n = 4 - column % 4;
                (n, n)
            }
            ch if ch.is_control() => (8, 8),
            ch => (ch.len_utf8(), 1),
        };
        bytes = bytes.saturating_add(encoded);
        column = column.saturating_add(visual);
    }
    (bytes, column.saturating_sub(start))
}

pub(super) fn render(
    source: SourceFile<'_>,
    diagnostics: &DiagnosticSet,
) -> Result<String, RenderError> {
    let map = SourceMap::new(source.text);
    let mut bound = if diagnostics.is_truncated() {
        "\nnote: additional diagnostics suppressed after 32 errors\n".len()
    } else {
        0
    };
    let filename_bytes = source
        .name
        .chars()
        .map(|ch| match ch {
            '\t' | '\n' | '\r' => 2,
            ch if ch.is_control() => 8,
            ch => ch.len_utf8(),
        })
        .fold(0usize, usize::saturating_add);
    for (index, diagnostic) in diagnostics.items().iter().enumerate() {
        map.validate_span(diagnostic.span)
            .map_err(|_| RenderError::InvalidSpan)?;
        let position = map
            .location(diagnostic.span.start)
            .map_err(|_| RenderError::InvalidSpan)?;
        let start = diagnostic.span.start;
        let bytes = source.text.as_bytes();
        // Match V1 even for a diagnostic pointing at a CR/LF byte.
        let line_start = bytes[..start]
            .iter()
            .rposition(|b| matches!(b, b'\r' | b'\n'))
            .map_or(0, |i| i + 1);
        let line_end = bytes[start..]
            .iter()
            .position(|b| matches!(b, b'\r' | b'\n'))
            .map_or(bytes.len(), |i| start + i);
        let end = diagnostic.span.end.min(line_end).max(start);
        let marker = metrics(&source.text[line_start..start], 0).1;
        let carets = metrics(&source.text[start..end], marker).1.max(1);
        let snippet = metrics(&source.text[line_start..line_end], 0).0;
        let line_digits = position.line.to_string().len();
        // Heading, location, gutter, numbered snippet, marker and separator.
        let pieces = [
            usize::from(index > 0),
            diagnostic.severity.as_str().len(),
            diagnostic.category.as_str().len(),
            diagnostic.message().len(),
            5,
            9,
            filename_bytes,
            line_digits,
            position.column.to_string().len(),
            5,
            line_digits,
            snippet,
            4,
            7,
            marker,
            carets,
            diagnostic.label().len(),
        ];
        bound = pieces.into_iter().fold(bound, usize::saturating_add);
        if bound > MAX_REPORT_BYTES {
            return Err(RenderError::ResourceLimit);
        }
    }
    let rendered = render_diagnostics(source, diagnostics);
    debug_assert_eq!(rendered.len(), bound, "legacy render layout changed");
    if rendered.len() > MAX_REPORT_BYTES {
        Err(RenderError::ResourceLimit)
    } else {
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fitting_legacy_report_is_not_rejected_by_an_overestimate() {
        let text = "?".repeat(3_000_000);
        let source = SourceFile::new("large-syntax", &text);
        let diagnostics = crate::parse(source).unwrap_err();
        let expected = render_diagnostics(source, &diagnostics);
        assert!(expected.len() < MAX_REPORT_BYTES);
        assert_eq!(render(source, &diagnostics).unwrap(), expected);
    }
    #[test]
    fn byte_count_matches_unicode_controls_and_crlf() {
        use crate::{Diagnostic, DiagnosticCategory, MessageKey, Span};
        let text = "é\t\0\u{0085}\r\n";
        for span in [
            Span::new(0, 2),
            Span::new(3, 4),
            Span::new(6, 7),
            Span::new(7, 8),
            Span::new(8, 8),
        ] {
            let d = DiagnosticSet::from_one(Diagnostic::new(
                DiagnosticCategory::UnexpectedToken,
                MessageKey::UnexpectedToken,
                span,
            ));
            let source = SourceFile::new("x\n\0é", text);
            assert_eq!(render(source, &d).unwrap(), render_diagnostics(source, &d));
        }
    }
}
