use jocky_language::cli::{ProcessOutcome, SourceReader, USAGE, run_with_reader};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

// Isolated repository-local working directory; never depend on a cwd catalogue.
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/module-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        for _ in 0..100 {
            let path = parent.join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create isolated test directory: {e}"),
            }
        }
        panic!("cannot allocate test directory");
    }
    fn binary(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_jockyc"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir(&self.0).expect("directory stays empty: discovery writes no files");
    }
}

#[test]
fn binary_discovers_embedded_catalogue_outside_the_source_tree() {
    let cwd = Workspace::new();
    let output = cwd.binary(&["modules", "list"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let expected = [
        "forensic.driver.list",
        "forensic.event.list",
        "forensic.event.ubuntu_journal",
        "forensic.event.windows_log",
        "forensic.file.inspect",
        "forensic.indicator.correlate",
        "forensic.network.connections",
        "forensic.persistence.list",
        "forensic.process.list",
        "forensic.system.profile",
    ];
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected
            .iter()
            .map(|name| format!("{name} 0.1.0 unavailable\n"))
            .collect::<String>()
    );
    for name in expected {
        let result = cwd.binary(&["modules", "describe", name]);
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stderr.is_empty());
        let text = String::from_utf8(result.stdout).unwrap();
        let keys = text
            .lines()
            .map(|line| line.split_once(": ").unwrap().0)
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "module",
                "name",
                "version",
                "parameters",
                "result",
                "supported_platforms",
                "required_privilege",
                "capability",
                "read_only",
                "lab_only",
                "possible_errors",
                "availability"
            ]
        );
        assert!(text.ends_with("availability: unavailable\n"));
        assert!(text.contains("read_only: true\nlab_only: false\n"));
        if name.ends_with("windows_log") {
            assert!(text.contains("supported_platforms: windows\n"));
        }
        if name.ends_with("ubuntu_journal") {
            assert!(text.contains("supported_platforms: ubuntu\n"));
        }
    }
}

#[test]
fn unknown_malformed_and_compiler_only_names_have_typed_failures() {
    let cwd = Workspace::new();
    for (name, escaped) in [
        ("forensic.missing.function", "forensic.missing.function"),
        ("forensic_result", "forensic_result"),
        ("forensic..list", "forensic..list"),
        ("bad\n\t\u{1b}", "bad\\n\\t\\u{001B}"),
        ("", ""),
    ] {
        let output = cwd.binary(&["modules", "describe", name]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("error[module-not-found]: module function not found: {escaped}\n")
        );
    }
}

#[test]
fn invalid_module_argument_shapes_return_usage_without_guessing() {
    let cwd = Workspace::new();
    for args in [
        vec!["modules"],
        vec!["modules", "describe"],
        vec!["modules", "list", "extra"],
        vec!["modules", "describe", "forensic.process.list", "extra"],
        vec!["modules", "--list"],
        vec!["modules", "describe", "--registry"],
        vec!["modules", "list", "--registry", "file.json"],
        vec!["describe", "modules", "forensic.process.list"],
        vec![
            "modules",
            "describe",
            "forensic.process.list",
            "--diagnostic-format",
            "json",
        ],
    ] {
        let output = cwd.binary(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, USAGE.as_bytes());
    }
}

struct NoSource;
impl SourceReader for NoSource {
    fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
        panic!("module commands must not read source")
    }
    fn read_bounded(
        &mut self,
        _: &Path,
        _: usize,
    ) -> Result<Vec<u8>, jocky_language::cli::ReadFailure> {
        panic!("no bounded source read either")
    }
}
struct FailedWriter {
    flush_only: bool,
}
impl Write for FailedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.flush_only {
            Ok(bytes.len())
        } else {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}

#[test]
fn discovery_never_uses_source_reader_and_write_or_flush_failures_exit_two() {
    for args in [
        vec!["modules", "list"],
        vec!["modules", "describe", "forensic.process.list"],
        vec!["modules", "describe", "missing.name"],
        vec!["modules", "list", "extra"],
    ] {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let outcome = run_with_reader(
            args.iter().map(OsString::from),
            &mut NoSource,
            &mut out,
            &mut err,
        );
        assert_eq!(
            outcome,
            if args.contains(&"missing.name") || args.contains(&"extra") {
                ProcessOutcome::CommandOrHostFailure
            } else {
                ProcessOutcome::Success
            }
        );
    }
    for args in [
        vec!["modules", "list"],
        vec!["modules", "describe", "forensic.process.list"],
    ] {
        for flush_only in [false, true] {
            let mut err = Vec::new();
            let result = run_with_reader(
                args.iter().map(OsString::from),
                &mut NoSource,
                &mut FailedWriter { flush_only },
                &mut err,
            );
            assert_eq!(result, ProcessOutcome::CommandOrHostFailure);
            assert_eq!(err, b"error[output-failure]: output could not be written\n");
        }
    }
}
