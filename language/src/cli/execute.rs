use super::{AstFormat, CliError, Command, DiagnosticFormat, ProcessOutcome, USAGE, args_os};
use crate::{
    SourceFile, escape_filename, parse_source, render_diagnostics,
    report::{CheckReport, HostFailure, RenderError, render_failure, render_semantic},
    semantic::{CheckFailure, analyze, limits::MAX_SOURCE_BYTES},
};
use jocky_forensic::contracts::{Registry, RegistryError, builtin_registry};
use std::{
    ffi::OsString,
    fs,
    io::{self, Read, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadFailure {
    Input(io::ErrorKind),
    ResourceLimit,
}

/// Injected readers remain compatible; the real check reader bounds allocation.
pub trait SourceReader {
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>>;
    fn read_bounded(&mut self, path: &Path, limit: usize) -> Result<Vec<u8>, ReadFailure> {
        let bytes = self.read(path).map_err(|e| ReadFailure::Input(e.kind()))?;
        if bytes.len() > limit {
            Err(ReadFailure::ResourceLimit)
        } else {
            Ok(bytes)
        }
    }
}
#[derive(Default)]
pub struct FileSourceReader;
impl SourceReader for FileSourceReader {
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }
    fn read_bounded(&mut self, path: &Path, limit: usize) -> Result<Vec<u8>, ReadFailure> {
        let mut file = fs::File::open(path).map_err(|e| ReadFailure::Input(e.kind()))?;
        let bound = limit.checked_add(1).ok_or(ReadFailure::ResourceLimit)?;
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 8192];
        while bytes.len() < bound {
            let available = (bound - bytes.len()).min(chunk.len());
            let count = match file.read(&mut chunk[..available]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(|e| ReadFailure::Input(e.kind()))?,
            };
            if count == 0 {
                break;
            }
            let next = bytes
                .len()
                .checked_add(count)
                .ok_or(ReadFailure::ResourceLimit)?;
            if next > bytes.capacity() {
                let capacity = next.checked_next_power_of_two().unwrap_or(bound).min(bound);
                bytes
                    .try_reserve_exact(capacity - bytes.len())
                    .map_err(|_| ReadFailure::ResourceLimit)?;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        if bytes.len() > limit {
            Err(ReadFailure::ResourceLimit)
        } else {
            Ok(bytes)
        }
    }
}

pub fn run<I: IntoIterator<Item = OsString>>(
    arguments: I,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    run_with_reader(arguments, &mut FileSourceReader, stdout, stderr)
}
pub fn run_with_reader<I: IntoIterator<Item = OsString>, R: SourceReader>(
    arguments: I,
    reader: &mut R,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let json_check = super::args::is_json_check(&arguments);
    let command = match args_os(arguments) {
        Ok(command) => command,
        Err(CliError::InvalidPathEncoding) if json_check => {
            return super::check_output::host(
                HostFailure::InvalidPathEncoding,
                None,
                stdout,
                stderr,
            );
        }
        Err(error) => return finish_cli_error(error, None, stderr),
    };
    execute(command, reader, stdout, stderr, builtin_registry)
}

fn execute<'a, R: SourceReader, F: FnOnce() -> Result<&'a Registry, RegistryError>>(
    command: Command,
    reader: &mut R,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    registry: F,
) -> ProcessOutcome {
    enum SourceAction {
        Parse(Option<AstFormat>),
        Check(DiagnosticFormat),
    }
    // Dispatch before the source-reader boundary. Embedded discovery cannot
    // accidentally interpret a module name as a file or probe the working directory.
    let (path, action) = match command {
        Command::RunFixture { path, directory } => {
            return super::fixture_commands::run(
                &path,
                &directory,
                &mut super::files::LocalArtifactIo,
                stdout,
                stderr,
                registry,
            );
        }
        Command::VerifyIr { path } => {
            return super::ir_commands::verify_artifact(
                &path,
                &mut super::files::LocalArtifactIo,
                stderr,
                registry,
            );
        }
        Command::BuildIr { path, output } => {
            return super::ir_commands::build(
                &path,
                &output,
                &mut super::files::LocalArtifactIo,
                stderr,
                registry,
            );
        }
        Command::ModulesList => return execute_modules(None, registry(), stdout, stderr),
        Command::ModulesDescribe { name } => {
            return execute_modules(Some(&name), registry(), stdout, stderr);
        }
        Command::Parse { path, emit_ast } => (path, SourceAction::Parse(emit_ast)),
        Command::Check { path, format } => (path, SourceAction::Check(format)),
    };
    let json_check = matches!(action, SourceAction::Check(DiagnosticFormat::Json));
    let input_failure = |error, stdout: &mut dyn Write, stderr: &mut dyn Write| {
        if json_check {
            super::check_output::host(error, Some(&path), stdout, stderr)
        } else {
            let error = match error {
                HostFailure::SourceLimit => CliError::SourceTooLarge,
                HostFailure::InputNotFound => CliError::InputNotFound,
                HostFailure::InvalidUtf8 => CliError::InvalidUtf8,
                _ => CliError::InputUnreadable,
            };
            finish_cli_error(error, Some(&path), stderr)
        }
    };
    let bytes = match if matches!(action, SourceAction::Check(_)) {
        reader.read_bounded(Path::new(&path), MAX_SOURCE_BYTES)
    } else {
        reader
            .read(Path::new(&path))
            .map_err(|e| ReadFailure::Input(e.kind()))
    } {
        Ok(bytes) => bytes,
        Err(ReadFailure::ResourceLimit) => {
            return input_failure(HostFailure::SourceLimit, stdout, stderr);
        }
        Err(ReadFailure::Input(io::ErrorKind::NotFound)) => {
            return input_failure(HostFailure::InputNotFound, stdout, stderr);
        }
        Err(_) => return input_failure(HostFailure::InputUnreadable, stdout, stderr),
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(_) => return input_failure(HostFailure::InvalidUtf8, stdout, stderr),
    };
    let source = SourceFile::new(&path, text);
    match action {
        SourceAction::Parse(emit_ast) => match parse_source(source) {
            Ok(parsed) => match emit_ast {
                None => write_success(
                    stdout,
                    stderr,
                    format!("parsed: {}\n", escape_filename(&path)).as_bytes(),
                ),
                Some(AstFormat::Json) => match parsed.to_pretty_json() {
                    Ok(bytes) => write_success(stdout, stderr, &bytes),
                    Err(_) => finish_cli_error(CliError::OutputFailure, None, stderr),
                },
            },
            Err(diagnostics) => {
                let output = render_diagnostics(source, &diagnostics);
                if stderr.write_all(output.as_bytes()).is_err()
                    || stderr.flush().is_err()
                    || diagnostics.has_resource_limit()
                {
                    ProcessOutcome::CommandOrHostFailure
                } else {
                    ProcessOutcome::SyntaxFailure
                }
            }
        },
        SourceAction::Check(format) => {
            let registry = match registry() {
                Ok(registry) => registry,
                Err(_) if json_check => {
                    return super::check_output::host(
                        HostFailure::InvalidRegistry,
                        Some(&path),
                        stdout,
                        stderr,
                    );
                }
                Err(_) => return finish_cli_error(CliError::InvalidRegistry, None, stderr),
            };
            let result = analyze(source, registry);
            if format == DiagnosticFormat::Json {
                let report = match &result {
                    Ok(checked) => CheckReport::from_checked(source, checked),
                    Err(failure) => CheckReport::from_failure(source, failure),
                };
                return super::check_output::emit(report, Some(&path), stdout, stderr);
            }
            let (rendered, outcome) = match result {
                Ok(checked) => (
                    render_semantic(source, checked.warnings()),
                    ProcessOutcome::Success,
                ),
                Err(failure) => {
                    let outcome = if matches!(failure, CheckFailure::Resource(_)) {
                        ProcessOutcome::CommandOrHostFailure
                    } else {
                        ProcessOutcome::SyntaxFailure
                    };
                    (render_failure(source, &failure), outcome)
                }
            };
            match rendered {
                Ok(output) => {
                    if stderr.write_all(output.as_bytes()).is_ok() && stderr.flush().is_ok() {
                        outcome
                    } else {
                        finish_cli_error(CliError::OutputFailure, None, stderr)
                    }
                }
                Err(RenderError::ResourceLimit) => {
                    finish_cli_error(CliError::ReportTooLarge, None, stderr)
                }
                Err(RenderError::InvalidSpan) => {
                    finish_cli_error(CliError::OutputFailure, None, stderr)
                }
            }
        }
    }
}

fn execute_modules(
    name: Option<&str>,
    registry: Result<&Registry, RegistryError>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    let registry = match registry {
        Ok(registry) => registry,
        Err(_) => return finish_cli_error(CliError::InvalidRegistry, None, stderr),
    };
    let output = match name {
        None => super::modules::list(registry),
        Some(name) => match super::modules::describe(registry, name) {
            Ok(output) => output,
            Err(_) => return finish_cli_error(CliError::ModuleNotFound, Some(name), stderr),
        },
    };
    if stdout.write_all(output.as_bytes()).is_ok() && stdout.flush().is_ok() {
        ProcessOutcome::Success
    } else {
        finish_cli_error(CliError::OutputFailure, None, stderr)
    }
}

fn write_success(stdout: &mut dyn Write, stderr: &mut dyn Write, bytes: &[u8]) -> ProcessOutcome {
    if stdout.write_all(bytes).is_ok() && stdout.flush().is_ok() {
        ProcessOutcome::Success
    } else {
        finish_cli_error(CliError::OutputFailure, None, stderr)
    }
}
pub(super) fn finish_cli_error(
    error: CliError,
    path: Option<&str>,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    let _ = stderr.write_all(render_cli_error(error, path).as_bytes());
    let _ = stderr.flush();
    ProcessOutcome::CommandOrHostFailure
}
pub fn render_cli_error(error: CliError, path: Option<&str>) -> String {
    if error == CliError::InvalidCommand {
        return USAGE.to_owned();
    }
    let message = match error {
        CliError::InvalidCommand => unreachable!("handled above"),
        CliError::InvalidPathEncoding => "input path must be valid UTF-8".to_owned(),
        CliError::InputNotFound => format!(
            "input file not found: {}",
            escape_filename(path.unwrap_or("<unknown>"))
        ),
        CliError::InputUnreadable => format!(
            "input file is not readable: {}",
            escape_filename(path.unwrap_or("<unknown>"))
        ),
        CliError::InvalidUtf8 => format!(
            "input file is not valid UTF-8: {}",
            escape_filename(path.unwrap_or("<unknown>"))
        ),
        CliError::ResourceLimit => "frontend nesting limit exceeded".to_owned(),
        CliError::SourceTooLarge => "source bytes limit exceeded".to_owned(),
        CliError::ReportTooLarge => "report bytes limit exceeded".to_owned(),
        CliError::InvalidRegistry => "module registry is invalid".to_owned(),
        CliError::ModuleNotFound => format!(
            "module function not found: {}",
            escape_filename(path.unwrap_or("<unknown>"))
        ),
        CliError::OutputFailure => "output could not be written".to_owned(),
    };
    format!("error[{}]: {message}\n", error.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_registry_failure_is_structured_and_input_failure_precedes_registry() {
        struct BadInput;
        impl SourceReader for BadInput {
            fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
                Ok(vec![0xff])
            }
        }
        let json_command = || Command::Check {
            path: "memory".into(),
            format: DiagnosticFormat::Json,
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(
            execute(json_command(), &mut Memory, &mut out, &mut err, || Err(
                Registry::from_json("{}").unwrap_err()
            ))
            .exit_code(),
            2
        );
        let report: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(report["diagnostics"][0]["code"], "invalid-registry");
        assert!(report["diagnostics"][0]["location"].is_null());
        assert!(report["metadata"].is_null() && err.is_empty());
        out.clear();
        assert_eq!(
            execute(
                json_command(),
                &mut BadInput,
                &mut out,
                &mut err,
                || panic!("input failure must precede registry initialization")
            )
            .exit_code(),
            2
        );
        let report: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(report["diagnostics"][0]["code"], "invalid-utf8");
        assert!(err.is_empty());
    }
    #[test]
    fn module_dispatch_never_reads_source_even_when_the_registry_is_invalid() {
        struct NoSource;
        impl SourceReader for NoSource {
            fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
                panic!("source read in module dispatch")
            }
        }
        for command in [
            Command::ModulesList,
            Command::ModulesDescribe {
                name: "forensic.process.list".into(),
            },
            Command::ModulesDescribe {
                name: "bad.name".into(),
            },
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let called = std::cell::Cell::new(0);
            let outcome = execute(command, &mut NoSource, &mut out, &mut err, || {
                called.set(called.get() + 1);
                Err(Registry::from_json("{}").unwrap_err())
            });
            assert_eq!(called.get(), 1);
            assert_eq!(outcome, ProcessOutcome::CommandOrHostFailure);
            assert!(out.is_empty());
            assert_eq!(
                err,
                b"error[invalid-registry]: module registry is invalid\n"
            );
        }
    }
    #[test]
    fn both_parse_modes_remain_registry_independent_after_discovery_dispatch() {
        for emit_ast in [None, Some(AstFormat::Json)] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                execute(
                    Command::Parse {
                        path: "memory".into(),
                        emit_ast
                    },
                    &mut Memory,
                    &mut out,
                    &mut err,
                    || panic!("parse cannot initialise registry")
                ),
                ProcessOutcome::Success
            );
            assert!(!out.is_empty());
            assert!(err.is_empty());
        }
    }
    struct Memory;
    impl SourceReader for Memory {
        fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
            Ok(include_bytes!("../../../examples/triage.jky").to_vec())
        }
    }
    #[test]
    fn parse_never_initializes_registry_and_invalid_registry_never_yields_check_success() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let result = execute(
            Command::Parse {
                path: "memory".into(),
                emit_ast: None,
            },
            &mut Memory,
            &mut out,
            &mut err,
            || panic!("parse must not initialize registry"),
        );
        assert_eq!(result, ProcessOutcome::Success);
        out.clear();
        let result = execute(
            Command::Check {
                path: "memory".into(),
                format: DiagnosticFormat::Human,
            },
            &mut Memory,
            &mut out,
            &mut err,
            || Err(Registry::from_json("{}").unwrap_err()),
        );
        assert_eq!(result, ProcessOutcome::CommandOrHostFailure);
        assert!(out.is_empty());
        assert_eq!(
            err,
            b"error[invalid-registry]: module registry is invalid\n"
        );
    }
    #[test]
    fn both_explicit_check_formats_are_accepted() {
        assert_eq!(
            args_os(["check", "source", "--diagnostic-format", "human"].map(OsString::from)),
            Ok(Command::Check {
                path: "source".into(),
                format: DiagnosticFormat::Human,
            })
        );
        assert_eq!(
            args_os(["check", "source", "--diagnostic-format", "json"].map(OsString::from)),
            Ok(Command::Check {
                path: "source".into(),
                format: DiagnosticFormat::Json
            })
        );
        assert_eq!(
            args_os(["modules", "list"].map(OsString::from)),
            Ok(Command::ModulesList)
        );
    }

    #[test]
    fn synthetic_lab_registry_is_injected_only_through_the_private_test_seam() {
        struct LabSource(String);
        impl SourceReader for LabSource {
            fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
                Ok(self.0.as_bytes().to_vec())
            }
        }
        let registry = Registry::from_json(include_str!(
            "../../../forensic-lib/tests/fixtures/valid/consistency-lab-registry.json"
        ))
        .unwrap();
        let ordinary = include_str!("../../tests/fixtures/semantic/lab/indirect.jky");
        for opted_in in [false, true] {
            let text = if opted_in {
                ordinary.replace(
                    "target windows | ubuntu",
                    "target windows | ubuntu profile lab",
                )
            } else {
                ordinary.to_owned()
            };
            let mut out = Vec::new();
            let mut err = Vec::new();
            let outcome = execute(
                Command::Check {
                    path: "memory".into(),
                    format: DiagnosticFormat::Human,
                },
                &mut LabSource(text),
                &mut out,
                &mut err,
                || Ok(&registry),
            );
            assert_eq!(
                outcome,
                if opted_in {
                    ProcessOutcome::Success
                } else {
                    ProcessOutcome::SyntaxFailure
                }
            );
            assert!(out.is_empty());
            if opted_in {
                assert!(err.is_empty());
            } else {
                let err = String::from_utf8(err).unwrap();
                assert!(err.starts_with("error[lab-profile-required]:"));
                assert!(err.contains("--> memory:4:12"));
            }
        }
        assert_eq!(
            registry
                .lookup("forensic.lab.inspect")
                .unwrap()
                .availability,
            jocky_forensic::contracts::ImplementationAvailability::Unavailable
        );
    }
}
