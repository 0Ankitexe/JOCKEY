//! Pure, bounded human and JSON check reports; never execution results.
mod human;
mod json;
mod legacy;
mod location;
mod metadata;
mod model;
pub use human::{RenderError, render_failure, render_semantic};
pub(crate) use human::{render_compiler_diagnostic, render_fixture_diagnostic};
pub use model::{CheckReport, HostFailure, ReportError, ReportStatus};
