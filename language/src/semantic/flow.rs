//! Conservative flow and lexical scopes; no literal or call evaluation.
use super::{
    Analysis, ResourceFailure,
    diagnostic::Cause,
    symbols::{BindingKind, FunctionId, NodeKind},
    types::TypeNode,
};
use crate::{
    Span,
    ast::{Block, FunctionDeclaration, Statement},
};
use jocky_shared::compiler::NamedType;
use std::collections::BTreeMap;

fn fall_through(root: &Block) -> BTreeMap<Span, bool> {
    let mut facts = BTreeMap::new();
    let mut work = vec![(root, false)];
    while let Some((block, finish)) = work.pop() {
        if !finish {
            work.push((block, true));
            for statement in block.statements.iter().rev() {
                match statement {
                    Statement::If {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        work.push((then_branch, false));
                        if let Some(b) = else_branch {
                            work.push((b, false));
                        }
                    }
                    Statement::For { body, .. } => work.push((body, false)),
                    _ => {}
                }
            }
        } else {
            let falls = block
                .statements
                .iter()
                .all(|statement| statement_falls(statement, &facts));
            facts.insert(block.span, falls);
        }
    }
    facts
}
fn statement_falls(statement: &Statement, facts: &BTreeMap<Span, bool>) -> bool {
    match statement {
        Statement::Return { .. } => false,
        Statement::If {
            then_branch,
            else_branch: Some(other),
            ..
        } => facts[&then_branch.span] || facts[&other.span],
        _ => true,
    }
}
fn schedule<'a>(
    block: &'a Block,
    scope: usize,
    mut reachable: bool,
    facts: &BTreeMap<Span, bool>,
    work: &mut Vec<(&'a Statement, usize, bool)>,
) {
    let start = work.len();
    for statement in &block.statements {
        work.push((statement, scope, reachable));
        reachable &= statement_falls(statement, facts);
    }
    work[start..].reverse();
}

impl Analysis<'_> {
    pub(super) fn body(
        &mut self,
        function: &FunctionDeclaration,
        owner: FunctionId,
    ) -> Result<(), ResourceFailure> {
        let scope = self.scope(None);
        for (index, parameter) in function.parameters.iter().enumerate() {
            let ty = self.model.functions[owner.0].signature.parameters[index];
            self.binding(scope, &parameter.name, ty, BindingKind::Parameter, owner);
        }
        let facts = fall_through(&function.body);
        let mut work = Vec::new();
        schedule(&function.body, scope, true, &facts, &mut work);
        while let Some((statement, scope, reachable)) = work.pop() {
            let node = self
                .model
                .tables
                .node(NodeKind::Statement, statement.span());
            if !reachable {
                self.diagnostics
                    .push(statement.span(), Cause::UnreachableCode);
            }
            match statement {
                Statement::Let { name, value, .. } => {
                    let (_, ty) = self.expression(value, scope, reachable, owner)?;
                    self.binding(scope, name, ty, BindingKind::Local, owner);
                }
                Statement::Call {
                    callee,
                    arguments,
                    span,
                } => {
                    let mut resolved = Vec::new();
                    for argument in arguments {
                        resolved.push(self.expression(argument, scope, reachable, owner)?);
                    }
                    let ty = self.call(
                        super::resolve::CallSite {
                            node,
                            callee,
                            scope,
                            owner,
                            span: *span,
                            reachable,
                        },
                        &resolved,
                    )?;
                    self.model.tables.annotate(node, ty);
                }
                Statement::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if let (_, Some(actual)) =
                        self.expression(condition, scope, reachable, owner)?
                    {
                        if actual != self.model.arena.named(NamedType::Bool) {
                            self.diagnostics.push(
                                condition.span(),
                                Cause::ConditionNotBool(self.model.arena.display(actual)),
                            );
                        }
                    }
                    if let Some(other) = else_branch {
                        let child = self.scope(Some(scope));
                        schedule(other, child, reachable, &facts, &mut work);
                    }
                    let child = self.scope(Some(scope));
                    schedule(then_branch, child, reachable, &facts, &mut work);
                }
                Statement::For {
                    binding,
                    collection,
                    body,
                    ..
                } => {
                    let (_, collection_type) =
                        self.expression(collection, scope, reachable, owner)?;
                    let element = match collection_type.map(|ty| (ty, self.model.arena.node(ty))) {
                        Some((_, TypeNode::List(element))) => Some(element),
                        Some((actual, _)) => {
                            self.diagnostics.push(
                                collection.span(),
                                Cause::NotACollection(self.model.arena.display(actual)),
                            );
                            None
                        }
                        None => None,
                    };
                    let child = self.scope(Some(scope));
                    self.binding(child, binding, element, BindingKind::Loop, owner);
                    schedule(body, child, reachable, &facts, &mut work);
                }
                Statement::Return { value, .. } => {
                    let (_, actual) = self.expression(value, scope, reachable, owner)?;
                    if let (Some(expected), Some(actual)) =
                        (self.model.functions[owner.0].signature.result, actual)
                    {
                        if expected != actual {
                            self.diagnostics.push(
                                value.span(),
                                Cause::ReturnType {
                                    expected: self.model.arena.display(expected),
                                    actual: self.model.arena.display(actual),
                                },
                            );
                        }
                    }
                }
            }
        }
        if facts[&function.body.span] {
            if let Some(expected) = self.model.functions[owner.0].signature.result {
                self.diagnostics.push(
                    Span::new(function.body.span.end - 1, function.body.span.end),
                    Cause::MissingReturn(self.model.arena.display(expected)),
                );
            }
        }
        Ok(())
    }
    pub(super) fn unused_bindings(&mut self) {
        for binding in &self.model.bindings {
            if binding.kind != BindingKind::Parameter && !binding.used {
                self.diagnostics
                    .push(binding.span, Cause::UnusedBinding(binding.name.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn loops_may_be_empty_and_branch_bindings_never_escape() {
        for body in [
            "for item in items { return item }",
            "if true { let hidden = 1 } return hidden",
            "for hidden in items { return hidden } return hidden",
        ] {
            let text = format!(
                "module demo target ubuntu fn helper(items: list<int>) -> int {{ {body} }} fn main(target: endpoint) -> forensic_result {{ return forensic.system.profile(target) }} run main on selected_endpoints"
            );
            assert!(matches!(
                analyze(
                    SourceFile::new("x", &text),
                    jocky_forensic::contracts::builtin_registry().unwrap()
                ),
                Err(CheckFailure::Semantic(_))
            ));
        }
    }
}
