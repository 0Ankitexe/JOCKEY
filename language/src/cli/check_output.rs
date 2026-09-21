use super::{CliError, ProcessOutcome, execute::finish_cli_error};
use crate::report::{CheckReport, HostFailure, ReportError, ReportStatus};
use std::io::Write;

pub(super) fn host(
    failure: HostFailure,
    file: Option<&str>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    emit(
        Ok(CheckReport::host_failure(file, failure)),
        file,
        stdout,
        stderr,
    )
}
pub(super) fn emit(
    report: Result<CheckReport<'_>, ReportError>,
    file: Option<&str>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ProcessOutcome {
    emit_with(report, file, stdout, stderr, |report| {
        report.to_pretty_json()
    })
}
fn emit_with<F>(
    report: Result<CheckReport<'_>, ReportError>,
    file: Option<&str>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    serialize: F,
) -> ProcessOutcome
where
    F: FnOnce(&CheckReport<'_>) -> Result<Vec<u8>, ReportError>,
{
    let encoded =
        report.and_then(|report| serialize(&report).map(|bytes| (report.status(), bytes)));
    let (status, bytes) = match encoded {
        Ok(value) => value,
        Err(error) => {
            let failure = if error == ReportError::ResourceLimit {
                HostFailure::ReportLimit
            } else {
                HostFailure::OutputFailure
            };
            match CheckReport::host_failure(file, failure).to_pretty_json() {
                Ok(bytes) => (ReportStatus::Failure, bytes),
                Err(_) => return finish_cli_error(CliError::OutputFailure, None, stderr),
            }
        }
    };
    // Never retry stdout or append a second JSON document after any write began.
    if stdout.write_all(&bytes).is_err() || stdout.flush().is_err() {
        return finish_cli_error(CliError::OutputFailure, None, stderr);
    }
    match status {
        ReportStatus::Valid => ProcessOutcome::Success,
        ReportStatus::Invalid => ProcessOutcome::SyntaxFailure,
        ReportStatus::Failure => ProcessOutcome::CommandOrHostFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_construction_serialization_and_resource_errors_emit_only_failure() {
        for error in [
            ReportError::InvalidSpan,
            ReportError::InvalidMetadata,
            ReportError::Serialization,
            ReportError::ResourceLimit,
        ] {
            for construction in [false, true] {
                let report = if construction {
                    Err(error)
                } else {
                    Ok(CheckReport::host_failure(
                        Some("memory"),
                        HostFailure::InputNotFound,
                    ))
                };
                let (mut out, mut err) = (Vec::new(), Vec::new());
                let outcome = emit_with(report, Some("memory"), &mut out, &mut err, |_| Err(error));
                assert_eq!(outcome.exit_code(), 2);
                assert!(err.is_empty());
                let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
                assert_eq!(value["status"], "failure");
                assert!(value["metadata"].is_null());
                assert_eq!(
                    value["diagnostics"][0]["code"],
                    if error == ReportError::ResourceLimit {
                        "resource-limit"
                    } else {
                        "output-failure"
                    }
                );
            }
        }
    }
}
