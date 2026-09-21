//! Conversion from a successful strict Pest parse into the owned AST.

use pest::iterators::Pair;

use super::Rule;
use crate::Span;
use crate::ast::{
    Block, DurationUnit, Expression, FunctionDeclaration, Identifier, ModuleDeclaration, Parameter,
    Program, QualifiedName, RunStatement, Statement, TargetDeclaration, TargetPlatformNode,
    TypeReference,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BuildError {
    pub(super) span: Span,
}

pub(super) fn build_program(
    pair: Pair<'_, Rule>,
    source_len: usize,
) -> Result<Program, BuildError> {
    let fallback = pair_span(&pair);
    let mut module = None;
    let mut target = None;
    let mut functions = Vec::new();
    let mut run = None;

    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::module_declaration => module = Some(build_module(child)?),
            Rule::target_declaration => target = Some(build_target(child)?),
            Rule::function_declaration => functions.push(build_function(child)?),
            Rule::run_statement => run = Some(build_run(child)?),
            Rule::EOI | Rule::lab_profile_declaration => {}
            _ => return Err(error_at(&child)),
        }
    }

    Ok(Program {
        span: Span::new(0, source_len),
        module: module.ok_or(BuildError { span: fallback })?,
        target: target.ok_or(BuildError { span: fallback })?,
        functions,
        run,
    })
}

fn build_module(pair: Pair<'_, Rule>) -> Result<ModuleDeclaration, BuildError> {
    let span = pair_span(&pair);
    let name = find_identifier(pair.into_inner()).ok_or(BuildError { span })?;
    Ok(ModuleDeclaration { name, span })
}

fn build_target(pair: Pair<'_, Rule>) -> Result<TargetDeclaration, BuildError> {
    let span = pair_span(&pair);
    let selector = pair
        .into_inner()
        .find(|child| child.as_rule() == Rule::target_selector)
        .ok_or(BuildError { span })?;
    let mut platforms = Vec::new();
    for child in selector.into_inner() {
        let platform_span = pair_span(&child);
        match child.as_rule() {
            Rule::windows_keyword => {
                platforms.push(TargetPlatformNode::Windows {
                    span: platform_span,
                });
            }
            Rule::ubuntu_keyword => {
                platforms.push(TargetPlatformNode::Ubuntu {
                    span: platform_span,
                });
            }
            _ => {
                return Err(BuildError {
                    span: platform_span,
                });
            }
        }
    }
    if platforms.is_empty() {
        return Err(BuildError { span });
    }
    Ok(TargetDeclaration { platforms, span })
}

fn build_function(pair: Pair<'_, Rule>) -> Result<FunctionDeclaration, BuildError> {
    let span = pair_span(&pair);
    let mut name = None;
    let mut parameters = Vec::new();
    let mut return_type = None;
    let mut body = None;

    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::identifier if name.is_none() => name = Some(build_identifier(child)),
            Rule::parameter_list => {
                parameters = child
                    .into_inner()
                    .map(build_parameter)
                    .collect::<Result<Vec<_>, _>>()?;
            }
            Rule::type_reference => return_type = Some(build_type(child)?),
            Rule::block => body = Some(build_block(child)?),
            Rule::fn_keyword => {}
            _ => return Err(error_at(&child)),
        }
    }

    Ok(FunctionDeclaration {
        name: name.ok_or(BuildError { span })?,
        parameters,
        return_type: return_type.ok_or(BuildError { span })?,
        body: body.ok_or(BuildError { span })?,
        span,
    })
}

fn build_parameter(pair: Pair<'_, Rule>) -> Result<Parameter, BuildError> {
    let span = pair_span(&pair);
    let mut name = None;
    let mut type_ref = None;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::identifier => name = Some(build_identifier(child)),
            Rule::type_reference => type_ref = Some(build_type(child)?),
            _ => return Err(error_at(&child)),
        }
    }
    Ok(Parameter {
        name: name.ok_or(BuildError { span })?,
        type_ref: type_ref.ok_or(BuildError { span })?,
        span,
    })
}

fn build_type(pair: Pair<'_, Rule>) -> Result<TypeReference, BuildError> {
    let fallback = pair_span(&pair);
    let concrete = if pair.as_rule() == Rule::type_reference {
        pair.into_inner()
            .next()
            .ok_or(BuildError { span: fallback })?
    } else {
        pair
    };
    let span = pair_span(&concrete);

    match concrete.as_rule() {
        Rule::named_type => {
            let name = find_identifier(concrete.into_inner()).ok_or(BuildError { span })?;
            Ok(TypeReference::Named { name, span })
        }
        Rule::list_type | Rule::option_type => {
            let is_list = concrete.as_rule() == Rule::list_type;
            let element = concrete
                .into_inner()
                .find(|child| child.as_rule() == Rule::type_reference)
                .ok_or(BuildError { span })?;
            let element = Box::new(build_type(element)?);
            if is_list {
                Ok(TypeReference::List { element, span })
            } else {
                Ok(TypeReference::Option { element, span })
            }
        }
        Rule::result_type => {
            let arguments = concrete
                .into_inner()
                .filter(|child| child.as_rule() == Rule::type_reference)
                .map(build_type)
                .collect::<Result<Vec<_>, _>>()?;
            let mut arguments = arguments.into_iter();
            let ok = arguments.next().ok_or(BuildError { span })?;
            let error = arguments.next().ok_or(BuildError { span })?;
            if arguments.next().is_some() {
                return Err(BuildError { span });
            }
            Ok(TypeReference::Result {
                ok: Box::new(ok),
                error: Box::new(error),
                span,
            })
        }
        _ => Err(BuildError { span }),
    }
}

fn build_block(pair: Pair<'_, Rule>) -> Result<Block, BuildError> {
    let span = pair_span(&pair);
    let statements = pair
        .into_inner()
        .filter(|child| child.as_rule() == Rule::statement)
        .map(build_statement)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Block { statements, span })
}

fn build_statement(pair: Pair<'_, Rule>) -> Result<Statement, BuildError> {
    let fallback = pair_span(&pair);
    let concrete = pair
        .into_inner()
        .next()
        .ok_or(BuildError { span: fallback })?;
    let span = pair_span(&concrete);

    match concrete.as_rule() {
        Rule::let_statement => {
            let mut name = None;
            let mut value = None;
            for child in concrete.into_inner() {
                match child.as_rule() {
                    Rule::identifier => name = Some(build_identifier(child)),
                    Rule::expression => value = Some(build_expression(child)?),
                    Rule::let_keyword => {}
                    _ => return Err(error_at(&child)),
                }
            }
            Ok(Statement::Let {
                name: name.ok_or(BuildError { span })?,
                value: value.ok_or(BuildError { span })?,
                span,
            })
        }
        Rule::call_statement => {
            let call = concrete.into_inner().next().ok_or(BuildError { span })?;
            let (callee, arguments, call_span) = build_call(call)?;
            Ok(Statement::Call {
                callee,
                arguments,
                span: call_span,
            })
        }
        Rule::if_statement => build_if(concrete),
        Rule::for_statement => build_for(concrete),
        Rule::return_statement => {
            let value = concrete
                .into_inner()
                .find(|child| child.as_rule() == Rule::expression)
                .ok_or(BuildError { span })?;
            Ok(Statement::Return {
                value: build_expression(value)?,
                span,
            })
        }
        _ => Err(BuildError { span }),
    }
}

fn build_if(pair: Pair<'_, Rule>) -> Result<Statement, BuildError> {
    let span = pair_span(&pair);
    let mut condition = None;
    let mut branches = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::expression => condition = Some(build_expression(child)?),
            Rule::block => branches.push(build_block(child)?),
            Rule::if_keyword | Rule::else_keyword => {}
            _ => return Err(error_at(&child)),
        }
    }
    let mut branches = branches.into_iter();
    let then_branch = branches.next().ok_or(BuildError { span })?;
    let else_branch = branches.next();
    if branches.next().is_some() {
        return Err(BuildError { span });
    }
    Ok(Statement::If {
        condition: condition.ok_or(BuildError { span })?,
        then_branch,
        else_branch,
        span,
    })
}

fn build_for(pair: Pair<'_, Rule>) -> Result<Statement, BuildError> {
    let span = pair_span(&pair);
    let mut binding = None;
    let mut collection = None;
    let mut body = None;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::identifier => binding = Some(build_identifier(child)),
            Rule::expression => collection = Some(build_expression(child)?),
            Rule::block => body = Some(build_block(child)?),
            Rule::for_keyword | Rule::in_keyword => {}
            _ => return Err(error_at(&child)),
        }
    }
    Ok(Statement::For {
        binding: binding.ok_or(BuildError { span })?,
        collection: collection.ok_or(BuildError { span })?,
        body: body.ok_or(BuildError { span })?,
        span,
    })
}

fn build_expression(pair: Pair<'_, Rule>) -> Result<Expression, BuildError> {
    let fallback = pair_span(&pair);
    let concrete = if pair.as_rule() == Rule::expression {
        pair.into_inner()
            .next()
            .ok_or(BuildError { span: fallback })?
    } else {
        pair
    };
    let span = pair_span(&concrete);

    match concrete.as_rule() {
        Rule::identifier_expression => {
            let identifier = find_identifier(concrete.into_inner()).ok_or(BuildError { span })?;
            Ok(Expression::Identifier { identifier, span })
        }
        Rule::string_literal => Ok(Expression::String {
            value: decode_string(concrete.as_str()).ok_or(BuildError { span })?,
            span,
        }),
        Rule::integer_literal => Ok(Expression::Integer {
            lexeme: concrete.as_str().to_owned(),
            span,
        }),
        Rule::boolean_literal => Ok(Expression::Boolean {
            value: concrete.as_str() == "true",
            span,
        }),
        Rule::duration_literal => {
            let (magnitude, unit) = split_duration(concrete.as_str()).ok_or(BuildError { span })?;
            Ok(Expression::Duration {
                magnitude: magnitude.to_owned(),
                unit,
                span,
            })
        }
        Rule::call_expression => {
            let (callee, arguments, span) = build_call(concrete)?;
            Ok(Expression::Call {
                callee,
                arguments,
                span,
            })
        }
        _ => Err(BuildError { span }),
    }
}

fn build_call(pair: Pair<'_, Rule>) -> Result<(QualifiedName, Vec<Expression>, Span), BuildError> {
    let span = pair_span(&pair);
    let mut callee = None;
    let mut arguments = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::qualified_name => callee = Some(build_qualified_name(child)?),
            Rule::argument_list => {
                arguments = child
                    .into_inner()
                    .filter(|argument| argument.as_rule() == Rule::expression)
                    .map(build_expression)
                    .collect::<Result<Vec<_>, _>>()?;
            }
            _ => return Err(error_at(&child)),
        }
    }
    Ok((callee.ok_or(BuildError { span })?, arguments, span))
}

fn build_qualified_name(pair: Pair<'_, Rule>) -> Result<QualifiedName, BuildError> {
    let span = pair_span(&pair);
    let segments = pair
        .into_inner()
        .filter(|child| child.as_rule() == Rule::identifier)
        .map(build_identifier)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Err(BuildError { span });
    }
    Ok(QualifiedName { segments, span })
}

fn build_run(pair: Pair<'_, Rule>) -> Result<RunStatement, BuildError> {
    let span = pair_span(&pair);
    let mut function = None;
    let mut destination_span = None;
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::identifier => function = Some(build_identifier(child)),
            Rule::selected_endpoints_keyword => destination_span = Some(pair_span(&child)),
            Rule::run_keyword | Rule::on_keyword => {}
            _ => return Err(error_at(&child)),
        }
    }
    Ok(RunStatement {
        function: function.ok_or(BuildError { span })?,
        destination: "selected_endpoints".to_owned(),
        destination_span: destination_span.ok_or(BuildError { span })?,
        span,
    })
}

fn find_identifier<'i>(pairs: impl Iterator<Item = Pair<'i, Rule>>) -> Option<Identifier> {
    pairs
        .filter(|pair| pair.as_rule() == Rule::identifier)
        .map(build_identifier)
        .next()
}

fn build_identifier(pair: Pair<'_, Rule>) -> Identifier {
    Identifier {
        text: pair.as_str().to_owned(),
        span: pair_span(&pair),
    }
}

fn pair_span(pair: &Pair<'_, Rule>) -> Span {
    let span = pair.as_span();
    Span::new(span.start(), span.end())
}

fn error_at(pair: &Pair<'_, Rule>) -> BuildError {
    BuildError {
        span: pair_span(pair),
    }
}

fn decode_string(raw: &str) -> Option<String> {
    let body = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        decoded.push(match characters.next()? {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => return None,
        });
    }
    Some(decoded)
}

fn split_duration(raw: &str) -> Option<(&str, DurationUnit)> {
    for (suffix, unit) in [
        ("ms", DurationUnit::Milliseconds),
        ("s", DurationUnit::Seconds),
        ("m", DurationUnit::Minutes),
        ("h", DurationUnit::Hours),
        ("d", DurationUnit::Days),
    ] {
        if let Some(magnitude) = raw.strip_suffix(suffix) {
            return Some((magnitude, unit));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{decode_string, split_duration};
    use crate::ast::DurationUnit;

    #[test]
    fn decodes_only_the_supported_string_escapes() {
        assert_eq!(
            decode_string(r#""a\n\r\t\"\\b""#),
            Some("a\n\r\t\"\\b".to_owned())
        );
        assert_eq!(decode_string(r#""\q""#), None);
    }

    #[test]
    fn preserves_duration_magnitude_text() {
        assert_eq!(
            split_duration("999999999999999999999ms"),
            Some(("999999999999999999999", DurationUnit::Milliseconds))
        );
    }
}
