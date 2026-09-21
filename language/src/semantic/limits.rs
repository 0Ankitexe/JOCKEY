//! Explicit bounded traversal and checked resource accounting.
use crate::{ParsedSource, Span, ast::*};

pub const MAX_SOURCE_BYTES: usize = 4_194_304;
pub const MAX_REPORT_BYTES: usize = 8_388_608;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    SourceBytes,
    Functions,
    SyntaxNodes,
    Bindings,
    CallEdges,
    TypeDepth,
    TypeWork,
    TypeStorage,
    ReportBytes,
    ParserNesting,
}
impl ResourceKind {
    pub const fn maximum(self) -> usize {
        match self {
            Self::SourceBytes => MAX_SOURCE_BYTES,
            Self::Functions => 1024,
            Self::SyntaxNodes => 100_000,
            Self::Bindings => 20_000,
            Self::CallEdges => 32_768,
            Self::TypeDepth => 64,
            Self::TypeWork | Self::TypeStorage => 100_000,
            Self::ReportBytes => MAX_REPORT_BYTES,
            Self::ParserNesting => 256,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::SourceBytes => "source bytes",
            Self::Functions => "functions",
            Self::SyntaxNodes => "syntax nodes",
            Self::Bindings => "bindings",
            Self::CallEdges => "source-call edges",
            Self::TypeDepth => "semantic type depth",
            Self::TypeWork => "semantic type work",
            Self::TypeStorage => "semantic type storage",
            Self::ReportBytes => "report bytes",
            Self::ParserNesting => "parser nesting",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceFailure {
    pub kind: ResourceKind,
    pub span: Option<Span>,
}
impl ResourceFailure {
    pub(super) const fn new(kind: ResourceKind, span: Option<Span>) -> Self {
        Self { kind, span }
    }
}

pub(super) fn require(
    kind: ResourceKind,
    amount: usize,
    span: Option<Span>,
) -> Result<(), ResourceFailure> {
    if amount > kind.maximum() {
        Err(ResourceFailure::new(kind, span))
    } else {
        Ok(())
    }
}
pub(super) fn charge(
    counter: &mut usize,
    kind: ResourceKind,
    amount: usize,
    span: Option<Span>,
) -> Result<(), ResourceFailure> {
    let next = counter
        .checked_add(amount)
        .ok_or(ResourceFailure::new(kind, span))?;
    require(kind, next, span)?;
    *counter = next;
    Ok(())
}

enum Visit<'a> {
    Leaf(Span),
    Function(&'a FunctionDeclaration),
    Parameter(&'a Parameter),
    Type(&'a TypeReference),
    Block(&'a Block),
    Statement(&'a Statement),
    Expression(&'a Expression),
    Qualified(&'a QualifiedName),
}
#[derive(Debug, Default)]
pub(super) struct Census {
    pub nodes: usize,
    pub bindings: usize,
}
pub(super) fn census(parsed: &ParsedSource) -> Result<Census, ResourceFailure> {
    let program = &parsed.program;
    require(
        ResourceKind::Functions,
        program.functions.len(),
        Some(program.span),
    )?;
    let mut stack = vec![
        Visit::Leaf(program.span),
        Visit::Leaf(program.module.span),
        Visit::Leaf(program.module.name.span),
        Visit::Leaf(program.target.span),
    ];
    stack.extend(
        program
            .target
            .platforms
            .iter()
            .map(|p| Visit::Leaf(p.span())),
    );
    if let Some(profile) = &parsed.profile {
        stack.extend([Visit::Leaf(profile.span), Visit::Leaf(profile.name_span)]);
    }
    if let Some(run) = &program.run {
        stack.extend([Visit::Leaf(run.span), Visit::Leaf(run.function.span)]);
    }
    stack.extend(program.functions.iter().rev().map(Visit::Function));
    let mut count = Census::default();
    while let Some(node) = stack.pop() {
        let span = match &node {
            Visit::Leaf(s) => *s,
            Visit::Function(f) => f.span,
            Visit::Parameter(p) => p.span,
            Visit::Type(t) => t.span(),
            Visit::Block(b) => b.span,
            Visit::Statement(s) => s.span(),
            Visit::Expression(e) => e.span(),
            Visit::Qualified(q) => q.span,
        };
        charge(&mut count.nodes, ResourceKind::SyntaxNodes, 1, Some(span))?;
        match node {
            Visit::Leaf(_) => {}
            Visit::Function(f) => {
                stack.extend([
                    Visit::Leaf(f.name.span),
                    Visit::Type(&f.return_type),
                    Visit::Block(&f.body),
                ]);
                stack.extend(f.parameters.iter().rev().map(Visit::Parameter));
            }
            Visit::Parameter(p) => {
                charge(
                    &mut count.bindings,
                    ResourceKind::Bindings,
                    1,
                    Some(p.name.span),
                )?;
                stack.extend([Visit::Leaf(p.name.span), Visit::Type(&p.type_ref)]);
            }
            Visit::Type(t) => match t {
                TypeReference::Named { name, .. } => stack.push(Visit::Leaf(name.span)),
                TypeReference::List { element, .. } | TypeReference::Option { element, .. } => {
                    stack.push(Visit::Type(element))
                }
                TypeReference::Result { ok, error, .. } => {
                    stack.extend([Visit::Type(error), Visit::Type(ok)])
                }
            },
            Visit::Block(b) => stack.extend(b.statements.iter().rev().map(Visit::Statement)),
            Visit::Qualified(q) => {
                stack.extend(q.segments.iter().rev().map(|i| Visit::Leaf(i.span)))
            }
            Visit::Statement(s) => match s {
                Statement::Let { name, value, .. } => {
                    charge(
                        &mut count.bindings,
                        ResourceKind::Bindings,
                        1,
                        Some(name.span),
                    )?;
                    stack.extend([Visit::Leaf(name.span), Visit::Expression(value)]);
                }
                Statement::Call {
                    callee, arguments, ..
                } => {
                    stack.push(Visit::Qualified(callee));
                    stack.extend(arguments.iter().rev().map(Visit::Expression));
                }
                Statement::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    stack.extend([Visit::Expression(condition), Visit::Block(then_branch)]);
                    if let Some(b) = else_branch {
                        stack.push(Visit::Block(b));
                    }
                }
                Statement::For {
                    binding,
                    collection,
                    body,
                    ..
                } => {
                    charge(
                        &mut count.bindings,
                        ResourceKind::Bindings,
                        1,
                        Some(binding.span),
                    )?;
                    stack.extend([
                        Visit::Leaf(binding.span),
                        Visit::Expression(collection),
                        Visit::Block(body),
                    ]);
                }
                Statement::Return { value, .. } => stack.push(Visit::Expression(value)),
            },
            Visit::Expression(e) => match e {
                Expression::Identifier { identifier, .. } => {
                    stack.push(Visit::Leaf(identifier.span))
                }
                Expression::Call {
                    callee, arguments, ..
                } => {
                    stack.push(Visit::Qualified(callee));
                    stack.extend(arguments.iter().rev().map(Visit::Expression));
                }
                _ => {}
            },
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_counter_accepts_boundary_rejects_above_and_overflow_without_mutation() {
        for kind in [
            ResourceKind::SourceBytes,
            ResourceKind::Functions,
            ResourceKind::SyntaxNodes,
            ResourceKind::Bindings,
            ResourceKind::CallEdges,
            ResourceKind::TypeDepth,
            ResourceKind::TypeWork,
            ResourceKind::TypeStorage,
            ResourceKind::ReportBytes,
        ] {
            let mut count = 0;
            charge(&mut count, kind, kind.maximum(), None).unwrap();
            assert_eq!(charge(&mut count, kind, 1, None).unwrap_err().kind, kind);
            assert_eq!(count, kind.maximum());
            count = usize::MAX;
            assert!(charge(&mut count, kind, 1, None).is_err());
            assert_eq!(count, usize::MAX);
        }
    }
    #[test]
    fn census_counts_occurrences_and_profile_leaves_not_trivia() {
        let source = "module demo target ubuntu fn f(x: int) -> int { return x } run f on selected_endpoints";
        let parsed = crate::parse_source(crate::SourceFile::new("x", source)).unwrap();
        let counts = census(&parsed).unwrap();
        assert_eq!(counts.nodes, 19);
        assert_eq!(counts.bindings, 1);
        let text = source.replace("target ubuntu", "target ubuntu profile lab");
        assert_eq!(
            census(&crate::parse_source(crate::SourceFile::new("x", &text)).unwrap())
                .unwrap()
                .nodes,
            21
        );
    }
}
