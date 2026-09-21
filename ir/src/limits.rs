//! Checked logical accounting, independent of allocator/host RSS.
use crate::diagnostic::Failure;

pub const IR_BYTES: usize = 16 * 1024 * 1024;
pub const SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const LABEL_BYTES: usize = 4096;
pub const FIXTURE_BYTES: usize = 8 * 1024 * 1024;
pub const JSON_DEPTH: usize = 96;
pub const JSON_NODES: usize = 200_000;
pub const FUNCTIONS: usize = 1024;
pub const ITEMS: usize = 100_000;
pub const EDGES: usize = 32_768;
pub const CONTRACTS: usize = 512;
pub const TYPE_DEPTH: usize = 64;
pub const TYPE_NODES: usize = 1_000_000;
pub const VISITS: usize = 10_000_000;
pub const STORAGE: usize = 64 * 1024 * 1024;
pub const COLLECTION: usize = 10_000;
pub const VALUE_NODES: usize = 200_000;

pub(crate) fn require(value: usize, max: usize) -> Result<(), Failure> {
    if value > max {
        Err(Failure::resource())
    } else {
        Ok(())
    }
}
pub(crate) fn charge(value: &mut usize, amount: usize, max: usize) -> Result<(), Failure> {
    let next = value.checked_add(amount).ok_or_else(Failure::resource)?;
    require(next, max)?;
    *value = next;
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Budget {
    pub storage: usize,
    pub visits: usize,
    pub types: usize,
}
impl Budget {
    pub fn bytes(&mut self, n: usize) -> Result<(), Failure> {
        charge(&mut self.storage, n, STORAGE)
    }
    pub fn entries(&mut self, n: usize) -> Result<(), Failure> {
        self.bytes(n.checked_mul(64).ok_or_else(Failure::resource)?)
    }
    pub fn references(&mut self, n: usize) -> Result<(), Failure> {
        self.bytes(n.checked_mul(8).ok_or_else(Failure::resource)?)
    }
    pub fn visit(&mut self) -> Result<(), Failure> {
        charge(&mut self.visits, 1, VISITS)
    }
    pub fn type_node(&mut self, depth: usize) -> Result<(), Failure> {
        require(depth, TYPE_DEPTH)?;
        self.visit()?;
        charge(&mut self.types, 1, TYPE_NODES)?;
        self.entries(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_boundaries_overflow_and_failure_do_not_mutate_counters() {
        for max in [
            IR_BYTES,
            SOURCE_BYTES,
            LABEL_BYTES,
            FIXTURE_BYTES,
            JSON_DEPTH,
            JSON_NODES,
            FUNCTIONS,
            ITEMS,
            EDGES,
            CONTRACTS,
            TYPE_DEPTH,
            TYPE_NODES,
            VISITS,
            STORAGE,
            COLLECTION,
            VALUE_NODES,
        ] {
            let mut used = max - 1;
            charge(&mut used, 1, max).unwrap();
            assert!(charge(&mut used, 1, max).is_err());
            assert_eq!(used, max);
        }
        let mut used = usize::MAX;
        assert!(charge(&mut used, 1, usize::MAX).is_err());
        assert_eq!(used, usize::MAX);
        let mut b = Budget::default();
        assert!(b.entries(usize::MAX).is_err());
        assert!(b.references(usize::MAX).is_err());
        assert_eq!(b.storage, 0);
    }
}
