//! Explicit profile envelope; the original Program and AST document stay unchanged.
use crate::{AstDocument, EmitError, Program, Span};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabProfileDeclaration {
    pub name_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedSource {
    pub program: Program,
    pub profile: Option<LabProfileDeclaration>,
}

impl ParsedSource {
    /// Emit V1 for ordinary syntax, V2 only when an explicit profile is present.
    pub fn to_pretty_json(&self) -> Result<Vec<u8>, EmitError> {
        let Some(profile) = &self.profile else {
            return AstDocument::new(self.program.clone()).to_pretty_json();
        };
        #[derive(Serialize)]
        struct Profile {
            name: &'static str,
            name_span: Span,
            span: Span,
        }
        #[derive(Serialize)]
        struct Document<'a> {
            schema_version: &'static str,
            program: &'a Program,
            profile: Profile,
        }
        let document = Document {
            schema_version: "2.0.0",
            program: &self.program,
            profile: Profile {
                name: "lab",
                name_span: profile.name_span,
                span: profile.span,
            },
        };
        let mut bytes = serde_json::to_vec_pretty(&document).map_err(EmitError::Serialize)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
