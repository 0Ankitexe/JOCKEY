use jocky_forensic::contracts::builtin_registry;
use jocky_language::{
    SourceFile, parse_source,
    semantic::{CheckFailure, analyze},
};
use serde_json::Value;
use std::path::Path;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/semantic")
}

#[test]
fn positive_manifest_resolves_without_execution_and_preserves_syntax() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/semantic/valid/cases.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let text = std::fs::read_to_string(
            fixtures()
                .join("valid")
                .join(case["file"].as_str().unwrap()),
        )
        .unwrap();
        let source = SourceFile::new("does/not/exist.jky", &text);
        let syntax = parse_source(source).unwrap();
        let checked = analyze(source, builtin_registry().unwrap()).unwrap();
        assert_eq!(checked.syntax(), &syntax);
        assert!(!checked.model().calls().is_empty());
        for node in checked.model().nodes() {
            assert!(text.get(node.span.start..node.span.end).is_some());
        }
        assert!(!checked.warnings().has_errors());
    }
    let source = SourceFile::new(
        "nonexistent-triage",
        include_str!("../../examples/triage.jky"),
    );
    let checked = analyze(source, builtin_registry().unwrap()).unwrap();
    assert_eq!(checked.model().calls().len(), 5);
    assert!(checked.warnings().items().is_empty());
    assert!(!checked.model().references().is_empty());
}

#[test]
fn twenty_seven_distinct_syntax_valid_negatives_have_exact_causes_and_spans() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/semantic/invalid/cases.json")).unwrap();
    assert!(cases.as_array().unwrap().len() >= 27);
    for case in cases.as_array().unwrap() {
        let text = std::fs::read_to_string(
            fixtures()
                .join("invalid")
                .join(case["file"].as_str().unwrap()),
        )
        .unwrap();
        let source = SourceFile::new("not-a-real-path", &text);
        parse_source(source).unwrap_or_else(|error| panic!("syntax fixture {case}: {error:?}"));
        let CheckFailure::Semantic(diagnostics) =
            analyze(source, builtin_registry().unwrap()).unwrap_err()
        else {
            panic!("expected semantic failure: {case}");
        };
        assert!(diagnostics.has_errors());
        assert!(
            diagnostics
                .items()
                .iter()
                .any(|d| d.code().as_str() == case["code"].as_str().unwrap()
                    && d.span.start == case["span"]["start"].as_u64().unwrap() as usize
                    && d.span.end == case["span"]["end"].as_u64().unwrap() as usize),
            "{case}: {diagnostics:?}"
        );
    }
}

#[test]
fn poisoned_calls_still_validate_arguments_without_dependent_errors() {
    let text = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { missing(also_missing) return forensic.system.profile(target) } run main on selected_endpoints";
    let CheckFailure::Semantic(errors) =
        analyze(SourceFile::new("x", text), builtin_registry().unwrap()).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(errors.items().len(), 2);
    assert!(
        errors
            .items()
            .iter()
            .all(|d| d.code().as_str() == "unknown-name")
    );
}

#[test]
fn remaining_scope_constructor_and_collection_negative_families_are_closed() {
    let base = "module demo target ubuntu HELPERS fn main(target: endpoint) -> forensic_result { BODY return forensic.system.profile(target) } run main on selected_endpoints";
    for (helper, body, code) in [
        ("", "let forensic_result = 1", "duplicate-definition"),
        (
            "fn helper(forensic_result: int) -> int { return 1 }",
            "",
            "duplicate-definition",
        ),
        (
            "",
            "for forensic_result in forensic.process.list(target) { forensic.system.profile(target) }",
            "duplicate-definition",
        ),
        ("fn helper() -> mystery { return 1 }", "", "unknown-type"),
        (
            "fn helper(value: bytes) -> int { for item in value { return 1 } return 1 }",
            "",
            "not-a-collection",
        ),
        (
            "fn helper(value: option<int>) -> int { for item in value { return 1 } return 1 }",
            "",
            "not-a-collection",
        ),
        (
            "fn helper(value: result<int, diagnostic_error>) -> int { for item in value { return 1 } return 1 }",
            "",
            "not-a-collection",
        ),
        ("", "let value = path(\"x\")", "unknown-name"),
        (
            "fn helper(value: list<int>) -> bytes { return value }",
            "",
            "return-type-mismatch",
        ),
        (
            "fn helper(x: int) -> int { return x }",
            "helper(false, 1)",
            "argument-count-mismatch",
        ),
        (
            "fn helper(x: int) -> int { return x }",
            "helper(false, 1)",
            "argument-type-mismatch",
        ),
    ] {
        let text = base.replace("HELPERS", helper).replace("BODY", body);
        parse_source(SourceFile::new("x", &text)).unwrap();
        let CheckFailure::Semantic(errors) =
            analyze(SourceFile::new("x", &text), builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert!(
            errors.items().iter().any(|e| e.code().as_str() == code),
            "{text}: {errors:?}"
        );
    }
    for module in ["builtin", "forensic"] {
        let text = base
            .replace("HELPERS", "")
            .replace("BODY", "")
            .replace("module demo", &format!("module {module}"));
        assert!(matches!(
            analyze(SourceFile::new("x", &text), builtin_registry().unwrap()),
            Err(CheckFailure::Semantic(_))
        ));
    }
}

#[test]
fn shadow_initializers_resolve_to_outer_binding_and_types_are_exact() {
    let text = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { let outer = 1 if true { let outer = outer consume(outer) } else { consume(outer) } return forensic.system.profile(target) } fn consume(value: int) -> int { return value } run main on selected_endpoints";
    let checked = analyze(SourceFile::new("x", text), builtin_registry().unwrap()).unwrap();
    assert!(checked.warnings().items().is_empty());
    let bindings = checked
        .model()
        .bindings()
        .iter()
        .filter(|b| b.name == "outer")
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 2);
    assert_ne!(bindings[0].id, bindings[1].id);
    assert_eq!(
        checked
            .model()
            .type_name(bindings[1].type_id().unwrap())
            .unwrap(),
        "int"
    );
    let initializer = text.find("= outer").unwrap() + 2;
    let reference = checked
        .model()
        .references()
        .iter()
        .find(|r| {
            checked
                .model()
                .nodes()
                .iter()
                .any(|node| node.id == r.node && node.span.start == initializer)
        })
        .unwrap();
    assert_eq!(reference.binding, bindings[0].id);
    for call in checked.model().calls() {
        assert!(
            checked
                .model()
                .nodes()
                .iter()
                .any(|n| n.id == call.node && n.span == call.call_span)
        );
        assert!(
            call.arguments.iter().all(|id| checked
                .model()
                .nodes()
                .iter()
                .any(|node| node.id == *id))
        );
    }
}
