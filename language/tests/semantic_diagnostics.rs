mod support;
use jocky_language::{
    SourceFile,
    report::{CheckReport, render_failure, render_semantic},
    semantic::analyze,
};
use serde_json::{Value, json};

#[test]
fn each_semantic_category_and_syntax_mapping_has_stable_human_json_parity() {
    let registry = jocky_forensic::contracts::builtin_registry().unwrap();
    let root = support::root().join("language/tests/fixtures");
    for directory in [
        "semantic/invalid",
        "semantic/warnings",
        "semantic/platforms",
        "invalid",
    ] {
        for file in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = file.unwrap().path();
            if path.extension().is_none_or(|e| e != "jky") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let source = SourceFile::new(path.file_name().unwrap().to_str().unwrap(), &text);
            let result = analyze(source, registry);
            let (human, report) = match &result {
                Ok(checked) => (
                    render_semantic(source, checked.warnings()).unwrap(),
                    CheckReport::from_checked(source, checked).unwrap(),
                ),
                Err(failure) => (
                    render_failure(source, failure).unwrap(),
                    CheckReport::from_failure(source, failure).unwrap(),
                ),
            };
            let bytes = report.to_pretty_json().unwrap();
            let value = support::assert_report(&bytes);
            for d in value["diagnostics"].as_array().unwrap() {
                assert!(
                    human.contains(&format!(
                        "{}[{}]: {}",
                        d["severity"].as_str().unwrap(),
                        d["code"].as_str().unwrap(),
                        d["message"].as_str().unwrap()
                    )),
                    "{path:?}"
                );
                if !d["location"].is_null() {
                    assert!(human.contains(d["location"]["snippet"].as_str().unwrap()));
                    assert!(human.contains(d["label"].as_str().unwrap()));
                }
            }
            for _ in 0..10 {
                assert_eq!(report.to_pretty_json().unwrap(), bytes);
            }
        }
    }
}
#[test]
fn independent_caps_lf_crlf_unicode_tabs_controls_and_eof_are_preserved() {
    let mut body = "// é 💾\n".to_owned();
    for i in 0..40 {
        body.push_str(&format!("\tlet unused{i} = 1 if 1 {{}}\n"));
    }
    body.push_str("return forensic.system.profile(target)\n");
    let text = format!(
        "module m\ntarget ubuntu\nfn main(target: endpoint) -> forensic_result {{\n{body}}}\nrun main on selected_endpoints\n"
    );
    let lf = support::assert_report(&support::report("file\n\t\u{001b}", &text));
    let crlf = support::assert_report(&support::report(
        "file\n\t\u{001b}",
        &text.replace('\n', "\r\n"),
    ));
    assert_eq!(lf["truncated"], json!({"errors":true,"warnings":true}));
    assert_eq!(lf["diagnostics"].as_array().unwrap().len(), 64);
    for (a, b) in lf["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .zip(crlf["diagnostics"].as_array().unwrap())
    {
        for key in ["start", "end", "snippet", "marker_start", "marker_width"] {
            assert_eq!(a["location"][key], b["location"][key]);
        }
    }
    let text = "module m\ntarget ubuntu\nfn helper(x: int) -> int { return x }\n";
    let report = support::assert_report(&support::report("EOF", text));
    let d = &report["diagnostics"][0];
    assert_eq!(d["code"], "missing-run");
    assert_eq!(
        d["location"]["span"],
        json!({"start":text.len(),"end":text.len()})
    );
    assert_eq!(d["location"]["marker_width"], 1);
    assert_eq!(d["location"]["snippet"], "");
}
#[test]
fn committed_snapshot_pairs_match_ten_repeated_renders() {
    let root = support::root().join("language/tests/fixtures/semantic/diagnostics");
    let mut count = 0;
    let lab_registry = jocky_forensic::contracts::Registry::from_json(include_str!(
        "../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
    ))
    .unwrap();
    for file in std::fs::read_dir(&root).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|e| e != "jky") {
            continue;
        }
        count += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        let source = SourceFile::new(name, &text);
        let registry = if name == "lab-profile-required.jky" {
            &lab_registry
        } else {
            jocky_forensic::contracts::builtin_registry().unwrap()
        };
        let result = analyze(source, registry);
        for _ in 0..10 {
            let human = match &result {
                Ok(c) => render_semantic(source, c.warnings()).unwrap(),
                Err(f) => render_failure(source, f).unwrap(),
            };
            assert_eq!(
                human,
                std::fs::read_to_string(path.with_extension("stderr")).unwrap()
            );
            let bytes = match &result {
                Ok(checked) => CheckReport::from_checked(source, checked),
                Err(failure) => CheckReport::from_failure(source, failure),
            }
            .unwrap()
            .to_pretty_json()
            .unwrap();
            assert_eq!(bytes, std::fs::read(path.with_extension("json")).unwrap());
            let _: Value = support::assert_report(&bytes);
        }
    }
    assert_eq!(count, 25);
}
