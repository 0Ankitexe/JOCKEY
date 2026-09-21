//! Four closed data mocks. No provider trait, retry or native fallback exists.
use super::{FixtureFailure, Outcome, ValidatedFixture};
use crate::{
    interpreter::{
        budget::Budget,
        failure::{ExecutionCode, ExecutionFailure},
    },
    model::{Instruction, IrDocument},
    value::{
        Value,
        arena::{ValueArena, ValueHandle},
        wire::{self, ForensicView, Node},
    },
};
use jocky_forensic::contracts::{PossibleErrorCode as Error, builtin_registry};

#[derive(Clone, Copy)]
enum StoredOutcome {
    Ok(ValueHandle),
    Error(Error),
}
#[derive(Clone, Copy)]
enum Dispatch {
    Mock(usize),
    Unavailable(Option<Error>),
}
pub(crate) struct Providers {
    endpoint: ValueHandle,
    outcomes: [StoredOutcome; 4],
    expected: [ValueHandle; 3],
    dispatch: Vec<Dispatch>,
}
pub(crate) fn retain(
    value: &Value,
    arena: &mut ValueArena,
    budget: &mut Budget,
) -> Result<ValueHandle, FixtureFailure> {
    let cost = arena.insertion_cost(value)?;
    budget.grow(cost).map_err(|_| FixtureFailure::Resource)?;
    Ok(arena.insert(value.clone())?)
}
fn store(
    outcome: &Outcome,
    arena: &mut ValueArena,
    budget: &mut Budget,
) -> Result<StoredOutcome, FixtureFailure> {
    match outcome {
        Outcome::Ok { value } => Ok(StoredOutcome::Ok(retain(value, arena, budget)?)),
        Outcome::Error { code } => Ok(StoredOutcome::Error(*code)),
    }
}
impl Providers {
    pub fn prepare(
        f: &ValidatedFixture,
        d: &IrDocument,
        arena: &mut ValueArena,
        budget: &mut Budget,
    ) -> Result<Self, FixtureFailure> {
        let p = &f.bundle.providers;
        let endpoint = retain(
            &Value::Endpoint {
                value: f.endpoint().clone(),
            },
            arena,
            budget,
        )?;
        let outcomes = [
            store(&p.system, arena, budget)?,
            store(&p.processes, arena, budget)?,
            store(&p.connections, arena, budget)?,
            store(&p.correlation.outcome, arena, budget)?,
        ];
        let e = &p.correlation.expected;
        let expected = [
            retain(&e.system, arena, budget)?,
            retain(&e.processes, arena, budget)?,
            retain(&e.connections, arena, budget)?,
        ];
        let registry = builtin_registry().map_err(|_| FixtureFailure::Registry)?;
        let names = [
            "forensic.system.profile",
            "forensic.process.list",
            "forensic.network.connections",
            "forensic.indicator.correlate",
        ];
        budget
            .grow(d.contracts.len() * 8)
            .map_err(|_| FixtureFailure::Resource)?;
        let dispatch = d
            .contracts
            .iter()
            .map(|c| {
                let name = c.qualified_name();
                if let Some(index) = names.iter().position(|n| *n == name) {
                    if let Ok(planned) = registry.lookup(&name) {
                        let mut planned = planned.clone();
                        planned.supported_platforms.sort();
                        planned.possible_errors.sort_by_key(|e| e.code);
                        if &planned == c && !c.lab_only {
                            return Dispatch::Mock(index);
                        }
                    }
                }
                Dispatch::Unavailable(
                    [Error::Unsupported, Error::NotImplemented]
                        .into_iter()
                        .find(|code| c.possible_errors.iter().any(|e| e.code == *code)),
                )
            })
            .collect();
        Ok(Self {
            endpoint,
            outcomes,
            expected,
            dispatch,
        })
    }
    pub fn endpoint(&self) -> ValueHandle {
        self.endpoint
    }
    pub fn invoke(
        &self,
        contract: usize,
        args: &[ValueHandle],
        d: &IrDocument,
        arena: &ValueArena,
        budget: &mut Budget,
        origin: &Instruction,
    ) -> Result<ValueHandle, ExecutionFailure> {
        let invalid = |code| ExecutionFailure::interpreter(code, Some(origin));
        let provider = |code| ExecutionFailure::Provider {
            code,
            function: d.contracts[contract].qualified_name(),
            origin: Some(origin.into()),
        };
        let index = match self.dispatch.get(contract) {
            Some(Dispatch::Mock(index)) => *index,
            Some(Dispatch::Unavailable(Some(code))) => return Err(provider(*code)),
            _ => return Err(invalid(ExecutionCode::InvalidValue)),
        };
        if index < 3 {
            if args.len() != 1
                || !wire::equal(
                    Node::Handle(arena, args[0]),
                    Node::Handle(arena, self.endpoint),
                    budget,
                )
                .map_err(invalid)?
            {
                return Err(invalid(ExecutionCode::InvalidValue));
            }
        } else {
            if args.len() != 3 {
                return Err(invalid(ExecutionCode::InvalidValue));
            }
            for (&actual, &expected) in args.iter().zip(&self.expected) {
                if !wire::equal(
                    Node::Handle(arena, actual),
                    Node::Handle(arena, expected),
                    budget,
                )
                .map_err(invalid)?
                {
                    return Err(provider(Error::InvalidInput));
                }
            }
        }
        match self.outcomes[index] {
            StoredOutcome::Ok(h) => Ok(h),
            StoredOutcome::Error(e) => Err(provider(e)),
        }
    }
}

pub(crate) fn context_valid(result: ForensicView<'_>, fixture: &ValidatedFixture) -> bool {
    if result.endpoint_id != fixture.endpoint().endpoint_id
        || result.observed_at != fixture.observed_at()
        || result
            .system
            .is_some_and(|s| s.platform != fixture.endpoint().platform)
    {
        return false;
    }
    let mut ids = std::collections::BTreeSet::new();
    result.indicators.iter().all(|i| {
        ids.insert(&i.indicator_id) && i.record_ids.iter().all(|id| fixture.records.contains(id))
    })
}
pub(crate) fn compose(
    args: &[ValueHandle],
    arena: &mut ValueArena,
    fixture: &ValidatedFixture,
    budget: &mut Budget,
) -> Result<ValueHandle, ExecutionCode> {
    if args.len() != 2 {
        return Err(ExecutionCode::InvalidComposition);
    }
    for &h in args {
        wire::visit(Node::Handle(arena, h), budget)?;
    }
    let (a, _, _) = arena
        .forensic(args[0])
        .ok_or(ExecutionCode::InvalidComposition)?;
    let (b, _, _) = arena
        .forensic(args[1])
        .ok_or(ExecutionCode::InvalidComposition)?;
    if !context_valid(a, fixture)
        || !context_valid(b, fixture)
        || a.system.is_none()
        || !a.indicators.is_empty()
        || b.system.is_some()
    {
        return Err(ExecutionCode::InvalidComposition);
    }
    // Charge the logical constructed value before retaining its shared shell.
    wire::visit(
        Node::ResultView(ForensicView {
            endpoint_id: a.endpoint_id,
            observed_at: a.observed_at,
            system: a.system,
            indicators: b.indicators,
        }),
        budget,
    )?;
    budget.grow(80)?;
    let h = arena
        .compose(args[0], args[1])
        .map_err(|_| ExecutionCode::MemoryLimit)?;
    Ok(h)
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
