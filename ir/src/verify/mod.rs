//! Complete verification is the sole constructor of a usable program.
mod flow;
mod metadata;
mod structure;
mod types;

use crate::{
    diagnostic::Failure,
    identity, json,
    limits::{self, Budget, charge, require},
    model::*,
    value,
};
use jocky_forensic::contracts::Registry;

/// Immutable verified data; neither deserialization nor public field mutation
/// can bypass verification.
///
/// ```compile_fail
/// use jocky_ir::{VerifiedProgram, IrDocument};
/// fn forge(document: IrDocument) -> VerifiedProgram { VerifiedProgram { document } }
/// ```
/// ```compile_fail
/// fn mutate(program: &mut jocky_ir::VerifiedProgram) { program.document().entry = 99; }
/// ```
#[derive(Debug)]
pub struct VerifiedProgram {
    document: IrDocument,
    indexes: Vec<structure::FunctionIndex>,
}
impl VerifiedProgram {
    pub fn document(&self) -> &IrDocument {
        &self.document
    }
    pub fn to_json(&self) -> Result<Vec<u8>, Failure> {
        json::serialize_bounded(&self.document, limits::IR_BYTES)
    }
    pub fn function_count(&self) -> usize {
        self.indexes.len()
    }
}

pub fn verify(document: IrDocument, registry: &Registry) -> Result<VerifiedProgram, Failure> {
    let mut budget = census(&document)?;
    identity::header(&document)?;
    // Programmatically supplied objects pass the same closed wire/format checks.
    let bytes = json::serialize_bounded(
        &document,
        limits::IR_BYTES.min(limits::STORAGE - budget.storage),
    )?;
    budget.bytes(bytes.len())?;
    let (raw, next_budget) = json::read_json_metered(&bytes, limits::IR_BYTES, budget)?;
    budget = next_budget;
    json::version(&raw)?;
    json::validate_schema(json::Schema::Ir, &raw)?;
    drop(raw);
    drop(bytes);
    // Reserve the bounded diagnostic pool before any pass can retain errors.
    budget.bytes(20 * (64 + 36 + 128 + 8))?;
    let indexes = structure::check(&document, &mut budget)?;
    types::check(&document, &indexes, &mut budget)?;
    flow::check(&document, &indexes, &mut budget)?;
    metadata::check(&document, registry, &mut budget)?;
    Ok(VerifiedProgram { document, indexes })
}

fn text(s: &str, b: &mut Budget) -> Result<(), Failure> {
    b.bytes(s.len())
}
fn meta(m: &Metadata, b: &mut Budget) -> Result<(), Failure> {
    require(m.capabilities.len(), 8)?;
    require(m.supported_platforms.len(), 2)?;
    require(m.unavailable_dependencies.len(), 513)?;
    b.references(
        m.capabilities.len() + m.supported_platforms.len() + m.unavailable_dependencies.len(),
    )?;
    for d in &m.unavailable_dependencies {
        text(d, b)?;
    }
    Ok(())
}

pub(crate) fn census(document: &IrDocument) -> Result<Budget, Failure> {
    let mut b = Budget::default();
    require(document.source.label.len(), limits::LABEL_BYTES)?;
    require(document.source.byte_length, limits::SOURCE_BYTES)?;
    require(document.functions.len(), limits::FUNCTIONS)?;
    require(document.constants.len(), limits::ITEMS)?;
    require(document.contracts.len(), limits::CONTRACTS)?;
    require(document.targets.len(), 2)?;
    for s in [
        &document.schema_version,
        &document.language_version,
        &document.build_seed,
        &document.source.label,
        &document.source.sha256,
        &document.registry.schema_version,
        &document.registry.fingerprint,
        &document.module.name,
    ] {
        text(s, &mut b)?;
    }
    meta(&document.entry_metadata, &mut b)?;
    b.entries(document.functions.len() + document.constants.len() + document.contracts.len())?;
    for c in &document.constants {
        value::census(c, &mut b)?;
    }
    for c in &document.contracts {
        require(c.parameters.len(), limits::ITEMS)?;
        require(c.possible_errors.len(), 8)?;
        for s in [&c.module, &c.name, &c.version] {
            text(s, &mut b)?;
        }
        value::type_census(&c.result, 1, &mut b)?;
        b.entries(c.parameters.len() + c.possible_errors.len())?;
        for p in &c.parameters {
            text(&p.name, &mut b)?;
            value::type_census(&p.r#type, 1, &mut b)?;
        }
        for e in &c.possible_errors {
            value::type_census(&e.r#type, 1, &mut b)?;
        }
    }
    let (mut slots, mut regions, mut instructions) = (0, 0, 0);
    for f in &document.functions {
        b.visit()?;
        text(&f.name, &mut b)?;
        meta(&f.metadata, &mut b)?;
        value::type_census(&f.result, 1, &mut b)?;
        charge(&mut slots, f.slots.len(), limits::ITEMS)?;
        charge(&mut regions, f.regions.len(), limits::ITEMS)?;
        require(f.parameters.len(), limits::ITEMS)?;
        b.references(f.parameters.len())?;
        b.entries(f.slots.len() + f.regions.len())?;
        for s in &f.slots {
            b.visit()?;
            value::type_census(&s.r#type, 1, &mut b)?;
            if let Some(name) = &s.name {
                text(name, &mut b)?;
            }
        }
        for r in &f.regions {
            b.visit()?;
            charge(&mut instructions, r.instructions.len(), limits::ITEMS)?;
            b.entries(r.instructions.len())?;
            for i in &r.instructions {
                b.visit()?;
                text(&i.id, &mut b)?;
                require(i.operation.reads().len(), limits::ITEMS)?;
                b.references(i.operation.reads().len())?;
            }
        }
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticCode as Code;
    #[test]
    fn direct_deep_type_rejection_drops_iteratively_without_success() {
        let mut doc: IrDocument = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let mut ty = jocky_shared::compiler::Type::named(jocky_shared::compiler::NamedType::Bool);
        for _ in 0..20_000 {
            ty = jocky_shared::compiler::Type::List {
                element: Box::new(ty),
            };
        }
        doc.functions[0].result = ty;
        assert_eq!(
            verify(doc, jocky_forensic::contracts::builtin_registry().unwrap())
                .unwrap_err()
                .code(),
            Code::Resource
        );
    }
}
