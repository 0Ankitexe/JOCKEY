//! Metadata-only restrictions and iterative strongly connected components.
use super::{
    ResourceFailure, ResourceKind,
    diagnostic::{Cause, SemanticDiagnostics},
    limits::charge,
    symbols::{FunctionId, FunctionSymbol, QualifiedFunctionName},
};
use crate::Span;
use jocky_forensic::contracts::FunctionContract;
use jocky_shared::compiler::{Capability, Platform, Privilege};
use std::collections::BTreeSet;
use std::sync::Arc;

pub(super) fn check_restrictions(
    contract: &FunctionContract,
    selected: &[Platform],
    lab: bool,
    span: Span,
    diagnostics: &mut SemanticDiagnostics,
) {
    if selected
        .iter()
        .any(|p| !contract.supported_platforms.contains(p))
    {
        diagnostics.push(span, Cause::UnsupportedTarget(contract.qualified_name()));
    }
    if contract.lab_only && !lab {
        diagnostics.push(span, Cause::LabProfileRequired(contract.qualified_name()));
    }
}

/// Complete static requirements of one source function, never runtime authority.
/// Obtained only from a successfully checked program; callers cannot change fields.
///
/// ```compile_fail
/// use jocky_language::semantic::FunctionMetadata;
/// fn modify(summary: &mut FunctionMetadata) {
///     summary.capabilities.clear();
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
pub struct FunctionMetadata {
    pub(super) name: QualifiedFunctionName,
    pub(super) required_privilege: Privilege,
    pub(super) capabilities: Vec<Capability>,
    pub(super) supported_platforms: Vec<Platform>,
    pub(super) unavailable_dependencies: Vec<Arc<str>>,
}
impl FunctionMetadata {
    /// Canonical module.function identity. Formats an owned string on demand;
    /// stored summaries share the module prefix rather than copying it per function.
    pub fn name(&self) -> String {
        self.name.to_string()
    }
    // Internal report adapter borrows prefixes; serializing many long names must
    // not first allocate their full Cartesian repetition in an owned report.
    pub(crate) fn name_parts(&self) -> (&str, &str) {
        (&self.name.module, &self.name.function)
    }
    pub fn required_privilege(&self) -> Privilege {
        self.required_privilege
    }
    /// Deduplicated union in the shared capability vocabulary's fixed order.
    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }
    /// Dependency intersection, Windows before Ubuntu; not the selected targets.
    pub fn supported_platforms(&self) -> &[Platform] {
        &self.supported_platforms
    }
    /// Unique external names in ASCII order, including forensic_result if used.
    pub fn unavailable_dependencies(&self) -> impl ExactSizeIterator<Item = &str> {
        self.unavailable_dependencies.iter().map(AsRef::as_ref)
    }
}
pub(super) struct Graph {
    edges: BTreeSet<(usize, usize)>,
    direct: Vec<Vec<u64>>,
    edge_count: usize,
}
impl Graph {
    pub fn new(functions: usize, externals: usize) -> Self {
        Self {
            edges: BTreeSet::new(),
            direct: vec![vec![0; externals.div_ceil(64)]; functions],
            edge_count: 0,
        }
    }
    pub fn source(
        &mut self,
        from: FunctionId,
        to: FunctionId,
        span: Span,
    ) -> Result<(), ResourceFailure> {
        let edge = (from.0, to.0);
        if !self.edges.contains(&edge) {
            charge(&mut self.edge_count, ResourceKind::CallEdges, 1, Some(span))?;
            self.edges.insert(edge);
        }
        Ok(())
    }
    pub fn external(&mut self, owner: FunctionId, id: usize) {
        self.direct[owner.0][id / 64] |= 1u64 << (id % 64);
    }
    pub fn summarize(
        self,
        functions: &[FunctionSymbol],
        contracts: &[&FunctionContract],
    ) -> Vec<FunctionMetadata> {
        let count = functions.len();
        let mut forward = vec![Vec::new(); count];
        let mut backward = vec![Vec::new(); count];
        for &(from, to) in &self.edges {
            forward[from].push(to);
            backward[to].push(from);
        }
        let mut seen = vec![false; count];
        let mut finished = Vec::new();
        for root in 0..count {
            let mut stack = vec![(root, false)];
            while let Some((node, exit)) = stack.pop() {
                if exit {
                    finished.push(node);
                    continue;
                }
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                stack.push((node, true));
                stack.extend(forward[node].iter().rev().map(|&next| (next, false)));
            }
        }
        let mut component = vec![usize::MAX; count];
        let mut components = 0;
        for &root in finished.iter().rev() {
            if component[root] != usize::MAX {
                continue;
            }
            let mut stack = vec![root];
            component[root] = components;
            while let Some(node) = stack.pop() {
                for &next in &backward[node] {
                    if component[next] == usize::MAX {
                        component[next] = components;
                        stack.push(next);
                    }
                }
            }
            components += 1;
        }
        let words = (contracts.len() + 1).div_ceil(64);
        let mut dependencies = vec![vec![0u64; words]; components];
        for (node, direct) in self.direct.iter().enumerate() {
            union(&mut dependencies[component[node]], direct);
        }
        let mut condensed = BTreeSet::new();
        for (from, to) in self.edges {
            if component[from] != component[to] {
                condensed.insert((component[from], component[to]));
            }
        }
        let mut remaining = vec![0; components];
        let mut parents = vec![Vec::new(); components];
        for (from, to) in condensed {
            remaining[from] += 1;
            parents[to].push(from);
        }
        let mut ready = (0..components)
            .filter(|&c| remaining[c] == 0)
            .collect::<BTreeSet<_>>();
        while let Some(callee) = ready.pop_first() {
            let complete = dependencies[callee].clone(); // <=9 words, not a type tree.
            for &caller in &parents[callee] {
                union(&mut dependencies[caller], &complete);
                remaining[caller] -= 1;
                if remaining[caller] == 0 {
                    ready.insert(caller);
                }
            }
        }
        let mut names = contracts
            .iter()
            .map(|contract| Arc::<str>::from(contract.qualified_name()))
            .collect::<Vec<_>>();
        names.push(Arc::from("forensic_result"));
        functions
            .iter()
            .enumerate()
            .map(|(index, function)| {
                reduce(
                    &function.name,
                    &dependencies[component[index]],
                    contracts,
                    &names,
                )
            })
            .collect()
    }
}
fn union(destination: &mut [u64], source: &[u64]) {
    for (to, from) in destination.iter_mut().zip(source) {
        *to |= *from;
    }
}
fn reduce(
    name: &QualifiedFunctionName,
    bits: &[u64],
    contracts: &[&FunctionContract],
    names: &[Arc<str>],
) -> FunctionMetadata {
    let mut privilege = Privilege::User;
    let mut capabilities = BTreeSet::new();
    let mut platforms = Platform::ALL.to_vec();
    let mut unavailable = BTreeSet::new();
    for (id, contract) in contracts.iter().enumerate() {
        if bits[id / 64] & (1u64 << (id % 64)) != 0 {
            privilege = privilege.max(contract.required_privilege);
            capabilities.insert(contract.capability);
            platforms.retain(|p| contract.supported_platforms.contains(p));
            unavailable.insert(Arc::clone(&names[id]));
        }
    }
    let helper = contracts.len();
    if bits[helper / 64] & (1u64 << (helper % 64)) != 0 {
        unavailable.insert(Arc::clone(&names[helper]));
    }
    FunctionMetadata {
        name: name.clone(),
        required_privilege: privilege,
        capabilities: capabilities.into_iter().collect(),
        supported_platforms: platforms,
        unavailable_dependencies: unavailable.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::Span;
    use jocky_forensic::contracts::builtin_registry;
    use jocky_shared::compiler::{Capability, Platform, Privilege};

    #[test]
    fn recursive_dependencies_and_uncalled_functions_are_separate() {
        let text = "module demo target windows | ubuntu fn first(target: endpoint) -> forensic_result { second(target) return forensic.system.profile(target) } fn second(target: endpoint) -> forensic_result { return first(target) } fn unused(target: endpoint) -> list<driver_record> { return forensic.driver.list(target) } fn pure(x: int) -> int { return x } run first on selected_endpoints";
        let checked = analyze(
            SourceFile::new("not-a-file", text),
            builtin_registry().unwrap(),
        )
        .unwrap();
        let summaries = &checked.metadata;
        assert_eq!(summaries[0].capabilities, vec![Capability::System]);
        assert_eq!(summaries[1].capabilities, summaries[0].capabilities);
        assert_eq!(
            summaries[0]
                .unavailable_dependencies
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>(),
            vec!["forensic.system.profile"]
        );
        assert_eq!(summaries[0].required_privilege, Privilege::User);
        assert_eq!(summaries[2].required_privilege, Privilege::Elevated);
        assert!(summaries[3].unavailable_dependencies.is_empty());
        assert_eq!(summaries[3].supported_platforms, Platform::ALL);
    }

    #[test]
    fn real_call_sites_enforce_targets_and_explicit_lab_even_in_dead_helpers() {
        let text = "module demo target windows fn helper(target: endpoint, since: timestamp) -> forensic_result { return forensic.system.profile(target) forensic.event.ubuntu_journal(target,since,since,1) } run helper on selected_endpoints";
        let CheckFailure::Semantic(errors) = analyze(
            SourceFile::new("ubuntu-label", text),
            builtin_registry().unwrap(),
        )
        .unwrap_err() else {
            panic!()
        };
        let diagnostic = errors
            .items()
            .iter()
            .find(|d| d.code().as_str() == "unsupported-target")
            .unwrap();
        assert_eq!(
            &text[diagnostic.span.start..diagnostic.span.end],
            "forensic.event.ubuntu_journal"
        );
        let registry = Registry::from_json(include_str!(
            "../../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
        ))
        .unwrap();
        let text = "module demo target ubuntu fn main(target: endpoint) -> forensic_result { forensic.lab.inspect() return main(target) } run main on selected_endpoints";
        assert!(matches!(
            analyze(SourceFile::new("lab", text), &registry),
            Err(CheckFailure::Semantic(_))
        ));
        let opted = text.replace("target ubuntu", "target ubuntu profile lab");
        let checked = analyze(SourceFile::new("lab", &opted), &registry).unwrap();
        assert_eq!(checked.metadata[0].required_privilege, Privilege::LabOnly);
    }

    #[test]
    fn distinct_edges_are_bounded_but_repeated_calls_are_not_extra_edges() {
        let mut graph = super::Graph::new(1024, 1);
        for from in 0..32 {
            for to in 0..1024 {
                graph
                    .source(FunctionId(from), FunctionId(to), Span::new(0, 0))
                    .unwrap();
            }
        }
        graph
            .source(FunctionId(0), FunctionId(0), Span::new(0, 0))
            .unwrap();
        assert_eq!(
            graph
                .source(FunctionId(32), FunctionId(0), Span::new(0, 0))
                .unwrap_err()
                .kind,
            ResourceKind::CallEdges
        );
    }

    #[test]
    fn both_platform_directions_are_checked_at_calls_in_uncalled_helpers() {
        for (target, call, matching) in [
            (
                "windows",
                "forensic.event.windows_log(target, \"System\", stamp, stamp, 1)",
                true,
            ),
            (
                "ubuntu",
                "forensic.event.windows_log(target, \"System\", stamp, stamp, 1)",
                false,
            ),
            (
                "ubuntu",
                "forensic.event.ubuntu_journal(target, stamp, stamp, 1)",
                true,
            ),
            (
                "windows",
                "forensic.event.ubuntu_journal(target, stamp, stamp, 1)",
                false,
            ),
        ] {
            let text = format!(
                "module demo target {target} fn helper(target: endpoint, stamp: timestamp) -> list<event_record> {{ return {call} }} fn main(target: endpoint) -> forensic_result {{ return forensic.system.profile(target) }} run main on selected_endpoints"
            );
            let result = analyze(
                SourceFile::new("host-independent", &text),
                builtin_registry().unwrap(),
            );
            if matching {
                assert!(result.is_ok());
            } else {
                let CheckFailure::Semantic(d) = result.unwrap_err() else {
                    panic!()
                };
                assert_eq!(d.items().len(), 1);
                assert_eq!(d.items()[0].code(), DiagnosticCode::UnsupportedTarget);
            }
        }
    }

    #[test]
    fn indirect_lab_and_composition_dependencies_are_honest() {
        let registry = Registry::from_json(include_str!(
            "../../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
        ))
        .unwrap();
        let text = "module demo target ubuntu fn helper() -> list<driver_record> { return forensic.lab.inspect() } fn main(target: endpoint) -> forensic_result { helper() return main(target) } run main on selected_endpoints";
        let CheckFailure::Semantic(d) = analyze(SourceFile::new("x", text), &registry).unwrap_err()
        else {
            panic!()
        };
        assert!(
            d.items()
                .iter()
                .any(|d| d.code() == DiagnosticCode::LabProfileRequired)
        );
        let permitted = text.replace("target ubuntu", "target ubuntu profile lab");
        let checked = analyze(SourceFile::new("x", &permitted), &registry).unwrap();
        assert_eq!(
            checked.metadata[checked.entry.0].required_privilege,
            Privilege::LabOnly
        );
        let triage = analyze(
            SourceFile::new("x", include_str!("../../../examples/triage.jky")),
            builtin_registry().unwrap(),
        )
        .unwrap();
        assert!(
            triage.metadata[0]
                .unavailable_dependencies
                .iter()
                .any(|name| name.as_ref() == "forensic_result")
        );
        assert_eq!(
            triage.metadata[0].capabilities,
            vec![
                Capability::System,
                Capability::Processes,
                Capability::Network,
                Capability::Indicators
            ]
        );
    }
}
