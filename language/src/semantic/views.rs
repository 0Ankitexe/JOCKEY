//! Bounded indexes built once, only after successful analysis. No display-string parsing.
use super::{symbols::*, types};
use crate::Span;
use jocky_shared::compiler::NamedType;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeNode {
    Named(NamedType),
    List(TypeId),
    Option(TypeId),
    Result { ok: TypeId, error: TypeId },
}

#[derive(Default, Debug)]
pub(super) struct Indexes {
    nodes: BTreeMap<(NodeKind, Span), NodeId>,
    bindings: BTreeMap<Span, BindingId>,
    types: Vec<Option<TypeId>>,
    references: Vec<Option<BindingId>>,
    calls: Vec<Option<usize>>,
}
impl Indexes {
    pub fn new(model: &SemanticModel) -> Self {
        // The semantic census bounds all source occurrences and bindings before
        // side tables exist. These indexes grow at most once per bounded entry.
        let mut out = Self {
            nodes: model
                .nodes()
                .iter()
                .map(|n| ((n.kind, n.span), n.id))
                .collect(),
            bindings: model.bindings().iter().map(|b| (b.span, b.id)).collect(),
            types: vec![None; model.nodes().len()],
            references: vec![None; model.nodes().len()],
            calls: vec![None; model.nodes().len()],
        };
        for a in model.annotations() {
            out.types[a.node.0] = Some(a.type_id);
        }
        for r in model.references() {
            out.references[r.node.0] = Some(r.binding);
        }
        for (i, c) in model.calls().iter().enumerate() {
            out.calls[c.node.0] = Some(i);
        }
        out
    }
}
impl SemanticModel {
    pub fn functions(&self) -> &[FunctionSymbol] {
        &self.functions
    }
    pub fn function(&self, id: FunctionId) -> Option<&FunctionSymbol> {
        self.functions.get(id.0)
    }
    pub fn binding(&self, id: BindingId) -> Option<&BindingSymbol> {
        self.bindings.get(id.0)
    }
    pub fn declaration_binding(&self, span: Span) -> Option<&BindingSymbol> {
        self.binding(*self.indexes.bindings.get(&span)?)
    }
    pub fn node_at(&self, kind: NodeKind, span: Span) -> Option<NodeId> {
        self.indexes.nodes.get(&(kind, span)).copied()
    }
    pub fn node_type(&self, id: NodeId) -> Option<TypeId> {
        self.indexes.types.get(id.0).copied().flatten()
    }
    pub fn reference(&self, id: NodeId) -> Option<BindingId> {
        self.indexes.references.get(id.0).copied().flatten()
    }
    pub fn call(&self, id: NodeId) -> Option<&ResolvedCall> {
        self.calls()
            .get(self.indexes.calls.get(id.0).copied().flatten()?)
    }
    pub fn type_node(&self, id: TypeId) -> Option<TypeNode> {
        if !self.arena.contains(id) {
            return None;
        }
        Some(match self.arena.node(id) {
            types::TypeNode::Named(t) => TypeNode::Named(t),
            types::TypeNode::List(t) => TypeNode::List(t),
            types::TypeNode::Option(t) => TypeNode::Option(t),
            types::TypeNode::Result(ok, error) => TypeNode::Result { ok, error },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_opaque_indexes_are_safe_misses() {
        let checked = crate::semantic::analyze(
            crate::SourceFile::new("x", include_str!("../../../examples/triage.jky")),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        let m = checked.model();
        assert!(m.function(FunctionId(usize::MAX)).is_none());
        assert!(m.binding(BindingId(usize::MAX)).is_none());
        assert!(m.type_node(TypeId(usize::MAX)).is_none());
        assert!(m.node_type(NodeId(usize::MAX)).is_none());
        assert!(m.reference(NodeId(usize::MAX)).is_none());
        assert!(m.call(NodeId(usize::MAX)).is_none());
        assert!(checked.contract(ContractId(usize::MAX)).is_none());
    }
}
