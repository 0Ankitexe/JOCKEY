//! Borrowed schema-ordered JSON nodes. Traversal is iterative and never clones payloads.
use super::{
    arena::{ArenaNode, ValueArena, ValueHandle},
    *,
};
use crate::interpreter::{budget::Budget, failure::ExecutionCode};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ForensicView<'a> {
    pub endpoint_id: &'a str,
    pub observed_at: &'a str,
    pub system: Option<&'a System>,
    pub indicators: &'a [Indicator],
}
impl ForensicView<'_> {
    pub fn owned(self) -> ForensicResult {
        ForensicResult {
            endpoint_id: self.endpoint_id.into(),
            observed_at: self.observed_at.into(),
            system: self.system.cloned(),
            indicators: self.indicators.to_vec(),
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Value(&'a Value),
    Handle(&'a ValueArena, ValueHandle),
    Type(&'a Type),
    ResultView(ForensicView<'a>),
    Endpoint(&'a Endpoint),
    System(&'a System),
    Forensic(ForensicView<'a>),
    Indicator(&'a Indicator),
    Process(&'a ProcessRecord),
    Connection(&'a ConnectionRecord),
    File(&'a FileRecord),
    Event(&'a EventRecord),
    Persistence(&'a PersistenceRecord),
    Driver(&'a DriverRecord),
    Text(&'a str),
    Number(u64),
    Bool(bool),
    Null,
    Values(&'a [Value]),
    Handles(&'a ValueArena, &'a [ValueHandle]),
    Indicators(&'a [Indicator]),
    Strings(&'a [String]),
}
enum Shape<'a> {
    Text(&'a str),
    Number(u64),
    Bool(bool),
    Null,
    Object(Vec<(&'static str, Node<'a>)>),
    Array(Children<'a>),
}
enum Children<'a> {
    Fields(std::vec::IntoIter<(&'static str, Node<'a>)>),
    Values(std::slice::Iter<'a, Value>),
    Handles(&'a ValueArena, std::slice::Iter<'a, ValueHandle>),
    Indicators(std::slice::Iter<'a, Indicator>),
    Strings(std::slice::Iter<'a, String>),
}
impl<'a> Iterator for Children<'a> {
    type Item = Node<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Fields(i) => i.next().map(|(_, v)| v),
            Self::Values(i) => i.next().map(Node::Value),
            Self::Handles(a, i) => i.next().map(|h| Node::Handle(a, *h)),
            Self::Indicators(i) => i.next().map(Node::Indicator),
            Self::Strings(i) => i.next().map(|s| Node::Text(s)),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = match self {
            Self::Fields(i) => i.len(),
            Self::Values(i) => i.len(),
            Self::Handles(_, i) => i.len(),
            Self::Indicators(i) => i.len(),
            Self::Strings(i) => i.len(),
        };
        (n, Some(n))
    }
}
fn object<'a>(fields: Vec<(&'static str, Node<'a>)>) -> Shape<'a> {
    Shape::Object(fields)
}
fn tagged<'a>(kind: &'static str, value: Node<'a>) -> Shape<'a> {
    object(vec![("kind", Node::Text(kind)), ("value", value)])
}
impl<'a> Node<'a> {
    fn shape(self) -> Result<Shape<'a>, ExecutionCode> {
        Ok(match self {
            Self::ResultView(v) => tagged("forensic_result", Self::Forensic(v)),
            Self::Text(v) => Shape::Text(v),
            Self::Number(v) => Shape::Number(v),
            Self::Bool(v) => Shape::Bool(v),
            Self::Null => Shape::Null,
            Self::Values(v) => Shape::Array(Children::Values(v.iter())),
            Self::Handles(a, v) => Shape::Array(Children::Handles(a, v.iter())),
            Self::Indicators(v) => Shape::Array(Children::Indicators(v.iter())),
            Self::Strings(v) => Shape::Array(Children::Strings(v.iter())),
            Self::Handle(a, h) => match a.get(h).ok_or(ExecutionCode::InvalidValue)? {
                ArenaNode::Atom(v) => return Self::Value(v).shape(),
                ArenaNode::Composition { .. } => tagged(
                    "forensic_result",
                    Self::Forensic(a.forensic(h).ok_or(ExecutionCode::InvalidValue)?.0),
                ),
                ArenaNode::List {
                    element_type,
                    values,
                } => object(vec![
                    ("kind", Self::Text("list")),
                    ("element_type", Self::Type(element_type)),
                    ("values", Self::Handles(a, values)),
                ]),
                ArenaNode::Option {
                    element_type,
                    value,
                } => object(vec![
                    ("kind", Self::Text("option")),
                    ("element_type", Self::Type(element_type)),
                    ("value", value.map_or(Self::Null, |h| Self::Handle(a, h))),
                ]),
                ArenaNode::Result {
                    ok_type,
                    error_type,
                    variant,
                    value,
                } => object(vec![
                    ("kind", Self::Text("result")),
                    ("ok_type", Self::Type(ok_type)),
                    ("error_type", Self::Type(error_type)),
                    (
                        "variant",
                        Self::Text(match variant {
                            ResultVariant::Ok => "ok",
                            ResultVariant::Error => "error",
                        }),
                    ),
                    ("value", Self::Handle(a, *value)),
                ]),
            },
            Self::Value(v) => match v {
                Value::Bool { value } => tagged("bool", Self::Bool(*value)),
                Value::Int { value } => tagged("int", Self::Text(value)),
                Value::String { value } => tagged("string", Self::Text(value)),
                Value::Bytes { value } => tagged("bytes", Self::Text(value)),
                Value::Duration { magnitude, unit } => object(vec![
                    ("kind", Self::Text("duration")),
                    ("magnitude", Self::Text(magnitude)),
                    (
                        "unit",
                        Self::Text(match unit {
                            DurationUnit::Milliseconds => "ms",
                            DurationUnit::Seconds => "s",
                            DurationUnit::Minutes => "m",
                            DurationUnit::Hours => "h",
                            DurationUnit::Days => "d",
                        }),
                    ),
                ]),
                Value::Timestamp { value } => tagged("timestamp", Self::Text(value)),
                Value::Endpoint { value } => tagged("endpoint", Self::Endpoint(value)),
                Value::Platform { value } => tagged("platform", Self::Text(value.as_str())),
                Value::IpAddress { value } => tagged("ip_address", Self::Text(value)),
                Value::Path { value } => tagged("path", Self::Text(value)),
                Value::ForensicResult { value } => tagged(
                    "forensic_result",
                    Self::Forensic(ForensicView {
                        endpoint_id: &value.endpoint_id,
                        observed_at: &value.observed_at,
                        system: value.system.as_ref(),
                        indicators: &value.indicators,
                    }),
                ),
                Value::DiagnosticError { code } => object(vec![
                    ("kind", Self::Text("diagnostic_error")),
                    (
                        "code",
                        Self::Text(crate::interpreter::failure::provider_code(*code)),
                    ),
                ]),
                Value::ProcessRecord { value } => tagged("process_record", Self::Process(value)),
                Value::ConnectionRecord { value } => {
                    tagged("connection_record", Self::Connection(value))
                }
                Value::FileRecord { value } => tagged("file_record", Self::File(value)),
                Value::EventRecord { value } => tagged("event_record", Self::Event(value)),
                Value::PersistenceRecord { value } => {
                    tagged("persistence_record", Self::Persistence(value))
                }
                Value::DriverRecord { value } => tagged("driver_record", Self::Driver(value)),
                Value::List {
                    element_type,
                    values,
                } => object(vec![
                    ("kind", Self::Text("list")),
                    ("element_type", Self::Type(element_type)),
                    ("values", Self::Values(values)),
                ]),
                Value::Option {
                    element_type,
                    value,
                } => object(vec![
                    ("kind", Self::Text("option")),
                    ("element_type", Self::Type(element_type)),
                    ("value", value.as_deref().map_or(Self::Null, Self::Value)),
                ]),
                Value::Result {
                    ok_type,
                    error_type,
                    variant,
                    value,
                } => object(vec![
                    ("kind", Self::Text("result")),
                    ("ok_type", Self::Type(ok_type)),
                    ("error_type", Self::Type(error_type)),
                    (
                        "variant",
                        Self::Text(match variant {
                            ResultVariant::Ok => "ok",
                            ResultVariant::Error => "error",
                        }),
                    ),
                    ("value", Self::Value(value)),
                ]),
            },
            Self::Type(ty) => match ty {
                Type::Named { name } => object(vec![
                    ("kind", Self::Text("named")),
                    ("name", Self::Text(name.as_str())),
                ]),
                Type::List { element } => object(vec![
                    ("kind", Self::Text("list")),
                    ("element", Self::Type(element)),
                ]),
                Type::Option { element } => object(vec![
                    ("kind", Self::Text("option")),
                    ("element", Self::Type(element)),
                ]),
                Type::Result { ok, error } => object(vec![
                    ("kind", Self::Text("result")),
                    ("ok", Self::Type(ok)),
                    ("error", Self::Type(error)),
                ]),
            },
            Self::Forensic(v) => object(vec![
                ("endpoint_id", Self::Text(v.endpoint_id)),
                ("observed_at", Self::Text(v.observed_at)),
                ("system", v.system.map_or(Self::Null, Self::System)),
                ("indicators", Self::Indicators(v.indicators)),
            ]),
            Self::Indicator(v) => object(vec![
                ("indicator_id", Self::Text(&v.indicator_id)),
                ("kind", Self::Text("fixture_match")),
                ("message", Self::Text(&v.message)),
                ("record_ids", Self::Strings(&v.record_ids)),
            ]),
            Self::Endpoint(v) => object(vec![
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("platform", Self::Text(v.platform.as_str())),
                ("label", Self::Text(&v.label)),
            ]),
            Self::System(v) => object(vec![
                ("platform", Self::Text(v.platform.as_str())),
                ("hostname", Self::Text(&v.hostname)),
                ("release", Self::Text(&v.release)),
            ]),
            Self::Process(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                ("pid", Self::Text(&v.pid)),
                ("name", Self::Text(&v.name)),
                ("image_path", Self::Text(&v.image_path)),
            ]),
            Self::Connection(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                (
                    "process_record_id",
                    v.process_record_id
                        .as_deref()
                        .map_or(Self::Null, Self::Text),
                ),
                (
                    "protocol",
                    Self::Text(match v.protocol {
                        Protocol::Tcp => "tcp",
                        Protocol::Udp => "udp",
                    }),
                ),
                ("local_address", Self::Text(&v.local_address)),
                ("local_port", Self::Number(v.local_port as u64)),
                ("remote_address", Self::Text(&v.remote_address)),
                ("remote_port", Self::Number(v.remote_port as u64)),
            ]),
            Self::File(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                ("path", Self::Text(&v.path)),
                ("size", Self::Text(&v.size)),
                ("sha256", v.sha256.as_deref().map_or(Self::Null, Self::Text)),
            ]),
            Self::Event(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                ("source", Self::Text(&v.source)),
                ("event_code", Self::Text(&v.event_code)),
                ("message", Self::Text(&v.message)),
            ]),
            Self::Persistence(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                ("mechanism", Self::Text(&v.mechanism)),
                ("location", Self::Text(&v.location)),
                ("description", Self::Text(&v.description)),
            ]),
            Self::Driver(v) => object(vec![
                ("record_id", Self::Text(&v.record_id)),
                ("endpoint_id", Self::Text(&v.endpoint_id)),
                ("observed_at", Self::Text(&v.observed_at)),
                ("name", Self::Text(&v.name)),
                ("path", Self::Text(&v.path)),
                ("sha256", v.sha256.as_deref().map_or(Self::Null, Self::Text)),
            ]),
        })
    }
}
impl Serialize for Node<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self
            .shape()
            .map_err(|_| serde::ser::Error::custom("invalid retained value"))?
        {
            Shape::Text(v) => s.serialize_str(v),
            Shape::Number(v) => s.serialize_u64(v),
            Shape::Bool(v) => s.serialize_bool(v),
            Shape::Null => s.serialize_none(),
            Shape::Object(fields) => {
                let mut map = s.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, &v)?;
                }
                map.end()
            }
            Shape::Array(children) => {
                let mut seq = s.serialize_seq(children.size_hint().1)?;
                for v in children {
                    seq.serialize_element(&v)?;
                }
                seq.end()
            }
        }
    }
}
/// Same ordered traversal for aliased and independently retained values.
pub(crate) fn equal(a: Node<'_>, b: Node<'_>, budget: &mut Budget) -> Result<bool, ExecutionCode> {
    let mut stack: Vec<(Children<'_>, Children<'_>)> = Vec::new();
    let mut current = Some((a, b));
    while let Some((a, b)) = current.take() {
        let (a, b) = (a.shape()?, b.shape()?);
        let extra = match (&a, &b) {
            (Shape::Text(a), Shape::Text(b)) => a.len().max(b.len()),
            _ => 0,
        };
        budget.work(1 + extra)?;
        match (a, b) {
            (Shape::Text(a), Shape::Text(b)) if a == b => {}
            (Shape::Number(a), Shape::Number(b)) if a == b => {}
            (Shape::Bool(a), Shape::Bool(b)) if a == b => {}
            (Shape::Null, Shape::Null) => {}
            (Shape::Object(a), Shape::Object(b)) => {
                if a.len() != b.len() || a.iter().map(|x| x.0).ne(b.iter().map(|x| x.0)) {
                    return Ok(false);
                }
                stack.push((
                    Children::Fields(a.into_iter()),
                    Children::Fields(b.into_iter()),
                ));
            }
            (Shape::Array(a), Shape::Array(b)) => {
                if a.size_hint() != b.size_hint() {
                    return Ok(false);
                }
                stack.push((a, b));
            }
            _ => return Ok(false),
        }
        if stack.len() > crate::limits::JSON_DEPTH {
            return Err(ExecutionCode::ValueDepth);
        }
        while let Some((a, b)) = stack.last_mut() {
            match (a.next(), b.next()) {
                (Some(a), Some(b)) => {
                    current = Some((a, b));
                    break;
                }
                (None, None) => {
                    stack.pop();
                }
                _ => return Ok(false),
            }
        }
    }
    Ok(true)
}
pub(crate) fn visit(node: Node<'_>, budget: &mut Budget) -> Result<(), ExecutionCode> {
    equal(node, node, budget).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_matches_schema_order_and_charges_identical_values() {
        let v = Value::String { value: "é".into() };
        let mut a = Budget::new(ExecutionLimits::default(), 0).unwrap();
        assert!(equal(Node::Value(&v), Node::Value(&v), &mut a).unwrap());
        // Object + kind ("string") + value ("é") = 3 nodes + 6 + 2 bytes.
        assert_eq!(a.accounting.work, 11);
        assert_eq!(
            serde_json::to_value(Node::Value(&v)).unwrap(),
            serde_json::to_value(&v).unwrap()
        );
        let other = Value::String { value: "x".into() };
        assert!(!equal(Node::Value(&v), Node::Value(&other), &mut a).unwrap());
    }
    #[test]
    fn immutable_handles_preserve_generic_data_and_do_not_skip_comparison_work() {
        let int = Type::named(NamedType::Int);
        let error = Type::named(NamedType::DiagnosticError);
        for v in [
            Value::Option {
                element_type: int.clone(),
                value: None,
            },
            Value::Option {
                element_type: int.clone(),
                value: Some(Box::new(Value::Int {
                    value: "999999999999999999999999999999".into(),
                })),
            },
            Value::Result {
                ok_type: int.clone(),
                error_type: error,
                variant: ResultVariant::Error,
                value: Box::new(Value::DiagnosticError {
                    code: PossibleErrorCode::ResourceLimit,
                }),
            },
            Value::List {
                element_type: int,
                values: vec![
                    Value::Int { value: "1".into() },
                    Value::Int { value: "2".into() },
                ],
            },
        ] {
            let raw = serde_json::to_value(&v).unwrap();
            let mut arena = ValueArena::default();
            let h = arena.insert(v.clone()).unwrap();
            assert_eq!(serde_json::to_value(Node::Handle(&arena, h)).unwrap(), raw);
            let mut independent = Budget::new(ExecutionLimits::default(), 0).unwrap();
            let mut alias = Budget::new(ExecutionLimits::default(), 0).unwrap();
            assert!(equal(Node::Handle(&arena, h), Node::Value(&v), &mut independent).unwrap());
            assert!(equal(Node::Handle(&arena, h), Node::Handle(&arena, h), &mut alias).unwrap());
            assert!(independent.accounting.work > 0);
            assert_eq!(alias.accounting.work, independent.accounting.work);
        }
    }
    use crate::interpreter::budget::ExecutionLimits;
}
