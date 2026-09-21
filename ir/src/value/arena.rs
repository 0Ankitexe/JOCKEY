//! Immutable local handles. Insertion validates before changing arena state.
use super::{ResultVariant, Value, validate};
use crate::{
    diagnostic::Failure,
    limits::{self, charge, require},
};
use jocky_shared::compiler::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueHandle(usize);

#[derive(Debug)]
pub enum ArenaNode {
    /// A composed forensic result shares already retained profile/indicator atoms.
    Composition {
        profile: ValueHandle,
        indicators: ValueHandle,
    },
    Atom(Box<Value>),
    List {
        element_type: Type,
        values: Vec<ValueHandle>,
    },
    Option {
        element_type: Type,
        value: Option<ValueHandle>,
    },
    Result {
        ok_type: Type,
        error_type: Type,
        variant: ResultVariant,
        value: ValueHandle,
    },
}

#[derive(Debug, Default)]
pub struct ValueArena {
    nodes: Vec<ArenaNode>,
    type_nodes: usize,
    children: usize,
    scalar_bytes: usize,
    storage: usize,
}
impl ValueArena {
    pub(crate) fn insertion_cost(&self, value: &Value) -> Result<usize, Failure> {
        let s = Stats::measure(value)?;
        let mut nodes = self.nodes.len();
        charge(&mut nodes, s.nodes, limits::VALUE_NODES)?;
        let mut children = self.children;
        charge(&mut children, s.children, limits::VALUE_NODES)?;
        let mut scalars = self.scalar_bytes;
        charge(&mut scalars, s.scalars, limits::FIXTURE_BYTES)?;
        let mut types = self.type_nodes;
        charge(&mut types, s.type_nodes, limits::TYPE_NODES)?;
        let cost = s
            .nodes
            .checked_add(s.type_nodes)
            .and_then(|n| n.checked_mul(64))
            .and_then(|n| n.checked_add((s.children + s.type_references) * 8))
            .and_then(|n| n.checked_add(s.scalars))
            .ok_or_else(Failure::resource)?;
        require(
            self.storage
                .checked_add(cost)
                .ok_or_else(Failure::resource)?,
            limits::STORAGE,
        )?;
        Ok(cost)
    }
    pub(crate) fn forensic(
        &self,
        handle: ValueHandle,
    ) -> Option<(super::wire::ForensicView<'_>, ValueHandle, ValueHandle)> {
        let (profile, indicators) = match self.get(handle)? {
            ArenaNode::Composition {
                profile,
                indicators,
            } => (*profile, *indicators),
            ArenaNode::Atom(v) if matches!(**v, Value::ForensicResult { .. }) => (handle, handle),
            _ => return None,
        };
        let (ArenaNode::Atom(p), ArenaNode::Atom(i)) = (self.get(profile)?, self.get(indicators)?)
        else {
            return None;
        };
        let (Value::ForensicResult { value: p }, Value::ForensicResult { value: i }) = (&**p, &**i)
        else {
            return None;
        };
        Some((
            super::wire::ForensicView {
                endpoint_id: &p.endpoint_id,
                observed_at: &p.observed_at,
                system: p.system.as_ref(),
                indicators: &i.indicators,
            },
            profile,
            indicators,
        ))
    }
    pub(crate) fn compose(
        &mut self,
        profile: ValueHandle,
        indicators: ValueHandle,
    ) -> Result<ValueHandle, Failure> {
        let (_, profile, _) = self
            .forensic(profile)
            .ok_or_else(|| Failure::at(crate::DiagnosticCode::Type, ""))?;
        let (_, _, indicators) = self
            .forensic(indicators)
            .ok_or_else(|| Failure::at(crate::DiagnosticCode::Type, ""))?;
        let mut children = self.children;
        charge(&mut children, 2, limits::VALUE_NODES)?;
        require(self.nodes.len() + 1, limits::VALUE_NODES)?;
        let mut storage = self.storage;
        charge(&mut storage, 80, limits::STORAGE)?;
        self.nodes.try_reserve(1).map_err(|_| Failure::resource())?;
        let handle = ValueHandle(self.nodes.len());
        self.nodes.push(ArenaNode::Composition {
            profile,
            indicators,
        });
        self.children = children;
        self.storage = storage;
        Ok(handle)
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn storage_bytes(&self) -> usize {
        self.storage
    }
    pub fn get(&self, handle: ValueHandle) -> Option<&ArenaNode> {
        self.nodes.get(handle.0)
    }
    pub fn insert(&mut self, value: Value) -> Result<ValueHandle, Failure> {
        // Rejection of an arbitrarily deep owned input must not recursively
        // drop its children on the host stack.
        let mut owned = OwnedValue(vec![value]);
        let value = &owned.0[0];
        validate(value)?;
        let stats = Stats::measure(value)?;
        let mut next_nodes = self.nodes.len();
        charge(&mut next_nodes, stats.nodes, limits::VALUE_NODES)?;
        let mut children = self.children;
        charge(&mut children, stats.children, limits::VALUE_NODES)?;
        let mut scalar_bytes = self.scalar_bytes;
        charge(&mut scalar_bytes, stats.scalars, limits::FIXTURE_BYTES)?;
        let mut type_nodes = self.type_nodes;
        charge(&mut type_nodes, stats.type_nodes, limits::TYPE_NODES)?;
        let cost = stats
            .nodes
            .checked_add(stats.type_nodes)
            .and_then(|n| n.checked_mul(64))
            .and_then(|n| {
                stats
                    .type_references
                    .checked_mul(8)
                    .and_then(|r| n.checked_add(r))
            })
            .and_then(|n| stats.children.checked_mul(8).and_then(|r| n.checked_add(r)))
            .and_then(|n| n.checked_add(stats.scalars))
            .ok_or_else(Failure::resource)?;
        let mut storage = self.storage;
        charge(&mut storage, cost, limits::STORAGE)?;
        self.nodes
            .try_reserve(stats.nodes)
            .map_err(|_| Failure::resource())?;
        let mut work = Vec::new();
        work.try_reserve(stats.nodes * 2)
            .map_err(|_| Failure::resource())?;
        let mut handles = Vec::new();
        handles
            .try_reserve(stats.nodes)
            .map_err(|_| Failure::resource())?;
        work.push(Work::Value(owned.0.pop().expect("one owned input")));
        while let Some(item) = work.pop() {
            let node = match item {
                Work::Value(Value::List {
                    element_type,
                    values,
                }) => {
                    work.push(Work::List(element_type, values.len()));
                    work.extend(values.into_iter().rev().map(Work::Value));
                    continue;
                }
                Work::Value(Value::Option {
                    element_type,
                    value,
                }) => {
                    work.push(Work::Option(element_type, value.is_some()));
                    if let Some(v) = value {
                        work.push(Work::Value(*v));
                    }
                    continue;
                }
                Work::Value(Value::Result {
                    ok_type,
                    error_type,
                    variant,
                    value,
                }) => {
                    work.push(Work::Result(ok_type, error_type, variant));
                    work.push(Work::Value(*value));
                    continue;
                }
                Work::Value(atom) => ArenaNode::Atom(Box::new(atom)),
                Work::List(element_type, length) => {
                    let values = handles.split_off(handles.len() - length);
                    ArenaNode::List {
                        element_type,
                        values,
                    }
                }
                Work::Option(element_type, some) => ArenaNode::Option {
                    element_type,
                    value: if some { handles.pop() } else { None },
                },
                Work::Result(ok_type, error_type, variant) => ArenaNode::Result {
                    ok_type,
                    error_type,
                    variant,
                    value: handles.pop().expect("validated postorder result child"),
                },
            };
            let handle = ValueHandle(self.nodes.len());
            self.nodes.push(node);
            handles.push(handle);
        }
        self.children = children;
        self.type_nodes = type_nodes;
        self.scalar_bytes = scalar_bytes;
        self.storage = storage;
        Ok(handles.pop().expect("one validated root"))
    }
}

struct OwnedValue(Vec<Value>);
impl Drop for OwnedValue {
    fn drop(&mut self) {
        super::drain_values(&mut self.0);
    }
}

enum Work {
    Value(Value),
    List(Type, usize),
    Option(Type, bool),
    Result(Type, Type, ResultVariant),
}
#[derive(Default)]
pub(super) struct Stats {
    pub nodes: usize,
    pub children: usize,
    pub scalars: usize,
    type_nodes: usize,
    type_references: usize,
}
impl Stats {
    fn type_tree(&mut self, root: &Type) -> Result<(), Failure> {
        let mut work = vec![(root, 2)];
        while let Some((ty, depth)) = work.pop() {
            require(depth, limits::TYPE_DEPTH)?;
            charge(&mut self.type_nodes, 1, limits::TYPE_NODES)?;
            match ty {
                Type::Named { .. } => {}
                Type::List { element } | Type::Option { element } => {
                    charge(&mut self.type_references, 1, limits::TYPE_NODES)?;
                    work.push((element, depth + 1));
                }
                Type::Result { ok, error } => {
                    charge(&mut self.type_references, 2, limits::TYPE_NODES)?;
                    work.extend([(error.as_ref(), depth + 1), (ok.as_ref(), depth + 1)]);
                }
            }
        }
        Ok(())
    }
    fn strings<'a>(&mut self, strings: impl IntoIterator<Item = &'a str>) -> Result<(), Failure> {
        for s in strings {
            charge(&mut self.scalars, s.len(), limits::FIXTURE_BYTES)?;
        }
        Ok(())
    }
    fn children(&mut self, n: usize) -> Result<(), Failure> {
        charge(&mut self.children, n, limits::VALUE_NODES)
    }
    pub(super) fn measure(root: &Value) -> Result<Self, Failure> {
        let mut result = Self::default();
        let mut stack = vec![root];
        while let Some(value) = stack.pop() {
            charge(&mut result.nodes, 1, limits::VALUE_NODES)?;
            match value {
                Value::String { value }
                | Value::Int { value }
                | Value::Bytes { value }
                | Value::Timestamp { value }
                | Value::Path { value }
                | Value::IpAddress { value } => result.strings([value.as_str()])?,
                Value::Duration { magnitude, .. } => result.strings([magnitude.as_str()])?,
                Value::Endpoint { value } => {
                    result.strings([value.endpoint_id.as_str(), value.label.as_str()])?
                }
                Value::ProcessRecord { value: v } => result.strings([
                    v.record_id.as_str(),
                    &v.endpoint_id,
                    &v.observed_at,
                    &v.pid,
                    &v.name,
                    &v.image_path,
                ])?,
                Value::ConnectionRecord { value: v } => {
                    result.strings([
                        v.record_id.as_str(),
                        &v.endpoint_id,
                        &v.observed_at,
                        &v.local_address,
                        &v.remote_address,
                    ])?;
                    result.strings(v.process_record_id.as_deref())?;
                }
                Value::FileRecord { value: v } => {
                    result.strings([
                        v.record_id.as_str(),
                        &v.endpoint_id,
                        &v.observed_at,
                        &v.path,
                        &v.size,
                    ])?;
                    result.strings(v.sha256.as_deref())?;
                }
                Value::EventRecord { value: v } => result.strings([
                    v.record_id.as_str(),
                    &v.endpoint_id,
                    &v.observed_at,
                    &v.source,
                    &v.event_code,
                    &v.message,
                ])?,
                Value::PersistenceRecord { value: v } => result.strings([
                    v.record_id.as_str(),
                    &v.endpoint_id,
                    &v.observed_at,
                    &v.mechanism,
                    &v.location,
                    &v.description,
                ])?,
                Value::DriverRecord { value: v } => {
                    result.strings([
                        v.record_id.as_str(),
                        &v.endpoint_id,
                        &v.observed_at,
                        &v.name,
                        &v.path,
                    ])?;
                    result.strings(v.sha256.as_deref())?;
                }
                Value::ForensicResult { value: v } => {
                    result.strings([v.endpoint_id.as_str(), &v.observed_at])?;
                    if let Some(s) = &v.system {
                        result.strings([s.hostname.as_str(), &s.release])?;
                    }
                    result.children(v.indicators.len())?;
                    for i in &v.indicators {
                        result.strings([i.indicator_id.as_str(), &i.message])?;
                        result.children(i.record_ids.len())?;
                        result.strings(i.record_ids.iter().map(String::as_str))?;
                    }
                }
                Value::List {
                    element_type,
                    values,
                } => {
                    result.type_tree(element_type)?;
                    require(values.len(), limits::COLLECTION)?;
                    result.children(values.len())?;
                    stack.extend(values.iter().rev());
                }
                Value::Option {
                    element_type,
                    value,
                } => {
                    result.type_tree(element_type)?;
                    if let Some(v) = value {
                        result.children(1)?;
                        stack.push(v);
                    }
                }
                Value::Result {
                    ok_type,
                    error_type,
                    value,
                    ..
                } => {
                    result.type_tree(ok_type)?;
                    result.type_tree(error_type)?;
                    result.children(1)?;
                    stack.push(value);
                }
                Value::Bool { .. } | Value::Platform { .. } | Value::DiagnosticError { .. } => {}
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jocky_shared::compiler::NamedType;
    #[test]
    fn composition_child_handles_share_atoms_until_the_exact_monotonic_cap() {
        let d = crate::decode_ir(include_bytes!(
            "../../tests/fixtures/valid/hand-authored.ir.json"
        ))
        .unwrap();
        let mut arena = ValueArena::default();
        let mut value = d.constants[0].clone();
        let Value::ForensicResult { value: result } = &mut value else {
            unreachable!()
        };
        result.indicators.clear(); // This probe starts with zero existing child handles.
        let atom = arena.insert(value).unwrap();
        let base = arena.storage_bytes();
        let mut previous = atom;
        for _ in 0..limits::VALUE_NODES / 2 {
            previous = arena.compose(previous, atom).unwrap();
        }
        assert_eq!(arena.children, limits::VALUE_NODES);
        assert_eq!(arena.storage_bytes(), base + 80 * (limits::VALUE_NODES / 2));
        assert_eq!(arena.forensic(previous).unwrap().1, atom);
        let before = (arena.len(), arena.storage_bytes());
        assert!(arena.compose(previous, atom).is_err());
        assert_eq!((arena.len(), arena.storage_bytes()), before);
    }
    #[test]
    fn retained_type_annotations_are_charged_across_insertions() {
        let value = || Value::Option {
            element_type: Type::List {
                element: Box::new(Type::named(NamedType::Int)),
            },
            value: None,
        };
        let mut arena = ValueArena::default();
        arena.insert(value()).unwrap();
        assert_eq!(arena.storage_bytes(), 64 + 2 * 64 + 8);
        arena.type_nodes = limits::TYPE_NODES;
        let before = arena.storage_bytes();
        assert_eq!(
            arena.insert(value()).unwrap_err().code(),
            crate::DiagnosticCode::Resource
        );
        assert_eq!(arena.storage_bytes(), before);
        assert_eq!(arena.len(), 1);
    }
    #[test]
    fn owned_deep_rejection_is_stack_safe() {
        let mut value = Value::Bool { value: false };
        for _ in 0..20_000 {
            value = Value::Option {
                element_type: Type::named(NamedType::Bool),
                value: Some(Box::new(value)),
            };
        }
        let mut arena = ValueArena::default();
        assert_eq!(
            arena.insert(value).unwrap_err().code(),
            crate::DiagnosticCode::Resource
        );
        assert!(arena.is_empty());
    }
    #[test]
    fn handles_share_nodes_and_failed_insertion_leaves_state_unchanged() {
        let mut arena = ValueArena::default();
        assert!(arena.is_empty());
        let h = arena.insert(Value::Bool { value: true }).unwrap();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.storage_bytes(), 64);
        let copy = h;
        assert!(std::ptr::eq(
            arena.get(h).unwrap(),
            arena.get(copy).unwrap()
        ));
        assert!(arena.get(ValueHandle(999)).is_none());
        assert!(
            arena
                .insert(Value::Int {
                    value: "bad".into()
                })
                .is_err()
        );
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.storage_bytes(), 64);
        let list = arena
            .insert(Value::List {
                element_type: Type::named(NamedType::Bool),
                values: vec![Value::Bool { value: true }, Value::Bool { value: false }],
            })
            .unwrap();
        let ArenaNode::List { values, .. } = arena.get(list).unwrap() else {
            panic!()
        };
        assert_eq!(values.len(), 2);
        // Existing bool + three new value nodes + retained named annotation
        // + two child handles. Copying a handle itself allocated nothing.
        assert_eq!(arena.storage_bytes(), 64 + 3 * 64 + 64 + 2 * 8);
        assert!(
            matches!(arena.get(values[0]),Some(ArenaNode::Atom(v))if **v==Value::Bool{value:true})
        );
    }
    #[test]
    fn cumulative_scalar_and_node_limits_do_not_reset_between_insertions() {
        let mut arena = ValueArena {
            scalar_bytes: limits::FIXTURE_BYTES,
            ..ValueArena::default()
        };
        assert!(arena.insert(Value::String { value: "x".into() }).is_err());
        assert!(arena.is_empty());
        let mut arena = ValueArena {
            children: limits::VALUE_NODES,
            ..ValueArena::default()
        };
        assert!(
            arena
                .insert(Value::Option {
                    element_type: Type::named(NamedType::Int),
                    value: Some(Box::new(Value::Int { value: "1".into() }))
                })
                .is_err()
        );
        assert!(arena.is_empty());
    }
}
