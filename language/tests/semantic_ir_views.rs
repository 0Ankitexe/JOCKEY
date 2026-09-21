use jocky_forensic::contracts::{Registry, builtin_registry};
use jocky_language::{
    SourceFile, Span,
    semantic::{NodeKind, TypeNode, analyze},
};

#[test]
fn source_spans_bindings_and_structured_types_are_immutable_and_indexed() {
    let text = include_str!("../../examples/triage.jky");
    let checked = analyze(
        SourceFile::new("logical/triage", text),
        builtin_registry().unwrap(),
    )
    .unwrap();
    assert_eq!(checked.source_text(), text);
    assert_eq!(checked.source_label(), "logical/triage");
    let model = checked.model();
    let entry = model.function(checked.entry_function()).unwrap();
    assert_eq!(entry.name(), "investigate");
    assert_eq!(entry.span(), checked.syntax().program.functions[0].span);
    assert_eq!(entry.parameters().len(), 1);
    let param = model.binding(entry.parameters()[0]).unwrap();
    assert_eq!(param.owner(), entry.id());
    assert!(matches!(
        model.type_node(param.type_id().unwrap()),
        Some(TypeNode::Named(_))
    ));
    assert_eq!(model.declaration_binding(param.span).unwrap().id, param.id);
    assert!(model.declaration_binding(Span::new(0, 0)).is_none());
    assert!(
        model
            .node_at(NodeKind::Expression, Span::new(0, 0))
            .is_none()
    );
    for node in model.nodes() {
        assert_eq!(model.node_at(node.kind, node.span), Some(node.id));
    }
    for call in model.calls() {
        assert_eq!(model.call(call.node), Some(call));
        if let jocky_language::semantic::CallTarget::Contract(id) = call.target {
            assert!(checked.contract(id).is_some());
        }
    }
    for reference in model.references() {
        assert_eq!(model.reference(reference.node), Some(reference.binding));
    }
}

#[test]
fn complete_registry_snapshot_cannot_be_rebound_after_analysis() {
    let text = include_str!("../../examples/triage.jky");
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../forensic-lib/contracts/catalogue.v1.json"
    ))
    .unwrap();
    let original = Registry::from_json(&raw.to_string()).unwrap();
    let checked = analyze(SourceFile::new("x", text), &original).unwrap();
    raw["functions"].as_array_mut().unwrap().reverse();
    let permuted = Registry::from_json(&raw.to_string()).unwrap();
    let other = analyze(SourceFile::new("x", text), &permuted).unwrap();
    assert_eq!(
        jocky_ir::identity::registry_fingerprint(checked.registry_snapshot()).unwrap(),
        jocky_ir::identity::registry_fingerprint(other.registry_snapshot()).unwrap()
    );
    for c in raw["functions"].as_array_mut().unwrap() {
        if c["module"] == "forensic.system" {
            c["read_only"] = false.into();
        }
    }
    let replaced = Registry::from_json(&raw.to_string()).unwrap();
    assert_ne!(
        jocky_ir::identity::registry_fingerprint(checked.registry_snapshot()).unwrap(),
        jocky_ir::identity::registry_fingerprint(&replaced).unwrap()
    );
    assert!(
        checked
            .registry_snapshot()
            .lookup("forensic.system.profile")
            .unwrap()
            .read_only
    );
}
