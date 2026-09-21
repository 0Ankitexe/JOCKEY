use super::structure::{Definition, FunctionIndex};
use crate::{
    diagnostic::{Diagnostic, DiagnosticCode as Code, Diagnostics, Failure},
    limits::Budget,
    model::*,
};

fn available(
    function: &Function,
    index: &FunctionIndex,
    slot: usize,
    mut region: usize,
    mut position: usize,
    budget: &mut Budget,
) -> Result<bool, Failure> {
    let owner = function.slots[slot].region;
    while region != owner {
        budget.visit()?;
        let Some(parent) = index.parents[region] else {
            return Ok(false);
        };
        region = parent.region;
        position = parent.instruction;
    }
    Ok(match index.definitions[slot] {
        Definition::Parameter | Definition::Loop => true,
        Definition::Instruction {
            position: definition,
        } => definition < position,
    })
}

pub(super) fn check(
    document: &IrDocument,
    indexes: &[FunctionIndex],
    budget: &mut Budget,
) -> Result<(), Failure> {
    let mut diagnostics = Diagnostics::default();
    for (f, function) in document.functions.iter().enumerate() {
        let index = &indexes[f];
        budget.bytes(function.regions.len() * 2)?;
        let mut falls = vec![true; function.regions.len()];
        for &r in index.order.iter().rev() {
            for ins in &function.regions[r].instructions {
                budget.visit()?;
                if !falls[r] {
                    break;
                }
                falls[r] = match &ins.operation {
                    Operation::Return { .. } => false,
                    Operation::If {
                        then_region,
                        else_region: Some(other),
                        ..
                    } => falls[*then_region] || falls[*other],
                    _ => true,
                };
            }
        }
        let mut reachable = vec![false; function.regions.len()];
        reachable[0] = true;
        for &r in &index.order {
            let mut live = reachable[r];
            for (i, ins) in function.regions[r].instructions.iter().enumerate() {
                budget.visit()?;
                if live {
                    for &read in ins.operation.reads() {
                        if !available(function, index, read, r, i, budget)? {
                            diagnostics.push(Diagnostic::instruction(
                                Code::Initialization,
                                f,
                                r,
                                i,
                                ins,
                                "",
                            ));
                        }
                    }
                }
                for child in ins.operation.children().into_iter().flatten() {
                    reachable[child] = live;
                }
                live &= match &ins.operation {
                    Operation::Return { .. } => false,
                    Operation::If {
                        then_region,
                        else_region: Some(other),
                        ..
                    } => falls[*then_region] || falls[*other],
                    _ => true,
                };
            }
        }
        if falls[0] {
            diagnostics.push(Diagnostic::at(
                Code::Return,
                format!("/functions/{f}/root_region"),
            ));
        }
    }
    diagnostics.finish()
}
