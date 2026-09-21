use super::{budget::Budget, failure::ExecutionCode};
use crate::{model::IrDocument, value::arena::ValueHandle};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Position {
    pub function: usize,
    pub region: usize,
    pub instruction: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExecutionLimits,
        model::{Region, Slot, SlotKind},
        value::{Value, arena::ValueArena},
    };
    #[test]
    fn calls_and_reentered_regions_have_fresh_slots_and_returns_unwind_all_cursors() {
        let mut d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let f = &mut d.functions[0];
        f.regions.push(Region {
            instructions: Vec::new(),
        });
        for _ in 0..2 {
            f.slots.push(Slot {
                r#type: f.result.clone(),
                region: 1,
                kind: SlotKind::Temporary,
                name: None,
                span: None,
            });
        }
        let mut arena = ValueArena::default();
        let a = arena.insert(d.constants[0].clone()).unwrap();
        let b = arena.insert(Value::Bool { value: false }).unwrap();
        let mut budget = Budget::new(ExecutionLimits::default(), 0).unwrap();
        let mut frames = Frames::prepare(&d, &mut budget).unwrap();
        assert_eq!(frames.read(0), Err(ExecutionCode::InvalidValue));
        assert_eq!(frames.write(0, a), Err(ExecutionCode::InvalidValue));
        frames.enter(&d, 0, &[a], None, &mut budget).unwrap();
        frames.write(1, a).unwrap();
        frames.enter(&d, 0, &[b], Some(1), &mut budget).unwrap();
        assert_eq!(frames.read(0).unwrap(), b);
        assert_eq!(frames.read(1), Err(ExecutionCode::InvalidValue));
        frames.region(1, Some((2, a)), &mut budget).unwrap();
        frames.write(3, b).unwrap();
        frames.pop_cursor(&mut budget);
        frames.region(1, Some((2, b)), &mut budget).unwrap();
        assert_eq!(frames.read(2).unwrap(), b);
        assert_eq!(frames.read(3), Err(ExecutionCode::InvalidValue));
        assert_eq!(frames.leave(b, &mut budget).unwrap(), None);
        assert_eq!(frames.stack.len(), 1);
        assert_eq!(frames.read(0).unwrap(), a);
        assert_eq!(frames.read(1).unwrap(), b);
        assert_eq!(frames.leave(a, &mut budget).unwrap(), Some(a));
        assert!(frames.stack.is_empty());
        assert_eq!(
            frames.leave(a, &mut budget),
            Err(ExecutionCode::InvalidValue)
        );
        assert_eq!(
            frames.enter(&d, 1, &[a], None, &mut budget),
            Err(ExecutionCode::InvalidValue)
        );
        assert_eq!(
            frames.enter(&d, 0, &[], None, &mut budget),
            Err(ExecutionCode::InvalidValue)
        );
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Cursor {
    Region {
        region: usize,
        next: usize,
    },
    Loop {
        origin: Position,
        collection: ValueHandle,
        next: usize,
        item: usize,
        body: usize,
    },
}
pub(crate) struct Frame {
    pub function: usize,
    pub slots: Vec<Option<ValueHandle>>,
    pub cursors: Vec<Cursor>,
    pub destination: Option<usize>,
}
pub(crate) struct Frames {
    pub stack: Vec<Frame>,
    owners: Vec<Vec<Vec<usize>>>,
}
impl Frames {
    pub fn prepare(d: &IrDocument, budget: &mut Budget) -> Result<Self, ExecutionCode> {
        let mut owners = Vec::new();
        budget.grow(d.functions.len() * 8)?;
        for f in &d.functions {
            budget.grow(f.regions.len() * 8 + f.slots.len() * 8)?;
            let mut regions = vec![Vec::new(); f.regions.len()];
            for (s, slot) in f.slots.iter().enumerate() {
                regions[slot.region].push(s);
            }
            owners.push(regions);
        }
        Ok(Self {
            stack: Vec::new(),
            owners,
        })
    }
    pub fn enter(
        &mut self,
        d: &IrDocument,
        function: usize,
        args: &[ValueHandle],
        destination: Option<usize>,
        budget: &mut Budget,
    ) -> Result<(), ExecutionCode> {
        let f = d
            .functions
            .get(function)
            .ok_or(ExecutionCode::InvalidValue)?;
        if args.len() != f.parameters.len() {
            return Err(ExecutionCode::InvalidValue);
        }
        budget.enter_frame(f.slots.len())?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(f.slots.len())
            .map_err(|_| ExecutionCode::MemoryLimit)?;
        slots.resize(f.slots.len(), None);
        for (&parameter, &value) in f.parameters.iter().zip(args) {
            slots[parameter] = Some(value);
        }
        self.stack
            .try_reserve(1)
            .map_err(|_| ExecutionCode::MemoryLimit)?;
        self.stack.push(Frame {
            function,
            slots,
            cursors: Vec::new(),
            destination,
        });
        // Root parameters are initialized by the call, not cleared on activation.
        self.push_cursor(Cursor::Region { region: 0, next: 0 }, budget)
    }
    pub fn read(&self, slot: usize) -> Result<ValueHandle, ExecutionCode> {
        self.stack
            .last()
            .and_then(|f| f.slots.get(slot))
            .copied()
            .flatten()
            .ok_or(ExecutionCode::InvalidValue)
    }
    pub fn write(&mut self, slot: usize, value: ValueHandle) -> Result<(), ExecutionCode> {
        let location = self
            .stack
            .last_mut()
            .and_then(|f| f.slots.get_mut(slot))
            .ok_or(ExecutionCode::InvalidValue)?;
        *location = Some(value);
        Ok(())
    }
    pub fn push_cursor(
        &mut self,
        cursor: Cursor,
        budget: &mut Budget,
    ) -> Result<(), ExecutionCode> {
        let f = self.stack.last_mut().ok_or(ExecutionCode::InvalidValue)?;
        budget.cursor(f.cursors.len())?;
        f.cursors
            .try_reserve(1)
            .map_err(|_| ExecutionCode::MemoryLimit)?;
        f.cursors.push(cursor);
        Ok(())
    }
    pub fn region(
        &mut self,
        region: usize,
        item: Option<(usize, ValueHandle)>,
        budget: &mut Budget,
    ) -> Result<(), ExecutionCode> {
        let f = self.stack.last_mut().ok_or(ExecutionCode::InvalidValue)?;
        for &slot in &self.owners[f.function][region] {
            f.slots[slot] = None;
        }
        if let Some((slot, value)) = item {
            f.slots[slot] = Some(value);
        }
        self.push_cursor(Cursor::Region { region, next: 0 }, budget)
    }
    pub fn pop_cursor(&mut self, budget: &mut Budget) {
        self.stack.last_mut().expect("active frame").cursors.pop();
        budget.release(32);
    }
    pub fn leave(
        &mut self,
        value: ValueHandle,
        budget: &mut Budget,
    ) -> Result<Option<ValueHandle>, ExecutionCode> {
        let f = self.stack.pop().ok_or(ExecutionCode::InvalidValue)?;
        budget.release(f.cursors.len() * 32);
        budget.exit_frame(f.slots.len());
        match f.destination {
            Some(slot) => {
                self.write(slot, value)?;
                Ok(None)
            }
            None => Ok(Some(value)),
        }
    }
}
