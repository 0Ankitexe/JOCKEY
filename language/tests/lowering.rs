use jocky_forensic::contracts::builtin_registry;
use jocky_ir::model::{Callee, Operation, SlotKind};
use jocky_language::{
    SourceFile, SourceMap, Span,
    lowering::{BuildOptions, LoweringFailure, lower},
    semantic::{BindingKind, CallTarget, SemanticModel, TypeId, TypeNode, analyze},
};
use jocky_shared::compiler::Type;

fn same_type(model: &SemanticModel, id: TypeId, ty: &Type) {
    let mut work = vec![(id, ty)];
    while let Some((id, ty)) = work.pop() {
        match (model.type_node(id).unwrap(), ty) {
            (TypeNode::Named(a), Type::Named { name: b }) => assert_eq!(a, *b),
            (TypeNode::List(a), Type::List { element: b })
            | (TypeNode::Option(a), Type::Option { element: b }) => work.push((a, b)),
            (TypeNode::Result { ok: a, error: ae }, Type::Result { ok: b, error: be }) => {
                work.extend([(a, b.as_ref()), (ae, be.as_ref())])
            }
            pair => panic!("different structural types: {pair:?}"),
        }
    }
}

#[test]
fn every_golden_preserves_bindings_signatures_resolved_calls_and_static_metadata() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ir/golden/manifest.json")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["source"].as_str().unwrap();
        let text = std::fs::read_to_string(root.join(name)).unwrap();
        let checked = analyze(SourceFile::new(name, &text), builtin_registry().unwrap()).unwrap();
        let ir = lower(&checked, BuildOptions::default()).unwrap();
        let d = ir.document();
        let model = checked.model();
        let map = SourceMap::new(&text);
        for (f, function) in d.functions.iter().enumerate() {
            let symbol = &model.functions()[f];
            let syntax = &checked.syntax().program.functions[f];
            assert_eq!(function.name, symbol.name());
            assert_eq!(function.span.unwrap().start, syntax.span.start);
            assert_eq!(function.span.unwrap().end, syntax.span.end);
            same_type(model, symbol.result_type().unwrap(), &function.result);
            let metadata = &checked.function_metadata()[f];
            assert_eq!(
                function.metadata.required_privilege,
                metadata.required_privilege()
            );
            assert_eq!(function.metadata.capabilities, metadata.capabilities());
            assert_eq!(
                function.metadata.supported_platforms,
                metadata.supported_platforms()
            );
            assert_eq!(
                function.metadata.unavailable_dependencies,
                metadata.unavailable_dependencies().collect::<Vec<_>>()
            );
            for (position, &binding) in symbol.parameters().iter().enumerate() {
                let b = model.binding(binding).unwrap();
                let slot = &function.slots[function.parameters[position]];
                assert_eq!(slot.name.as_deref(), Some(b.name.as_str()));
                assert_eq!(slot.span.unwrap().start, b.span.start);
                same_type(
                    model,
                    symbol.parameter_type(position).unwrap(),
                    &slot.r#type,
                );
            }
            for b in model.bindings().iter().filter(|b| b.owner() == symbol.id()) {
                let slot = function
                    .slots
                    .iter()
                    .find(|s| {
                        s.span
                            .is_some_and(|s| s.start == b.span.start && s.end == b.span.end)
                    })
                    .unwrap();
                same_type(model, b.type_id().unwrap(), &slot.r#type);
                assert_eq!(
                    slot.kind,
                    match b.kind {
                        BindingKind::Parameter => SlotKind::Parameter,
                        BindingKind::Local => SlotKind::Local,
                        BindingKind::Loop => SlotKind::LoopBinding,
                    }
                );
            }
            for instruction in function.regions.iter().flat_map(|r| &r.instructions) {
                let s = instruction.span.unwrap();
                map.validate_span(Span::new(s.start, s.end)).unwrap();
                if let Operation::Call {
                    callee, arguments, ..
                } = &instruction.operation
                {
                    let call = model
                        .calls()
                        .iter()
                        .find(|c| {
                            c.owner == symbol.id()
                                && c.call_span.start == s.start
                                && c.call_span.end == s.end
                        })
                        .unwrap();
                    assert_eq!(arguments.len(), call.arguments.len());
                    for (&slot, &node) in arguments.iter().zip(&call.arguments) {
                        same_type(
                            model,
                            model.node_type(node).unwrap(),
                            &function.slots[slot].r#type,
                        );
                    }
                    match (call.target, callee) {
                        (CallTarget::Source(id), Callee::Source { function: to }) => {
                            assert_eq!(model.function(id).unwrap().name(), d.functions[*to].name)
                        }
                        (CallTarget::Contract(id), Callee::Contract { contract }) => assert_eq!(
                            checked.contract(id).unwrap().qualified_name(),
                            d.contracts[*contract].qualified_name()
                        ),
                        (CallTarget::Composition, Callee::Composition) => {}
                        other => panic!("resolved call identity changed: {other:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn triage_has_reviewed_operations_slots_and_argument_order_not_an_execution_result() {
    let checked = analyze(
        SourceFile::new(
            "examples/triage.jky",
            include_str!("../../examples/triage.jky"),
        ),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let ir = lower(&checked, BuildOptions::default()).unwrap();
    let d = ir.document();
    assert_eq!(d.constants.len(), 0);
    assert_eq!(d.functions.len(), 1);
    assert_eq!(d.contracts.len(), 4);
    let f = &d.functions[0];
    assert_eq!(f.slots.len(), 10);
    assert_eq!(f.regions.len(), 1);
    let ops = f.regions[0]
        .instructions
        .iter()
        .map(|i| &i.operation)
        .collect::<Vec<_>>();
    assert_eq!(ops.len(), 10);
    let mut calls = Vec::new();
    let mut copies = Vec::new();
    for op in &ops {
        match op {
            Operation::Call {
                callee, arguments, ..
            } => calls.push((*callee, arguments.clone())),
            Operation::Copy {
                destination,
                source,
            } => copies.push((*destination, *source)),
            _ => {}
        }
    }
    assert_eq!(copies, [(2, 1), (4, 3), (6, 5), (8, 7)]);
    assert_eq!(
        calls,
        [
            (Callee::Contract { contract: 3 }, vec![0]),
            (Callee::Contract { contract: 2 }, vec![0]),
            (Callee::Contract { contract: 1 }, vec![0]),
            (Callee::Contract { contract: 0 }, vec![2, 4, 6]),
            (Callee::Composition, vec![2, 8])
        ]
    );
    assert_eq!(ops[9], &Operation::Return { value: 9 });
}

#[test]
fn nested_call_order_shadow_initializers_and_dead_calls_are_preserved() {
    let registry = builtin_registry().unwrap();
    let c = analyze(
        SourceFile::new(
            "nested",
            include_str!("fixtures/ir/golden/nested-arguments.jky"),
        ),
        registry,
    )
    .unwrap();
    let ir = lower(&c, BuildOptions::default()).unwrap();
    let calls = ir.document().functions[3].regions[0]
        .instructions
        .iter()
        .filter_map(|i| {
            if let Operation::Call { callee, .. } = i.operation {
                Some(callee)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls,
        [
            Callee::Source { function: 0 },
            Callee::Source { function: 1 },
            Callee::Source { function: 2 },
            Callee::Contract { contract: 0 }
        ]
    );
    let c = analyze(
        SourceFile::new("shadow", include_str!("fixtures/ir/golden/shadowing.jky")),
        registry,
    )
    .unwrap();
    let ir = lower(&c, BuildOptions::default()).unwrap();
    assert!(matches!(
        ir.document().functions[0].regions[1].instructions[0].operation,
        Operation::Copy { source: 0, .. }
    ));
    let c = analyze(
        SourceFile::new("early", include_str!("fixtures/ir/golden/early-return.jky")),
        registry,
    )
    .unwrap();
    let ir = lower(&c, BuildOptions::default()).unwrap();
    assert!(matches!(
        ir.document().functions[0].regions[0].instructions[1].operation,
        Operation::Return { .. }
    ));
    assert!(matches!(
        ir.document().functions[0].regions[0]
            .instructions
            .last()
            .unwrap()
            .operation,
        Operation::Call { .. }
    ));
    assert!(!c.warnings().items().is_empty());
}

#[test]
fn seed_only_changes_identity_and_captured_registry_cannot_be_substituted() {
    let registry = builtin_registry().unwrap();
    let text = include_str!("../../examples/triage.jky");
    let c = analyze(SourceFile::new("x", text), registry).unwrap();
    let a = lower(&c, BuildOptions::default()).unwrap();
    let b = lower(&c, BuildOptions { seed: 42 }).unwrap();
    let mut da = a.document().clone();
    let mut db = b.document().clone();
    da.build_seed.clear();
    db.build_seed.clear();
    for d in [&mut da, &mut db] {
        for i in d
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.regions)
            .flat_map(|r| &mut r.instructions)
        {
            i.id.clear();
        }
    }
    assert_eq!(da, db);
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    raw["functions"][0]["read_only"] = false.into();
    let replacement = jocky_forensic::contracts::Registry::from_json(&raw.to_string()).unwrap();
    assert_eq!(
        jocky_ir::verify(a.document().clone(), &replacement)
            .unwrap_err()
            .code(),
        jocky_ir::DiagnosticCode::Contract
    );
}

#[test]
fn excessive_expansion_fails_typed_and_lf_crlf_keep_real_spans() {
    let registry = builtin_registry().unwrap();
    let source = include_str!("fixtures/ir/golden/nested-loops.jky");
    let mut previous = None;
    for text in [source.to_owned(), source.replace('\n', "\r\n")] {
        let c = analyze(SourceFile::new("same", &text), registry).unwrap();
        let ir = lower(&c, BuildOptions::default()).unwrap();
        let map = SourceMap::new(&text);
        for i in ir
            .document()
            .functions
            .iter()
            .flat_map(|f| &f.regions)
            .flat_map(|r| &r.instructions)
        {
            let s = i.span.unwrap();
            map.validate_span(Span::new(s.start, s.end)).unwrap();
        }
        if let Some(hash) = previous {
            assert_ne!(ir.document().source.sha256, hash);
        }
        previous = Some(ir.document().source.sha256.clone());
    }
    let mut text =
        String::from("module large target ubuntu fn main(target: endpoint) -> forensic_result { ");
    for n in 0..19000 {
        text.push_str(&format!("let n{n} = true\n"));
    }
    text.push_str("return forensic.system.profile(target) } run main on selected_endpoints");
    let c = analyze(SourceFile::new("bounded", &text), registry).unwrap();
    assert!(matches!(
        lower(&c, BuildOptions::default()),
        Err(LoweringFailure::Resource(_))
    ));
}

fn fixture_execution(source: &str, fixture: &serde_json::Value) -> serde_json::Value {
    let checked = analyze(
        SourceFile::new("behavior.jky", source),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let ir = lower(&checked, BuildOptions::default()).unwrap();
    let limits = jocky_ir::ExecutionLimits::default();
    let fixture = jocky_ir::decode_fixture(&serde_json::to_vec(fixture).unwrap(), &limits).unwrap();
    let report = jocky_ir::execute(jocky_ir::prepare(&ir, &fixture, &limits).unwrap());
    serde_json::from_slice(&report.to_json().unwrap()).unwrap()
}
fn ubuntu_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../ir/tests/fixtures/valid/ubuntu/fixture.json"
    ))
    .unwrap()
}

#[test]
fn every_golden_has_an_independently_expected_fixture_behavior() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ir/golden/manifest.json")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = std::fs::read_to_string(root.join(case["source"].as_str().unwrap())).unwrap();
        let checked = analyze(
            SourceFile::new("behavior.jky", &text),
            builtin_registry().unwrap(),
        )
        .unwrap();
        let ir = lower(&checked, BuildOptions::default()).unwrap();
        let platform = if ir
            .document()
            .targets
            .contains(&jocky_shared::compiler::Platform::Ubuntu)
        {
            "ubuntu"
        } else {
            "windows"
        };
        let raw: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("ir/tests/fixtures/valid/{platform}/fixture.json")))
                .unwrap(),
        )
        .unwrap();
        let report = fixture_execution(&text, &raw);
        match name {
            "cross-platform" => {
                assert_eq!(report["outcome"]["error"]["code"], "Unsupported", "{name}")
            }
            "recursion" => assert_eq!(
                report["outcome"]["error"]["code"], "EXEC_CALL_DEPTH",
                "{name}"
            ),
            "triage" => {
                let expected: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(root.join(format!(
                        "ir/tests/fixtures/valid/{platform}/expected-result.json"
                    )))
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(report["outcome"]["value"], expected, "{name}");
            }
            _ => assert_eq!(
                report["outcome"]["value"], raw["providers"]["forensic.system.profile"]["value"],
                "{name}"
            ),
        }
    }
}

#[test]
fn fresh_calls_shadowing_selected_branches_and_unreached_calls_preserve_behavior() {
    let source = r#"module flow target windows | ubuntu
fn choose(v: bool) -> bool {
    if true { let v = v return v } else { return false }
}
fn unused(t: endpoint) -> list<driver_record> { return forensic.driver.list(t) }
fn main(t: endpoint) -> forensic_result {
    if choose(false) { forensic.driver.list(t) }
    if choose(true) { return forensic.system.profile(t) }
    forensic.driver.list(t)
    return forensic.system.profile(t)
}
run main on selected_endpoints"#;
    let raw = ubuntu_fixture();
    let r = fixture_execution(source, &raw);
    assert_eq!(
        r["outcome"]["value"],
        raw["providers"]["forensic.system.profile"]["value"]
    );
    assert_eq!(r["accounting"]["peak_call_depth"], 2);
}

#[test]
fn loops_charge_n_plus_two_and_early_return_unwinds_entire_function() {
    let ordinary = r#"module loops target windows | ubuntu
fn keep(p: process_record) -> process_record { return p }
fn main(t: endpoint) -> forensic_result {
    let ps = forensic.process.list(t)
    for p in ps { keep(p) }
    return forensic.system.profile(t)
}
run main on selected_endpoints"#;
    let early = ordinary
        .replace("keep(p)", "return forensic.system.profile(t)")
        .replace(
            "    return forensic.system.profile(t)\n}",
            "    forensic.driver.list(t)\n    return forensic.system.profile(t)\n}",
        );
    let mut raw = ubuntu_fixture();
    for nonempty in [true, false] {
        if !nonempty {
            for pointer in [
                "/providers/forensic.process.list/value/values",
                "/providers/forensic.network.connections/value/values",
                "/providers/forensic.indicator.correlate/expected/processes/values",
                "/providers/forensic.indicator.correlate/expected/connections/values",
                "/providers/forensic.indicator.correlate/outcome/value/value/indicators",
            ] {
                *raw.pointer_mut(pointer).unwrap() = serde_json::json!([]);
            }
        }
        let r = fixture_execution(ordinary, &raw);
        assert_eq!(r["outcome"]["status"], "success");
        // 2 (provider/copy) + N+2 (loop) + 2N (call/return) + 2 (profile/return).
        assert_eq!(
            r["accounting"]["instructions"],
            if nonempty { 9 } else { 6 }
        );
        let r = fixture_execution(&early, &raw);
        if nonempty {
            assert_eq!(r["outcome"]["status"], "success");
            assert_eq!(r["accounting"]["instructions"], 6);
        } else {
            assert_eq!(r["outcome"]["error"]["code"], "Unsupported");
        }
    }
}

#[test]
fn arguments_evaluate_left_to_right_and_nested_failures_keep_the_provider_origin() {
    let source = r#"module order target windows | ubuntu
fn first(t: endpoint) -> forensic_result { return forensic.system.profile(t) }
fn second(t: endpoint) -> list<process_record> { return forensic.process.list(t) }
fn pair(a: forensic_result, b: list<process_record>) -> forensic_result { return a }
fn outer(t: endpoint) -> forensic_result { return pair(first(t), second(t)) }
fn main(t: endpoint) -> forensic_result {
    let value = outer(t)
    forensic.driver.list(t)
    return value
}
run main on selected_endpoints"#;
    let mut raw = ubuntu_fixture();
    raw["providers"]["forensic.system.profile"] =
        serde_json::json!({"status":"error","code":"PermissionDenied"});
    raw["providers"]["forensic.process.list"] =
        serde_json::json!({"status":"error","code":"CollectionFailed"});
    let checked = analyze(
        SourceFile::new("behavior.jky", source),
        builtin_registry().unwrap(),
    )
    .unwrap();
    let ir = lower(&checked, BuildOptions::default()).unwrap();
    let provider = &ir.document().functions[0].regions[0].instructions[0];
    let r = fixture_execution(source, &raw);
    assert_eq!(r["outcome"]["error"]["code"], "PermissionDenied");
    assert_eq!(
        r["outcome"]["error"]["origin"]["instruction_id"],
        provider.id
    );
    assert_eq!(
        r["outcome"]["error"]["origin"]["span"],
        serde_json::to_value(provider.span).unwrap()
    );
    assert_eq!(r["accounting"]["instructions"], 3);
    assert_eq!(r["accounting"]["peak_call_depth"], 3);
}
