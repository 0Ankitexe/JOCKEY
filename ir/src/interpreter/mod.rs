//! An explicit-stack interpreter. Its only inputs are immutable verified data.
pub(crate) mod budget;
pub(crate) mod failure;
mod frames;
mod report;
use crate::{
    VerifiedProgram,
    fixture::{
        FixtureFailure, ValidatedFixture,
        providers::{self, Providers},
    },
    model::{Callee, Operation},
    value::{
        Value,
        arena::{ArenaNode, ValueArena, ValueHandle},
        wire::{self, Node},
    },
};
use budget::Budget;
pub use budget::{Accounting, ExecutionLimits, LimitConfigurationFailure};
pub use failure::{DiagnosticOrigin, ExecutionCode, ExecutionFailure};
use frames::{Cursor, Frames, Position};
pub use report::{ExecutionOutcome, ExecutionReport, ReportWriteFailure};

/// Preparation is the only constructor. It grants no host capabilities.
/// ```compile_fail
/// fn forge<'a>() -> jocky_ir::PreparedInvocation<'a> { jocky_ir::PreparedInvocation {} }
/// ```
pub struct PreparedInvocation<'a> {
    program: &'a VerifiedProgram,
    fixture: &'a ValidatedFixture,
    arena: ValueArena,
    providers: Providers,
    constants: Vec<ValueHandle>,
    frames: Frames,
    budget: Budget,
    reserve: Vec<u8>,
}
impl std::fmt::Debug for PreparedInvocation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedInvocation { fixture_only: true }")
    }
}

pub fn prepare<'a>(
    program: &'a VerifiedProgram,
    fixture: &'a ValidatedFixture,
    limits: &ExecutionLimits,
) -> Result<PreparedInvocation<'a>, FixtureFailure> {
    let d = program.document();
    if !d.targets.contains(&fixture.endpoint().platform) {
        return Err(FixtureFailure::Target);
    }
    let mut preflight = crate::limits::Budget::default();
    for value in d.constants.iter().chain(fixture.values()) {
        crate::value::census(value, &mut preflight)?;
        crate::fixture::validate::collection_limits(value, limits.collection())?;
    }
    let mut budget = Budget::new(*limits, 0).map_err(|_| FixtureFailure::Resource)?;
    let mut reserve = Vec::new();
    reserve
        .try_reserve_exact(budget::FAILURE_RESERVE)
        .map_err(|_| FixtureFailure::Resource)?;
    let mut arena = ValueArena::default();
    budget
        .grow(d.constants.len() * 8)
        .map_err(|_| FixtureFailure::Resource)?;
    let mut constants = Vec::new();
    constants
        .try_reserve_exact(d.constants.len())
        .map_err(|_| FixtureFailure::Resource)?;
    for value in &d.constants {
        constants.push(providers::retain(value, &mut arena, &mut budget)?);
    }
    let providers = Providers::prepare(fixture, d, &mut arena, &mut budget)?;
    let mut frames = Frames::prepare(d, &mut budget).map_err(|_| FixtureFailure::Resource)?;
    frames
        .enter(d, d.entry, &[providers.endpoint()], None, &mut budget)
        .map_err(|_| FixtureFailure::Resource)?;
    Ok(PreparedInvocation {
        program,
        fixture,
        arena,
        providers,
        constants,
        frames,
        budget,
        reserve,
    })
}

pub fn execute(mut invocation: PreparedInvocation<'_>) -> ExecutionReport {
    let outcome = match run(&mut invocation) {
        Ok(value) => ExecutionOutcome::Success {
            value: Value::ForensicResult { value },
        },
        Err(error) => ExecutionOutcome::Failure { error },
    };
    report::finish(
        invocation.program.document(),
        invocation.fixture,
        invocation.budget.limits,
        invocation.budget.accounting,
        outcome,
        invocation.reserve,
    )
}

fn run(p: &mut PreparedInvocation<'_>) -> Result<crate::value::ForensicResult, ExecutionFailure> {
    loop {
        let f = p
            .frames
            .stack
            .last()
            .ok_or_else(|| ExecutionFailure::interpreter(ExecutionCode::InvalidValue, None))?;
        let cursor = f
            .cursors
            .last()
            .copied()
            .ok_or_else(|| ExecutionFailure::interpreter(ExecutionCode::InvalidValue, None))?;
        let d = p.program.document();
        let position = match cursor {
            Cursor::Region { region, next } => {
                if next == d.functions[f.function].regions[region].instructions.len() {
                    p.frames.pop_cursor(&mut p.budget);
                    continue;
                }
                Position {
                    function: f.function,
                    region,
                    instruction: next,
                }
            }
            Cursor::Loop {
                origin: position,
                collection,
                next,
                item,
                body,
            } => {
                let instruction = &d.functions[position.function].regions[position.region]
                    .instructions[position.instruction];
                let fail = |code| ExecutionFailure::interpreter(code, Some(instruction));
                p.budget.instruction().map_err(fail)?;
                let Some(ArenaNode::List { values, .. }) = p.arena.get(collection) else {
                    return Err(fail(ExecutionCode::InvalidValue));
                };
                if values.len() > p.budget.limits.collection {
                    return Err(fail(ExecutionCode::CollectionLimit));
                }
                if let Some(&value) = values.get(next) {
                    if let Some(Cursor::Loop { next, .. }) =
                        p.frames.stack.last_mut().and_then(|f| f.cursors.last_mut())
                    {
                        *next += 1;
                    }
                    p.frames
                        .region(body, Some((item, value)), &mut p.budget)
                        .map_err(fail)?;
                } else {
                    p.frames.pop_cursor(&mut p.budget);
                }
                continue;
            }
        };
        let instruction = &d.functions[position.function].regions[position.region].instructions
            [position.instruction];
        let fail = |code| ExecutionFailure::interpreter(code, Some(instruction));
        p.budget.instruction().map_err(fail)?;
        if let Some(Cursor::Region { next, .. }) =
            p.frames.stack.last_mut().and_then(|f| f.cursors.last_mut())
        {
            *next += 1;
        }
        match &instruction.operation {
            Operation::Const {
                destination,
                constant,
            } => p
                .frames
                .write(*destination, p.constants[*constant])
                .map_err(fail)?,
            Operation::Copy {
                destination,
                source,
            } => {
                let h = p.frames.read(*source).map_err(fail)?;
                p.frames.write(*destination, h).map_err(fail)?;
            }
            Operation::If {
                condition,
                then_region,
                else_region,
            } => {
                let h = p.frames.read(*condition).map_err(fail)?;
                let Some(ArenaNode::Atom(v)) = p.arena.get(h) else {
                    return Err(fail(ExecutionCode::InvalidValue));
                };
                let Value::Bool { value } = &**v else {
                    return Err(fail(ExecutionCode::InvalidValue));
                };
                if let Some(region) = if *value {
                    Some(*then_region)
                } else {
                    *else_region
                } {
                    p.frames.region(region, None, &mut p.budget).map_err(fail)?;
                }
            }
            Operation::ForEach {
                collection,
                item,
                body_region,
            } => {
                let h = p.frames.read(*collection).map_err(fail)?;
                let Some(ArenaNode::List { values, .. }) = p.arena.get(h) else {
                    return Err(fail(ExecutionCode::InvalidValue));
                };
                if values.len() > p.budget.limits.collection {
                    return Err(fail(ExecutionCode::CollectionLimit));
                }
                p.frames
                    .push_cursor(
                        Cursor::Loop {
                            origin: position,
                            collection: h,
                            next: 0,
                            item: *item,
                            body: *body_region,
                        },
                        &mut p.budget,
                    )
                    .map_err(fail)?;
            }
            Operation::Call {
                destination,
                callee,
                arguments,
                ..
            } => {
                let cost = arguments.len() * 8;
                p.budget.grow(cost).map_err(fail)?;
                let mut args = Vec::new();
                args.try_reserve_exact(arguments.len())
                    .map_err(|_| fail(ExecutionCode::MemoryLimit))?;
                for &slot in arguments {
                    args.push(p.frames.read(slot).map_err(fail)?);
                }
                let result = match callee {
                    Callee::Source { function } => p
                        .frames
                        .enter(d, *function, &args, Some(*destination), &mut p.budget)
                        .map_err(fail),
                    Callee::Contract { contract } => p
                        .providers
                        .invoke(*contract, &args, d, &p.arena, &mut p.budget, instruction)
                        .and_then(|h| p.frames.write(*destination, h).map_err(fail)),
                    Callee::Composition => {
                        providers::compose(&args, &mut p.arena, p.fixture, &mut p.budget)
                            .map_err(fail)
                            .and_then(|h| p.frames.write(*destination, h).map_err(fail))
                    }
                };
                p.budget.release(cost);
                result?;
            }
            Operation::Return { value } => {
                let h = p.frames.read(*value).map_err(fail)?;
                if p.frames.stack.len() == 1 {
                    wire::visit(Node::Handle(&p.arena, h), &mut p.budget).map_err(fail)?;
                    let (result, _, _) = p
                        .arena
                        .forensic(h)
                        .ok_or_else(|| fail(ExecutionCode::InvalidValue))?;
                    if !providers::context_valid(result, p.fixture) {
                        return Err(fail(ExecutionCode::InvalidValue));
                    }
                    // Snapshot only the terminal result for the separate report channel.
                    return Ok(result.owned());
                }
                p.frames.leave(h, &mut p.budget).map_err(fail)?;
            }
        }
    }
}
