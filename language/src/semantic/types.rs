//! Interned exact types. Children are IDs, so cache hits never clone trees.
use super::{
    ResourceFailure, ResourceKind,
    diagnostic::{Cause, SemanticDiagnostics},
    limits::{charge, require},
    symbols::{NodeKind, Tables, TypeId},
};
use crate::{Span, ast::TypeReference};
use jocky_shared::compiler::{NamedType, Type};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum TypeNode {
    Named(NamedType),
    List(TypeId),
    Option(TypeId),
    Result(TypeId, TypeId),
}
#[derive(Debug)]
pub(super) struct TypeArena {
    nodes: Vec<TypeNode>,
    index: BTreeMap<TypeNode, TypeId>,
    work: usize,
    storage: usize,
}
impl TypeArena {
    pub fn new() -> Self {
        let nodes = NamedType::ALL
            .into_iter()
            .map(TypeNode::Named)
            .collect::<Vec<_>>();
        let index = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (*node, TypeId(i)))
            .collect();
        Self {
            work: nodes.len(),
            storage: nodes.len(),
            nodes,
            index,
        }
    }
    pub fn contains(&self, id: TypeId) -> bool {
        id.0 < self.nodes.len()
    }
    pub fn named(&self, name: NamedType) -> TypeId {
        self.index[&TypeNode::Named(name)]
    }
    pub fn node(&self, id: TypeId) -> TypeNode {
        self.nodes[id.0]
    }
    fn intern(&mut self, node: TypeNode, span: Option<Span>) -> Result<TypeId, ResourceFailure> {
        if let Some(id) = self.index.get(&node) {
            return Ok(*id);
        }
        charge(&mut self.storage, ResourceKind::TypeStorage, 1, span)?;
        let id = TypeId(self.nodes.len());
        self.nodes.push(node);
        self.index.insert(node, id);
        Ok(id)
    }
    fn visit(&mut self, depth: usize, span: Option<Span>) -> Result<(), ResourceFailure> {
        require(ResourceKind::TypeDepth, depth, span)?;
        charge(&mut self.work, ResourceKind::TypeWork, 1, span)
    }
    pub fn lower_source(
        &mut self,
        root: &TypeReference,
        diagnostics: &mut SemanticDiagnostics,
        tables: &mut Tables,
    ) -> Result<Option<TypeId>, ResourceFailure> {
        let mut stack = vec![(root, 1, false, None)];
        let mut values = Vec::new();
        while let Some((ty, depth, finish, id)) = stack.pop() {
            if !finish {
                self.visit(depth, Some(ty.span()))?;
                let id = tables.node(NodeKind::TypeReference, ty.span());
                stack.push((ty, depth, true, Some(id)));
                match ty {
                    TypeReference::List { element, .. } | TypeReference::Option { element, .. } => {
                        stack.push((element, depth + 1, false, None))
                    }
                    TypeReference::Result { ok, error, .. } => {
                        stack.push((error, depth + 1, false, None));
                        stack.push((ok, depth + 1, false, None));
                    }
                    _ => {}
                }
                continue;
            }
            let node = match ty {
                TypeReference::Named { name, .. } => match NamedType::from_name(&name.text) {
                    Some(name) => Some(TypeNode::Named(name)),
                    None => {
                        diagnostics.push(name.span, Cause::UnknownType(name.text.clone()));
                        None
                    }
                },
                TypeReference::List { .. } => values.pop().flatten().map(TypeNode::List),
                TypeReference::Option { .. } => values.pop().flatten().map(TypeNode::Option),
                TypeReference::Result { .. } => {
                    let error = values.pop().flatten();
                    let ok = values.pop().flatten();
                    ok.zip(error).map(|(ok, error)| TypeNode::Result(ok, error))
                }
            };
            let lowered = node
                .map(|node| self.intern(node, Some(ty.span())))
                .transpose()?;
            if let Some(id) = id {
                tables.annotate(id, lowered);
            }
            values.push(lowered);
        }
        Ok(values.pop().flatten())
    }
    pub fn lower_contract(&mut self, root: &Type) -> Result<TypeId, ResourceFailure> {
        let mut stack = vec![(root, 1, false)];
        let mut values = Vec::new();
        while let Some((ty, depth, finish)) = stack.pop() {
            if !finish {
                self.visit(depth, None)?;
                stack.push((ty, depth, true));
                match ty {
                    Type::List { element } | Type::Option { element } => {
                        stack.push((element, depth + 1, false))
                    }
                    Type::Result { ok, error } => {
                        stack.push((error, depth + 1, false));
                        stack.push((ok, depth + 1, false));
                    }
                    _ => {}
                }
                continue;
            }
            let node = match ty {
                Type::Named { name } => TypeNode::Named(*name),
                Type::List { .. } => TypeNode::List(values.pop().expect("postorder child")),
                Type::Option { .. } => TypeNode::Option(values.pop().expect("postorder child")),
                Type::Result { .. } => {
                    let error = values.pop().expect("postorder error");
                    let ok = values.pop().expect("postorder ok");
                    TypeNode::Result(ok, error)
                }
            };
            values.push(self.intern(node, None)?);
        }
        Ok(values.pop().expect("contract root"))
    }
    pub fn display(&self, id: TypeId) -> String {
        enum Part {
            Type(TypeId),
            Text(&'static str),
        }
        let mut stack = vec![Part::Type(id)];
        let mut text = String::new();
        while let Some(part) = stack.pop() {
            match part {
                Part::Text(t) => text.push_str(t),
                Part::Type(id) => match self.node(id) {
                    TypeNode::Named(name) => text.push_str(name.as_str()),
                    TypeNode::List(t) => {
                        stack.extend([Part::Text(">"), Part::Type(t), Part::Text("list<")])
                    }
                    TypeNode::Option(t) => {
                        stack.extend([Part::Text(">"), Part::Type(t), Part::Text("option<")])
                    }
                    TypeNode::Result(ok, error) => stack.extend([
                        Part::Text(">"),
                        Part::Type(error),
                        Part::Text(", "),
                        Part::Type(ok),
                        Part::Text("result<"),
                    ]),
                },
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interning_preserves_exact_identity_counts_cache_hits_and_poison_stays_private() {
        let mut arena = TypeArena::new();
        let value = Type::List {
            element: Box::new(Type::named(NamedType::Int)),
        };
        let first = arena.lower_contract(&value).unwrap();
        let storage = arena.storage;
        assert_eq!(first, arena.lower_contract(&value).unwrap());
        assert_eq!(arena.storage, storage);
        assert_eq!(arena.display(first), "list<int>");
        assert_ne!(first, arena.named(NamedType::Bytes));
        arena.work = ResourceKind::TypeWork.maximum() - 2;
        assert!(arena.lower_contract(&value).is_ok());
        assert_eq!(
            arena.lower_contract(&value).unwrap_err().kind,
            ResourceKind::TypeWork
        );
        let source = crate::parse(crate::SourceFile::new(
            "x",
            "module m target ubuntu fn f(x: list<mystery>) -> int { return 1 }",
        ))
        .unwrap();
        let mut diagnostics = SemanticDiagnostics::default();
        let mut tables = Tables::default();
        assert_eq!(
            TypeArena::new()
                .lower_source(
                    &source.functions[0].parameters[0].type_ref,
                    &mut diagnostics,
                    &mut tables
                )
                .unwrap(),
            None
        );
        assert!(diagnostics.has_errors());
        assert!(tables.types.is_empty());
    }

    #[test]
    fn actual_storage_interning_has_an_independent_boundary() {
        let mut arena = TypeArena::new();
        arena.storage = ResourceKind::TypeStorage.maximum() - 1;
        let first = arena
            .intern(TypeNode::List(arena.named(NamedType::Int)), None)
            .unwrap();
        assert_eq!(arena.storage, ResourceKind::TypeStorage.maximum());
        assert_eq!(
            arena
                .intern(TypeNode::Option(first), None)
                .unwrap_err()
                .kind,
            ResourceKind::TypeStorage
        );
        assert_eq!(
            arena
                .intern(TypeNode::List(arena.named(NamedType::Int)), None)
                .unwrap(),
            first
        );
    }
}
