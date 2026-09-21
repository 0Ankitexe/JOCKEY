use jocky_forensic::contracts::builtin_registry;
use jocky_language::{
    SourceFile, parse,
    semantic::{CheckFailure, ResourceKind, analyze},
};

fn resource(text: &str, kind: ResourceKind) {
    let result = analyze(SourceFile::new("limits", text), builtin_registry().unwrap());
    assert!(
        matches!(result,Err(CheckFailure::Resource(ref error)) if error.kind==kind),
        "{result:?}"
    );
}

#[test]
fn source_function_and_type_depth_limits_do_not_change_legacy_parse() {
    let base = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { return forensic.system.profile(target) } run main on selected_endpoints";
    let mut exact = base.to_owned();
    exact.push_str(&" ".repeat(4_194_304 - exact.len()));
    assert!(
        analyze(
            SourceFile::new("limits", &exact),
            builtin_registry().unwrap()
        )
        .is_ok()
    );
    exact.push(' ');
    resource(&exact, ResourceKind::SourceBytes);
    for count in [1023, 1024] {
        let functions = (0..count)
            .map(|i| format!("fn f{i}() -> int {{ return 1 }}\n"))
            .collect::<String>();
        let source = base.replacen("fn main", &format!("{functions}fn main"), 1);
        if count == 1023 {
            assert!(analyze(SourceFile::new("x", &source), builtin_registry().unwrap()).is_ok());
        } else {
            parse(SourceFile::new("legacy", &source)).unwrap();
            resource(&source, ResourceKind::Functions);
        }
    }
    for depth in [64, 65] {
        let ty = format!("{}int{}", "list<".repeat(depth - 1), ">".repeat(depth - 1));
        let source = base.replacen(
            "fn main",
            &format!("fn identity(value: {ty}) -> {ty} {{ return value }} fn main"),
            1,
        );
        parse(SourceFile::new("legacy", &source)).unwrap();
        if depth == 64 {
            assert!(analyze(SourceFile::new("x", &source), builtin_registry().unwrap()).is_ok());
        } else {
            resource(&source, ResourceKind::TypeDepth);
        }
    }
    resource(&"(".repeat(257), ResourceKind::ParserNesting);
}

#[test]
fn arbitrary_input_campaign_never_panics() {
    let alphabet = [
        'a', 'f', '(', ')', '{', '}', '<', '>', '"', '\n', '\r', '\t', '\0', 'é', '🙂',
    ];
    let mut seed = 0x1234_5678_u64;
    for iteration in 0..1000 {
        let text = (0..iteration % 100)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                alphabet[(seed as usize) % alphabet.len()]
            })
            .collect::<String>();
        let _ = analyze(
            SourceFile::new("memory", &text),
            builtin_registry().unwrap(),
        );
    }
}

#[test]
fn real_syntax_node_and_binding_boundaries_are_enforced() {
    let base = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { BODY return forensic.system.profile(target) } run main on selected_endpoints";
    let with_helper = base.replacen("fn main", "fn noop() -> int { return 1 } fn main", 1);
    // 31 fixed syntax occurrences + 33,323 calls of three nodes = 100,000.
    let exact = with_helper.replace("BODY", &"noop() ".repeat(33_323));
    assert!(
        analyze(
            SourceFile::new("nodes", &exact),
            builtin_registry().unwrap()
        )
        .is_ok()
    );
    resource(
        &with_helper.replace("BODY", &"noop() ".repeat(33_324)),
        ResourceKind::SyntaxNodes,
    );
    for bindings in [19_999, 20_000] {
        let locals = (0..bindings)
            .map(|i| format!("let binding{i} = 1 "))
            .collect::<String>();
        let source = base.replace("BODY", &locals);
        if bindings == 19_999 {
            assert!(
                analyze(
                    SourceFile::new("bindings", &source),
                    builtin_registry().unwrap()
                )
                .is_ok()
            );
        } else {
            resource(&source, ResourceKind::Bindings);
        }
    }
}

#[test]
fn real_dense_call_edges_reach_the_bound_before_the_node_limit() {
    let make = |extra: usize| {
        let functions = (0..183)
            .map(|from| {
                let count = if from < 179 {
                    183
                } else if from == 179 {
                    11 + extra
                } else {
                    0
                };
                let calls = (0..count).map(|to| format!("f{to}() ")).collect::<String>();
                format!("fn f{from}() -> int {{ {calls} return 1 }}\n")
            })
            .collect::<String>();
        format!(
            "module demo target ubuntu {functions} fn main(target: endpoint) -> forensic_result {{ return forensic.system.profile(target) }} run main on selected_endpoints"
        )
    };
    assert!(
        analyze(
            SourceFile::new("edges", &make(0)),
            builtin_registry().unwrap()
        )
        .is_ok()
    );
    resource(&make(1), ResourceKind::CallEdges);
}

#[test]
fn actual_repeated_type_lowering_counts_cache_hits_at_the_work_boundary() {
    use jocky_forensic::contracts::Registry;
    use serde_json::json;
    let mut ty = json!({"kind":"named","name":"int"});
    for _ in 1..64 {
        ty = json!({"kind":"list","element":ty});
    }
    let functions=(0..3).map(|i|json!({
        "module":"forensic.test","name":format!("f{i}"),"version":"0.1.0",
        "parameters":(0..64).map(|p|json!({"name":format!("p{p}"),"type":ty})).collect::<Vec<_>>(),
        "result":{"kind":"named","name":"int"},"supported_platforms":["windows","ubuntu"],
        "required_privilege":"user","capability":"system","read_only":true,"lab_only":false,
        "possible_errors":[{"code":"Unsupported","type":{"kind":"named","name":"diagnostic_error"}}],
        "availability":"unavailable"
    })).collect::<Vec<_>>();
    let registry =
        Registry::from_json(&json!({"schema_version":"1.0.0","functions":functions}).to_string())
            .unwrap();
    let deep = format!("{}int{}", "list<".repeat(63), ">".repeat(63));
    for extra in [8, 9] {
        let mut parameters = (0..1370)
            .map(|p| format!("p{p}: {deep}"))
            .collect::<Vec<_>>();
        parameters.extend((0..extra).map(|p| format!("leaf{p}: int")));
        let source = format!(
            "module demo target ubuntu fn helper({}) -> int {{ return 1 }} fn main(target: endpoint) -> forensic_result {{ return main(target) }} run main on selected_endpoints",
            parameters.join(", ")
        );
        let checked = analyze(SourceFile::new("types", &source), &registry);
        // 18 primitive slots + 3*(64*64+1) contract visits + source visits.
        if extra == 8 {
            assert!(checked.is_ok(), "{checked:?}");
        } else {
            assert!(
                matches!(checked,Err(CheckFailure::Resource(ref e)) if e.kind==ResourceKind::TypeWork),
                "{checked:?}"
            );
        }
    }
}
