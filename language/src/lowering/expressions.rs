use super::{LoweringFailure, Result, context::Context, need};
use crate::{
    Span,
    ast::{DurationUnit, Expression},
    semantic::{CallTarget, NodeKind},
};
use jocky_ir::{
    model::{Callee, ErrorPolicy, Operation, SlotKind},
    value::{DurationUnit as Unit, Value},
};

impl Context<'_> {
    pub fn expression(&mut self, f: usize, r: usize, root: &Expression) -> Result<usize> {
        enum Work<'a> {
            Visit(&'a Expression),
            Call(Span, usize),
        }
        let mut work = vec![Work::Visit(root)];
        let mut values = Vec::new();
        while let Some(step) = work.pop() {
            self.budget.visit()?;
            match step {
                Work::Visit(e) => {
                    if let Expression::Call {
                        arguments, span, ..
                    } = e
                    {
                        self.budget.refs(arguments.len())?;
                        work.push(Work::Call(*span, arguments.len()));
                        work.extend(arguments.iter().rev().map(Work::Visit));
                        continue;
                    }
                    if let Expression::Identifier { identifier, .. } = e {
                        let n = need(
                            self.checked
                                .model()
                                .node_at(NodeKind::Identifier, identifier.span),
                            Some(identifier.span),
                        )?;
                        let b = need(self.checked.model().reference(n), Some(identifier.span))?;
                        values.push(*need(self.bindings.get(&b), Some(identifier.span))?);
                        continue;
                    }
                    let value = match e {
                        Expression::String { value, .. } => {
                            self.budget.bytes(value.len())?;
                            Value::String {
                                value: value.clone(),
                            }
                        }
                        Expression::Integer { lexeme, .. } => {
                            self.budget.bytes(lexeme.len())?;
                            Value::Int {
                                value: lexeme.clone(),
                            }
                        }
                        Expression::Boolean { value, .. } => Value::Bool { value: *value },
                        Expression::Duration {
                            magnitude, unit, ..
                        } => {
                            self.budget.bytes(magnitude.len())?;
                            Value::Duration {
                                magnitude: magnitude.clone(),
                                unit: match unit {
                                    DurationUnit::Milliseconds => Unit::Milliseconds,
                                    DurationUnit::Seconds => Unit::Seconds,
                                    DurationUnit::Minutes => Unit::Minutes,
                                    DurationUnit::Hours => Unit::Hours,
                                    DurationUnit::Days => Unit::Days,
                                },
                            }
                        }
                        _ => return Err(LoweringFailure::Invariant(Some(e.span()))),
                    };
                    let constant = self.constant(value)?;
                    let ty = self.node_type(NodeKind::Expression, e.span())?;
                    let destination = self.slot(f, r, SlotKind::Temporary, None, e.span(), ty)?;
                    self.emit(
                        f,
                        r,
                        e.span(),
                        Operation::Const {
                            destination,
                            constant,
                        },
                    )?;
                    values.push(destination);
                }
                Work::Call(s, count) => {
                    let start = values
                        .len()
                        .checked_sub(count)
                        .ok_or(LoweringFailure::Invariant(Some(s)))?;
                    let arguments = values.split_off(start);
                    let result = self.call(f, r, NodeKind::Expression, s, arguments)?;
                    values.push(result);
                }
            }
        }
        need(values.pop(), Some(root.span()))
    }
    pub fn call(
        &mut self,
        f: usize,
        r: usize,
        kind: NodeKind,
        s: Span,
        arguments: Vec<usize>,
    ) -> Result<usize> {
        let node = need(self.checked.model().node_at(kind, s), Some(s))?;
        let call = need(self.checked.model().call(node), Some(s))?;
        let callee = match call.target {
            CallTarget::Source(id) => Callee::Source {
                function: *need(self.functions.get(&id), Some(s))?,
            },
            CallTarget::Contract(id) => Callee::Contract {
                contract: *need(self.contracts.get(&id), Some(s))?,
            },
            CallTarget::Composition => Callee::Composition,
        };
        let ty = need(self.checked.model().node_type(node), Some(s))?;
        let destination = self.slot(f, r, SlotKind::Temporary, None, s, ty)?;
        self.emit(
            f,
            r,
            s,
            Operation::Call {
                destination,
                callee,
                arguments,
                on_error: ErrorPolicy::Propagate,
            },
        )?;
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_and_reference_must_have_real_semantic_annotations() {
        let c = crate::semantic::analyze(
            crate::SourceFile::new("x", include_str!("../../../examples/triage.jky")),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        let mut context = Context::new(&c, super::super::BuildOptions::default()).unwrap();
        let fake = Expression::Integer {
            lexeme: "1".into(),
            span: Span::new(0, 1),
        };
        assert!(matches!(
            context.expression(0, 0, &fake),
            Err(LoweringFailure::Invariant(_))
        ));
        let fake = Expression::Identifier {
            identifier: crate::ast::Identifier {
                text: "x".into(),
                span: Span::new(0, 1),
            },
            span: Span::new(0, 1),
        };
        assert!(matches!(
            context.expression(0, 0, &fake),
            Err(LoweringFailure::Invariant(_))
        ));
    }
}
