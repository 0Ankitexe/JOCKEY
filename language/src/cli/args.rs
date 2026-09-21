use std::ffi::OsString;

pub const USAGE: &str = concat!(
    "Usage:\n",
    "  jockyc parse <file>\n",
    "  jockyc parse <file> --emit-ast json\n",
    "  jockyc check <file>\n",
    "  jockyc check <file> --diagnostic-format human|json\n",
    "  jockyc modules list\n",
    "  jockyc modules describe <qualified-name>\n",
    "  jockyc build-ir <file> --output <file.json>\n",
    "  jockyc verify-ir <file.json>\n",
    "  jockyc run-fixture <file> --fixture <fixture-directory>\n",
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AstFormat {
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticFormat {
    Human,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    RunFixture {
        path: String,
        directory: String,
    },
    VerifyIr {
        path: String,
    },
    BuildIr {
        path: String,
        output: String,
    },
    Parse {
        path: String,
        emit_ast: Option<AstFormat>,
    },
    Check {
        path: String,
        format: DiagnosticFormat,
    },
    ModulesList,
    ModulesDescribe {
        name: String,
    },
}

impl Command {
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Parse { path, .. }
            | Self::RunFixture { path, .. }
            | Self::Check { path, .. }
            | Self::BuildIr { path, .. }
            | Self::VerifyIr { path } => Some(path),
            Self::ModulesList | Self::ModulesDescribe { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliError {
    InvalidCommand,
    InvalidPathEncoding,
    InputNotFound,
    InputUnreadable,
    InvalidUtf8,
    ResourceLimit,
    SourceTooLarge,
    ReportTooLarge,
    InvalidRegistry,
    ModuleNotFound,
    OutputFailure,
}

impl CliError {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::InvalidCommand => "invalid-command",
            Self::InvalidPathEncoding => "invalid-path-encoding",
            Self::InputNotFound => "input-not-found",
            Self::InputUnreadable => "input-unreadable",
            Self::InvalidUtf8 => "invalid-utf8",
            Self::ResourceLimit | Self::SourceTooLarge | Self::ReportTooLarge => "resource-limit",
            Self::InvalidRegistry => "invalid-registry",
            Self::ModuleNotFound => "module-not-found",
            Self::OutputFailure => "output-failure",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    Success,
    SyntaxFailure,
    CommandOrHostFailure,
}

impl ProcessOutcome {
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::SyntaxFailure => 1,
            Self::CommandOrHostFailure => 2,
        }
    }
}

/// Parse arguments after the executable name into the implemented command set.
pub fn args_os<I>(arguments: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, path, option, directory] if command == "run-fixture" && option == "--fixture" => {
            let path = utf8_path(path)?;
            let directory = utf8_path(directory)?;
            if path.is_empty()
                || directory.is_empty()
                || path.starts_with('-')
                || directory.starts_with('-')
            {
                return Err(CliError::InvalidCommand);
            }
            Ok(Command::RunFixture { path, directory })
        }
        [command, path] if command == "verify-ir" => {
            let path = utf8_path(path)?;
            if path.is_empty() || path.starts_with('-') {
                return Err(CliError::InvalidCommand);
            }
            Ok(Command::VerifyIr { path })
        }
        [command, path, option, output] if command == "build-ir" && option == "--output" => {
            let path = utf8_path(path)?;
            let output = utf8_path(output)?;
            if path.is_empty()
                || output.is_empty()
                || path.starts_with('-')
                || output.starts_with('-')
            {
                return Err(CliError::InvalidCommand);
            }
            Ok(Command::BuildIr { path, output })
        }
        [command, action] if command == "modules" && action == "list" => Ok(Command::ModulesList),
        [command, action, name] if command == "modules" && action == "describe" => {
            let name = name.to_str().ok_or(CliError::InvalidCommand)?;
            if name.starts_with('-') {
                return Err(CliError::InvalidCommand);
            }
            Ok(Command::ModulesDescribe {
                name: name.to_owned(),
            })
        }
        [command, path] if command == "parse" => Ok(Command::Parse {
            path: utf8_path(path)?,
            emit_ast: None,
        }),
        [command, path, option, format]
            if command == "parse" && option == "--emit-ast" && format == "json" =>
        {
            Ok(Command::Parse {
                path: utf8_path(path)?,
                emit_ast: Some(AstFormat::Json),
            })
        }
        [command, path] if command == "check" => Ok(Command::Check {
            path: utf8_path(path)?,
            format: DiagnosticFormat::Human,
        }),
        [command, path, option, format]
            if command == "check"
                && option == "--diagnostic-format"
                && (format == "human" || format == "json") =>
        {
            Ok(Command::Check {
                path: utf8_path(path)?,
                format: if format == "json" {
                    DiagnosticFormat::Json
                } else {
                    DiagnosticFormat::Human
                },
            })
        }
        _ => Err(CliError::InvalidCommand),
    }
}

// Only an exact supported shape selects JSON before the path's UTF-8 check.
pub(super) fn is_json_check(arguments: &[OsString]) -> bool {
    matches!(arguments, [command, _, option, format] if command == "check" && option == "--diagnostic-format" && format == "json")
}

fn utf8_path(path: &OsString) -> Result<String, CliError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(CliError::InvalidPathEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_shape_requires_explicit_source_and_directory_only() {
        let command = args_os(
            ["run-fixture", "source.jky", "--fixture", "fixtures/ubuntu"].map(OsString::from),
        )
        .unwrap();
        assert_eq!(
            command,
            Command::RunFixture {
                path: "source.jky".into(),
                directory: "fixtures/ubuntu".into()
            }
        );
        assert_eq!(command.path(), Some("source.jky"));
        for args in [
            ["run-fixture", "", "--fixture", "dir"],
            ["run-fixture", "source", "--fixture", ""],
            ["run-fixture", "source", "--fixture", "--live"],
            ["run-fixture", "--source", "--fixture", "dir"],
            ["run-fixture", "source", "--provider", "dir"],
        ] {
            assert_eq!(
                args_os(args.map(OsString::from)),
                Err(CliError::InvalidCommand)
            );
        }
    }
    #[test]
    fn verify_shape_has_exactly_one_explicit_artifact() {
        let command = args_os(["verify-ir", "artifact.json"].map(OsString::from)).unwrap();
        assert_eq!(
            command,
            Command::VerifyIr {
                path: "artifact.json".into()
            }
        );
        assert_eq!(command.path(), Some("artifact.json"));
        for args in [
            vec!["verify-ir"],
            vec!["verify-ir", ""],
            vec!["verify-ir", "--help"],
            vec!["verify-ir", "a", "b"],
        ] {
            assert_eq!(
                args_os(args.into_iter().map(OsString::from)),
                Err(CliError::InvalidCommand)
            );
        }
    }
    #[test]
    fn module_commands_have_no_source_path_and_accept_only_exact_shapes() {
        let list = args_os(["modules", "list"].map(OsString::from)).unwrap();
        assert_eq!(list, Command::ModulesList);
        assert_eq!(list.path(), None);
        let describe =
            args_os(["modules", "describe", "forensic.process.list"].map(OsString::from)).unwrap();
        assert_eq!(
            describe,
            Command::ModulesDescribe {
                name: "forensic.process.list".into()
            }
        );
        assert_eq!(describe.path(), None);
        for operation in ["parse", "check"] {
            assert_eq!(
                args_os([operation, "a.jky"].map(OsString::from))
                    .unwrap()
                    .path(),
                Some("a.jky")
            );
        }
        for invalid in [
            vec!["modules"],
            vec!["modules", "describe"],
            vec!["modules", "list", "extra"],
            vec!["modules", "describe", "name", "extra"],
            vec!["modules", "describe", "--help"],
            vec!["modules", "--registry", "file"],
            vec!["modules", "name", "describe"],
        ] {
            assert_eq!(
                args_os(invalid.into_iter().map(OsString::from)),
                Err(CliError::InvalidCommand)
            );
        }
    }
    #[test]
    #[cfg(unix)]
    fn non_utf8_module_name_is_a_command_error_without_lossy_decoding() {
        use std::os::unix::ffi::OsStringExt;
        assert_eq!(
            args_os([
                OsString::from("modules"),
                OsString::from("describe"),
                OsString::from_vec(vec![0xff])
            ]),
            Err(CliError::InvalidCommand)
        );
    }
}
