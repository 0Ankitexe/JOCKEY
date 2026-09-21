use jocky_language::{AstDocument, SourceFile, parse};
use serde_json::Value;

const LF_SOURCE: &str = "module newline\ntarget windows | ubuntu\nfn inspect(target: endpoint) -> report {\n\treturn report(target, \"é\")\n}\nrun inspect on selected_endpoints\n";

#[test]
fn lf_and_crlf_have_equal_syntax_but_independent_byte_spans() {
    let crlf_source = LF_SOURCE.replace('\n', "\r\n");
    let lf = parse(SourceFile::new("lf.jky", LF_SOURCE)).expect("LF program");
    let crlf = parse(SourceFile::new("crlf.jky", &crlf_source)).expect("CRLF program");

    assert_eq!(lf.span.end, LF_SOURCE.len());
    assert_eq!(crlf.span.end, crlf_source.len());
    assert_ne!(lf.functions[0].body.span, crlf.functions[0].body.span);

    let mut lf_json = serde_json::to_value(AstDocument::new(lf)).unwrap();
    let mut crlf_json = serde_json::to_value(AstDocument::new(crlf)).unwrap();
    remove_spans(&mut lf_json);
    remove_spans(&mut crlf_json);
    assert_eq!(lf_json, crlf_json);
}

#[test]
fn final_newline_does_not_change_the_logical_tree() {
    let without = LF_SOURCE.strip_suffix('\n').unwrap();
    let with_program = parse(SourceFile::new("with.jky", LF_SOURCE)).unwrap();
    let without_program = parse(SourceFile::new("without.jky", without)).unwrap();
    let mut with_json = serde_json::to_value(AstDocument::new(with_program)).unwrap();
    let mut without_json = serde_json::to_value(AstDocument::new(without_program)).unwrap();
    remove_spans(&mut with_json);
    remove_spans(&mut without_json);
    assert_eq!(with_json, without_json);
}

#[test]
fn invalid_lf_and_crlf_keep_categories_and_logical_locations() {
    let lf = "module bad\ntarget macos\nfn inspect() -> report {}\n";
    let crlf = lf.replace('\n', "\r\n");
    let lf_diagnostics = parse(SourceFile::new("bad.jky", lf)).unwrap_err();
    let crlf_diagnostics = parse(SourceFile::new("bad.jky", &crlf)).unwrap_err();
    assert_eq!(
        lf_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.category)
            .collect::<Vec<_>>(),
        crlf_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.category)
            .collect::<Vec<_>>()
    );
    let lf_span = lf_diagnostics.items()[0].span;
    let crlf_span = crlf_diagnostics.items()[0].span;
    assert_ne!(lf_span, crlf_span);
    assert_eq!(
        jocky_language::SourceMap::new(lf)
            .location(lf_span.start)
            .unwrap(),
        jocky_language::SourceMap::new(&crlf)
            .location(crlf_span.start)
            .unwrap()
    );
}

fn remove_spans(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("span");
            object.remove("destination_span");
            for child in object.values_mut() {
                remove_spans(child);
            }
        }
        Value::Array(array) => {
            for child in array {
                remove_spans(child);
            }
        }
        _ => {}
    }
}
