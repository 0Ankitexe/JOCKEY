use super::{location::ReportLocation, metadata::Metadata};
use crate::{
    DiagnosticCategory, SourceFile, SourceMap, Span,
    semantic::{CheckFailure, CheckedProgram, ResourceKind, SemanticDiagnostic},
};
use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Valid,
    Invalid,
    Failure,
}
impl ReportStatus {
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Valid => 0,
            Self::Invalid => 1,
            Self::Failure => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportError {
    InvalidSpan,
    InvalidMetadata,
    ResourceLimit,
    Serialization,
}

/// Host failures contain only stable project text, never native error contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFailure {
    InvalidPathEncoding,
    InputNotFound,
    InputUnreadable,
    InvalidUtf8,
    InvalidRegistry,
    SourceLimit,
    ReportLimit,
    OutputFailure,
}
impl HostFailure {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidPathEncoding => "invalid-path-encoding",
            Self::InputNotFound => "input-not-found",
            Self::InputUnreadable => "input-unreadable",
            Self::InvalidUtf8 => "invalid-utf8",
            Self::InvalidRegistry => "invalid-registry",
            Self::SourceLimit | Self::ReportLimit => "resource-limit",
            Self::OutputFailure => "output-failure",
        }
    }
    fn message(self, file: Option<&str>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self {
            Self::InvalidPathEncoding => "input path must be valid UTF-8",
            Self::InputNotFound => "input file not found: ",
            Self::InputUnreadable => "input file is not readable: ",
            Self::InvalidUtf8 => "input file is not valid UTF-8: ",
            Self::InvalidRegistry => "module registry is invalid",
            Self::SourceLimit => "source bytes limit exceeded",
            Self::ReportLimit => "report bytes limit exceeded",
            Self::OutputFailure => "output could not be written",
        };
        f.write_str(prefix)?;
        if matches!(
            self,
            Self::InputNotFound | Self::InputUnreadable | Self::InvalidUtf8
        ) {
            super::location::write_filename(file.unwrap_or("<unknown>"), f)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) enum Message<'a> {
    Static(&'static str),
    Semantic(&'a SemanticDiagnostic),
    Resource(ResourceKind),
    Host(HostFailure, Option<&'a str>),
}
impl fmt::Display for Message<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static(text) => f.write_str(text),
            Self::Semantic(d) => f.write_str(&d.message()),
            Self::Resource(kind) => write!(f, "{} limit exceeded", kind.name()),
            Self::Host(error, file) => error.message(*file, f),
        }
    }
}
impl Serialize for Message<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
#[derive(Debug, Serialize)]
pub(super) struct ReportDiagnostic<'a> {
    pub severity: &'static str,
    pub code: &'static str,
    pub message: Message<'a>,
    pub location: Option<ReportLocation<'a>>,
    pub label: Option<&'static str>,
}
#[derive(Debug, Default, Serialize)]
pub(super) struct Truncated {
    pub errors: bool,
    pub warnings: bool,
}
#[derive(Debug, Serialize)]
pub(super) struct Document<'a> {
    pub schema_version: &'static str,
    pub status: ReportStatus,
    pub file: Option<&'a str>,
    pub diagnostics: Vec<ReportDiagnostic<'a>>,
    pub truncated: Truncated,
    pub metadata: Option<Metadata<'a>>,
}

/// Immutable report adapter. A valid report requires a complete CheckedProgram.
/// No public constructor or deserializer accepts unchecked success metadata.
#[derive(Debug)]
pub struct CheckReport<'a> {
    pub(super) document: Document<'a>,
}
impl<'a> CheckReport<'a> {
    pub fn from_checked(
        source: SourceFile<'a>,
        checked: &'a CheckedProgram,
    ) -> Result<Self, ReportError> {
        let map = SourceMap::new(source.text);
        let metadata = Metadata::checked(source, &map, checked)?;
        let diagnostics = semantic_diagnostics(source, &map, checked.warnings().items())?;
        if checked.warnings().has_errors() {
            return Err(ReportError::InvalidMetadata);
        }
        Ok(Self {
            document: Document {
                schema_version: "1.0.0",
                status: ReportStatus::Valid,
                file: Some(source.name),
                diagnostics,
                truncated: Truncated {
                    errors: false,
                    warnings: checked.warnings().warnings_truncated(),
                },
                metadata: Some(metadata),
            },
        })
    }
    pub fn from_failure(
        source: SourceFile<'a>,
        failure: &'a CheckFailure,
    ) -> Result<Self, ReportError> {
        let locationless = matches!(failure, CheckFailure::Resource(f) if f.span.is_none());
        if !locationless && source.text.len() > crate::semantic::limits::MAX_SOURCE_BYTES {
            return Err(ReportError::ResourceLimit);
        }
        let map = SourceMap::new(if locationless { "" } else { source.text });
        let mut document = Document {
            schema_version: "1.0.0",
            status: ReportStatus::Invalid,
            file: Some(source.name),
            diagnostics: Vec::new(),
            truncated: Truncated::default(),
            metadata: None,
        };
        match failure {
            CheckFailure::Semantic(set) => {
                if !set.has_errors() {
                    return Err(ReportError::InvalidMetadata);
                }
                document.diagnostics = semantic_diagnostics(source, &map, set.items())?;
                document.truncated = Truncated {
                    errors: set.errors_truncated(),
                    warnings: set.warnings_truncated(),
                };
            }
            CheckFailure::Syntax(set) => {
                // Resource failures supersede any source diagnostics, even for a
                // manually supplied legacy DiagnosticSet with mixed categories.
                let resource = set
                    .items()
                    .iter()
                    .find(|d| d.category == DiagnosticCategory::ResourceLimit);
                for d in set
                    .items()
                    .iter()
                    .filter(|d| {
                        resource.is_none() || d.category == DiagnosticCategory::ResourceLimit
                    })
                    .take(if resource.is_some() { 1 } else { 32 })
                {
                    document.diagnostics.push(ReportDiagnostic {
                        severity: "error",
                        code: d.category.as_str(),
                        message: Message::Static(d.message()),
                        location: Some(ReportLocation::new(source, &map, d.span, true)?),
                        label: Some(d.label()),
                    });
                }
                if resource.is_some() {
                    document.status = ReportStatus::Failure;
                } else {
                    document.truncated.errors = set.is_truncated();
                }
            }
            CheckFailure::Resource(failure) => {
                document.status = ReportStatus::Failure;
                document.diagnostics.push(ReportDiagnostic {
                    severity: "error",
                    code: "resource-limit",
                    message: Message::Resource(failure.kind),
                    location: location(source, &map, failure.span)?,
                    label: failure.span.map(|_| "resource limit exceeded"),
                });
            }
        }
        Ok(Self { document })
    }
    pub fn host_failure(file: Option<&'a str>, failure: HostFailure) -> Self {
        Self {
            document: Document {
                schema_version: "1.0.0",
                status: ReportStatus::Failure,
                file,
                diagnostics: vec![ReportDiagnostic {
                    severity: "error",
                    code: failure.code(),
                    message: Message::Host(failure, file),
                    location: None,
                    label: None,
                }],
                truncated: Truncated::default(),
                metadata: None,
            },
        }
    }
    pub const fn status(&self) -> ReportStatus {
        self.document.status
    }
    /// Serialize to a complete bounded buffer; no I/O or partial buffer escapes.
    pub fn to_pretty_json(&self) -> Result<Vec<u8>, ReportError> {
        super::json::serialize(&self.document)
    }
}
fn location<'a>(
    source: SourceFile<'a>,
    map: &SourceMap<'a>,
    span: Option<Span>,
) -> Result<Option<ReportLocation<'a>>, ReportError> {
    span.map(|span| ReportLocation::new(source, map, span, false))
        .transpose()
}
fn semantic_diagnostics<'a>(
    source: SourceFile<'a>,
    map: &SourceMap<'a>,
    diagnostics: &'a [SemanticDiagnostic],
) -> Result<Vec<ReportDiagnostic<'a>>, ReportError> {
    diagnostics
        .iter()
        .map(|d| {
            Ok(ReportDiagnostic {
                severity: d.severity().as_str(),
                code: d.code().as_str(),
                message: Message::Semantic(d),
                location: Some(ReportLocation::new(source, map, d.span, false)?),
                label: Some(d.label()),
            })
        })
        .collect()
}
