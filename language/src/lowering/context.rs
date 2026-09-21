use super::{BuildOptions, LoweringFailure, Result, identity, need, span};
use crate::{
    Span,
    semantic::{BindingId, CheckedProgram, ContractId, FunctionId, NodeKind, TypeId, TypeNode},
};
use jocky_ir::{limits, model::*, value::Value};
use jocky_shared::compiler::Type;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct Budget {
    storage: usize,
    types: usize,
    visits: usize,
    slots: usize,
    regions: usize,
    instructions: usize,
    constants: usize,
}
fn charge(used: &mut usize, n: usize, max: usize) -> Result<()> {
    let next = used.checked_add(n).ok_or(LoweringFailure::Resource(None))?;
    if next > max {
        return Err(LoweringFailure::Resource(None));
    }
    *used = next;
    Ok(())
}
impl Budget {
    pub fn bytes(&mut self, n: usize) -> Result<()> {
        charge(&mut self.storage, n, limits::STORAGE)
    }
    pub fn entries(&mut self, n: usize) -> Result<()> {
        self.bytes(n.checked_mul(64).ok_or(LoweringFailure::Resource(None))?)
    }
    pub fn refs(&mut self, n: usize) -> Result<()> {
        self.bytes(n.checked_mul(8).ok_or(LoweringFailure::Resource(None))?)
    }
    pub fn visit(&mut self) -> Result<()> {
        charge(&mut self.visits, 1, limits::VISITS)
    }
    fn type_node(&mut self, depth: usize) -> Result<()> {
        if depth > limits::TYPE_DEPTH {
            return Err(LoweringFailure::Resource(None));
        }
        self.visit()?;
        charge(&mut self.types, 1, limits::TYPE_NODES)?;
        self.entries(1)
    }
    fn contract_type(&mut self, root: &Type) -> Result<()> {
        let mut work = vec![(root, 1)];
        while let Some((ty, depth)) = work.pop() {
            self.type_node(depth)?;
            match ty {
                Type::Named { .. } => {}
                Type::List { element } | Type::Option { element } => {
                    work.push((element, depth + 1))
                }
                Type::Result { ok, error } => {
                    work.extend([(error.as_ref(), depth + 1), (ok.as_ref(), depth + 1)])
                }
            }
        }
        Ok(())
    }
}

pub(super) struct Context<'a> {
    pub checked: &'a CheckedProgram,
    pub document: IrDocument,
    pub functions: BTreeMap<FunctionId, usize>,
    pub contracts: BTreeMap<ContractId, usize>,
    pub bindings: BTreeMap<BindingId, usize>,
    pub budget: Budget,
}
impl<'a> Context<'a> {
    pub fn new(checked: &'a CheckedProgram, options: BuildOptions) -> Result<Self> {
        let (source, registry) = identity::identities(checked, options)?;
        let mut budget = Budget::default();
        budget.bytes(
            source.label.len()
                + source.sha256.len()
                + registry.fingerprint.len()
                + checked.syntax().program.module.name.text.len(),
        )?;
        budget.entries(checked.model().functions().len())?;
        let functions: BTreeMap<_, _> = checked
            .model()
            .functions()
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id(), i))
            .collect();
        let used: BTreeSet<_> = checked
            .model()
            .calls()
            .iter()
            .filter_map(|c| {
                if let crate::semantic::CallTarget::Contract(id) = c.target {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        if used.len() > limits::CONTRACTS {
            return Err(LoweringFailure::Resource(None));
        }
        budget.entries(used.len())?;
        let mut descriptors = Vec::new();
        let mut contracts = BTreeMap::new();
        for id in used {
            let c = need(checked.contract(id), None)?;
            budget.bytes(c.module.len() + c.name.len() + c.version.len())?;
            budget.entries(c.parameters.len() + c.possible_errors.len())?;
            budget.contract_type(&c.result)?;
            for p in &c.parameters {
                budget.bytes(p.name.len())?;
                budget.contract_type(&p.r#type)?;
            }
            for e in &c.possible_errors {
                budget.contract_type(&e.r#type)?;
            }
            let mut c = c.clone();
            c.supported_platforms.sort();
            c.possible_errors.sort_by_key(|e| e.code);
            contracts.insert(id, descriptors.len());
            descriptors.push(c);
        }
        let entry = *need(functions.get(&checked.entry_function()), None)?;
        let document = IrDocument {
            schema_version: SCHEMA_VERSION.into(),
            language_version: LANGUAGE_VERSION.into(),
            build_seed: options.seed.to_string(),
            source,
            registry,
            module: Module {
                name: checked.syntax().program.module.name.text.clone(),
                span: span(checked.syntax().program.module.span),
            },
            targets: checked.selected_targets().to_vec(),
            profile: checked.profile().map(|_| Profile::Lab),
            entry,
            entry_metadata: Metadata::default(),
            contracts: descriptors,
            constants: Vec::new(),
            functions: Vec::new(),
        };
        let mut context = Self {
            checked,
            document,
            functions,
            contracts,
            bindings: BTreeMap::new(),
            budget,
        };
        context.document.entry_metadata = context.metadata(entry)?;
        Ok(context)
    }
    pub fn metadata(&mut self, index: usize) -> Result<Metadata> {
        let m = need(self.checked.function_metadata().get(index), None)?;
        self.budget.refs(m.unavailable_dependencies().len())?;
        let mut out = Metadata {
            required_privilege: m.required_privilege(),
            capabilities: m.capabilities().to_vec(),
            supported_platforms: m.supported_platforms().to_vec(),
            ..Metadata::default()
        };
        for name in m.unavailable_dependencies() {
            self.budget.bytes(name.len())?;
            out.unavailable_dependencies.push(name.to_owned());
            if name != "forensic_result" {
                let c = self
                    .checked
                    .registry_snapshot()
                    .lookup(name)
                    .map_err(|_| LoweringFailure::Invariant(None))?;
                out.read_only &= c.read_only;
                out.lab_only |= c.lab_only;
            }
        }
        Ok(out)
    }
    pub fn ty(&mut self, id: TypeId) -> Result<Type> {
        let mut work = vec![(id, 1, false)];
        let mut values = Vec::new();
        while let Some((id, depth, finish)) = work.pop() {
            let node = need(self.checked.model().type_node(id), None)?;
            if !finish {
                self.budget.type_node(depth)?;
                work.push((id, depth, true));
                match node {
                    TypeNode::Named(_) => {}
                    TypeNode::List(child) | TypeNode::Option(child) => {
                        work.push((child, depth + 1, false))
                    }
                    TypeNode::Result { ok, error } => {
                        work.extend([(error, depth + 1, false), (ok, depth + 1, false)])
                    }
                }
                continue;
            }
            let ty = match node {
                TypeNode::Named(name) => Type::named(name),
                TypeNode::List(_) => Type::List {
                    element: Box::new(need(values.pop(), None)?),
                },
                TypeNode::Option(_) => Type::Option {
                    element: Box::new(need(values.pop(), None)?),
                },
                TypeNode::Result { .. } => {
                    let error = Box::new(need(values.pop(), None)?);
                    let ok = Box::new(need(values.pop(), None)?);
                    Type::Result { ok, error }
                }
            };
            values.push(ty);
        }
        need(values.pop(), None)
    }
    pub fn node_type(&mut self, kind: NodeKind, s: Span) -> Result<TypeId> {
        let id = need(self.checked.model().node_at(kind, s), Some(s))?;
        need(self.checked.model().node_type(id), Some(s))
    }
    pub fn slot(
        &mut self,
        f: usize,
        region: usize,
        kind: SlotKind,
        name: Option<&str>,
        s: Span,
        ty: TypeId,
    ) -> Result<usize> {
        charge(&mut self.budget.slots, 1, limits::ITEMS)?;
        self.budget.entries(1)?;
        self.budget.bytes(name.map_or(0, str::len))?;
        let ty = self.ty(ty)?;
        let function = need(self.document.functions.get_mut(f), Some(s))?;
        let id = function.slots.len();
        function.slots.push(Slot {
            r#type: ty,
            region,
            kind,
            name: name.map(str::to_owned),
            span: span(s),
        });
        Ok(id)
    }
    pub fn region(&mut self, f: usize) -> Result<usize> {
        charge(&mut self.budget.regions, 1, limits::ITEMS)?;
        self.budget.entries(1)?;
        let function = need(self.document.functions.get_mut(f), None)?;
        let id = function.regions.len();
        function.regions.push(Region {
            instructions: Vec::new(),
        });
        Ok(id)
    }
    pub fn emit(&mut self, f: usize, r: usize, s: Span, operation: Operation) -> Result<usize> {
        charge(&mut self.budget.instructions, 1, limits::ITEMS)?;
        self.budget.entries(1)?;
        self.budget.bytes(36)?;
        let region = need(
            self.document
                .functions
                .get_mut(f)
                .and_then(|f| f.regions.get_mut(r)),
            Some(s),
        )?;
        let i = region.instructions.len();
        region.instructions.push(Instruction {
            id: String::new(),
            span: span(s),
            operation,
        });
        Ok(i)
    }
    pub fn constant(&mut self, value: Value) -> Result<usize> {
        charge(&mut self.budget.constants, 1, limits::ITEMS)?;
        self.budget.entries(1)?;
        let id = self.document.constants.len();
        self.document.constants.push(value);
        Ok(id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lowering_counters_bound_every_allocation_family() {
        for cap in [
            limits::STORAGE,
            limits::TYPE_NODES,
            limits::ITEMS,
            limits::VISITS,
        ] {
            let mut n = cap;
            assert!(matches!(
                charge(&mut n, 1, cap),
                Err(LoweringFailure::Resource(_))
            ));
            assert_eq!(n, cap);
        }
        let mut b = Budget::default();
        assert!(b.entries(usize::MAX).is_err());
        assert!(b.refs(usize::MAX).is_err());
        assert!(b.type_node(65).is_err());
    }
}
