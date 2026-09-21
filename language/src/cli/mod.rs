//! Typed CLI and explicit-file adapters; execution is synthetic-fixture-only.
mod args;
mod check_output;
mod execute;
pub mod files;
mod fixture_commands;
mod ir_commands;
mod ir_diagnostics;
mod modules;
pub use args::{AstFormat, CliError, Command, DiagnosticFormat, ProcessOutcome, USAGE, args_os};
pub use execute::{
    FileSourceReader, ReadFailure, SourceReader, render_cli_error, run, run_with_reader,
};

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::io;
    use std::path::Path;

    use super::{
        AstFormat, CliError, Command, ProcessOutcome, SourceReader, USAGE, args_os, run_with_reader,
    };

    struct MemoryReader(Result<Vec<u8>, io::ErrorKind>);

    impl SourceReader for MemoryReader {
        fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
            match &self.0 {
                Ok(bytes) => Ok(bytes.clone()),
                Err(kind) => Err(io::Error::from(*kind)),
            }
        }
    }

    fn argv(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn keeps_the_original_three_command_shapes() {
        assert_eq!(
            args_os(argv(&["parse", "a.jky"])),
            Ok(Command::Parse {
                path: "a.jky".to_owned(),
                emit_ast: None,
            })
        );
        assert_eq!(
            args_os(argv(&["parse", "a.jky", "--emit-ast", "json"])),
            Ok(Command::Parse {
                path: "a.jky".to_owned(),
                emit_ast: Some(AstFormat::Json),
            })
        );
        assert_eq!(
            args_os(argv(&["check", "a.jky"])),
            Ok(Command::Check {
                path: "a.jky".to_owned(),
                format: super::DiagnosticFormat::Human,
            })
        );
    }

    #[test]
    fn rejects_all_other_argument_shapes_with_fixed_usage() {
        for invalid in [
            argv(&[]),
            argv(&["parse"]),
            argv(&["check", "a", "extra"]),
            argv(&["parse", "a", "json", "--emit-ast"]),
            argv(&["run", "a"]),
        ] {
            assert_eq!(args_os(invalid), Err(CliError::InvalidCommand));
        }
        assert!(USAGE.ends_with('\n'));
        assert!(USAGE.ends_with("  jockyc run-fixture <file> --fixture <fixture-directory>\n"));
    }

    #[test]
    fn injected_reader_keeps_input_errors_stable() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let outcome = run_with_reader(
            argv(&["check", "missing.jky"]),
            &mut MemoryReader(Err(io::ErrorKind::NotFound)),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(outcome, ProcessOutcome::CommandOrHostFailure);
        assert!(stdout.is_empty());
        assert_eq!(
            stderr,
            b"error[input-not-found]: input file not found: missing.jky\n"
        );
    }
}
