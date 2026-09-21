mod support;
use jocky_language::{
    SourceFile, SourceMap, Span,
    report::{CheckReport, ReportError},
    semantic::analyze,
};
use serde_json::json;

#[test]
fn metadata_order_entry_identity_and_selected_targets_are_source_derived() {
    for file in [
        "examples/triage.jky",
        "examples/cross-platform.jky",
        "examples/windows-events.jky",
        "examples/ubuntu-events.jky",
        "language/tests/fixtures/profile/valid.jky",
    ] {
        let text = std::fs::read_to_string(support::root().join(file)).unwrap();
        let source = SourceFile::new(file, &text);
        let checked = analyze(
            source,
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        let bytes = CheckReport::from_checked(source, &checked)
            .unwrap()
            .to_pretty_json()
            .unwrap();
        let value = support::assert_report(&bytes);
        let m = &value["metadata"];
        assert_eq!(m["module"], checked.syntax().program.module.name.text);
        assert_eq!(m["selected_targets"], json!(checked.selected_targets()));
        let functions = m["functions"].as_array().unwrap();
        assert_eq!(functions.len(), checked.function_metadata().len());
        for (actual, expected) in functions.iter().zip(checked.function_metadata()) {
            assert_eq!(actual["name"], expected.name());
            assert_eq!(actual["capabilities"], json!(expected.capabilities()));
            assert_eq!(
                actual["supported_platforms"],
                json!(expected.supported_platforms())
            );
            assert_eq!(
                actual["required_privilege"],
                json!(expected.required_privilege())
            );
            assert_eq!(
                actual["unavailable_dependencies"],
                json!(expected.unavailable_dependencies().collect::<Vec<_>>())
            );
        }
        assert_eq!(
            functions
                .iter()
                .filter(|f| f["name"] == m["entry"]["name"])
                .count(),
            1
        );
        assert_eq!(
            &m["entry"],
            functions
                .iter()
                .find(|f| f["name"] == m["entry"]["name"])
                .unwrap()
        );
    }
}
#[test]
fn diagnostic_locations_retain_original_utf8_spans_and_first_line_markers() {
    let base = "module m\ntarget ubuntu\nfn main(target: endpoint) -> forensic_result {\n\tif \"é\u{001b}\" {\n let unused = 1\n}\nreturn forensic.system.profile(target)\nlet dead = \"💾\"\n}\n";
    for text in [base.to_owned(), base.replace('\n', "\r\n")] {
        let value = support::assert_report(&support::report("weird\n\t\u{001b}", &text));
        let map = SourceMap::new(&text);
        for diagnostic in value["diagnostics"].as_array().unwrap() {
            let loc = &diagnostic["location"];
            let span: Span = serde_json::from_value(loc["span"].clone()).unwrap();
            map.validate_span(span).unwrap();
            for (key, offset) in [("start", span.start), ("end", span.end)] {
                let expected = map.location(offset).unwrap();
                assert_eq!(
                    loc[key],
                    json!({"line":expected.line,"column":expected.column})
                );
            }
            assert!(loc["marker_width"].as_u64().unwrap() >= 1);
        }
        let condition = value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "condition-not-bool")
            .unwrap();
        assert_eq!(condition["location"]["snippet"], "    if \"é\\u{001B}\" {");
        assert_eq!(condition["location"]["marker_start"], 7);
    }
}
#[test]
fn report_constructors_reject_inconsistent_source_and_empty_fake_failure() {
    let text = include_str!("../../examples/triage.jky");
    let checked = analyze(
        SourceFile::new("triage", text),
        jocky_forensic::contracts::builtin_registry().unwrap(),
    )
    .unwrap();
    let wrong = text.replacen("triage", "broken", 1);
    assert_eq!(
        CheckReport::from_checked(SourceFile::new("other", &wrong), &checked).unwrap_err(),
        ReportError::InvalidMetadata
    );
    // Same byte length and same module/functions are still a different program.
    let wrong = text.replacen("system.profile", "system.missing", 1);
    assert_eq!(
        CheckReport::from_checked(SourceFile::new("other", &wrong), &checked).unwrap_err(),
        ReportError::InvalidMetadata
    );
    let failure = jocky_language::semantic::CheckFailure::Semantic(Default::default());
    assert_eq!(
        CheckReport::from_failure(SourceFile::new("other", ""), &failure).unwrap_err(),
        ReportError::InvalidMetadata
    );
    for span in [
        Span { start: 1, end: 0 },
        Span { start: 0, end: 3 },
        Span { start: 1, end: 2 },
    ] {
        let failure = jocky_language::semantic::CheckFailure::Resource(
            jocky_language::semantic::ResourceFailure {
                kind: jocky_language::semantic::ResourceKind::TypeDepth,
                span: Some(span),
            },
        );
        assert_eq!(
            CheckReport::from_failure(SourceFile::new("unicode", "é"), &failure).unwrap_err(),
            ReportError::InvalidSpan
        );
    }
}

#[test]
fn every_host_constructor_is_typed_locationless_and_cannot_include_metadata() {
    use jocky_language::report::{HostFailure, ReportStatus};
    for failure in [
        HostFailure::InvalidPathEncoding,
        HostFailure::InputNotFound,
        HostFailure::InputUnreadable,
        HostFailure::InvalidUtf8,
        HostFailure::InvalidRegistry,
        HostFailure::SourceLimit,
        HostFailure::ReportLimit,
        HostFailure::OutputFailure,
    ] {
        for file in [None, Some("file\n\u{001b}")] {
            let report = CheckReport::host_failure(file, failure);
            assert_eq!(report.status(), ReportStatus::Failure);
            assert_eq!(report.status().exit_code(), 2);
            let value = support::assert_report(&report.to_pretty_json().unwrap());
            assert_eq!(value["diagnostics"][0]["code"], failure.code());
            assert_eq!(value["diagnostics"][0]["location"], serde_json::Value::Null);
            assert_eq!(value["diagnostics"][0]["label"], serde_json::Value::Null);
            assert_eq!(value["metadata"], serde_json::Value::Null);
        }
    }
}
