use super::structure::{FunctionIndex, visible};
use crate::{
    diagnostic::{Diagnostic, DiagnosticCode as Code, Diagnostics, Failure},
    limits::Budget,
    model::*,
    value,
};
use jocky_shared::compiler::{NamedType, Type};

pub(super) fn check(
    document: &IrDocument,
    indexes: &[FunctionIndex],
    budget: &mut Budget,
) -> Result<(), Failure> {
    let mut diagnostics = Diagnostics::default();
    let mut constants = Vec::new();
    budget.entries(document.constants.len())?;
    for (c, value) in document.constants.iter().enumerate() {
        if let Err(error) = value::consistency(value, budget) {
            if error.code() == Code::Resource {
                return Err(error);
            }
            diagnostics.push(Diagnostic::at(Code::Type, format!("/constants/{c}")));
        }
        constants.push(value::value_type(value));
    }
    let entry = &document.functions[document.entry];
    if entry.parameters.len() != 1
        || entry.result != Type::named(NamedType::ForensicResult)
        || entry
            .parameters
            .first()
            .is_none_or(|p| entry.slots[*p].r#type != Type::named(NamedType::Endpoint))
    {
        diagnostics.push(Diagnostic::at(Code::Return, "/entry"));
    }
    for (f, function) in document.functions.iter().enumerate() {
        for (r, region) in function.regions.iter().enumerate() {
            for (i, ins) in region.instructions.iter().enumerate() {
                budget.visit()?;
                for &read in ins.operation.reads() {
                    if !visible(&indexes[f], function.slots[read].region, r, budget)? {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            ins,
                            "",
                        ));
                    }
                }
                let slot = |id: usize| &function.slots[id].r#type;
                let (code, valid) = match &ins.operation {
                    Operation::Const {
                        destination,
                        constant,
                    } => (
                        Code::Type,
                        value::types_equal(slot(*destination), &constants[*constant], budget)?,
                    ),
                    Operation::Copy {
                        destination,
                        source,
                    } => (
                        Code::Type,
                        value::types_equal(slot(*destination), slot(*source), budget)?,
                    ),
                    Operation::Return { value } => (
                        Code::Return,
                        value::types_equal(slot(*value), &function.result, budget)?,
                    ),
                    Operation::If { condition, .. } => (
                        Code::Type,
                        *slot(*condition) == Type::named(NamedType::Bool),
                    ),
                    Operation::ForEach {
                        collection, item, ..
                    } => {
                        let valid = if let Type::List { element } = slot(*collection) {
                            value::types_equal(element, slot(*item), budget)?
                        } else {
                            false
                        };
                        (Code::Type, valid)
                    }
                    Operation::Call {
                        destination,
                        callee,
                        arguments,
                        ..
                    } => {
                        let (parameters, result): (Vec<&Type>, &Type) = match callee {
                            Callee::Source { function: target } => {
                                let called = &document.functions[*target];
                                budget.references(called.parameters.len())?;
                                (
                                    called
                                        .parameters
                                        .iter()
                                        .map(|p| &called.slots[*p].r#type)
                                        .collect(),
                                    &called.result,
                                )
                            }
                            Callee::Contract { contract } => {
                                let c = &document.contracts[*contract];
                                budget.references(c.parameters.len())?;
                                (c.parameters.iter().map(|p| &p.r#type).collect(), &c.result)
                            }
                            Callee::Composition => {
                                budget.references(2)?;
                                let expected = &Type::Named {
                                    name: NamedType::ForensicResult,
                                };
                                (vec![expected, expected], expected)
                            }
                        };
                        let mut valid = arguments.len() == parameters.len()
                            && value::types_equal(slot(*destination), result, budget)?;
                        for (&argument, expected) in arguments.iter().zip(parameters) {
                            valid &= value::types_equal(slot(argument), expected, budget)?;
                        }
                        (Code::Type, valid)
                    }
                };
                if !valid {
                    diagnostics.push(Diagnostic::instruction(code, f, r, i, ins, ""));
                }
            }
        }
    }
    diagnostics.finish()
}
