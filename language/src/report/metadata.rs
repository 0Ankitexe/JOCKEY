use super::ReportError;
use crate::{
    SourceFile, SourceMap,
    semantic::{CheckedProgram, FunctionMetadata},
};
use jocky_shared::compiler::{Capability, Platform, Privilege};
use serde::{Serialize, Serializer};
use std::{collections::BTreeSet, fmt};

#[derive(Debug)]
struct Name<'a>(&'a str, &'a str);
impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}
impl Serialize for Name<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
#[derive(Debug, Serialize)]
struct Function<'a> {
    name: Name<'a>,
    required_privilege: Privilege,
    capabilities: &'a [Capability],
    supported_platforms: &'a [Platform],
    unavailable_dependencies: Vec<&'a str>,
}
impl<'a> Function<'a> {
    fn new(value: &'a FunctionMetadata) -> Self {
        let (module, function) = value.name_parts();
        Self {
            name: Name(module, function),
            required_privilege: value.required_privilege(),
            capabilities: value.capabilities(),
            supported_platforms: value.supported_platforms(),
            unavailable_dependencies: value.unavailable_dependencies().collect(),
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct Metadata<'a> {
    module: &'a str,
    selected_targets: &'a [Platform],
    profile: &'static str,
    entry: Function<'a>,
    functions: Vec<Function<'a>>,
}
impl<'a> Metadata<'a> {
    pub fn checked(
        source: SourceFile<'a>,
        map: &SourceMap<'a>,
        checked: &'a CheckedProgram,
    ) -> Result<Self, ReportError> {
        if !checked.matches_source(source.text) {
            return Err(ReportError::InvalidMetadata);
        }
        let program = &checked.syntax().program;
        let module = &program.module.name;
        if program.span.start != 0
            || program.span.end != source.text.len()
            || map
                .slice(module.span)
                .map_err(|_| ReportError::InvalidSpan)?
                != module.text
        {
            return Err(ReportError::InvalidMetadata);
        }
        for node in checked.model().nodes() {
            map.validate_span(node.span)
                .map_err(|_| ReportError::InvalidSpan)?;
        }
        if let Some(profile) = checked.profile() {
            if map
                .slice(profile.name_span)
                .map_err(|_| ReportError::InvalidSpan)?
                != "lab"
            {
                return Err(ReportError::InvalidMetadata);
            }
        }
        let summaries = checked.function_metadata();
        if summaries.len() != program.functions.len() {
            return Err(ReportError::InvalidMetadata);
        }
        let mut names = BTreeSet::new();
        for (decl, summary) in program.functions.iter().zip(summaries) {
            if map
                .slice(decl.name.span)
                .map_err(|_| ReportError::InvalidSpan)?
                != decl.name.text
                || summary.name_parts() != (module.text.as_str(), decl.name.text.as_str())
                || !names.insert(decl.name.text.as_str())
            {
                return Err(ReportError::InvalidMetadata);
            }
        }
        let metadata = Self {
            module: &module.text,
            selected_targets: checked.selected_targets(),
            profile: if checked.profile().is_some() {
                "lab"
            } else {
                "standard"
            },
            entry: Function::new(checked.entry_metadata()),
            functions: summaries.iter().map(Function::new).collect(),
        };
        metadata.validate()?;
        let run = program.run.as_ref().ok_or(ReportError::InvalidMetadata)?;
        if run.function.text != metadata.entry.name.1 {
            return Err(ReportError::InvalidMetadata);
        }
        Ok(metadata)
    }
    fn validate(&self) -> Result<(), ReportError> {
        let ordered =
            |values: &[Platform]| !values.is_empty() && values.windows(2).all(|v| v[0] < v[1]);
        if !ordered(self.selected_targets)
            || self.functions.is_empty()
            || self.functions.len() > 1024
        {
            return Err(ReportError::InvalidMetadata);
        }
        let mut names = BTreeSet::new();
        for function in &self.functions {
            if function.name.0 != self.module
                || !names.insert(function.name.1)
                || !ordered(function.supported_platforms)
                || self
                    .selected_targets
                    .iter()
                    .any(|p| !function.supported_platforms.contains(p))
                || !function.capabilities.windows(2).all(|v| v[0] < v[1])
                || !function
                    .unavailable_dependencies
                    .windows(2)
                    .all(|v| v[0] < v[1])
                || function.unavailable_dependencies.len() > 513
            {
                return Err(ReportError::InvalidMetadata);
            }
        }
        let Some(entry) = self
            .functions
            .iter()
            .find(|f| f.name.1 == self.entry.name.1)
        else {
            return Err(ReportError::InvalidMetadata);
        };
        if entry.name.0 != self.entry.name.0
            || entry.required_privilege != self.entry.required_privilege
            || entry.capabilities != self.entry.capabilities
            || entry.supported_platforms != self.entry.supported_platforms
            || entry.unavailable_dependencies != self.entry.unavailable_dependencies
        {
            return Err(ReportError::InvalidMetadata);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_cross_field_inconsistencies_are_rejected_without_success() {
        let text = include_str!("../../../examples/triage.jky");
        let source = SourceFile::new("triage", text);
        let checked = crate::semantic::analyze(
            source,
            jocky_forensic::contracts::builtin_registry().unwrap(),
        )
        .unwrap();
        for case in 0..8 {
            let mut metadata = Metadata::checked(source, &SourceMap::new(text), &checked).unwrap();
            match case {
                0 => metadata.entry.required_privilege = Privilege::LabOnly,
                1 => metadata
                    .functions
                    .push(Function::new(checked.entry_metadata())),
                2 => metadata.functions[0].supported_platforms = &[Platform::Ubuntu],
                3 => metadata.functions[0].capabilities = &[Capability::System, Capability::System],
                4 => metadata.functions[0].unavailable_dependencies.reverse(),
                5 => metadata.entry.name = Name("triage", "absent"),
                6 => metadata.selected_targets = &[],
                _ => metadata.functions[0].name = Name("other", "investigate"),
            }
            assert_eq!(
                metadata.validate(),
                Err(ReportError::InvalidMetadata),
                "case {case}"
            );
        }
    }
}
