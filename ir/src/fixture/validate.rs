use super::{Bundle, FixtureFailure, Outcome, values};
use crate::{
    interpreter::budget::ExecutionLimits,
    limits::Budget,
    value::{ForensicResult, Value},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn raw_collections(
    raw: &serde_json::Value,
    maximum: usize,
) -> Result<(), FixtureFailure> {
    let mut stack = vec![raw];
    while let Some(value) = stack.pop() {
        match value {
            serde_json::Value::Array(a) => {
                if a.len() > maximum {
                    return Err(FixtureFailure::Resource);
                }
                stack.extend(a);
            }
            serde_json::Value::Object(o) => stack.extend(o.values()),
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn collection_limits(root: &Value, maximum: usize) -> Result<(), FixtureFailure> {
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::List { values, .. } => {
                if values.len() > maximum {
                    return Err(FixtureFailure::Resource);
                }
                stack.extend(values);
            }
            Value::Option { value: Some(v), .. } | Value::Result { value: v, .. } => stack.push(v),
            Value::ForensicResult { value: v }
                if v.indicators.len() > maximum
                    || v.indicators.iter().any(|i| i.record_ids.len() > maximum) =>
            {
                return Err(FixtureFailure::Resource);
            }
            _ => {}
        }
    }
    Ok(())
}
fn result(v: &Value) -> Result<&ForensicResult, FixtureFailure> {
    if let Value::ForensicResult { value } = v {
        Ok(value)
    } else {
        Err(FixtureFailure::Context)
    }
}
fn profile(v: &Value) -> Result<(), FixtureFailure> {
    let r = result(v)?;
    if r.system.is_none() || !r.indicators.is_empty() {
        Err(FixtureFailure::Context)
    } else {
        Ok(())
    }
}
fn list(v: &Value) -> Result<&[Value], FixtureFailure> {
    if let Value::List { values, .. } = v {
        Ok(values)
    } else {
        Err(FixtureFailure::Context)
    }
}

pub(super) fn bundle(
    b: &Bundle,
    limits: &ExecutionLimits,
) -> Result<BTreeSet<String>, FixtureFailure> {
    if b.schema_version != "1.0.0" {
        return Err(FixtureFailure::Version);
    }
    let mut budget = Budget::default();
    let mut copies: BTreeMap<&str, &Value> = BTreeMap::new();
    for value in values(b) {
        crate::value::census(value, &mut budget)?;
        crate::value::consistency(value, &mut budget)?;
        collection_limits(value, limits.collection())?;
        let mut stack = vec![value];
        while let Some(v) = stack.pop() {
            budget.visit()?;
            match v {
                Value::ForensicResult { value: r } => {
                    if r.endpoint_id != b.endpoint.endpoint_id
                        || r.observed_at != b.observed_at
                        || r.system
                            .as_ref()
                            .is_some_and(|s| s.platform != b.endpoint.platform)
                    {
                        return Err(FixtureFailure::Context);
                    }
                    let mut ids = BTreeSet::new();
                    for i in &r.indicators {
                        budget.visit()?;
                        if !ids.insert(&i.indicator_id) {
                            return Err(FixtureFailure::Context);
                        }
                    }
                }
                Value::List { values, .. } => {
                    let mut ids = BTreeSet::new();
                    for child in values {
                        let (id, endpoint, time) = match child {
                            Value::ProcessRecord { value: r } => {
                                (&r.record_id, &r.endpoint_id, &r.observed_at)
                            }
                            Value::ConnectionRecord { value: r } => {
                                (&r.record_id, &r.endpoint_id, &r.observed_at)
                            }
                            _ => return Err(FixtureFailure::Context),
                        };
                        if endpoint != &b.endpoint.endpoint_id
                            || time != &b.observed_at
                            || !ids.insert(id)
                        {
                            return Err(FixtureFailure::Context);
                        }
                        budget.references(1)?;
                        if let Some(previous) = copies.insert(id, child) {
                            if previous != child {
                                return Err(FixtureFailure::Context);
                            }
                        }
                    }
                    stack.extend(values);
                }
                _ => {}
            }
        }
    }
    let p = &b.providers;
    if let Outcome::Ok { value } = &p.system {
        profile(value)?;
    }
    profile(&p.correlation.expected.system)?;
    if let Outcome::Ok { value } = &p.correlation.outcome {
        if result(value)?.system.is_some() {
            return Err(FixtureFailure::Context);
        }
    }
    let process_ids: BTreeSet<_> = list(&p.correlation.expected.processes)?
        .iter()
        .map(|v| match v {
            Value::ProcessRecord { value: r } => Ok(r.record_id.as_str()),
            _ => Err(FixtureFailure::Context),
        })
        .collect::<Result<_, _>>()?;
    let mut records: BTreeSet<String> = process_ids.iter().map(|s| (*s).to_owned()).collect();
    for v in list(&p.correlation.expected.connections)? {
        if let Value::ConnectionRecord { value: r } = v {
            records.insert(r.record_id.clone());
        } else {
            return Err(FixtureFailure::Context);
        }
    }
    for v in values(b) {
        match v {
            Value::List { values, .. } => {
                for child in values {
                    if let Value::ConnectionRecord { value: r } = child {
                        if r.process_record_id
                            .as_deref()
                            .is_some_and(|id| !process_ids.contains(id))
                        {
                            return Err(FixtureFailure::Context);
                        }
                    }
                }
            }
            Value::ForensicResult { value: r } => {
                for i in &r.indicators {
                    for id in &i.record_ids {
                        if !records.contains(id) {
                            return Err(FixtureFailure::Context);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(records)
}
