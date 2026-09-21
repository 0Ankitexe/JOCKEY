mod support;
use jocky_language::{
    cli::{SourceReader, run_with_reader},
    semantic::limits::MAX_REPORT_BYTES,
};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::Path,
};

struct Memory(Vec<u8>);
impl SourceReader for Memory {
    fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}
fn run(bytes: Vec<u8>, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    run_with_reader(
        ["check", "memory", "--diagnostic-format", "json"].map(OsString::from),
        &mut Memory(bytes),
        out,
        err,
    )
    .exit_code()
}
#[test]
fn report_overflow_replaces_valid_or_invalid_partial_data_with_one_small_failure() {
    for error in ["", "if 1 {}"] {
        let body = (0..40)
            .map(|i| format!("let unused{i} = 1 "))
            .collect::<String>();
        let text = format!(
            "module m target ubuntu fn main(target: endpoint) -> forensic_result {{ let padding = \"{}\" {error} {body} return forensic.system.profile(target) }} run main on selected_endpoints",
            "x".repeat(500_000)
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(text.into_bytes(), &mut out, &mut err), 2);
        assert!(out.len() < 1024 && out.len() < MAX_REPORT_BYTES && err.is_empty());
        let value = support::assert_report(&out);
        assert_eq!(value["status"], "failure");
        assert_eq!(value["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(value["diagnostics"][0]["code"], "resource-limit");
        assert!(value["metadata"].is_null());
    }
}
struct BrokenOutput {
    bytes: Vec<u8>,
    flush_only: bool,
}
impl Write for BrokenOutput {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if !self.flush_only && !self.bytes.is_empty() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let count = if self.flush_only {
            input.len()
        } else {
            input.len().min(17)
        };
        self.bytes.extend_from_slice(&input[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}
#[test]
fn partial_writes_and_flush_failures_never_become_success_or_a_second_document() {
    for flush_only in [false, true] {
        let mut out = BrokenOutput {
            bytes: Vec::new(),
            flush_only,
        };
        let mut err = Vec::new();
        assert_eq!(
            run(
                include_bytes!("../../examples/triage.jky").to_vec(),
                &mut out,
                &mut err
            ),
            2
        );
        assert_eq!(err, b"error[output-failure]: output could not be written\n");
        if !flush_only {
            assert_eq!(out.bytes.len(), 17);
            assert!(serde_json::from_slice::<serde_json::Value>(&out.bytes).is_err());
        } else {
            support::assert_report(&out.bytes);
        }
    }
}

#[test]
fn file_reader_bounds_allocation_at_boundary_and_check_uses_bounded_reader() {
    use jocky_language::cli::{FileSourceReader, ReadFailure};
    let dir = support::root().join("target/report-reader-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("input-{}.bin", std::process::id()));
    let file = std::fs::File::create_new(&path).unwrap();
    file.set_len(4_194_304).unwrap();
    let bytes = FileSourceReader.read_bounded(&path, 4_194_304).unwrap();
    assert_eq!(bytes.len(), 4_194_304);
    assert!(bytes.capacity() <= 4_194_305);
    file.set_len(4_194_305).unwrap();
    assert_eq!(
        FileSourceReader.read_bounded(&path, 4_194_304),
        Err(ReadFailure::ResourceLimit)
    );
    assert_eq!(
        FileSourceReader.read_bounded(&path, usize::MAX),
        Err(ReadFailure::ResourceLimit)
    );
    drop(file);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        FileSourceReader.read_bounded(&path, 1),
        Err(ReadFailure::Input(io::ErrorKind::NotFound))
    );
    struct BoundedOnly;
    impl SourceReader for BoundedOnly {
        fn read(&mut self, _: &Path) -> io::Result<Vec<u8>> {
            panic!("check must not use unbounded read")
        }
        fn read_bounded(&mut self, _: &Path, limit: usize) -> Result<Vec<u8>, ReadFailure> {
            assert_eq!(limit, 4_194_304);
            Err(ReadFailure::ResourceLimit)
        }
    }
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let outcome = run_with_reader(
        ["check", "fake", "--diagnostic-format", "json"].map(OsString::from),
        &mut BoundedOnly,
        &mut out,
        &mut err,
    );
    assert_eq!(outcome.exit_code(), 2);
    support::assert_report(&out);
    assert!(err.is_empty());
}
