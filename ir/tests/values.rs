mod support;

use jocky_ir::{
    DiagnosticCode,
    value::{Value, validate},
};
use jocky_shared::compiler::{NamedType, Type};
use serde_json::json;

#[test]
fn deeply_nested_options_results_and_lists_keep_exact_types_or_fail_bounded() {
    use jocky_ir::value::{ResultVariant, arena::ValueArena};
    for error in [false, true] {
        let mut value = Value::Int { value: "1".into() };
        let mut ty = Type::named(NamedType::Int);
        for depth in 2..=65 {
            let other = Type::named(NamedType::String);
            value = Value::Result {
                ok_type: if error { other.clone() } else { ty.clone() },
                error_type: if error { ty.clone() } else { other.clone() },
                variant: if error {
                    ResultVariant::Error
                } else {
                    ResultVariant::Ok
                },
                value: Box::new(value),
            };
            ty = Type::Result {
                ok: Box::new(if error { other.clone() } else { ty.clone() }),
                error: Box::new(if error { ty.clone() } else { other }),
            };
            if depth == 64 {
                assert_eq!(validate(&value).unwrap(), ty);
            }
        }
        assert_eq!(
            validate(&value).unwrap_err().code(),
            DiagnosticCode::Resource
        );
        assert!(ValueArena::default().insert(value).is_err());
    }
    let mut value = Value::Int {
        value: "9".repeat(10_000),
    };
    let mut ty = Type::named(NamedType::Int);
    for _ in 0..20 {
        value = Value::List {
            element_type: ty.clone(),
            values: vec![value],
        };
        ty = Type::List {
            element: Box::new(ty),
        };
    }
    assert_eq!(validate(&value).unwrap(), ty);
    let mut arena = ValueArena::default();
    let root = arena.insert(value).unwrap();
    assert_eq!(arena.len(), 21);
    assert!(matches!(
        arena.get(root),
        Some(jocky_ir::value::arena::ArenaNode::List { .. })
    ));
}

#[test]
fn aggregate_arena_nodes_and_scalar_bytes_reach_exact_caps_without_resetting() {
    use jocky_ir::{limits, value::arena::ValueArena};
    let mut arena = ValueArena::default();
    for count in std::iter::repeat_n(10_000, 19).chain([9980]) {
        arena
            .insert(Value::List {
                element_type: Type::named(NamedType::Bool),
                values: vec![Value::Bool { value: true }; count],
            })
            .unwrap();
    }
    assert_eq!(arena.len(), limits::VALUE_NODES);
    let before = arena.storage_bytes();
    assert_eq!(
        arena
            .insert(Value::Bool { value: false })
            .unwrap_err()
            .code(),
        DiagnosticCode::Resource
    );
    assert_eq!(arena.storage_bytes(), before);
    let mut arena = ValueArena::default();
    for _ in 0..8 {
        arena
            .insert(Value::String {
                value: "x".repeat(1024 * 1024),
            })
            .unwrap();
    }
    assert_eq!(arena.storage_bytes(), limits::FIXTURE_BYTES + 8 * 64);
    assert!(arena.insert(Value::String { value: "x".into() }).is_err());
    assert_eq!(arena.len(), 8);
}

#[test]
fn all_named_kinds_and_generic_variants_are_exact_data() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/valid/ubuntu/fixture.json")).unwrap();
    let context = json!({"record_id":"00000000-0000-4000-8000-000000000030","endpoint_id":fixture["endpoint"]["endpoint_id"],"observed_at":fixture["observed_at"]});
    for name in NamedType::ALL {
        let raw = match name {
            NamedType::Bool => json!({"kind":"bool","value":true}),
            NamedType::Int => json!({"kind":"int","value":"-123456789012345678901234567890"}),
            NamedType::String => json!({"kind":"string","value":"🙂\n"}),
            NamedType::Bytes => json!({"kind":"bytes","value":"00ff"}),
            NamedType::Duration => {
                json!({"kind":"duration","magnitude":"123456789012345678901234567890","unit":"d"})
            }
            NamedType::Timestamp => json!({"kind":"timestamp","value":fixture["observed_at"]}),
            NamedType::Endpoint => json!({"kind":"endpoint","value":fixture["endpoint"]}),
            NamedType::Platform => json!({"kind":"platform","value":"ubuntu"}),
            NamedType::IpAddress => json!({"kind":"ip_address","value":"2001:db8::1"}),
            NamedType::Path => json!({"kind":"path","value":"../../not-to-be-opened"}),
            NamedType::ForensicResult => {
                fixture["providers"]["forensic.system.profile"]["value"].clone()
            }
            NamedType::DiagnosticError => json!({"kind":"diagnostic_error","code":"Unsupported"}),
            name => {
                let mut payload = context.clone();
                let fields = match name {
                    NamedType::ProcessRecord => {
                        json!({"pid":"1","name":"synthetic","image_path":"/inert"})
                    }
                    NamedType::ConnectionRecord => {
                        json!({"process_record_id":null,"protocol":"tcp","local_address":"127.0.0.1","local_port":0,"remote_address":"192.0.2.1","remote_port":65535})
                    }
                    NamedType::FileRecord => json!({"path":"/inert","size":"0","sha256":null}),
                    NamedType::EventRecord => {
                        json!({"source":"fixture","event_code":"1","message":"authored"})
                    }
                    NamedType::PersistenceRecord => {
                        json!({"mechanism":"fixture","location":"inert","description":"authored"})
                    }
                    NamedType::DriverRecord => {
                        json!({"name":"fixture","path":"inert","sha256":null})
                    }
                    _ => unreachable!(),
                };
                payload
                    .as_object_mut()
                    .unwrap()
                    .extend(fields.as_object().unwrap().clone());
                json!({"kind":name.as_str(),"value":payload})
            }
        };
        assert!(
            support::validator("fixture-values.schema.json").is_valid(&raw),
            "{name}"
        );
        let value: Value = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(validate(&value).unwrap(), Type::named(name));
        assert_eq!(serde_json::to_value(&value).unwrap(), raw);
    }
    for raw in [
        json!({"kind":"list","element_type":{"kind":"named","name":"int"},"values":[]}),
        json!({"kind":"option","element_type":{"kind":"named","name":"int"},"value":null}),
        json!({"kind":"option","element_type":{"kind":"named","name":"int"},"value":{"kind":"int","value":"1"}}),
        json!({"kind":"result","ok_type":{"kind":"named","name":"int"},"error_type":{"kind":"named","name":"diagnostic_error"},"variant":"ok","value":{"kind":"int","value":"1"}}),
        json!({"kind":"result","ok_type":{"kind":"named","name":"int"},"error_type":{"kind":"named","name":"diagnostic_error"},"variant":"error","value":{"kind":"diagnostic_error","code":"Unsupported"}}),
    ] {
        let value: Value = serde_json::from_value(raw.clone()).unwrap();
        assert!(validate(&value).is_ok());
        assert_eq!(serde_json::to_value(value).unwrap(), raw);
    }
}

#[test]
fn malformed_scalars_and_schema_valid_wrong_children_never_coerce() {
    for raw in [
        json!({"kind":"int","value":"-0"}),
        json!({"kind":"int","value":"01"}),
        json!({"kind":"bytes","value":"0fA"}),
        json!({"kind":"duration","magnitude":"-1","unit":"s"}),
        json!({"kind":"timestamp","value":"not-a-time"}),
        json!({"kind":"ip_address","value":"999.1.2.3"}),
        json!({"kind":"list","element_type":{"kind":"named","name":"int"},"values":[{"kind":"bool","value":true}]}),
        json!({"kind":"option","element_type":{"kind":"named","name":"int"},"value":{"kind":"bool","value":true}}),
        json!({"kind":"result","ok_type":{"kind":"named","name":"int"},"error_type":{"kind":"named","name":"diagnostic_error"},"variant":"error","value":{"kind":"int","value":"1"}}),
    ] {
        let value: Value = serde_json::from_value(raw).unwrap();
        assert!(validate(&value).is_err());
    }
    let huge = Value::List {
        element_type: Type::named(NamedType::Bool),
        values: vec![Value::Bool { value: false }; 10_001],
    };
    assert_eq!(
        validate(&huge).unwrap_err().code(),
        DiagnosticCode::Resource
    );
}
