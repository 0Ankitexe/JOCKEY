use crate::{
    diagnostic::{Diagnostic, DiagnosticCode as Code, Diagnostics, Failure},
    identity,
    limits::{self, Budget},
    model::*,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub(super) enum Definition {
    Parameter,
    Instruction { position: usize },
    Loop,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Parent {
    pub region: usize,
    pub instruction: usize,
}
#[derive(Debug)]
pub(super) struct FunctionIndex {
    pub parents: Vec<Option<Parent>>,
    pub order: Vec<usize>,
    pub definitions: Vec<Definition>,
}

fn span_valid(span: Option<Span>, length: usize) -> bool {
    span.is_none_or(|s| s.start <= s.end && s.end <= length)
}

pub(super) fn check(
    document: &IrDocument,
    budget: &mut Budget,
) -> Result<Vec<FunctionIndex>, Failure> {
    let mut diagnostics = Diagnostics::default();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    if document.entry >= document.functions.len() {
        diagnostics.push(Diagnostic::at(Code::Reference, "/entry"));
    }
    if !span_valid(document.module.span, document.source.byte_length) {
        diagnostics.push(Diagnostic::at(Code::Reference, "/module/span"));
    }
    budget.entries(document.functions.len())?;
    let mut indexes = Vec::new();
    for (f, function) in document.functions.iter().enumerate() {
        if !names.insert(&function.name) {
            diagnostics.push(Diagnostic::at(
                Code::Reference,
                format!("/functions/{f}/name"),
            ));
        }
        if !span_valid(function.span, document.source.byte_length) {
            diagnostics.push(Diagnostic::at(
                Code::Reference,
                format!("/functions/{f}/span"),
            ));
        }
        if function.root_region != 0 || function.regions.is_empty() {
            return Err(Failure::at(
                Code::Control,
                format!("/functions/{f}/root_region"),
            ));
        }
        budget.entries(function.regions.len() + function.slots.len())?;
        budget.references(function.parameters.len())?;
        let mut parents = vec![None; function.regions.len()];
        let mut definitions = vec![None; function.slots.len()];
        let mut parameters = BTreeSet::new();
        for (position, &p) in function.parameters.iter().enumerate() {
            if p >= function.slots.len() {
                diagnostics.push(Diagnostic::at(
                    Code::Reference,
                    format!("/functions/{f}/parameters"),
                ));
                continue;
            }
            if p != position
                || !parameters.insert(p)
                || function.slots[p].kind != SlotKind::Parameter
                || function.slots[p].region != 0
            {
                diagnostics.push(Diagnostic::at(
                    Code::Initialization,
                    format!("/functions/{f}/parameters"),
                ));
            }
            definitions[p] = Some(Definition::Parameter);
        }
        for (s, slot) in function.slots.iter().enumerate() {
            if slot.region >= function.regions.len()
                || !span_valid(slot.span, document.source.byte_length)
            {
                diagnostics.push(Diagnostic::at(
                    Code::Reference,
                    format!("/functions/{f}/slots/{s}"),
                ));
            }
            if slot.kind == SlotKind::Parameter && !parameters.contains(&s) {
                diagnostics.push(Diagnostic::at(
                    Code::Initialization,
                    format!("/functions/{f}/slots/{s}"),
                ));
            }
        }
        for (r, region) in function.regions.iter().enumerate() {
            for (i, instruction) in region.instructions.iter().enumerate() {
                budget.visit()?;
                budget.references(1)?;
                if !ids.insert(instruction.id.as_str())
                    || instruction.id != identity::instruction_id(document, f, r, i)
                {
                    diagnostics.push(Diagnostic::instruction(
                        Code::Identity,
                        f,
                        r,
                        i,
                        instruction,
                        "/id",
                    ));
                }
                if !span_valid(instruction.span, document.source.byte_length) {
                    diagnostics.push(Diagnostic::instruction(
                        Code::Reference,
                        f,
                        r,
                        i,
                        instruction,
                        "/span",
                    ));
                }
                for &slot in instruction.operation.reads() {
                    if slot >= function.slots.len() {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            instruction,
                            "",
                        ));
                    }
                }
                if let Some(destination) = instruction.operation.destination() {
                    if let Some(slot) = function.slots.get(destination) {
                        if slot.region != r {
                            diagnostics.push(Diagnostic::instruction(
                                Code::Reference,
                                f,
                                r,
                                i,
                                instruction,
                                "/destination",
                            ));
                        }
                        if matches!(slot.kind, SlotKind::Parameter | SlotKind::LoopBinding)
                            || definitions[destination].is_some()
                        {
                            diagnostics.push(Diagnostic::instruction(
                                Code::Initialization,
                                f,
                                r,
                                i,
                                instruction,
                                "/destination",
                            ));
                        } else {
                            definitions[destination] =
                                Some(Definition::Instruction { position: i });
                        }
                    } else {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            instruction,
                            "/destination",
                        ));
                    }
                }
                match &instruction.operation {
                    Operation::Const { constant, .. } if *constant >= document.constants.len() => {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            instruction,
                            "/constant",
                        ))
                    }
                    Operation::Call {
                        callee: Callee::Source { function: target },
                        ..
                    } if *target >= document.functions.len() => {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            instruction,
                            "/callee/function",
                        ))
                    }
                    Operation::Call {
                        callee: Callee::Contract { contract },
                        ..
                    } if *contract >= document.contracts.len() => {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Reference,
                            f,
                            r,
                            i,
                            instruction,
                            "/callee/contract",
                        ))
                    }
                    Operation::ForEach {
                        item, body_region, ..
                    } => {
                        if let Some(slot) = function.slots.get(*item) {
                            if slot.region != *body_region
                                || slot.kind != SlotKind::LoopBinding
                                || definitions[*item].is_some()
                            {
                                diagnostics.push(Diagnostic::instruction(
                                    Code::Initialization,
                                    f,
                                    r,
                                    i,
                                    instruction,
                                    "/item",
                                ));
                            } else {
                                definitions[*item] = Some(Definition::Loop);
                            }
                        } else {
                            diagnostics.push(Diagnostic::instruction(
                                Code::Reference,
                                f,
                                r,
                                i,
                                instruction,
                                "/item",
                            ));
                        }
                    }
                    _ => {}
                }
                for child in instruction.operation.children().into_iter().flatten() {
                    if child == 0 || child >= parents.len() || parents[child].is_some() {
                        diagnostics.push(Diagnostic::instruction(
                            Code::Control,
                            f,
                            r,
                            i,
                            instruction,
                            "",
                        ));
                    } else {
                        parents[child] = Some(Parent {
                            region: r,
                            instruction: i,
                        });
                    }
                }
            }
        }
        for (s, definition) in definitions.iter().enumerate() {
            if definition.is_none() {
                diagnostics.push(Diagnostic::at(
                    Code::Initialization,
                    format!("/functions/{f}/slots/{s}"),
                ));
            }
        }
        budget.references(function.regions.len() * 2)?;
        let mut order = Vec::new();
        let mut stack = vec![0];
        let mut seen = vec![false; function.regions.len()];
        while let Some(r) = stack.pop() {
            budget.visit()?;
            if seen[r] {
                diagnostics.push(Diagnostic::at(
                    Code::Control,
                    format!("/functions/{f}/regions/{r}"),
                ));
                continue;
            }
            seen[r] = true;
            order.push(r);
            for ins in function.regions[r].instructions.iter().rev() {
                for child in ins.operation.children().into_iter().rev().flatten() {
                    if child < seen.len() && child != 0 {
                        stack.push(child);
                    }
                }
            }
        }
        if order.len() != function.regions.len() || parents.iter().skip(1).any(Option::is_none) {
            diagnostics.push(Diagnostic::at(
                Code::Control,
                format!("/functions/{f}/regions"),
            ));
        }
        // Canonical region numbering is lexical preorder, including dead bodies.
        if order.iter().copied().ne(0..function.regions.len()) {
            diagnostics.push(Diagnostic::at(
                Code::Control,
                format!("/functions/{f}/regions"),
            ));
        }
        // Missing definitions are placeholders only inside an already failing pass;
        // the diagnostic gate below prevents their exposure to dependent passes.
        indexes.push(FunctionIndex {
            parents,
            order,
            definitions: definitions
                .into_iter()
                .map(|d| d.unwrap_or(Definition::Parameter))
                .collect(),
        });
    }
    diagnostics.finish()?;
    limits::require(indexes.len(), limits::FUNCTIONS)?;
    Ok(indexes)
}

pub(super) fn visible(
    index: &FunctionIndex,
    owner: usize,
    mut region: usize,
    budget: &mut Budget,
) -> Result<bool, Failure> {
    loop {
        budget.visit()?;
        if region == owner {
            return Ok(true);
        }
        let Some(parent) = index.parents.get(region).copied().flatten() else {
            return Ok(false);
        };
        region = parent.region;
    }
}
