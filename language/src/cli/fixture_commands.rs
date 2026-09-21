use super::{
    ProcessOutcome,
    files::{ArtifactIo, ArtifactKind},
    ir_commands::{host, lower_failure, rejection},
};
use crate::{
    SourceFile,
    lowering::{self, BuildOptions},
    report,
    semantic::{self, CheckFailure},
};
use jocky_forensic::contracts::{Registry, RegistryError};
use jocky_ir::{ExecutionReport, FixtureFailure, PreparedInvocation};
use std::{io::Write, path::Path};

fn setup(error: FixtureFailure, stderr: &mut dyn Write) -> ProcessOutcome {
    host(
        match error {
            FixtureFailure::Resource => "V3_RESOURCE",
            FixtureFailure::Version => "V3_VERSION",
            FixtureFailure::Registry => "V3_REGISTRY",
            _ => "V3_FIXTURE",
        },
        stderr,
    )
}
pub(super) fn run<'a>(
    path: &str,
    directory: &str,
    io: &mut impl ArtifactIo,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    registry: impl FnOnce() -> Result<&'a Registry, RegistryError>,
) -> ProcessOutcome {
    with_executor(
        path,
        directory,
        io,
        stdout,
        stderr,
        registry,
        jocky_ir::execute,
    )
}
fn with_executor<'a>(
    path: &str,
    directory: &str,
    io: &mut impl ArtifactIo,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    registry: impl FnOnce() -> Result<&'a Registry, RegistryError>,
    executor: impl FnOnce(PreparedInvocation<'_>) -> ExecutionReport,
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
            let status = if matches!(e, CheckFailure::Resource(_)) {
                ProcessOutcome::CommandOrHostFailure
            } else {
                ProcessOutcome::SyntaxFailure
            };
            return rejection(report::render_failure(source, &e), status, stderr);
        }
    };
    let ir = match lowering::lower(&checked, BuildOptions::default()) {
        Ok(ir) => ir,
        Err(e) => return lower_failure(source, e, stderr),
    };
    let bytes = match io.read(Path::new(directory), ArtifactKind::Fixture) {
        Ok(text) => text,
        Err(e) => return host(e.code(), stderr),
    };
    let limits = jocky_ir::ExecutionLimits::default();
    let fixture = match jocky_ir::decode_fixture(bytes.as_bytes(), &limits) {
        Ok(f) => f,
        Err(e) => return setup(e, stderr),
    };
    let prepared = match jocky_ir::prepare(&ir, &fixture, &limits) {
        Ok(p) => p,
        Err(e) => return setup(e, stderr),
    };
    let warnings = match report::render_semantic(source, checked.warnings()) {
        Ok(w) => w,
        Err(e) => return rejection(Err(e), ProcessOutcome::CommandOrHostFailure, stderr),
    };
    if stderr.write_all(warnings.as_bytes()).is_err() || stderr.flush().is_err() {
        return host("V3_OUTPUT", stderr);
    }
    emit(source, executor(prepared), stdout, stderr)
}
fn emit(
    source: SourceFile<'_>,
    report: ExecutionReport,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    let bytes = match report.to_json() {
        Ok(b) => b,
        Err(_) => return host("V3_OUTPUT", stderr),
    };
    let mut status = ProcessOutcome::Success;
    let diagnostic = if let Some(error) = report.failure() {
        status = if error.is_resource() {
            ProcessOutcome::CommandOrHostFailure
        } else {
            ProcessOutcome::SyntaxFailure
        };
        let span = error
            .origin()
            .and_then(|o| o.span)
            .map(|s| crate::Span::new(s.start, s.end));
        let mut rendered = match crate::report::render_fixture_diagnostic(
            source,
            span,
            error.code(),
            if error.is_resource() {
                "fixture execution stopped at a resource limit"
            } else {
                "fixture execution failed; no successful result"
            },
        ) {
            Ok(r) => r,
            Err(_) => return host("V3_OUTPUT", stderr),
        };
        if let Some(origin) = error.origin() {
            rendered.push_str(&format!(
                "  instruction: {}\n",
                crate::escape_filename(&origin.instruction_id)
            ));
        }
        rendered
    } else {
        String::new()
    };
    if stdout.write_all(&bytes).is_err() || stdout.flush().is_err() {
        return host("V3_OUTPUT", stderr);
    }
    if stderr.write_all(diagnostic.as_bytes()).is_err() || stderr.flush().is_err() {
        return host("V3_OUTPUT", stderr);
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_report_overflow_is_one_resource_failure_with_exit_two() {
        let mut d = jocky_ir::decode_ir(include_bytes!(
            "../../../ir/tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let max = jocky_ir::limits::FIXTURE_BYTES;
        let jocky_ir::value::Value::ForensicResult { value: result } = &mut d.constants[0] else {
            unreachable!()
        };
        result.system.as_mut().unwrap().hostname = "\u{1}".repeat((max - 4096) / 6);
        let size = serde_json::to_vec_pretty(&d.constants[0]).unwrap().len() + 1;
        let jocky_ir::value::Value::ForensicResult { value: result } = &mut d.constants[0] else {
            unreachable!()
        };
        result
            .system
            .as_mut()
            .unwrap()
            .hostname
            .push_str(&"x".repeat(max - 128 - size));
        let p =
            jocky_ir::verify(d, jocky_forensic::contracts::builtin_registry().unwrap()).unwrap();
        let limits = jocky_ir::ExecutionLimits::new(100_000, 10_000, 10_000_000, 128).unwrap();
        let f = jocky_ir::decode_fixture(
            include_bytes!("../../../examples/fixtures/triage/ubuntu/fixture.json"),
            &limits,
        )
        .unwrap();
        let report = jocky_ir::execute(jocky_ir::prepare(&p, &f, &limits).unwrap());
        assert_eq!(report.failure().unwrap().code(), "EXEC_OUTPUT_LIMIT");
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        assert_eq!(
            emit(
                SourceFile::new("memory", ""),
                report,
                &mut stdout,
                &mut stderr
            ),
            ProcessOutcome::CommandOrHostFailure
        );
        let raw: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(raw["outcome"]["error"]["code"], "EXEC_OUTPUT_LIMIT");
        assert!(raw["outcome"].get("value").is_none());
        assert!(stdout.len() < 64 * 1024);
        assert!(!stderr.contains(&1));
    }
    struct Memory {
        reads: Vec<ArtifactKind>,
    }
    impl ArtifactIo for Memory {
        fn read(
            &mut self,
            _: &Path,
            kind: ArtifactKind,
        ) -> Result<String, super::super::files::ArtifactFailure> {
            self.reads.push(kind);
            Ok(match kind {
                ArtifactKind::Source => {
                    include_str!("../../tests/fixtures/ir/golden/early-return.jky")
                }
                ArtifactKind::Fixture => {
                    include_str!("../../../ir/tests/fixtures/valid/ubuntu/fixture.json")
                }
                ArtifactKind::Ir => panic!("fixture runner must read source"),
            }
            .into())
        }
        fn publish(
            &mut self,
            _: &Path,
            _: &[u8],
        ) -> Result<(), super::super::files::ArtifactFailure> {
            panic!("fixture runner must not publish files")
        }
    }
    struct Broken {
        writes: usize,
        flush_only: bool,
        bytes: Vec<u8>,
    }
    impl Write for Broken {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            if self.flush_only {
                self.bytes.extend_from_slice(b);
                Ok(b.len())
            } else {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }
    #[test]
    fn warnings_flush_before_any_execution_and_output_is_one_document() {
        for flush_only in [false, true] {
            let mut io = Memory { reads: vec![] };
            let mut stdout = Vec::new();
            let mut stderr = Broken {
                writes: 0,
                flush_only,
                bytes: vec![],
            };
            assert_eq!(
                with_executor(
                    "memory",
                    "fixture",
                    &mut io,
                    &mut stdout,
                    &mut stderr,
                    jocky_forensic::contracts::builtin_registry,
                    |_| panic!("execution before warning flush")
                ),
                ProcessOutcome::CommandOrHostFailure
            );
            assert!(stdout.is_empty());
            assert_eq!(io.reads, [ArtifactKind::Source, ArtifactKind::Fixture]);
            let mut io = Memory { reads: vec![] };
            let mut stdout = Broken {
                writes: 0,
                flush_only,
                bytes: vec![],
            };
            assert_eq!(
                run(
                    "memory",
                    "fixture",
                    &mut io,
                    &mut stdout,
                    &mut Vec::new(),
                    jocky_forensic::contracts::builtin_registry
                ),
                ProcessOutcome::CommandOrHostFailure
            );
            assert_eq!(stdout.writes, 1);
            if flush_only {
                serde_json::from_slice::<serde_json::Value>(&stdout.bytes).unwrap();
            }
        }
    }
}
