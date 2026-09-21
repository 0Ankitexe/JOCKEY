//! Closed synthetic input. It cannot contain callbacks or additional input paths.
pub(crate) mod providers;
pub(crate) mod validate;
use crate::{
    interpreter::budget::ExecutionLimits,
    json, limits,
    value::{Endpoint, Value},
};
use jocky_forensic::contracts::PossibleErrorCode;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureFailure {
    Json,
    Schema,
    Version,
    Context,
    Resource,
    Target,
    Registry,
}
impl std::fmt::Display for FixtureFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Json => "invalid complete fixture JSON",
            Self::Schema => "invalid fixture shape",
            Self::Version => "unsupported fixture version",
            Self::Context => "inconsistent synthetic fixture context",
            Self::Resource => "fixture preparation resource limit exceeded",
            Self::Target => "fixture platform does not match selected targets",
            Self::Registry => "fixture contract registry unavailable",
        })
    }
}
impl std::error::Error for FixtureFailure {}
impl From<crate::diagnostic::Failure> for FixtureFailure {
    fn from(e: crate::diagnostic::Failure) -> Self {
        match e.code() {
            crate::DiagnosticCode::Resource => Self::Resource,
            crate::DiagnosticCode::Json => Self::Json,
            _ => Self::Schema,
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bundle {
    pub schema_version: String,
    pub fixture_id: String,
    pub endpoint: Endpoint,
    pub observed_at: String,
    pub providers: Providers,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Providers {
    #[serde(rename = "forensic.system.profile")]
    pub system: Outcome,
    #[serde(rename = "forensic.process.list")]
    pub processes: Outcome,
    #[serde(rename = "forensic.network.connections")]
    pub connections: Outcome,
    #[serde(rename = "forensic.indicator.correlate")]
    pub correlation: Correlation,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Outcome {
    Ok { value: Value },
    Error { code: PossibleErrorCode },
}
impl Outcome {
    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::Ok { value } => Some(value),
            Self::Error { .. } => None,
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Correlation {
    pub expected: Expected,
    pub outcome: Outcome,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Expected {
    pub system: Value,
    pub processes: Value,
    pub connections: Value,
}
/// Validated immutable data, not an executable provider.
/// ```compile_fail
/// fn forge() -> jocky_ir::ValidatedFixture { jocky_ir::ValidatedFixture {} }
/// ```
#[derive(Debug)]
pub struct ValidatedFixture {
    pub(crate) bundle: Bundle,
    pub(crate) records: std::collections::BTreeSet<String>,
}
impl ValidatedFixture {
    pub fn endpoint(&self) -> &Endpoint {
        &self.bundle.endpoint
    }
    pub fn fixture_id(&self) -> &str {
        &self.bundle.fixture_id
    }
    pub fn observed_at(&self) -> &str {
        &self.bundle.observed_at
    }
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        values(&self.bundle)
    }
}
pub(crate) fn values(bundle: &Bundle) -> impl Iterator<Item = &Value> {
    let p = &bundle.providers;
    [
        p.system.value(),
        p.processes.value(),
        p.connections.value(),
        p.correlation.outcome.value(),
        Some(&p.correlation.expected.system),
        Some(&p.correlation.expected.processes),
        Some(&p.correlation.expected.connections),
    ]
    .into_iter()
    .flatten()
}
pub fn decode_fixture(
    bytes: &[u8],
    limits: &ExecutionLimits,
) -> Result<ValidatedFixture, FixtureFailure> {
    let raw = json::read_json(bytes, limits::FIXTURE_BYTES)?;
    if raw
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|v| v != "1.0.0")
    {
        return Err(FixtureFailure::Version);
    }
    validate::raw_collections(&raw, limits.collection())?;
    json::validate_schema(json::Schema::Bundle, &raw)?;
    let bundle: Bundle = serde_json::from_value(raw).map_err(|_| FixtureFailure::Schema)?;
    let records = validate::bundle(&bundle, limits)?;
    Ok(ValidatedFixture { bundle, records })
}
