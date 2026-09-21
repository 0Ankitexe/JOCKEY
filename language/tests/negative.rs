mod common;

use std::collections::BTreeSet;
use std::error::Error;

use jocky_language::{DiagnosticCategory, SourceFile, parse, render_diagnostics};

#[test]
fn invalid_corpus_is_paired_rejected_and_snapshot_stable() -> Result<(), Box<dyn Error>> {
    let pairs = common::discover_pairs("invalid", "stderr")?;
    assert!(pairs.len() >= 15, "expected at least 15 invalid pairs");
    let mut categories = BTreeSet::new();

    for pair in pairs {
        let source = common::read_utf8(&pair.source)?;
        let label = common::fixture_label(&pair.source)?;
        let source_file = SourceFile::new(&label, &source);
        let diagnostics = parse(source_file)
            .expect_err("invalid fixtures must never return a partial or complete AST");
        categories.extend(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.category.as_str()),
        );
        let rendered = render_diagnostics(source_file, &diagnostics);
        common::assert_exact_bytes(rendered.as_bytes(), &pair.oracle)?;
        assert!(!rendered.contains("Rule::"));
        assert!(!rendered.contains("ParsingError"));
    }

    for required in [
        "unexpected-token",
        "missing-delimiter",
        "invalid-target-declaration",
        "malformed-type",
        "duplicate-top-level-run",
    ] {
        assert!(categories.contains(required), "missing category {required}");
    }
    Ok(())
}

#[test]
fn additional_negative_forms_are_never_accepted() {
    let cases = [
        ("unterminated block comment", "/* open", "missing-delimiter"),
        (
            "unterminated parenthesis",
            "module m\ntarget windows\nfn f() -> r { return call( }",
            "missing-delimiter",
        ),
        (
            "nested block comment attempt",
            "module m\ntarget windows\nfn f() -> r { /* outer /* inner */ tail */ return r() }",
            "unexpected-token",
        ),
        (
            "invalid string escape",
            "module m\ntarget windows\nfn f() -> r { return \"bad\\q\" }",
            "unexpected-token",
        ),
        (
            "too many targets",
            "module m\ntarget windows | ubuntu | windows\nfn f() -> r {}",
            "invalid-target-declaration",
        ),
        (
            "trailing argument",
            "module m\ntarget windows\nfn f() -> r { return call(value,) }",
            "unexpected-token",
        ),
        (
            "leading zero integer",
            "module m\ntarget windows\nfn f() -> r { return 01 }",
            "unexpected-token",
        ),
        (
            "negative zero integer",
            "module m\ntarget windows\nfn f() -> r { return -0 }",
            "unexpected-token",
        ),
        (
            "malformed option",
            "module m\ntarget windows\nfn f() -> option<> {}",
            "malformed-type",
        ),
        (
            "malformed result",
            "module m\ntarget windows\nfn f() -> result<r> {}",
            "malformed-type",
        ),
        (
            "orphan syntax",
            "module m\ntarget windows\nfn f() -> r { else {} }",
            "unexpected-token",
        ),
        (
            "unsupported loop",
            "module m\ntarget windows\nfn f() -> r { while true {} }",
            "unexpected-token",
        ),
    ];

    for (name, source, category) in cases {
        let diagnostics = parse(SourceFile::new("negative.jky", source))
            .expect_err("negative case must not return an AST");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.category.as_str() == category),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn resource_limit_is_typed_and_not_a_syntax_category() {
    let source = "(".repeat(257);
    let diagnostics = parse(SourceFile::new("deep.jky", &source)).expect_err("must reject depth");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics.items()[0].category,
        DiagnosticCategory::ResourceLimit
    );
    assert!(!diagnostics.items()[0].category.is_syntax());
}
