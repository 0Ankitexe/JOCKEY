mod common;

use std::error::Error;

use jocky_language::{
    DelimiterKind, Diagnostic, DiagnosticCategory, DiagnosticDetail, DiagnosticSet, MessageKey,
    SourceFile, Span, parse, render_diagnostics,
};

#[test]
fn every_committed_snapshot_is_deterministic_across_ten_runs() -> Result<(), Box<dyn Error>> {
    for pair in common::discover_pairs("invalid", "stderr")? {
        let source = common::read_utf8(&pair.source)?;
        let label = common::fixture_label(&pair.source)?;
        let source_file = SourceFile::new(&label, &source);
        let expected = std::fs::read(&pair.oracle)?;
        for iteration in 0..10 {
            let diagnostics = parse(source_file).expect_err("invalid source");
            let actual = render_diagnostics(source_file, &diagnostics);
            assert_eq!(actual.as_bytes(), expected, "{} run {iteration}", pair.name);
        }
    }
    Ok(())
}

#[test]
fn rendering_handles_unicode_tabs_controls_eof_and_multiline_spans() {
    let text = "\tα\u{0001}bad\nsecond";
    let start = text.find("bad").unwrap();
    let diagnostics = DiagnosticSet::new(vec![
        Diagnostic::new(
            DiagnosticCategory::UnexpectedToken,
            MessageKey::UnexpectedToken,
            Span::new(start, text.len()),
        ),
        Diagnostic::new(
            DiagnosticCategory::MissingDelimiter,
            MessageKey::MissingDelimiter,
            Span::new(text.len(), text.len()),
        )
        .with_detail(DiagnosticDetail::MissingDelimiter(DelimiterKind::Brace)),
    ])
    .unwrap();
    let rendered = render_diagnostics(SourceFile::new("bad\t\u{007f}.jky", text), &diagnostics);

    assert!(rendered.contains("--> bad\\t\\u{007F}.jky:1:7\n"));
    assert!(rendered.contains("1 |     α\\u{0001}bad\n"));
    assert!(rendered.contains("--> bad\\t\\u{007F}.jky:2:7\n"));
    assert!(!rendered.contains('\r'));
}

#[test]
fn ordering_deduplication_and_truncation_are_stable() {
    let duplicate = Diagnostic::new(
        DiagnosticCategory::MalformedType,
        MessageKey::MalformedType,
        Span::new(1, 2),
    );
    let mut values = vec![duplicate.clone(), duplicate];
    values.extend((0..40).map(|index| {
        Diagnostic::new(
            DiagnosticCategory::UnexpectedToken,
            MessageKey::UnexpectedToken,
            Span::new(index + 10, index + 10),
        )
    }));
    let set = DiagnosticSet::new(values).unwrap();
    assert_eq!(set.len(), 32);
    assert!(set.is_truncated());
    assert_eq!(set.items()[0].category, DiagnosticCategory::MalformedType);
    let rendered = render_diagnostics(SourceFile::new("many.jky", ""), &set);
    assert!(rendered.ends_with("note: additional diagnostics suppressed after 32 errors\n"));
}
