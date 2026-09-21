use super::{ResultVariant, Value};
use crate::{
    diagnostic::{DiagnosticCode, Failure},
    limits::{self, Budget, require},
};
use jocky_shared::compiler::{NamedType, Type};

pub(crate) fn type_census(
    root: &Type,
    initial_depth: usize,
    budget: &mut Budget,
) -> Result<(), Failure> {
    let mut stack = vec![(root, initial_depth)];
    while let Some((ty, depth)) = stack.pop() {
        budget.type_node(depth)?;
        match ty {
            Type::List { element } | Type::Option { element } => stack.push((element, depth + 1)),
            Type::Result { ok, error } => {
                stack.push((error, depth + 1));
                stack.push((ok, depth + 1));
            }
            Type::Named { .. } => {}
        }
    }
    Ok(())
}

pub(crate) fn types_equal(a: &Type, b: &Type, budget: &mut Budget) -> Result<bool, Failure> {
    let mut stack = vec![(a, b)];
    while let Some((a, b)) = stack.pop() {
        budget.visit()?;
        match (a, b) {
            (Type::Named { name: a }, Type::Named { name: b }) if a == b => {}
            (Type::List { element: a }, Type::List { element: b })
            | (Type::Option { element: a }, Type::Option { element: b }) => stack.push((a, b)),
            (Type::Result { ok: a, error: ae }, Type::Result { ok: b, error: be }) => {
                stack.push((ae, be));
                stack.push((a, b));
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn named(value: &Value) -> Option<NamedType> {
    Some(match value {
        Value::Bool { .. } => NamedType::Bool,
        Value::Int { .. } => NamedType::Int,
        Value::String { .. } => NamedType::String,
        Value::Bytes { .. } => NamedType::Bytes,
        Value::Duration { .. } => NamedType::Duration,
        Value::Timestamp { .. } => NamedType::Timestamp,
        Value::Endpoint { .. } => NamedType::Endpoint,
        Value::Platform { .. } => NamedType::Platform,
        Value::IpAddress { .. } => NamedType::IpAddress,
        Value::Path { .. } => NamedType::Path,
        Value::ProcessRecord { .. } => NamedType::ProcessRecord,
        Value::ConnectionRecord { .. } => NamedType::ConnectionRecord,
        Value::FileRecord { .. } => NamedType::FileRecord,
        Value::EventRecord { .. } => NamedType::EventRecord,
        Value::PersistenceRecord { .. } => NamedType::PersistenceRecord,
        Value::DriverRecord { .. } => NamedType::DriverRecord,
        Value::ForensicResult { .. } => NamedType::ForensicResult,
        Value::DiagnosticError { .. } => NamedType::DiagnosticError,
        _ => return None,
    })
}

pub(crate) fn value_type(value: &Value) -> Type {
    match value {
        Value::List { element_type, .. } => Type::List {
            element: Box::new(element_type.clone()),
        },
        Value::Option { element_type, .. } => Type::Option {
            element: Box::new(element_type.clone()),
        },
        Value::Result {
            ok_type,
            error_type,
            ..
        } => Type::Result {
            ok: Box::new(ok_type.clone()),
            error: Box::new(error_type.clone()),
        },
        _ => Type::named(named(value).expect("all scalar variants have a named type")),
    }
}

fn matches(value: &Value, expected: &Type, budget: &mut Budget) -> Result<bool, Failure> {
    match (value, expected) {
        (Value::List { element_type, .. }, Type::List { element })
        | (Value::Option { element_type, .. }, Type::Option { element }) => {
            types_equal(element_type, element, budget)
        }
        (
            Value::Result {
                ok_type,
                error_type,
                ..
            },
            Type::Result { ok, error },
        ) => Ok(types_equal(ok_type, ok, budget)? && types_equal(error_type, error, budget)?),
        (_, Type::Named { name }) => Ok(named(value) == Some(*name)),
        _ => Ok(false),
    }
}

/// Census happens before recursive serialization or cloning of untrusted values.
pub(crate) fn census(root: &Value, budget: &mut Budget) -> Result<(), Failure> {
    let stats = super::arena::Stats::measure(root)?;
    budget.bytes(stats.scalars)?;
    let mut count = 0;
    let mut stack = vec![(root, None, 1)];
    while let Some((value, expected, depth)) = stack.pop() {
        budget.visit()?;
        require(depth, limits::TYPE_DEPTH)?;
        limits::charge(&mut count, 1, limits::VALUE_NODES)?;
        budget.entries(1)?;
        // Resource census is intentionally independent of semantic diagnostics.
        // Exact child agreement is checked after structural IR verification.
        let _ = expected;
        match value {
            Value::List {
                element_type,
                values,
            } => {
                type_census(element_type, 2, budget)?;
                require(values.len(), limits::COLLECTION)?;
                budget.references(values.len())?;
                for child in values.iter().rev() {
                    stack.push((child, Some(element_type), depth + 1));
                }
            }
            Value::Option {
                element_type,
                value,
            } => {
                type_census(element_type, 2, budget)?;
                if let Some(v) = value {
                    budget.references(1)?;
                    stack.push((v, Some(element_type), depth + 1));
                }
            }
            Value::Result {
                ok_type,
                error_type,
                variant,
                value,
            } => {
                type_census(ok_type, 2, budget)?;
                type_census(error_type, 2, budget)?;
                budget.references(1)?;
                stack.push((
                    value,
                    Some(if *variant == ResultVariant::Ok {
                        ok_type
                    } else {
                        error_type
                    }),
                    depth + 1,
                ));
            }
            Value::ForensicResult { value } => {
                require(value.indicators.len(), limits::COLLECTION)?;
                budget.entries(value.indicators.len())?;
                for indicator in &value.indicators {
                    require(indicator.record_ids.len(), limits::COLLECTION)?;
                    budget.references(indicator.record_ids.len())?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate shape, scalar formats and exact generic child identities, without I/O.
pub fn validate(value: &Value) -> Result<Type, Failure> {
    let mut budget = Budget::default();
    census(value, &mut budget)?;
    let bytes = crate::json::serialize_bounded(
        value,
        limits::FIXTURE_BYTES.min(limits::STORAGE - budget.storage),
    )?;
    budget.bytes(bytes.len())?;
    let (json, mut budget) = crate::json::read_json_metered(&bytes, limits::FIXTURE_BYTES, budget)?;
    crate::json::validate_schema(crate::json::Schema::Values, &json)?;
    consistency(value, &mut budget)?;
    Ok(value_type(value))
}

pub(crate) fn consistency(root: &Value, budget: &mut Budget) -> Result<(), Failure> {
    let mut stack = vec![(root, None)];
    while let Some((value, expected)) = stack.pop() {
        budget.visit()?;
        if let Some(expected) = expected {
            if !matches(value, expected, budget)? {
                return Err(Failure::at(DiagnosticCode::Type, ""));
            }
        }
        match value {
            Value::List {
                element_type,
                values,
            } => {
                for child in values.iter().rev() {
                    stack.push((child, Some(element_type)));
                }
            }
            Value::Option {
                element_type,
                value: Some(child),
            } => stack.push((child, Some(element_type))),
            Value::Result {
                ok_type,
                error_type,
                variant,
                value,
            } => stack.push((
                value,
                Some(if *variant == ResultVariant::Ok {
                    ok_type
                } else {
                    error_type
                }),
            )),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_types_do_not_coerce_and_depth_is_bounded() {
        let a = Type::named(NamedType::Int);
        let b = Type::named(NamedType::Bool);
        assert!(!types_equal(&a, &b, &mut Budget::default()).unwrap());
        let mut nested = Type::named(NamedType::Int);
        for _ in 0..64 {
            nested = Type::List {
                element: Box::new(nested),
            };
        }
        assert_eq!(
            type_census(&nested, 1, &mut Budget::default())
                .unwrap_err()
                .code(),
            DiagnosticCode::Resource
        );
        super::super::drain_type(&mut nested);
        assert_eq!(
            validate(&Value::Int {
                value: "184467440737095516160000".into()
            })
            .unwrap(),
            a
        );
        assert!(
            validate(&Value::Int {
                value: "1\n".into()
            })
            .is_err()
        );
    }
}
