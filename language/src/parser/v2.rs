use super::{JockyParser, Rule, builder, recovery};
use crate::{
    DiagnosticSet, LabProfileDeclaration, ParsedSource, SourceFile, Span, lexer::preflight,
};
use pest::{Parser as _, error::InputLocation};

pub(crate) fn parse_source(source: SourceFile<'_>) -> Result<ParsedSource, DiagnosticSet> {
    let legacy = match super::parse(source) {
        Ok(program) => {
            return Ok(ParsedSource {
                program,
                profile: None,
            });
        }
        Err(errors) if errors.has_resource_limit() => return Err(errors),
        Err(errors) => errors,
    };
    let Ok(lexical) = preflight(source.text) else {
        return Err(legacy);
    };
    match JockyParser::parse(Rule::program_v2, source.text) {
        Ok(mut pairs) => {
            let pair = pairs
                .next()
                .ok_or_else(|| super::internal_build_failure(Span::new(0, 0)))?;
            let profile = pair
                .clone()
                .into_inner()
                .find(|p| p.as_rule() == Rule::lab_profile_declaration)
                .map(|p| {
                    let span = Span::new(p.as_span().start(), p.as_span().end());
                    let name = p
                        .into_inner()
                        .find(|c| c.as_rule() == Rule::lab_keyword)
                        .ok_or_else(|| super::internal_build_failure(span))?;
                    Ok(LabProfileDeclaration {
                        span,
                        name_span: Span::new(name.as_span().start(), name.as_span().end()),
                    })
                })
                .transpose()?;
            let program = builder::build_program(pair, source.text.len())
                .map_err(|e| super::internal_build_failure(e.span))?;
            Ok(ParsedSource { program, profile })
        }
        Err(error) => {
            let candidates = recovery::profile_candidates(source.text, lexical.tokens());
            if candidates.is_empty() {
                return Err(legacy);
            }
            let offset = match error.location {
                InputLocation::Pos(p) | InputLocation::Span((p, _)) => p,
            };
            Err(recovery::recover_profiles(
                source,
                &lexical,
                offset,
                &candidates,
            ))
        }
    }
}
