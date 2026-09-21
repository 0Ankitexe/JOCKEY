use crate::{
    diagnostic::{Diagnostic, DiagnosticCode as Code, Diagnostics, Failure},
    identity,
    limits::{self, Budget},
    model::*,
};
use jocky_forensic::contracts::Registry;
use jocky_shared::compiler::Privilege;
use std::collections::BTreeSet;

// Iterative Kosaraju: recursive source calls are finite metadata, not execution.
fn components(
    edges: &[BTreeSet<usize>],
    budget: &mut Budget,
) -> Result<(Vec<usize>, usize), Failure> {
    let n = edges.len();
    budget.references(n * 5)?;
    let mut reverse = vec![Vec::new(); n];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets {
            budget.visit()?;
            budget.references(1)?;
            reverse[to].push(from);
        }
    }
    let mut seen = vec![false; n];
    let mut finish = Vec::new();
    for root in 0..n {
        if seen[root] {
            continue;
        }
        let mut stack = vec![(root, false)];
        while let Some((node, exit)) = stack.pop() {
            budget.visit()?;
            if exit {
                finish.push(node);
                continue;
            }
            if seen[node] {
                continue;
            }
            seen[node] = true;
            stack.push((node, true));
            for &child in edges[node].iter().rev() {
                if !seen[child] {
                    stack.push((child, false));
                }
            }
        }
    }
    let mut ids = vec![usize::MAX; n];
    let mut count = 0;
    for &root in finish.iter().rev() {
        if ids[root] != usize::MAX {
            continue;
        }
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            budget.visit()?;
            if ids[node] != usize::MAX {
                continue;
            }
            ids[node] = count;
            for &parent in &reverse[node] {
                if ids[parent] == usize::MAX {
                    stack.push(parent);
                }
            }
        }
        count += 1;
    }
    Ok((ids, count))
}

pub(super) fn check(
    document: &IrDocument,
    registry: &Registry,
    budget: &mut Budget,
) -> Result<(), Failure> {
    let mut diagnostics = Diagnostics::default();
    if identity::registry_fingerprint(registry)? != document.registry.fingerprint {
        return Err(Failure::at(Code::Contract, "/registry/fingerprint"));
    }
    let mut previous = None;
    for (c, contract) in document.contracts.iter().enumerate() {
        budget.visit()?;
        let name = contract.qualified_name();
        let key = (name.clone(), contract.version.clone());
        if previous.as_ref().is_some_and(|p| p >= &key)
            || registry
                .lookup(&name)
                .ok()
                .is_none_or(|expected| identity::canonical_contract(expected) != *contract)
        {
            diagnostics.push(Diagnostic::at(Code::Contract, format!("/contracts/{c}")));
        }
        previous = Some(key);
    }
    diagnostics.finish()?;
    let mut diagnostics = Diagnostics::default();
    let n = document.functions.len();
    let count = document.contracts.len() + 1;
    let words = count.div_ceil(64);
    budget.bytes(n * count.div_ceil(8))?;
    budget.references(n)?;
    let mut direct = vec![vec![0u64; words]; n];
    let mut edges = vec![BTreeSet::new(); n];
    let mut edge_count = 0;
    let mut used = vec![false; document.contracts.len()];
    for (f, function) in document.functions.iter().enumerate() {
        for (r, region) in function.regions.iter().enumerate() {
            for (i, ins) in region.instructions.iter().enumerate() {
                budget.visit()?;
                if let Operation::Call { callee, .. } = &ins.operation {
                    match callee {
                        Callee::Source { function: target } => {
                            if !edges[f].contains(target) {
                                limits::charge(&mut edge_count, 1, limits::EDGES)?;
                                budget.bytes(16)?;
                                edges[f].insert(*target);
                            }
                        }
                        Callee::Contract { contract } => {
                            used[*contract] = true;
                            direct[f][contract / 64] |= 1u64 << (contract % 64);
                            let c = &document.contracts[*contract];
                            if document
                                .targets
                                .iter()
                                .any(|p| !c.supported_platforms.contains(p))
                                || (c.lab_only && document.profile != Some(Profile::Lab))
                            {
                                diagnostics.push(Diagnostic::instruction(
                                    Code::Metadata,
                                    f,
                                    r,
                                    i,
                                    ins,
                                    "/callee",
                                ));
                            }
                        }
                        Callee::Composition => {
                            let c = count - 1;
                            direct[f][c / 64] |= 1u64 << (c % 64);
                        }
                    }
                }
            }
        }
    }
    for (c, used) in used.iter().enumerate() {
        if !used {
            diagnostics.push(Diagnostic::at(Code::Contract, format!("/contracts/{c}")));
        }
    }
    let (ids, components) = components(&edges, budget)?;
    budget.bytes(components * count.div_ceil(8))?;
    budget.references(components)?;
    let mut sets = vec![vec![0u64; words]; components];
    let mut dag = vec![BTreeSet::new(); components];
    for node in 0..n {
        for (w, bits) in direct[node].iter().enumerate() {
            budget.visit()?;
            sets[ids[node]][w] |= bits;
        }
        for &target in &edges[node] {
            if ids[node] != ids[target] {
                budget.bytes(16)?;
                dag[ids[node]].insert(ids[target]);
            }
        }
    }
    // SCC IDs are topological here; reverse order completes dependencies first.
    for component in (0..components).rev() {
        for &target in &dag[component] {
            let (earlier, later) = sets.split_at_mut(target);
            for (destination, source) in earlier[component].iter_mut().zip(&later[0]) {
                budget.visit()?;
                *destination |= source;
            }
        }
    }
    let mut entry = None;
    for (f, function) in document.functions.iter().enumerate() {
        let mut metadata = Metadata::default();
        let mut capabilities = BTreeSet::new();
        let mut dependencies = Vec::new();
        for c in 0..count {
            budget.visit()?;
            if sets[ids[f]][c / 64] & (1u64 << (c % 64)) == 0 {
                continue;
            }
            if c == count - 1 {
                dependencies.push("forensic_result".to_owned());
                continue;
            }
            let contract = &document.contracts[c];
            metadata.required_privilege =
                std::cmp::max(metadata.required_privilege, contract.required_privilege);
            capabilities.insert(contract.capability);
            metadata
                .supported_platforms
                .retain(|p| contract.supported_platforms.contains(p));
            metadata.read_only &= contract.read_only;
            metadata.lab_only |= contract.lab_only;
            budget.bytes(contract.module.len() + 1 + contract.name.len())?;
            dependencies.push(contract.qualified_name());
        }
        metadata.capabilities = capabilities.into_iter().collect();
        dependencies.sort();
        metadata.unavailable_dependencies = dependencies;
        if metadata.required_privilege == Privilege::LabOnly
            && document.profile != Some(Profile::Lab)
        {
            diagnostics.push(Diagnostic::at(
                Code::Metadata,
                format!("/functions/{f}/metadata"),
            ));
        }
        if function.metadata != metadata {
            diagnostics.push(Diagnostic::at(
                Code::Metadata,
                format!("/functions/{f}/metadata"),
            ));
        }
        if f == document.entry {
            entry = Some(metadata);
        }
    }
    if entry.as_ref() != Some(&document.entry_metadata) {
        diagnostics.push(Diagnostic::at(Code::Metadata, "/entry_metadata"));
    }
    diagnostics.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strongly_connected_components_use_no_recursive_host_stack() {
        let edges = vec![[1].into(), [0, 2].into(), [].into(), [2].into()];
        let (ids, count) = components(&edges, &mut Budget::default()).unwrap();
        assert_eq!(count, 3);
        assert_eq!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert!(ids[0] < ids[2]);
        assert!(ids[3] < ids[2]);
    }
}
