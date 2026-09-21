use super::{
    ProcessOutcome,
    files::{ArtifactIo, ArtifactKind},
};
use crate::{
    SourceFile,
    lowering::{self, BuildOptions, LoweringFailure},
    report::{self, RenderError},
    semantic::{self, CheckFailure},
};
use jocky_forensic::contracts::{Registry, RegistryError};
use std::{io::Write, path::Path};

pub(super) fn host(code: &str, stderr: &mut dyn Write) -> ProcessOutcome {
    let message = match code {
        "V3_INPUT" => "cannot read the explicitly named local input",
        "V3_FIXTURE" => "cannot read the explicitly named fixture",
        "V3_RESOURCE" => "compiler resource limit exceeded",
        "V3_VERSION" => "unsupported compiler artifact version",
        "V3_REGISTRY" => "compiler registry is unavailable or inconsistent",
        "V3_UNSUPPORTED_IO" => "safe local artifact publication is unsupported",
        _ => "artifact delivery failed; a complete final file or owned staging file may remain",
    };
    let _ = writeln!(stderr, "error[{code}]: {message}");
    let _ = stderr.flush();
    ProcessOutcome::CommandOrHostFailure
}
pub(super) fn rejection(
    rendered: Result<String, RenderError>,
    outcome: ProcessOutcome,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    match rendered {
        Ok(text) => {
            if stderr.write_all(text.as_bytes()).is_ok() && stderr.flush().is_ok() {
                outcome
            } else {
                host("V3_OUTPUT", stderr)
            }
        }
        Err(RenderError::ResourceLimit) => host("V3_RESOURCE", stderr),
        Err(RenderError::InvalidSpan) => host("V3_OUTPUT", stderr),
    }
}
pub(super) fn lower_failure(
    source: SourceFile<'_>,
    error: LoweringFailure,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    match &error {
        LoweringFailure::Resource(_) => return host("V3_RESOURCE", stderr),
        LoweringFailure::Verification(e) if e.code() == jocky_ir::DiagnosticCode::Version => {
            return host("V3_VERSION", stderr);
        }
        LoweringFailure::Verification(e) if e.code() == jocky_ir::DiagnosticCode::Contract => {
            return host("V3_REGISTRY", stderr);
        }
        _ => {}
    }
    let rendered = if let LoweringFailure::Verification(e) = &error {
        let mut output = String::new();
        for d in e.diagnostics() {
            let span = d.span.map(|s| crate::Span {
                start: s.start,
                end: s.end,
            });
            let part = match report::render_compiler_diagnostic(
                source,
                span,
                d.code.as_str(),
                d.code.message(),
            ) {
                Ok(part) => part,
                Err(e) => return rejection(Err(e), ProcessOutcome::SyntaxFailure, stderr),
            };
            if output.len().saturating_add(part.len() + 128) > semantic::limits::MAX_REPORT_BYTES {
                return host("V3_RESOURCE", stderr);
            }
            output.push_str(&part);
            if let Some(id) = &d.instruction_id {
                output.push_str(&format!("  instruction: {id}\n"));
            }
            if let Some(pointer) = &d.pointer {
                output.push_str(&format!("  IR pointer: {pointer}\n"));
            }
        }
        if e.truncated() {
            output.push_str("note: additional IR diagnostics suppressed after 20 errors\n");
        }
        Ok(output)
    } else {
        report::render_compiler_diagnostic(
            source,
            error.span(),
            error.code(),
            "checked program could not be lowered consistently",
        )
    };
    rejection(rendered, ProcessOutcome::SyntaxFailure, stderr)
}

pub(super) fn build<'a>(
    path: &str,
    output: &str,
    io: &mut impl ArtifactIo,
    stderr: &mut dyn Write,
    registry: impl FnOnce() -> Result<&'a Registry, RegistryError>,
) -> ProcessOutcome {
    let text = match io.read(Path::new(path), ArtifactKind::Source) {
        Ok(text) => text,
        Err(e) => return host(e.code(), stderr),
    };
    let source = SourceFile::new(path, &text);
    let registry = match registry() {
        Ok(r) => r,
        Err(_) => return host("V3_REGISTRY", stderr),
    };
    let label = path.replace('\\', "/");
    let checked = match semantic::analyze(SourceFile::new(&label, &text), registry) {
        Ok(c) => c,
        Err(e) => {
            let outcome = if matches!(e, CheckFailure::Resource(_)) {
                ProcessOutcome::CommandOrHostFailure
            } else {
                ProcessOutcome::SyntaxFailure
            };
            return rejection(report::render_failure(source, &e), outcome, stderr);
        }
    };
    let ir = match lowering::lower(&checked, BuildOptions::default()) {
        Ok(ir) => ir,
        Err(e) => return lower_failure(source, e, stderr),
    };
    let bytes = match ir.to_json() {
        Ok(b) => b,
        Err(e) => return lower_failure(source, e.into(), stderr),
    };
    let warnings = match report::render_semantic(source, checked.warnings()) {
        Ok(w) => w,
        Err(e) => return rejection(Err(e), ProcessOutcome::CommandOrHostFailure, stderr),
    };
    if stderr.write_all(warnings.as_bytes()).is_err() || stderr.flush().is_err() {
        return host("V3_OUTPUT", stderr);
    }
    match io.publish(Path::new(output), &bytes) {
        Ok(()) => ProcessOutcome::Success,
        Err(e) => host(e.code(), stderr),
    }
}

pub(super) fn verify_artifact<'a>(
    path: &str,
    io: &mut impl ArtifactIo,
    stderr: &mut dyn Write,
    registry: impl FnOnce() -> Result<&'a Registry, RegistryError>,
) -> ProcessOutcome {
    let text = match io.read(Path::new(path), ArtifactKind::Ir) {
        Ok(text) => text,
        Err(e) => return host(e.code(), stderr),
    };
    let document = match jocky_ir::decode_ir(text.as_bytes()) {
        Ok(document) => document,
        Err(e) => return super::ir_diagnostics::reject(path, &e, stderr),
    };
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => return host("V3_REGISTRY", stderr),
    };
    match jocky_ir::verify(document, registry) {
        Ok(_) => ProcessOutcome::Success,
        Err(e) => super::ir_diagnostics::reject(path, &e, stderr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::files::ArtifactFailure;
    use std::io;
    struct Memory {
        text: String,
        published: Option<Vec<u8>>,
        failure: Option<ArtifactFailure>,
    }
    impl ArtifactIo for Memory {
        fn read(&mut self, _: &Path, kind: ArtifactKind) -> Result<String, ArtifactFailure> {
            assert_eq!(kind, ArtifactKind::Source);
            Ok(self.text.clone())
        }
        fn publish(&mut self, _: &Path, bytes: &[u8]) -> Result<(), ArtifactFailure> {
            if let Some(e) = self.failure {
                return Err(e);
            }
            self.published = Some(bytes.to_vec());
            Ok(())
        }
    }
    struct Broken(bool);
    impl Write for Broken {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.0 {
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                Ok(b.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    fn input() -> Memory {
        Memory {
            text: include_str!("../../../examples/triage.jky").into(),
            published: None,
            failure: None,
        }
    }
    #[test]
    fn warning_delivery_precedes_publication_and_labels_are_not_paths() {
        for fail_write in [false, true] {
            let mut m = input();
            assert_eq!(
                build(
                    "logical\\name",
                    "out",
                    &mut m,
                    &mut Broken(fail_write),
                    jocky_forensic::contracts::builtin_registry
                ),
                ProcessOutcome::CommandOrHostFailure
            );
            assert!(m.published.is_none());
        }
        let mut m = input();
        assert_eq!(
            build(
                "logical\\name",
                "out",
                &mut m,
                &mut Vec::new(),
                jocky_forensic::contracts::builtin_registry
            ),
            ProcessOutcome::Success
        );
        assert_eq!(
            jocky_ir::decode_ir(&m.published.unwrap())
                .unwrap()
                .source
                .label,
            "logical/name"
        );
    }
    #[test]
    fn build_failures_never_publish_partial_buffers() {
        let mut m = input();
        m.text = "module".into();
        assert_eq!(
            build(
                "x",
                "out",
                &mut m,
                &mut Vec::new(),
                jocky_forensic::contracts::builtin_registry
            ),
            ProcessOutcome::SyntaxFailure
        );
        assert!(m.published.is_none());
        let mut m = input();
        let long = "x".repeat(jocky_ir::limits::LABEL_BYTES + 1);
        let mut stderr = Vec::new();
        assert_eq!(
            build(
                &long,
                "out",
                &mut m,
                &mut stderr,
                jocky_forensic::contracts::builtin_registry
            ),
            ProcessOutcome::CommandOrHostFailure
        );
        assert!(m.published.is_none());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .starts_with("error[V3_RESOURCE]")
        );
        let mut m = input();
        m.failure = Some(ArtifactFailure::UnsupportedIo);
        assert_eq!(
            build(
                "x",
                "out",
                &mut m,
                &mut Vec::new(),
                jocky_forensic::contracts::builtin_registry
            ),
            ProcessOutcome::CommandOrHostFailure
        );
    }
    #[test]
    fn source_located_internal_failure_reuses_existing_renderer() {
        let mut stderr = Vec::new();
        let source = SourceFile::new("label\n.jky", "abc\r\n");
        assert_eq!(
            lower_failure(
                source,
                LoweringFailure::Invariant(Some(crate::Span::new(1, 2))),
                &mut stderr
            ),
            ProcessOutcome::SyntaxFailure
        );
        let text = String::from_utf8(stderr).unwrap();
        assert!(text.contains("label\\n.jky:1:2"));
        assert!(text.contains("1 | abc"));
        assert!(text.contains("LOWER_INVARIANT"));
    }
}
