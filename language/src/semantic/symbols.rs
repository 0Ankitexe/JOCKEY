use super::{Analysis, ResourceFailure, diagnostic::Cause, types::TypeArena};
use crate::{
    Span,
    ast::{Identifier, Program},
};
use std::collections::BTreeMap;
use std::{fmt, sync::Arc};

macro_rules! ids {($($name:ident),+) => {$(#[derive(Clone,Copy,Debug,Eq,PartialEq,Ord,PartialOrd,Hash)] pub struct $name(pub(super) usize);)+};}
ids!(FunctionId, BindingId, NodeId, TypeId, ContractId);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NodeKind {
    Expression,
    Statement,
    Identifier,
    QualifiedName,
    TypeReference,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeInfo {
    pub id: NodeId,
    pub kind: NodeKind,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallTarget {
    Source(FunctionId),
    Contract(ContractId),
    Composition,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCall {
    pub node: NodeId,
    pub owner: FunctionId,
    pub callee_span: Span,
    pub call_span: Span,
    pub arguments: Vec<NodeId>,
    pub target: CallTarget,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedReference {
    pub node: NodeId,
    pub binding: BindingId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeAnnotation {
    pub node: NodeId,
    pub type_id: TypeId,
}
#[derive(Debug, Default)]
pub(super) struct Tables {
    pub nodes: Vec<NodeInfo>,
    pub types: Vec<TypeAnnotation>,
    pub references: Vec<ResolvedReference>,
    pub calls: Vec<ResolvedCall>,
}
impl Tables {
    pub fn node(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(NodeInfo { id, kind, span });
        id
    }
    pub fn annotate(&mut self, node: NodeId, ty: Option<TypeId>) {
        if let Some(type_id) = ty {
            self.types.push(TypeAnnotation { node, type_id });
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    Parameter,
    Local,
    Loop,
}
#[derive(Debug)]
pub struct BindingSymbol {
    pub id: BindingId,
    pub name: String,
    pub span: Span,
    pub kind: BindingKind,
    owner: FunctionId,
    pub(super) ty: Option<TypeId>,
    pub(super) used: bool,
}
impl BindingSymbol {
    pub fn owner(&self) -> FunctionId {
        self.owner
    }
    pub fn type_id(&self) -> Option<TypeId> {
        self.ty
    }
    pub fn is_used(&self) -> bool {
        self.used
    }
}
#[derive(Clone, Debug)]
pub(super) struct Signature {
    pub parameters: Vec<Option<TypeId>>,
    pub result: Option<TypeId>,
    valid: bool,
}
impl Signature {
    pub fn new(parameters: Vec<Option<TypeId>>, result: Option<TypeId>) -> Self {
        let valid = result.is_some() && parameters.iter().all(Option::is_some);
        Self {
            parameters,
            result,
            valid,
        }
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
}
#[derive(Debug)]
pub struct FunctionSymbol {
    pub(super) name: QualifiedFunctionName,
    pub(super) signature: Signature,
    id: FunctionId,
    span: Span,
    parameters: Vec<BindingId>,
}
impl FunctionSymbol {
    pub fn id(&self) -> FunctionId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name.function
    }
    pub fn module(&self) -> &str {
        &self.name.module
    }
    pub fn span(&self) -> Span {
        self.span
    }
    pub fn parameters(&self) -> &[BindingId] {
        &self.parameters
    }
    pub fn parameter_type(&self, index: usize) -> Option<TypeId> {
        self.signature.parameters.get(index).copied().flatten()
    }
    pub fn result_type(&self) -> Option<TypeId> {
        self.signature.result
    }
}
/// Share an arbitrarily long module prefix instead of copying it per function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct QualifiedFunctionName {
    pub module: Arc<str>,
    pub function: Arc<str>,
}
impl fmt::Display for QualifiedFunctionName {
    fn fmt(&self, writer: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(writer, "{}.{}", self.module, self.function)
    }
}
#[derive(Debug, Default)]
pub(super) struct Scope {
    parent: Option<usize>,
    bindings: BTreeMap<String, BindingId>,
}

/// Immutable resolved side tables, never an execution plan or runtime data.
#[derive(Debug)]
pub struct SemanticModel {
    pub(super) tables: Tables,
    pub(super) arena: TypeArena,
    pub(super) bindings: Vec<BindingSymbol>,
    pub(super) functions: Vec<FunctionSymbol>,
    pub(super) indexes: super::views::Indexes,
}
impl SemanticModel {
    pub fn nodes(&self) -> &[NodeInfo] {
        &self.tables.nodes
    }
    pub fn calls(&self) -> &[ResolvedCall] {
        &self.tables.calls
    }
    pub fn references(&self) -> &[ResolvedReference] {
        &self.tables.references
    }
    pub fn annotations(&self) -> &[TypeAnnotation] {
        &self.tables.types
    }
    pub fn bindings(&self) -> &[BindingSymbol] {
        &self.bindings
    }
    pub fn type_name(&self, id: TypeId) -> Option<String> {
        self.arena.contains(id).then(|| self.arena.display(id))
    }
}

impl Analysis<'_> {
    pub(super) fn predeclare(&mut self, program: &Program) -> Result<(), ResourceFailure> {
        let module = &program.module.name;
        if module.text == "builtin"
            || self
                .contracts
                .iter()
                .any(|f| f.module.split('.').next() == Some(&module.text))
        {
            self.diagnostics
                .push(module.span, Cause::DuplicateDefinition(module.text.clone()));
        }
        for (index, function) in program.functions.iter().enumerate() {
            let id = FunctionId(index);
            if function.name.text == "forensic_result"
                || self.function_names.contains_key(&function.name.text)
            {
                self.diagnostics.push(
                    function.name.span,
                    Cause::DuplicateDefinition(function.name.text.clone()),
                );
            } else {
                self.function_names.insert(function.name.text.clone(), id);
            }
            let mut parameters = Vec::new();
            for parameter in &function.parameters {
                parameters.push(self.model.arena.lower_source(
                    &parameter.type_ref,
                    &mut self.diagnostics,
                    &mut self.model.tables,
                )?);
            }
            let result = self.model.arena.lower_source(
                &function.return_type,
                &mut self.diagnostics,
                &mut self.model.tables,
            )?;
            self.model.functions.push(FunctionSymbol {
                id,
                span: function.span,
                parameters: Vec::new(),
                name: QualifiedFunctionName {
                    module: Arc::clone(&self.module),
                    function: Arc::from(function.name.text.as_str()),
                },
                signature: Signature::new(parameters, result),
            });
        }
        Ok(())
    }
    pub(super) fn scope(&mut self, parent: Option<usize>) -> usize {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            parent,
            ..Scope::default()
        });
        id
    }
    pub(super) fn binding(
        &mut self,
        scope: usize,
        name: &Identifier,
        ty: Option<TypeId>,
        kind: BindingKind,
        owner: FunctionId,
    ) {
        let id = BindingId(self.model.bindings.len());
        self.model.bindings.push(BindingSymbol {
            id,
            name: name.text.clone(),
            span: name.span,
            kind,
            owner,
            ty,
            used: false,
        });
        if kind == BindingKind::Parameter {
            self.model.functions[owner.0].parameters.push(id);
        }
        if name.text == "forensic_result" || self.scopes[scope].bindings.contains_key(&name.text) {
            self.diagnostics
                .push(name.span, Cause::DuplicateDefinition(name.text.clone()));
        } else {
            self.scopes[scope].bindings.insert(name.text.clone(), id);
        }
    }
    pub(super) fn lookup_binding(&self, mut scope: usize, name: &str) -> Option<BindingId> {
        loop {
            if let Some(id) = self.scopes[scope].bindings.get(name) {
                return Some(*id);
            }
            scope = self.scopes[scope].parent?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn conflicts_never_replace_first_and_later_duplicate_bodies_are_checked() {
        let text = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { return forensic.system.profile(target) } fn main(x: int) -> int { return undefined } run main on selected_endpoints";
        let CheckFailure::Semantic(d) = analyze(
            SourceFile::new("x", text),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap_err() else {
            panic!()
        };
        assert!(
            d.items()
                .iter()
                .any(|e| e.code() == DiagnosticCode::DuplicateDefinition)
        );
        assert!(
            d.items()
                .iter()
                .any(|e| e.code() == DiagnosticCode::UnknownName)
        );
        assert!(
            !d.items()
                .iter()
                .any(|e| e.code() == DiagnosticCode::InvalidEntryPoint)
        );
    }

    #[test]
    fn long_module_prefix_is_shared_not_repeated_for_every_function() {
        let text = format!(
            "module {} target ubuntu fn helper(x: int) -> int {{ return x }} fn main(target: endpoint) -> forensic_result {{ return forensic.system.profile(target) }} run main on selected_endpoints",
            "m".repeat(100_000)
        );
        let checked = analyze(
            SourceFile::new("x", &text),
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &checked.model.functions[0].name.module,
            &checked.model.functions[1].name.module
        ));
        assert!(std::sync::Arc::ptr_eq(
            &checked.model.functions[0].name.module,
            &checked.metadata[0].name.module
        ));
    }
}
