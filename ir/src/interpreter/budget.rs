//! Explicit logical limits; no host time, RSS measurement or ambient settings.
use super::failure::ExecutionCode;
use crate::limits;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutionLimits {
    pub(crate) instructions: u64,
    pub(crate) collection: usize,
    pub(crate) work: u64,
    pub(crate) call_depth: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitConfigurationFailure;
impl std::fmt::Display for LimitConfigurationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("execution limits must be positive and within hard caps")
    }
}
impl std::error::Error for LimitConfigurationFailure {}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            instructions: 100_000,
            collection: 10_000,
            work: 1_000_000,
            call_depth: 128,
        }
    }
}
impl ExecutionLimits {
    pub fn new(
        instructions: u64,
        collection: usize,
        work: u64,
        call_depth: usize,
    ) -> Result<Self, LimitConfigurationFailure> {
        if !(1..=1_000_000).contains(&instructions)
            || !(1..=10_000).contains(&collection)
            || !(1..=10_000_000).contains(&work)
            || !(1..=256).contains(&call_depth)
        {
            return Err(LimitConfigurationFailure);
        }
        Ok(Self {
            instructions,
            collection,
            work,
            call_depth,
        })
    }
    pub fn instructions(self) -> u64 {
        self.instructions
    }
    pub fn collection(self) -> usize {
        self.collection
    }
    pub fn work(self) -> u64 {
        self.work
    }
    pub fn call_depth(self) -> usize {
        self.call_depth
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Accounting {
    pub instructions: u64,
    pub work: u64,
    pub peak_call_depth: usize,
    pub peak_storage_bytes: usize,
}
pub(crate) const FAILURE_RESERVE: usize = 64 * 1024;
pub(crate) struct Budget {
    pub limits: ExecutionLimits,
    pub accounting: Accounting,
    storage: usize,
    slots: usize,
    frames: usize,
}
impl Budget {
    pub fn new(limits: ExecutionLimits, base: usize) -> Result<Self, ExecutionCode> {
        let mut result = Self {
            limits,
            accounting: Accounting::default(),
            storage: 0,
            slots: 0,
            frames: 0,
        };
        result.grow(
            base.checked_add(FAILURE_RESERVE)
                .ok_or(ExecutionCode::MemoryLimit)?,
        )?;
        Ok(result)
    }
    pub fn instruction(&mut self) -> Result<(), ExecutionCode> {
        charge(
            &mut self.accounting.instructions,
            1,
            self.limits.instructions,
            ExecutionCode::InstructionLimit,
        )
    }
    pub fn work(&mut self, n: usize) -> Result<(), ExecutionCode> {
        charge(
            &mut self.accounting.work,
            n as u64,
            self.limits.work,
            ExecutionCode::WorkLimit,
        )
    }
    pub fn grow(&mut self, n: usize) -> Result<(), ExecutionCode> {
        let next = self
            .storage
            .checked_add(n)
            .filter(|n| *n <= limits::STORAGE)
            .ok_or(ExecutionCode::MemoryLimit)?;
        self.storage = next;
        self.accounting.peak_storage_bytes = self.accounting.peak_storage_bytes.max(next);
        Ok(())
    }
    pub fn release(&mut self, n: usize) {
        self.storage -= n;
    }
    pub fn enter_frame(&mut self, slots: usize) -> Result<(), ExecutionCode> {
        if self.frames >= self.limits.call_depth {
            return Err(ExecutionCode::CallDepth);
        }
        let total = self
            .slots
            .checked_add(slots)
            .filter(|n| *n <= 1_000_000)
            .ok_or(ExecutionCode::MemoryLimit)?;
        let cost = slots
            .checked_mul(8)
            .and_then(|n| n.checked_add(64))
            .ok_or(ExecutionCode::MemoryLimit)?;
        self.grow(cost)?;
        self.slots = total;
        self.frames += 1;
        self.accounting.peak_call_depth = self.accounting.peak_call_depth.max(self.frames);
        Ok(())
    }
    pub fn exit_frame(&mut self, slots: usize) {
        self.release(64 + slots * 8);
        self.slots -= slots;
        self.frames -= 1;
    }
    pub fn cursor(&mut self, depth: usize) -> Result<(), ExecutionCode> {
        if depth >= 256 {
            return Err(ExecutionCode::MemoryLimit);
        }
        self.grow(32)
    }
}
fn charge(used: &mut u64, n: u64, maximum: u64, code: ExecutionCode) -> Result<(), ExecutionCode> {
    let next = used.checked_add(n).filter(|n| *n <= maximum).ok_or(code)?;
    *used = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_storage_slots_and_control_cursors_reject_growth_without_counter_changes() {
        let mut b = Budget::new(
            ExecutionLimits::default(),
            limits::STORAGE - FAILURE_RESERVE,
        )
        .unwrap();
        assert_eq!(b.storage, limits::STORAGE);
        assert_eq!(b.grow(1), Err(ExecutionCode::MemoryLimit));
        assert_eq!(b.accounting.peak_storage_bytes, limits::STORAGE);
        assert!(
            Budget::new(
                ExecutionLimits::default(),
                limits::STORAGE - FAILURE_RESERVE + 1
            )
            .is_err()
        );
        assert!(Budget::new(ExecutionLimits::default(), usize::MAX).is_err());
        let mut b = Budget::new(ExecutionLimits::default(), 0).unwrap();
        b.enter_frame(1_000_000).unwrap();
        let before = b.accounting;
        assert_eq!(b.enter_frame(1), Err(ExecutionCode::MemoryLimit));
        assert_eq!(b.accounting, before);
        b.exit_frame(1_000_000);
        assert_eq!(b.storage, FAILURE_RESERVE);
        assert_eq!((b.frames, b.slots), (0, 0));
        b.cursor(255).unwrap();
        let before = b.accounting;
        assert_eq!(b.cursor(256), Err(ExecutionCode::MemoryLimit));
        assert_eq!(b.accounting, before);
        assert_eq!(b.enter_frame(usize::MAX), Err(ExecutionCode::MemoryLimit));
    }
    #[test]
    fn limits_and_failed_charges_never_wrap_or_mutate() {
        assert_eq!(
            ExecutionLimits::default(),
            ExecutionLimits::new(100_000, 10_000, 1_000_000, 128).unwrap()
        );
        for (i, c, w, d) in [
            (0, 1, 1, 1),
            (1_000_001, 1, 1, 1),
            (1, 0, 1, 1),
            (1, 10_001, 1, 1),
            (1, 1, 0, 1),
            (1, 1, 10_000_001, 1),
            (1, 1, 1, 0),
            (1, 1, 1, 257),
        ] {
            assert!(ExecutionLimits::new(i, c, w, d).is_err());
        }
        let mut b = Budget::new(ExecutionLimits::new(1, 1, 3, 1).unwrap(), 0).unwrap();
        b.instruction().unwrap();
        assert_eq!(b.instruction(), Err(ExecutionCode::InstructionLimit));
        assert_eq!(b.accounting.instructions, 1);
        b.work(3).unwrap();
        assert_eq!(b.work(1), Err(ExecutionCode::WorkLimit));
        assert_eq!(b.accounting.work, 3);
        b.enter_frame(2).unwrap();
        assert_eq!(b.enter_frame(1), Err(ExecutionCode::CallDepth));
        b.exit_frame(2);
        let before = b.storage;
        assert!(b.grow(usize::MAX).is_err());
        assert_eq!(b.storage, before);
        let mut used = u64::MAX;
        assert!(charge(&mut used, 1, u64::MAX, ExecutionCode::InstructionLimit).is_err());
        assert_eq!(used, u64::MAX);
    }
}
