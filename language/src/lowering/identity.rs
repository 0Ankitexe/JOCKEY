use super::{BuildOptions, LoweringFailure, Result};
use crate::semantic::CheckedProgram;
use jocky_ir::{
    limits,
    model::{REGISTRY_VERSION, RegistryIdentity, SourceIdentity},
};

pub(super) fn identities(
    c: &CheckedProgram,
    _options: BuildOptions,
) -> Result<(SourceIdentity, RegistryIdentity)> {
    if c.source_label().len() > limits::LABEL_BYTES || c.source_text().len() > limits::SOURCE_BYTES
    {
        return Err(LoweringFailure::Resource(None));
    }
    Ok((
        SourceIdentity {
            label: c.source_label().to_owned(),
            byte_length: c.source_text().len(),
            sha256: jocky_ir::identity::source_hash(c.source_text().as_bytes())?,
        },
        RegistryIdentity {
            schema_version: REGISTRY_VERSION.into(),
            fingerprint: jocky_ir::identity::registry_fingerprint(c.registry_snapshot())?,
        },
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_unicode_newlines_seed_and_label_are_declared_inputs() {
        let text = include_str!("../../../examples/triage.jky");
        let registry = jocky_forensic::contracts::builtin_registry().unwrap();
        let c = crate::semantic::analyze(crate::SourceFile::new("λ", text), registry).unwrap();
        let a = super::super::lower(&c, BuildOptions::default()).unwrap();
        let b = super::super::lower(&c, BuildOptions { seed: u64::MAX }).unwrap();
        assert_eq!(a.document().source.label, "λ");
        assert_ne!(
            a.document().functions[0].regions[0].instructions[0].id,
            b.document().functions[0].regions[0].instructions[0].id
        );
        let crlf = text.replace('\n', "\r\n");
        let c = crate::semantic::analyze(crate::SourceFile::new("λ", &crlf), registry).unwrap();
        assert_ne!(
            a.document().source.sha256,
            identities(&c, BuildOptions::default()).unwrap().0.sha256
        );
        let long = "x".repeat(limits::LABEL_BYTES + 1);
        let c = crate::semantic::analyze(crate::SourceFile::new(&long, text), registry).unwrap();
        assert!(matches!(
            super::super::lower(&c, BuildOptions::default()),
            Err(LoweringFailure::Resource(_))
        ));
    }
}
