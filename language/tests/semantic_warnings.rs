use jocky_forensic::contracts::builtin_registry;
use jocky_language::{
    SourceFile,
    semantic::{CheckFailure, analyze},
};

#[test]
fn only_locals_and_loop_bindings_warn_including_underscore_names() {
    let checked = analyze(
        SourceFile::new(
            "unused",
            include_str!("fixtures/semantic/warnings/unused.jky"),
        ),
        builtin_registry().unwrap(),
    )
    .unwrap();
    assert_eq!(checked.warnings().items().len(), 2);
    assert!(
        checked
            .warnings()
            .items()
            .iter()
            .all(|d| d.code().as_str() == "unused-binding")
    );
}

#[test]
fn unreachable_reads_do_not_count_and_errors_are_still_checked() {
    let CheckFailure::Semantic(diagnostics) = analyze(
        SourceFile::new("dead", include_str!("fixtures/semantic/warnings/dead.jky")),
        builtin_registry().unwrap(),
    )
    .unwrap_err() else {
        panic!()
    };
    assert_eq!(
        diagnostics
            .items()
            .iter()
            .filter(|d| d.code().as_str() == "unused-binding")
            .count(),
        2
    );
    assert_eq!(
        diagnostics
            .items()
            .iter()
            .filter(|d| d.code().as_str() == "unreachable-code")
            .count(),
        2
    );
    assert_eq!(
        diagnostics
            .items()
            .iter()
            .filter(|d| d.code().as_str() == "unknown-name")
            .count(),
        1
    );
}

#[test]
fn errors_and_warnings_have_independent_caps_and_deterministic_order() {
    let body = (0..40)
        .map(|i| format!("let unused{i} = missing{i}\n"))
        .collect::<String>();
    let text = format!(
        "module demo target ubuntu fn main(target: endpoint) -> forensic_result {{ {body} return forensic.system.profile(target) }} run main on selected_endpoints"
    );
    let CheckFailure::Semantic(diagnostics) =
        analyze(SourceFile::new("caps", &text), builtin_registry().unwrap()).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(diagnostics.items().len(), 64);
    assert!(diagnostics.errors_truncated() && diagnostics.warnings_truncated());
    let rendered =
        jocky_language::report::render_semantic(SourceFile::new("caps", &text), &diagnostics)
            .unwrap();
    assert!(rendered.ends_with("\nnote: additional diagnostics suppressed after 32 errors\nnote: additional diagnostics suppressed after 32 warnings\n"));
    assert!(
        diagnostics
            .items()
            .windows(2)
            .all(|p| p[0].span.start <= p[1].span.start)
    );
    for _ in 0..10 {
        let CheckFailure::Semantic(repeated) =
            analyze(SourceFile::new("caps", &text), builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(repeated, diagnostics);
    }
}

#[test]
fn non_callable_references_count_only_when_reachable() {
    for (prefix, unused) in [
        ("", false),
        ("return forensic.system.profile(target)", true),
    ] {
        let text = format!(
            "module demo target ubuntu fn main(target: endpoint) -> forensic_result {{ let value = 1 {prefix} value() return forensic.system.profile(target) }} run main on selected_endpoints"
        );
        let CheckFailure::Semantic(diagnostics) =
            analyze(SourceFile::new("call", &text), builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert!(
            diagnostics
                .items()
                .iter()
                .any(|d| d.code().as_str() == "not-callable")
        );
        assert_eq!(
            diagnostics
                .items()
                .iter()
                .any(|d| d.code().as_str() == "unused-binding"),
            unused
        );
    }
}
