//! Typed, deterministic compiler IR. This crate has no host-operation interface.

#![forbid(unsafe_code)]

mod diagnostic;
mod fixture;
pub mod identity;
mod interpreter;
mod json;
pub mod limits;
pub mod model;
pub mod value;
mod verify;

pub use diagnostic::{Diagnostic, DiagnosticCode, IrReadFailure, IrWriteFailure, VerifyFailure};
pub use fixture::{FixtureFailure, ValidatedFixture, decode_fixture};
pub use interpreter::{
    Accounting, DiagnosticOrigin, ExecutionCode, ExecutionFailure, ExecutionLimits,
    ExecutionOutcome, ExecutionReport, LimitConfigurationFailure, PreparedInvocation,
    ReportWriteFailure, execute, prepare,
};
pub use json::decode_ir;
pub use model::IrDocument;
pub use verify::{VerifiedProgram, verify};
