use jocky_forensic::contracts::{Registry, builtin_registry};
use jocky_language::{
    SourceFile, parse_source,
    semantic::{CheckFailure, analyze},
};
use serde_json::Value;

#[test]
fn supplied_text_labels_and_registry_order_do_not_change_syntax_or_metadata() {
    let text = include_str!("../../examples/triage.jky");
    let before = parse_source(SourceFile::new("missing", text)).unwrap();
    let bytes_before = before.to_pretty_json().unwrap();
    let reference = analyze(
        SourceFile::new("missing", text),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let mut document: Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    document["functions"].as_array_mut().unwrap().reverse();
    let registry = Registry::from_json(&document.to_string()).unwrap();
    for label in [
        "never-created/windows.jky",
        "never-created/ubuntu.jky",
        "lab_only/elevated",
        "https://invalid.example/source.jky",
        "C:\\missing\\source.jky",
        "",
    ] {
        let checked = analyze(SourceFile::new(label, text), &registry).unwrap();
        assert_eq!(checked.syntax(), &before);
        assert_eq!(checked.syntax().to_pretty_json().unwrap(), bytes_before);
        assert_eq!(checked.function_metadata(), reference.function_metadata());
        assert_eq!(checked.entry_metadata(), reference.entry_metadata());
        assert_eq!(checked.selected_targets(), reference.selected_targets());
        assert!(checked.profile().is_none());
        for node in checked.model().nodes() {
            assert!(text.get(node.span.start..node.span.end).is_some());
        }
    }
    assert_eq!(before.to_pretty_json().unwrap(), bytes_before);
}

#[test]
fn failed_analysis_preserves_original_utf8_and_newline_spans() {
    let base = include_str!("fixtures/semantic/platforms/windows-call-on-ubuntu.jky");
    for text in [
        format!("// café 🐎\n{base}"),
        format!("// café 🐎\n{base}").replace('\n', "\r\n"),
    ] {
        let snapshot = text.clone();
        let source = SourceFile::new("windows-does-not-exist", &text);
        let before = parse_source(source).unwrap();
        let bytes = before.to_pretty_json().unwrap();
        let CheckFailure::Semantic(errors) =
            analyze(source, builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(text, snapshot);
        assert_eq!(before.to_pretty_json().unwrap(), bytes);
        assert_eq!(parse_source(source).unwrap(), before);
        for error in errors.items() {
            assert_eq!(
                &text[error.span.start..error.span.end],
                "forensic.event.windows_log"
            );
        }
    }
}
