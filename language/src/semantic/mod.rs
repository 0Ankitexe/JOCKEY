//! Pure, bounded semantic validation. There is deliberately no execution API.
mod diagnostic;
mod flow;
pub mod limits;
mod metadata;
mod resolve;
mod symbols;
mod types;
mod views;
pub use views::TypeNode;

use crate::{DiagnosticSet, ParsedSource, SourceFile, ast::TargetPlatformNode, parse_source};
pub use diagnostic::{
    Cause, DiagnosticCode, EntryFailure, SemanticDiagnostic, SemanticDiagnostics, Severity,
};
use jocky_forensic::contracts::{FunctionContract, Registry};
use jocky_shared::compiler::{NamedType, Platform};
pub use limits::{ResourceFailure, ResourceKind};
pub use metadata::FunctionMetadata;
use std::collections::BTreeMap;
pub use symbols::{
    BindingId, BindingKind, BindingSymbol, CallTarget, ContractId, FunctionId, FunctionSymbol,
    NodeId, NodeInfo, NodeKind, ResolvedCall, ResolvedReference, SemanticModel, TypeAnnotation,
    TypeId,
};
use symbols::{Scope, Signature, Tables};

#[derive(Debug)]
pub enum CheckFailure {
    Syntax(DiagnosticSet),
    Semantic(SemanticDiagnostics),
    Resource(ResourceFailure),
}
impl From<ResourceFailure> for CheckFailure {
    fn from(failure: ResourceFailure) -> Self {
        Self::Resource(failure)
    }
}

/// Only `analyze` can construct a checked program, after every gate succeeds.
#[derive(Debug)]
pub struct CheckedProgram {
    // One bounded original-source copy binds successful report metadata to the
    // actual checked bytes. It is never exposed as an execution/evidence value.
    source_text: Box<str>,
    source_label: Box<str>,
    registry: Registry,
    contract_names: Vec<String>,
    syntax: ParsedSource,
    model: SemanticModel,
    warnings: SemanticDiagnostics,
    metadata: Vec<FunctionMetadata>,
    entry: FunctionId,
    selected_targets: Vec<Platform>,
}
impl CheckedProgram {
    pub fn source_text(&self) -> &str {
        &self.source_text
    }
    pub fn source_label(&self) -> &str {
        &self.source_label
    }
    pub fn entry_function(&self) -> FunctionId {
        self.entry
    }
    pub fn registry_snapshot(&self) -> &Registry {
        &self.registry
    }
    pub fn contract(&self, id: ContractId) -> Option<&FunctionContract> {
        self.registry.lookup(self.contract_names.get(id.0)?).ok()
    }
    pub(crate) fn matches_source(&self, text: &str) -> bool {
        self.source_text.as_ref() == text
    }
    pub fn syntax(&self) -> &ParsedSource {
        &self.syntax
    }
    pub fn model(&self) -> &SemanticModel {
        &self.model
    }
    pub fn warnings(&self) -> &SemanticDiagnostics {
        &self.warnings
    }
    /// Complete summaries in source declaration order, including uncalled helpers.
    pub fn function_metadata(&self) -> &[FunctionMetadata] {
        &self.metadata
    }
    /// The exact run-function summary; unrelated helpers do not inflate it.
    pub fn entry_metadata(&self) -> &FunctionMetadata {
        &self.metadata[self.entry.0]
    }
    /// Source-selected targets in canonical Windows-before-Ubuntu order.
    pub fn selected_targets(&self) -> &[Platform] {
        &self.selected_targets
    }
    /// Explicit static lab opt-in and original spans; None is the standard profile.
    pub fn profile(&self) -> Option<&crate::LabProfileDeclaration> {
        self.syntax.profile.as_ref()
    }
}

struct Analysis<'a> {
    module: std::sync::Arc<str>,
    selected_targets: Vec<Platform>,
    lab: bool,
    model: SemanticModel,
    diagnostics: SemanticDiagnostics,
    scopes: Vec<Scope>,
    function_names: BTreeMap<String, FunctionId>,
    contracts: Vec<&'a FunctionContract>,
    contract_names: BTreeMap<String, ContractId>,
    contract_signatures: Vec<Signature>,
    composition: Signature,
    graph: metadata::Graph,
}

/// Parse and check supplied text. Its filename is only a diagnostic label.
pub fn analyze(
    source: SourceFile<'_>,
    registry: &Registry,
) -> Result<CheckedProgram, CheckFailure> {
    limits::require(ResourceKind::SourceBytes, source.text.len(), None)?;
    let syntax = match parse_source(source) {
        Ok(parsed) => parsed,
        Err(diagnostics) if diagnostics.has_resource_limit() => {
            return Err(ResourceFailure::new(
                ResourceKind::ParserNesting,
                diagnostics.items().first().map(|d| d.span),
            )
            .into());
        }
        Err(diagnostics) => return Err(CheckFailure::Syntax(diagnostics)),
    };
    limits::census(&syntax)?;
    let contracts = registry.functions().collect::<Vec<_>>();
    let contract_names = contracts
        .iter()
        .enumerate()
        .map(|(id, c)| (c.qualified_name(), ContractId(id)))
        .collect();
    let arena = types::TypeArena::new();
    let result = arena.named(NamedType::ForensicResult);
    let graph = metadata::Graph::new(syntax.program.functions.len(), contracts.len() + 1);
    let mut analysis = Analysis {
        module: std::sync::Arc::from(syntax.program.module.name.text.as_str()),
        selected_targets: syntax
            .program
            .target
            .platforms
            .iter()
            .map(|p| match p {
                TargetPlatformNode::Windows { .. } => Platform::Windows,
                TargetPlatformNode::Ubuntu { .. } => Platform::Ubuntu,
            })
            .collect(),
        lab: syntax.profile.is_some(),
        model: SemanticModel {
            tables: Tables::default(),
            arena,
            bindings: Vec::new(),
            functions: Vec::new(),
            indexes: views::Indexes::default(),
        },
        diagnostics: SemanticDiagnostics::default(),
        scopes: Vec::new(),
        function_names: BTreeMap::new(),
        contracts,
        contract_names,
        contract_signatures: Vec::new(),
        composition: Signature::new(vec![Some(result), Some(result)], Some(result)),
        graph,
    };
    analysis.predeclare(&syntax.program)?;
    for contract in &analysis.contracts {
        let parameters = contract
            .parameters
            .iter()
            .map(|p| analysis.model.arena.lower_contract(&p.r#type).map(Some))
            .collect::<Result<Vec<_>, _>>()?;
        let result = Some(analysis.model.arena.lower_contract(&contract.result)?);
        analysis
            .contract_signatures
            .push(Signature::new(parameters, result));
    }
    for (index, function) in syntax.program.functions.iter().enumerate() {
        analysis.body(function, FunctionId(index))?;
    }
    let entry = analysis.entry(&syntax.program, source.text.len());
    analysis.unused_bindings();
    if analysis.diagnostics.has_errors() {
        return Err(CheckFailure::Semantic(analysis.diagnostics));
    }
    // A missing entry always produces an error above, independently of display caps.
    let Some(entry) = entry else {
        return Err(CheckFailure::Semantic(analysis.diagnostics));
    };
    let metadata = analysis
        .graph
        .summarize(&analysis.model.functions, &analysis.contracts);
    analysis.model.indexes = views::Indexes::new(&analysis.model);
    Ok(CheckedProgram {
        source_text: source.text.into(),
        source_label: source.name.into(),
        registry: registry.clone(),
        contract_names: registry
            .functions()
            .map(FunctionContract::qualified_name)
            .collect(),
        syntax,
        model: analysis.model,
        warnings: analysis.diagnostics,
        metadata,
        entry,
        selected_targets: analysis.selected_targets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_accessors_preserve_identity_canonical_order_and_failure_boundary() {
        let text = "module demo target windows profile lab fn unused(target: endpoint) -> list<driver_record> { return forensic.driver.list(target) } fn main(target: endpoint) -> forensic_result { return forensic.system.profile(target) } run main on selected_endpoints";
        let registry = jocky_forensic::contracts::builtin_registry().unwrap();
        let checked = analyze(SourceFile::new("in-memory", text), registry).unwrap();
        assert_eq!(checked.selected_targets(), [Platform::Windows]);
        assert_eq!(checked.profile(), checked.syntax().profile.as_ref());
        assert!(std::ptr::eq(
            checked.entry_metadata(),
            &checked.function_metadata()[1]
        ));
        let repeated = analyze(SourceFile::new("another-label", text), registry).unwrap();
        assert_eq!(checked.function_metadata(), repeated.function_metadata());
        assert_ne!(
            checked.function_metadata()[0],
            checked.function_metadata()[1]
        );
        assert_eq!(checked.entry_metadata().name(), "demo.main");
        assert_eq!(
            checked.entry_metadata().required_privilege(),
            jocky_shared::compiler::Privilege::User
        );
        for metadata in checked.function_metadata() {
            assert!(
                metadata
                    .capabilities()
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            );
            assert!(
                metadata
                    .supported_platforms()
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            );
            assert!(
                checked
                    .selected_targets()
                    .iter()
                    .all(|p| metadata.supported_platforms().contains(p))
            );
            let deps = metadata.unavailable_dependencies().collect::<Vec<_>>();
            assert!(deps.windows(2).all(|pair| pair[0] < pair[1]));
        }
        let bad = text.replace("forensic.system.profile", "forensic.system.missing");
        assert!(matches!(
            analyze(SourceFile::new("x", &bad), registry),
            Err(CheckFailure::Semantic(_))
        ));
        assert!(matches!(
            analyze(SourceFile::new("x", "profile lab"), registry),
            Err(CheckFailure::Syntax(_))
        ));
    }
    #[test]
    fn success_has_distinct_node_ids_and_no_poisoned_bindings_or_error_diagnostics() {
        let source = SourceFile::new("not-a-path", include_str!("../../../examples/triage.jky"));
        let checked = analyze(
            source,
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        assert_eq!(
            checked.model().nodes().len(),
            checked
                .model()
                .nodes()
                .iter()
                .map(|n| n.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
        assert!(
            checked
                .model()
                .bindings()
                .iter()
                .all(|b| b.type_id().is_some())
        );
        assert!(!checked.warnings().has_errors());
        assert_eq!(
            checked.metadata.len(),
            checked.syntax.program.functions.len()
        );
        assert_eq!(
            checked.metadata[checked.entry.0].name.to_string(),
            "triage.investigate"
        );
    }
}
