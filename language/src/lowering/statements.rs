use super::{LoweringFailure, Result, context::Context, need, span};
use crate::{
    ast::{Block, FunctionDeclaration, Statement},
    semantic::NodeKind,
};
use jocky_ir::model::{Function, Operation, SlotKind};

enum Work<'a> {
    Statement(&'a Statement, usize),
    Else(&'a Block, usize, usize),
}
fn schedule<'a>(work: &mut Vec<Work<'a>>, block: &'a Block, region: usize) {
    work.extend(
        block
            .statements
            .iter()
            .rev()
            .map(|s| Work::Statement(s, region)),
    );
}

impl Context<'_> {
    pub fn function(&mut self, index: usize, source: &FunctionDeclaration) -> Result<()> {
        self.bindings.clear();
        let semantic = need(
            self.checked.model().functions().get(index),
            Some(source.span),
        )?;
        if self.functions.get(&semantic.id()) != Some(&index) {
            return Err(LoweringFailure::Invariant(Some(source.span)));
        }
        self.budget.bytes(source.name.text.len())?;
        let result = self.ty(need(
            semantic.result_type(),
            Some(source.return_type.span()),
        )?)?;
        let metadata = self.metadata(index)?;
        self.document.functions.push(Function {
            name: source.name.text.clone(),
            span: span(source.span),
            parameters: Vec::new(),
            result,
            slots: Vec::new(),
            root_region: 0,
            regions: Vec::new(),
            metadata,
        });
        self.region(index)?;
        for &id in semantic.parameters() {
            let b = need(self.checked.model().binding(id), Some(source.span))?;
            let slot = self.slot(
                index,
                0,
                SlotKind::Parameter,
                Some(&b.name),
                b.span,
                need(b.type_id(), Some(b.span))?,
            )?;
            self.budget.refs(1)?;
            self.document.functions[index].parameters.push(slot);
            self.bindings.insert(id, slot);
        }
        let mut work = Vec::new();
        self.budget.refs(source.body.statements.len())?;
        schedule(&mut work, &source.body, 0);
        while let Some(step) = work.pop() {
            self.budget.visit()?;
            match step {
                Work::Else(block, parent, instruction) => {
                    let child = self.region(index)?;
                    let op = &mut self.document.functions[index].regions[parent].instructions
                        [instruction]
                        .operation;
                    if let Operation::If { else_region, .. } = op {
                        *else_region = Some(child);
                    } else {
                        return Err(LoweringFailure::Invariant(Some(block.span)));
                    }
                    self.budget.refs(block.statements.len())?;
                    schedule(&mut work, block, child);
                }
                Work::Statement(statement, r) => match statement {
                    Statement::Let { name, value, span } => {
                        let source = self.expression(index, r, value)?;
                        let b = need(
                            self.checked.model().declaration_binding(name.span),
                            Some(name.span),
                        )?;
                        let dest = self.slot(
                            index,
                            r,
                            SlotKind::Local,
                            Some(&name.text),
                            name.span,
                            need(b.type_id(), Some(name.span))?,
                        )?;
                        self.bindings.insert(b.id, dest);
                        self.emit(
                            index,
                            r,
                            *span,
                            Operation::Copy {
                                destination: dest,
                                source,
                            },
                        )?;
                    }
                    Statement::Call {
                        arguments, span, ..
                    } => {
                        self.budget.refs(arguments.len())?;
                        let mut slots = Vec::new();
                        for a in arguments {
                            slots.push(self.expression(index, r, a)?);
                        }
                        self.call(index, r, NodeKind::Statement, *span, slots)?;
                    }
                    Statement::Return { value, span } => {
                        let value = self.expression(index, r, value)?;
                        self.emit(index, r, *span, Operation::Return { value })?;
                    }
                    Statement::If {
                        condition,
                        then_branch,
                        else_branch,
                        span,
                    } => {
                        let condition = self.expression(index, r, condition)?;
                        let child = self.region(index)?;
                        let instruction = self.emit(
                            index,
                            r,
                            *span,
                            Operation::If {
                                condition,
                                then_region: child,
                                else_region: None,
                            },
                        )?;
                        if let Some(other) = else_branch {
                            work.push(Work::Else(other, r, instruction));
                        }
                        self.budget.refs(then_branch.statements.len())?;
                        schedule(&mut work, then_branch, child);
                    }
                    Statement::For {
                        binding,
                        collection,
                        body,
                        span,
                    } => {
                        let collection = self.expression(index, r, collection)?;
                        let child = self.region(index)?;
                        let b = need(
                            self.checked.model().declaration_binding(binding.span),
                            Some(binding.span),
                        )?;
                        let item = self.slot(
                            index,
                            child,
                            SlotKind::LoopBinding,
                            Some(&binding.text),
                            binding.span,
                            need(b.type_id(), Some(binding.span))?,
                        )?;
                        self.bindings.insert(b.id, item);
                        self.emit(
                            index,
                            r,
                            *span,
                            Operation::ForEach {
                                collection,
                                item,
                                body_region: child,
                            },
                        )?;
                        self.budget.refs(body.statements.len())?;
                        schedule(&mut work, body, child);
                    }
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_regions_and_whole_statement_spans_survive_without_folding() {
        let source = "module x target ubuntu fn main(target: endpoint) -> forensic_result { if true { if false {return forensic.system.profile(target)} else {return forensic.system.profile(target)} } else {return forensic.system.profile(target)} } run main on selected_endpoints";
        let c = crate::semantic::analyze(
            crate::SourceFile::new("x", source),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        let ir = super::super::lower(&c, super::super::BuildOptions::default()).unwrap();
        let f = &ir.document().functions[0];
        assert_eq!(f.regions.len(), 5);
        assert!(matches!(
            f.regions[0].instructions[1].operation,
            Operation::If {
                then_region: 1,
                else_region: Some(4),
                ..
            }
        ));
        assert!(matches!(
            f.regions[1].instructions[1].operation,
            Operation::If {
                then_region: 2,
                else_region: Some(3),
                ..
            }
        ));
        let s = f.regions[0].instructions[1].span.unwrap();
        assert!(source[s.start..s.end].starts_with("if true"));
    }
}
