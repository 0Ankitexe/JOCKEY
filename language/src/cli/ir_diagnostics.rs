use super::{
    ProcessOutcome,
    ir_commands::{host, rejection},
};
use crate::{escape_filename, report::RenderError};
use jocky_ir::{DiagnosticCode, VerifyFailure};
use std::io::Write;

fn render(label: &str, failure: &VerifyFailure) -> Result<String, RenderError> {
    if label.len() > jocky_ir::limits::LABEL_BYTES {
        return Err(RenderError::ResourceLimit);
    }
    let mut output = String::new();
    for diagnostic in failure.diagnostics() {
        output.push_str(&format!(
            "error[{}]: {}\n  --> {}\n",
            diagnostic.code.as_str(),
            diagnostic.message(),
            escape_filename(label)
        ));
        if let Some(pointer) = &diagnostic.pointer {
            output.push_str(&format!("  IR pointer: {}\n", escape_filename(pointer)));
        }
        if let Some(id) = &diagnostic.instruction_id {
            output.push_str(&format!("  instruction: {}\n", escape_filename(id)));
        }
        if let Some(span) = diagnostic.span {
            output.push_str(&format!("  source bytes: {}..{}\n", span.start, span.end));
        }
    }
    if failure.truncated() {
        output.push_str("note: additional IR diagnostics suppressed after 20 errors\n");
    }
    Ok(output)
}

pub(super) fn reject(
    label: &str,
    failure: &VerifyFailure,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    match failure.code() {
        DiagnosticCode::Resource => host("V3_RESOURCE", stderr),
        DiagnosticCode::Version => host("V3_VERSION", stderr),
        _ => rejection(
            render(label, failure),
            ProcessOutcome::SyntaxFailure,
            stderr,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_caps_orders_and_escapes_without_embedded_source_reads() {
        let mut d = jocky_ir::decode_ir(include_bytes!(
            "../../../ir/tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        for index in 1..30 {
            let mut f = d.functions[0].clone();
            f.name = format!("f{index}");
            d.functions.push(f);
        }
        let failure = jocky_ir::verify(d, jocky_forensic::contracts::builtin_registry().unwrap())
            .unwrap_err();
        assert_eq!(failure.diagnostics().len(), 20);
        assert!(failure.truncated());
        let text = render("label\n\u{1b}.json", &failure).unwrap();
        assert!(text.contains("label\\n\\u{001B}.json"));
        assert!(!text.contains("1 |"));
        assert!(text.ends_with("note: additional IR diagnostics suppressed after 20 errors\n"));
        assert!(text.find("/functions/2/").unwrap() < text.find("/functions/10/").unwrap());
        for _ in 0..10 {
            assert_eq!(render("label\n\u{1b}.json", &failure).unwrap(), text);
        }
        assert!(render(&"x".repeat(jocky_ir::limits::LABEL_BYTES + 1), &failure).is_err());
    }
    #[test]
    fn diagnostic_write_or_flush_failure_overrides_rejection() {
        struct Broken(bool);
        impl Write for Broken {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self.0 {
                    Err(std::io::ErrorKind::BrokenPipe.into())
                } else {
                    Ok(b.len())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        let failure = jocky_ir::decode_ir(b"{").unwrap_err();
        for fail_write in [false, true] {
            assert_eq!(
                reject("memory", &failure, &mut Broken(fail_write)),
                ProcessOutcome::CommandOrHostFailure
            );
        }
    }
}
