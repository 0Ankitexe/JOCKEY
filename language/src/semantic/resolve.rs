use super::{
    Analysis, ResourceFailure,
    diagnostic::{Cause, EntryFailure, SemanticDiagnostics},
    symbols::*,
    types::TypeArena,
};
use crate::{
    Span,
    ast::{Expression, Program, QualifiedName},
};
use jocky_shared::compiler::NamedType;

type TypedNode = (NodeId, Option<TypeId>);
pub(super) struct CallSite<'a> {
    pub node: NodeId,
    pub callee: &'a QualifiedName,
    pub scope: usize,
    pub owner: FunctionId,
    pub span: Span,
    pub reachable: bool,
}
impl Analysis<'_> {
    pub(super) fn expression(
        &mut self,
        root: &Expression,
        scope: usize,
        reachable: bool,
        owner: FunctionId,
    ) -> Result<TypedNode, ResourceFailure> {
        enum Work<'a> {
            Visit(&'a Expression),
            Call(NodeId, &'a QualifiedName, usize, Span),
        }
        let mut work = vec![Work::Visit(root)];
        let mut values: Vec<TypedNode> = Vec::new();
        while let Some(step) = work.pop() {
            match step {
                Work::Visit(expression) => {
                    let id = self
                        .model
                        .tables
                        .node(NodeKind::Expression, expression.span());
                    let ty = match expression {
                        Expression::Identifier { identifier, .. } => {
                            let reference = self
                                .model
                                .tables
                                .node(NodeKind::Identifier, identifier.span);
                            match self.lookup_binding(scope, &identifier.text) {
                                Some(binding) => {
                                    self.model.tables.references.push(ResolvedReference {
                                        node: reference,
                                        binding,
                                    });
                                    let symbol = &mut self.model.bindings[binding.0];
                                    symbol.used |= reachable;
                                    symbol.ty
                                }
                                None => {
                                    self.diagnostics.push(
                                        identifier.span,
                                        Cause::UnknownName(identifier.text.clone()),
                                    );
                                    None
                                }
                            }
                        }
                        Expression::String { .. } => {
                            Some(self.model.arena.named(NamedType::String))
                        }
                        Expression::Integer { .. } => Some(self.model.arena.named(NamedType::Int)),
                        Expression::Boolean { .. } => Some(self.model.arena.named(NamedType::Bool)),
                        Expression::Duration { .. } => {
                            Some(self.model.arena.named(NamedType::Duration))
                        }
                        Expression::Call {
                            callee,
                            arguments,
                            span,
                        } => {
                            work.push(Work::Call(id, callee, arguments.len(), *span));
                            work.extend(arguments.iter().rev().map(Work::Visit));
                            continue;
                        }
                    };
                    self.model.tables.annotate(id, ty);
                    values.push((id, ty));
                }
                Work::Call(id, callee, count, span) => {
                    let arguments = values.split_off(values.len() - count);
                    let ty = self.call(
                        CallSite {
                            node: id,
                            callee,
                            scope,
                            owner,
                            span,
                            reachable,
                        },
                        &arguments,
                    )?;
                    self.model.tables.annotate(id, ty);
                    values.push((id, ty));
                }
            }
        }
        Ok(values.pop().expect("one postorder expression result"))
    }
    pub(super) fn call(
        &mut self,
        site: CallSite<'_>,
        arguments: &[TypedNode],
    ) -> Result<Option<TypeId>, ResourceFailure> {
        let CallSite {
            node,
            callee,
            scope,
            owner,
            span,
            reachable,
        } = site;
        let reference = self.model.tables.node(NodeKind::QualifiedName, callee.span);
        let name = callee
            .segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let target = if callee.segments.len() == 1 {
            if let Some(binding) = self.lookup_binding(scope, &name) {
                self.model.bindings[binding.0].used |= reachable;
                self.model.tables.references.push(ResolvedReference {
                    node: reference,
                    binding,
                });
                self.diagnostics.push(callee.span, Cause::NotCallable(name));
                return Ok(None);
            }
            self.function_names
                .get(&name)
                .copied()
                .map(CallTarget::Source)
                .or_else(|| (name == "forensic_result").then_some(CallTarget::Composition))
        } else if callee.segments.len() == 2
            && callee.segments[0].text.as_str() == self.module.as_ref()
        {
            self.function_names
                .get(&callee.segments[1].text)
                .copied()
                .map(CallTarget::Source)
        } else {
            self.contract_names
                .get(&name)
                .copied()
                .map(CallTarget::Contract)
        };
        let Some(target) = target else {
            self.diagnostics.push(callee.span, Cause::UnknownName(name));
            return Ok(None);
        };
        match target {
            CallTarget::Source(called) => self.graph.source(owner, called, callee.span)?,
            CallTarget::Contract(id) => {
                super::metadata::check_restrictions(
                    self.contracts[id.0],
                    &self.selected_targets,
                    self.lab,
                    callee.span,
                    &mut self.diagnostics,
                );
                self.graph.external(owner, id.0);
            }
            CallTarget::Composition => self.graph.external(owner, self.contracts.len()),
        }
        let signature = match target {
            CallTarget::Source(id) => &self.model.functions[id.0].signature,
            CallTarget::Contract(id) => &self.contract_signatures[id.0],
            CallTarget::Composition => &self.composition,
        };
        let result = check_arguments(
            signature,
            arguments,
            callee.span,
            &self.model.tables,
            &self.model.arena,
            &mut self.diagnostics,
        );
        self.model.tables.calls.push(ResolvedCall {
            node,
            owner,
            callee_span: callee.span,
            call_span: span,
            arguments: arguments.iter().map(|(id, _)| *id).collect(),
            target,
        });
        Ok(result)
    }
    pub(super) fn entry(&mut self, program: &Program, source_len: usize) -> Option<FunctionId> {
        let Some(run) = &program.run else {
            self.diagnostics
                .push(Span::new(source_len, source_len), Cause::MissingRun);
            return None;
        };
        let Some(id) = self.function_names.get(&run.function.text).copied() else {
            self.diagnostics.push(
                run.function.span,
                Cause::InvalidEntryPoint(EntryFailure::UnknownFunction),
            );
            return None;
        };
        let signature = &self.model.functions[id.0].signature;
        if signature.parameters != [Some(self.model.arena.named(NamedType::Endpoint))]
            || signature.result != Some(self.model.arena.named(NamedType::ForensicResult))
        {
            self.diagnostics.push(
                run.function.span,
                Cause::InvalidEntryPoint(EntryFailure::WrongSignature),
            );
            None
        } else {
            Some(id)
        }
    }
}

fn check_arguments(
    signature: &Signature,
    arguments: &[TypedNode],
    span: Span,
    tables: &Tables,
    arena: &TypeArena,
    diagnostics: &mut SemanticDiagnostics,
) -> Option<TypeId> {
    if !signature.is_valid() {
        return None;
    }
    let mut valid = signature.parameters.len() == arguments.len();
    if !valid {
        diagnostics.push(
            span,
            Cause::ArgumentCount {
                expected: signature.parameters.len(),
                actual: arguments.len(),
            },
        );
    }
    for (expected, (id, actual)) in signature.parameters.iter().zip(arguments) {
        match (expected, actual) {
            (Some(expected), Some(actual)) if expected != actual => {
                diagnostics.push(
                    tables.nodes[id.0].span,
                    Cause::ArgumentType {
                        expected: arena.display(*expected),
                        actual: arena.display(*actual),
                    },
                );
                valid = false;
            }
            (_, None) => valid = false,
            _ => {}
        }
    }
    if valid { signature.result } else { None }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use jocky_forensic::contracts::builtin_registry;
    #[test]
    fn locals_shadow_only_unqualified_calls_and_functions_are_not_values() {
        let text = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { let forensic = 1 let helper = 2 helper(target) let missing_value = main return forensic.system.profile(target) } run main on selected_endpoints";
        let CheckFailure::Semantic(errors) =
            analyze(SourceFile::new("x", text), builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert!(
            errors
                .items()
                .iter()
                .any(|d| d.code() == DiagnosticCode::NotCallable)
        );
        assert!(
            errors
                .items()
                .iter()
                .any(|d| d.code() == DiagnosticCode::UnknownName)
        );
        assert!(
            !errors
                .items()
                .iter()
                .any(|d| d.message().contains("unknown name: forensic"))
        );
    }
    #[test]
    fn unknown_run_carries_its_cause_without_a_dependent_unknown_name() {
        let text = "module demo target ubuntu fn helper(x: int) -> int { return x } run absent on selected_endpoints";
        let CheckFailure::Semantic(errors) =
            analyze(SourceFile::new("x", text), builtin_registry().unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(errors.items().len(), 1);
        assert_eq!(
            errors.items()[0].cause,
            Cause::InvalidEntryPoint(EntryFailure::UnknownFunction)
        );
    }
}
