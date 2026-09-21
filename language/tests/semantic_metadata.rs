use jocky_forensic::contracts::builtin_registry;
use jocky_language::{
    SourceFile, Span, parse_source,
    semantic::{CheckFailure, Severity, analyze},
};
use jocky_shared::compiler::{Capability, Platform, Privilege};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct ExpectedError {
    code: String,
    name: String,
    span: Span,
}
#[derive(Deserialize)]
struct Case {
    file: String,
    selected_targets: Vec<Platform>,
    supported_platforms: Vec<Platform>,
    errors: Vec<ExpectedError>,
}

#[test]
fn platform_matrix_checks_every_call_and_returns_only_complete_metadata() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/semantic/platforms/cases.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/semantic/platforms")
                .join(&case.file),
        )
        .unwrap();
        let source = SourceFile::new(&case.file, &text);
        parse_source(source).expect("semantic fixtures must be syntax-valid");
        let result = analyze(source, builtin_registry().unwrap());
        if case.errors.is_empty() {
            let checked = result.unwrap();
            assert_eq!(checked.selected_targets(), case.selected_targets);
            assert!(checked.profile().is_none());
            let summaries = checked.function_metadata();
            assert_eq!(summaries.len(), 2);
            assert_eq!(summaries[0].name(), "platforms.events");
            assert_eq!(summaries[0].supported_platforms(), case.supported_platforms);
            assert_eq!(summaries[0].required_privilege(), Privilege::Elevated);
            assert_eq!(summaries[0].capabilities(), [Capability::Events]);
            assert_eq!(checked.entry_metadata(), &summaries[1]);
            assert_eq!(checked.entry_metadata().name(), "platforms.main");
            assert_eq!(
                checked.entry_metadata().required_privilege(),
                Privilege::User
            );
            assert_eq!(
                checked.entry_metadata().capabilities(),
                [Capability::System]
            );
            assert_eq!(
                checked.entry_metadata().supported_platforms(),
                Platform::ALL
            );
            assert_eq!(
                checked
                    .entry_metadata()
                    .unavailable_dependencies()
                    .collect::<Vec<_>>(),
                ["forensic.system.profile"]
            );
        } else {
            let CheckFailure::Semantic(diagnostics) = result.unwrap_err() else {
                panic!("{}: wrong failure layer", case.file)
            };
            let errors = diagnostics
                .items()
                .iter()
                .filter(|d| d.severity() == Severity::Error)
                .collect::<Vec<_>>();
            assert_eq!(
                errors.len(),
                case.errors.len(),
                "{}: {diagnostics:?}",
                case.file
            );
            for (actual, expected) in errors.iter().zip(case.errors) {
                assert_eq!(actual.code().as_str(), expected.code);
                assert_eq!(actual.span, expected.span, "{}", case.file);
                assert_eq!(&text[actual.span.start..actual.span.end], expected.name);
            }
        }
    }
}

#[test]
fn transitive_platform_intersection_includes_dead_calls_but_not_uncalled_helpers() {
    let text = "module demo target windows fn leaf(target: endpoint, stamp: timestamp) -> list<event_record> { return forensic.event.windows_log(target, \"System\", stamp, stamp, 1) } fn caller(target: endpoint, stamp: timestamp) -> list<event_record> { return leaf(target, stamp) forensic.process.list(target) } fn main(target: endpoint) -> forensic_result { return forensic.system.profile(target) } run main on selected_endpoints";
    let checked = analyze(
        SourceFile::new("not-a-host", text),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let caller = &checked.function_metadata()[1];
    assert_eq!(caller.supported_platforms(), [Platform::Windows]);
    assert_eq!(
        caller.capabilities(),
        [Capability::Processes, Capability::Events]
    );
    assert_eq!(caller.required_privilege(), Privilege::Elevated);
    assert_eq!(
        caller.unavailable_dependencies().collect::<Vec<_>>(),
        ["forensic.event.windows_log", "forensic.process.list"]
    );
    assert_eq!(
        checked.entry_metadata().supported_platforms(),
        Platform::ALL
    );
    assert!(
        checked
            .warnings()
            .items()
            .iter()
            .any(|d| d.code().as_str() == "unreachable-code")
    );
    let rejected = text.replace("target windows", "target ubuntu");
    let CheckFailure::Semantic(diagnostics) = analyze(
        SourceFile::new("windows", &rejected),
        builtin_registry().unwrap(),
    )
    .unwrap_err() else {
        panic!()
    };
    let errors = diagnostics
        .items()
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        &rejected[errors[0].span.start..errors[0].span.end],
        "forensic.event.windows_log"
    );
}
