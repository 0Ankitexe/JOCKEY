use crate::semantic::{CheckFailure, SemanticDiagnostics, limits::MAX_REPORT_BYTES};
use crate::{SourceFile, SourceMap, Span};
use std::fmt::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    InvalidSpan,
    ResourceLimit,
}

struct Buffer(String);
impl Write for Buffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.len() > MAX_REPORT_BYTES.saturating_sub(self.0.len()) {
            return Err(fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}
impl Buffer {
    fn filename(&mut self, text: &str) -> fmt::Result {
        for ch in text.chars() {
            match ch {
                '\t' => self.write_str("\\t")?,
                '\n' => self.write_str("\\n")?,
                '\r' => self.write_str("\\r")?,
                ch if ch.is_control() => write!(self, "\\u{{{:04X}}}", ch as u32)?,
                ch => self.write_char(ch)?,
            }
        }
        Ok(())
    }
    fn segment(&mut self, text: &str, mut width: usize) -> fmt::Result {
        for ch in text.chars() {
            match ch {
                '\t' => {
                    let count = 4 - width % 4;
                    for _ in 0..count {
                        self.write_char(' ')?;
                    }
                    width += count;
                }
                ch if ch.is_control() => {
                    write!(self, "\\u{{{:04X}}}", ch as u32)?;
                    width += 8;
                }
                ch => {
                    self.write_char(ch)?;
                    width += 1;
                }
            }
        }
        Ok(())
    }
}
pub(super) fn width(text: &str, mut initial: usize) -> usize {
    let start = initial;
    for ch in text.chars() {
        initial += match ch {
            '\t' => 4 - initial % 4,
            ch if ch.is_control() => 8,
            _ => 1,
        };
    }
    initial - start
}

fn block(
    output: &mut Buffer,
    source: SourceFile<'_>,
    map: &SourceMap<'_>,
    span: Span,
    heading: fmt::Arguments<'_>,
    label: &str,
) -> Result<(), RenderError> {
    map.validate_span(span)
        .map_err(|_| RenderError::InvalidSpan)?;
    let position = map
        .location(span.start)
        .map_err(|_| RenderError::InvalidSpan)?;
    let line = map
        .line_at(span.start)
        .map_err(|_| RenderError::InvalidSpan)?;
    let start = span.start.min(line.end);
    let end = span.end.min(line.end).max(start);
    let marker = width(&source.text[line.start..start], 0);
    let carets = width(&source.text[start..end], marker).max(1);
    let mut render = || -> fmt::Result {
        writeln!(output, "{heading}")?;
        output.write_str("  --> ")?;
        output.filename(source.name)?;
        write!(
            output,
            ":{}:{}\n   |\n{} | ",
            position.line, position.column, position.line
        )?;
        output.segment(line.text, 0)?;
        output.write_str("\n   | ")?;
        for _ in 0..marker {
            output.write_char(' ')?;
        }
        for _ in 0..carets {
            output.write_char('^')?;
        }
        writeln!(output, " {label}")
    };
    render().map_err(|_| RenderError::ResourceLimit)
}

pub fn render_semantic(
    source: SourceFile<'_>,
    diagnostics: &SemanticDiagnostics,
) -> Result<String, RenderError> {
    let mut output = Buffer(String::new());
    let map = SourceMap::new(source.text);
    for (index, diagnostic) in diagnostics.items().iter().enumerate() {
        if index > 0 {
            output
                .write_char('\n')
                .map_err(|_| RenderError::ResourceLimit)?;
        }
        block(
            &mut output,
            source,
            &map,
            diagnostic.span,
            format_args!(
                "{}[{}]: {}",
                diagnostic.severity().as_str(),
                diagnostic.code().as_str(),
                diagnostic.message()
            ),
            diagnostic.label(),
        )?;
    }
    if diagnostics.errors_truncated() || diagnostics.warnings_truncated() {
        output
            .write_char('\n')
            .map_err(|_| RenderError::ResourceLimit)?;
    }
    for (truncated, severity) in [
        (diagnostics.errors_truncated(), "errors"),
        (diagnostics.warnings_truncated(), "warnings"),
    ] {
        if truncated {
            writeln!(
                output,
                "note: additional diagnostics suppressed after 32 {severity}"
            )
            .map_err(|_| RenderError::ResourceLimit)?;
        }
    }
    Ok(output.0)
}

/// Reuse the exact bounded source renderer for internal compiler failures.
pub(crate) fn render_compiler_diagnostic(
    source: SourceFile<'_>,
    span: Option<Span>,
    code: &str,
    message: &str,
) -> Result<String, RenderError> {
    render_located_diagnostic(
        source,
        span,
        code,
        message,
        "compiler rejected this construct",
    )
}

pub(crate) fn render_fixture_diagnostic(
    source: SourceFile<'_>,
    span: Option<Span>,
    code: &str,
    message: &str,
) -> Result<String, RenderError> {
    render_located_diagnostic(
        source,
        span,
        code,
        message,
        "fixture execution stopped here",
    )
}

fn render_located_diagnostic(
    source: SourceFile<'_>,
    span: Option<Span>,
    code: &str,
    message: &str,
    label: &str,
) -> Result<String, RenderError> {
    let mut output = Buffer(String::new());
    if let Some(span) = span {
        block(
            &mut output,
            source,
            &SourceMap::new(source.text),
            span,
            format_args!("error[{code}]: {message}"),
            label,
        )?;
    } else {
        writeln!(output, "error[{code}]: {message}").map_err(|_| RenderError::ResourceLimit)?;
    }
    Ok(output.0)
}

pub fn render_failure(
    source: SourceFile<'_>,
    failure: &CheckFailure,
) -> Result<String, RenderError> {
    match failure {
        CheckFailure::Semantic(diagnostics) => render_semantic(source, diagnostics),
        CheckFailure::Resource(failure) => {
            let mut output = Buffer(String::new());
            if let Some(span) = failure.span {
                block(
                    &mut output,
                    source,
                    &SourceMap::new(source.text),
                    span,
                    format_args!(
                        "error[resource-limit]: {} limit exceeded",
                        failure.kind.name()
                    ),
                    "resource limit exceeded",
                )?;
            } else {
                writeln!(
                    output,
                    "error[resource-limit]: {} limit exceeded",
                    failure.kind.name()
                )
                .map_err(|_| RenderError::ResourceLimit)?;
            }
            Ok(output.0)
        }
        CheckFailure::Syntax(diagnostics) => super::legacy::render(source, diagnostics),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::{CheckFailure, analyze};
    #[test]
    fn fixture_diagnostic_is_source_honest_and_distinct_from_compiler_rejection() {
        let source = SourceFile::new("demo.jky", "call()");
        let rendered = render_fixture_diagnostic(
            source,
            Some(Span::new(0, 6)),
            "Unsupported",
            "fixture execution failed",
        )
        .unwrap();
        assert!(rendered.contains("demo.jky:1:1"));
        assert!(rendered.contains("fixture execution stopped here"));
        assert!(!rendered.contains("compiler rejected"));
        assert!(
            render_fixture_diagnostic(source, Some(Span::new(0, 9)), "Unsupported", "failure")
                .is_err()
        );
        assert_eq!(
            render_fixture_diagnostic(source, None, "EXEC_OUTPUT_LIMIT", "failure").unwrap(),
            "error[EXEC_OUTPUT_LIMIT]: failure\n"
        );
    }
    #[test]
    fn exact_buffer_boundary_and_invalid_source_pair_fail_typed() {
        let mut buffer = Buffer(String::new());
        buffer.write_str(&"x".repeat(MAX_REPORT_BYTES)).unwrap();
        assert!(buffer.write_char('x').is_err());
        assert_eq!(buffer.0.len(), MAX_REPORT_BYTES);
        let failure = analyze(
            SourceFile::new(
                "x",
                "module m target ubuntu fn main(x: int) -> int { return false }",
            ),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap_err();
        assert_eq!(
            render_failure(SourceFile::new("wrong", ""), &failure),
            Err(RenderError::InvalidSpan)
        );
    }
    #[test]
    fn tabs_unicode_controls_and_crlf_have_stable_locations_and_carets() {
        let text = "module demo\ntarget ubuntu\nfn main(target: endpoint) -> forensic_result {\n\tif 1 { let word = \"é\" }\n\treturn forensic.system.profile(target)\n}\nrun main on selected_endpoints\n";
        let mut snapshots = Vec::new();
        for text in [text.to_owned(), text.replace('\n', "\r\n")] {
            let source = SourceFile::new("file\nname", &text);
            let failure = analyze(
                source,
                jocky_forensic::contracts::builtin_registry().unwrap(),
            )
            .unwrap_err();
            assert!(matches!(failure, CheckFailure::Semantic(_)));
            snapshots.push(render_failure(source, &failure).unwrap());
        }
        assert_eq!(snapshots[0], snapshots[1]);
        assert!(snapshots[0].contains("file\\nname:4:8"));
        assert!(snapshots[0].contains("4 |     if 1"));
    }
}
