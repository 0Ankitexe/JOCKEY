use super::{
    budget::{Accounting, ExecutionLimits, FAILURE_RESERVE},
    failure::{ExecutionCode, ExecutionFailure},
};
use crate::{
    fixture::ValidatedFixture,
    json, limits,
    model::{Callee, IrDocument, Operation},
    value::Value,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Success { value: Value },
    Failure { error: ExecutionFailure },
}
#[derive(Debug, Serialize)]
struct Body {
    schema_version: &'static str,
    mode: &'static str,
    fixture_id: String,
    endpoint_id: String,
    platform: jocky_shared::compiler::Platform,
    limits: ExecutionLimits,
    accounting: Accounting,
    outcome: ExecutionOutcome,
}
/// Constructed only after a prepared invocation reaches a terminal outcome.
#[derive(Debug)]
pub struct ExecutionReport {
    body: Body,
    bytes: Result<Vec<u8>, ReportWriteFailure>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportWriteFailure;
impl std::fmt::Display for ReportWriteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("fixture report cannot be delivered")
    }
}
impl std::error::Error for ReportWriteFailure {}
impl ExecutionReport {
    pub fn outcome(&self) -> &ExecutionOutcome {
        &self.body.outcome
    }
    pub fn accounting(&self) -> &Accounting {
        &self.body.accounting
    }
    pub fn failure(&self) -> Option<&ExecutionFailure> {
        match &self.body.outcome {
            ExecutionOutcome::Failure { error } => Some(error),
            _ => None,
        }
    }
    pub fn to_json(&self) -> Result<Vec<u8>, ReportWriteFailure> {
        let original = self.bytes.as_ref().map_err(|_| ReportWriteFailure)?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(original.len())
            .map_err(|_| ReportWriteFailure)?;
        copy.extend_from_slice(original);
        Ok(copy)
    }
}
fn consistent(d: &IrDocument, f: &ValidatedFixture, b: &Body) -> bool {
    if b.accounting.instructions > b.limits.instructions
        || b.accounting.work > b.limits.work
        || b.accounting.peak_call_depth > b.limits.call_depth
        || b.accounting.peak_storage_bytes > limits::STORAGE
    {
        return false;
    }
    match &b.outcome {
        ExecutionOutcome::Success {
            value: Value::ForensicResult { value: r },
        } => crate::fixture::providers::context_valid(
            crate::value::wire::ForensicView {
                endpoint_id: &r.endpoint_id,
                observed_at: &r.observed_at,
                system: r.system.as_ref(),
                indicators: &r.indicators,
            },
            f,
        ),
        ExecutionOutcome::Success { .. } => false,
        ExecutionOutcome::Failure { error } => {
            let Some(origin) = error.origin() else {
                return matches!(error, ExecutionFailure::Interpreter { .. });
            };
            let instruction = d
                .functions
                .iter()
                .flat_map(|f| &f.regions)
                .flat_map(|r| &r.instructions)
                .find(|i| i.id == origin.instruction_id);
            let Some(i) = instruction else {
                return false;
            };
            if i.span != origin.span {
                return false;
            }
            if let ExecutionFailure::Provider { code, function, .. } = error {
                let Operation::Call {
                    callee: Callee::Contract { contract },
                    ..
                } = i.operation
                else {
                    return false;
                };
                let c = &d.contracts[contract];
                c.qualified_name() == *function && c.possible_errors.iter().any(|e| e.code == *code)
            } else {
                true
            }
        }
    }
}
pub(super) fn finish(
    d: &IrDocument,
    f: &ValidatedFixture,
    limits: ExecutionLimits,
    accounting: Accounting,
    outcome: ExecutionOutcome,
    reserve: Vec<u8>,
) -> ExecutionReport {
    finish_with_limit(
        d,
        f,
        limits,
        accounting,
        outcome,
        reserve,
        limits::FIXTURE_BYTES,
    )
}
fn finish_with_limit(
    d: &IrDocument,
    f: &ValidatedFixture,
    limits: ExecutionLimits,
    accounting: Accounting,
    outcome: ExecutionOutcome,
    reserve: Vec<u8>,
    maximum: usize,
) -> ExecutionReport {
    let mut body = Body {
        schema_version: "1.0.0",
        mode: "fixture",
        fixture_id: f.fixture_id().into(),
        endpoint_id: f.endpoint().endpoint_id.clone(),
        platform: f.endpoint().platform,
        limits,
        accounting,
        outcome,
    };
    if !consistent(d, f, &body) {
        body.outcome = ExecutionOutcome::Failure {
            error: ExecutionFailure::interpreter(ExecutionCode::InvalidValue, None),
        };
        // An invalid payload/origin can become a typed failure. Invalid counters
        // cannot be repaired without falsifying accounting; refuse delivery.
        if !consistent(d, f, &body) {
            return ExecutionReport {
                body,
                bytes: Err(ReportWriteFailure),
            };
        }
    }
    let buffer = json::serialize_bounded(&body, maximum).and_then(|bytes| {
        let raw = json::read_json(&bytes, maximum)?;
        json::validate_schema(json::Schema::Report, &raw)?;
        Ok(bytes)
    });
    let bytes = match buffer {
        Ok(bytes) => Ok(bytes),
        Err(_) => {
            body.outcome = ExecutionOutcome::Failure {
                error: ExecutionFailure::interpreter(ExecutionCode::OutputLimit, None),
            };
            json::serialize_reserved(&body, FAILURE_RESERVE.min(maximum), reserve)
                .map_err(|_| ReportWriteFailure)
        }
    };
    ExecutionReport { body, bytes }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_byte_boundary_counts_the_final_newline_and_preserves_accounting() {
        let limits = ExecutionLimits::default();
        let f = crate::decode_fixture(
            include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
            &limits,
        )
        .unwrap();
        let d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let make = |maximum| {
            finish_with_limit(
                &d,
                &f,
                limits,
                Accounting::default(),
                ExecutionOutcome::Success {
                    value: d.constants[0].clone(),
                },
                Vec::with_capacity(FAILURE_RESERVE),
                maximum,
            )
        };
        let reference = make(limits::FIXTURE_BYTES).to_json().unwrap();
        let exact = make(reference.len());
        assert!(exact.failure().is_none());
        assert_eq!(exact.to_json().unwrap(), reference);
        let above = make(reference.len() - 1);
        assert_eq!(above.failure().unwrap().code(), "EXEC_OUTPUT_LIMIT");
        assert_eq!(above.accounting(), exact.accounting());
        assert!(above.to_json().unwrap().len() < reference.len());
    }
    #[test]
    fn invalid_success_and_output_overflow_cannot_keep_success_status() {
        let f = crate::decode_fixture(
            include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
            &ExecutionLimits::default(),
        )
        .unwrap();
        let d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let r = finish(
            &d,
            &f,
            ExecutionLimits::default(),
            Accounting::default(),
            ExecutionOutcome::Success {
                value: Value::Bool { value: true },
            },
            Vec::with_capacity(FAILURE_RESERVE),
        );
        assert_eq!(r.failure().unwrap().code(), "EXEC_INVALID_VALUE");
        let r = finish_with_limit(
            &d,
            &f,
            ExecutionLimits::default(),
            Accounting::default(),
            ExecutionOutcome::Success {
                value: Value::Bool { value: true },
            },
            Vec::with_capacity(FAILURE_RESERVE),
            1,
        );
        assert_eq!(r.failure().unwrap().code(), "EXEC_OUTPUT_LIMIT");
        assert!(r.to_json().is_err());
    }

    #[test]
    fn inconsistent_counters_are_not_silently_repaired_or_delivered() {
        let f = crate::decode_fixture(
            include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
            &ExecutionLimits::default(),
        )
        .unwrap();
        let d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let r = finish(
            &d,
            &f,
            ExecutionLimits::default(),
            Accounting {
                instructions: 100_001,
                ..Accounting::default()
            },
            ExecutionOutcome::Failure {
                error: ExecutionFailure::interpreter(ExecutionCode::WorkLimit, None),
            },
            Vec::with_capacity(FAILURE_RESERVE),
        );
        assert_eq!(r.accounting().instructions, 100_001);
        assert!(r.to_json().is_err());
    }

    #[test]
    fn deliverable_overflow_replaces_success_with_reserved_failure_document() {
        let f = crate::decode_fixture(
            include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
            &ExecutionLimits::default(),
        )
        .unwrap();
        let d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let mut value = d.constants[0].clone();
        let Value::ForensicResult { value: result } = &mut value else {
            unreachable!()
        };
        result.system.as_mut().unwrap().hostname = "x".repeat(4096);
        let r = finish_with_limit(
            &d,
            &f,
            ExecutionLimits::default(),
            Accounting::default(),
            ExecutionOutcome::Success { value },
            Vec::with_capacity(FAILURE_RESERVE),
            2048,
        );
        assert_eq!(r.failure().unwrap().code(), "EXEC_OUTPUT_LIMIT");
        assert!(r.failure().unwrap().is_resource());
        let bytes = r.to_json().unwrap();
        assert!(bytes.len() <= 2048 && bytes.ends_with(b"\n"));
        let raw = json::read_json(&bytes, 2048).unwrap();
        json::validate_schema(json::Schema::Report, &raw).unwrap();
        assert!(raw["outcome"].get("value").is_none());
    }

    #[test]
    fn forged_origin_and_undeclared_provider_error_cannot_be_delivered_as_provider_failure() {
        let f = crate::decode_fixture(
            include_bytes!("../../tests/fixtures/valid/ubuntu/fixture.json"),
            &ExecutionLimits::default(),
        )
        .unwrap();
        let d = crate::decode_ir(include_bytes!(
            "../../../language/tests/fixtures/ir/golden/triage.ir.json"
        ))
        .unwrap();
        let i = &d.functions[0].regions[0].instructions[0];
        let good = ExecutionFailure::Provider {
            code: jocky_forensic::contracts::PossibleErrorCode::PermissionDenied,
            function: "forensic.system.profile".into(),
            origin: Some(i.into()),
        };
        for changed in 0..5 {
            let mut error = good.clone();
            let ExecutionFailure::Provider {
                code,
                function,
                origin,
            } = &mut error
            else {
                unreachable!()
            };
            match changed {
                0 => {}
                1 => *code = jocky_forensic::contracts::PossibleErrorCode::NotImplemented,
                2 => *function = "forensic.process.list".into(),
                3 => *origin = None,
                _ => origin.as_mut().unwrap().span = None,
            }
            let r = finish(
                &d,
                &f,
                ExecutionLimits::default(),
                Accounting::default(),
                ExecutionOutcome::Failure { error },
                Vec::with_capacity(FAILURE_RESERVE),
            );
            assert_eq!(
                r.failure().unwrap().code(),
                if changed == 0 {
                    "PermissionDenied"
                } else {
                    "EXEC_INVALID_VALUE"
                }
            );
            assert!(r.to_json().is_ok());
        }
    }
}
