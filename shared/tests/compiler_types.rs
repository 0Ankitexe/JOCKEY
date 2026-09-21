use jocky_shared::compiler::{Capability, NamedType, Platform, Privilege, Type};
use serde_json::{Value, json};

#[test]
fn every_named_type_and_generic_round_trips_against_the_schema() {
    let schema: Value = serde_json::from_str(include_str!(
        "../compiler-contracts/compiler-types.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert_eq!(NamedType::ALL.len(), 18);
    for name in NamedType::ALL {
        assert_eq!(NamedType::from_name(name.as_str()), Some(name));
        let ty = Type::Result {
            ok: Box::new(Type::List {
                element: Box::new(Type::named(name)),
            }),
            error: Box::new(Type::Option {
                element: Box::new(Type::named(NamedType::DiagnosticError)),
            }),
        };
        let value = serde_json::to_value(&ty).unwrap();
        assert!(validator.is_valid(&value));
        assert_eq!(serde_json::from_value::<Type>(value).unwrap(), ty);
        assert_eq!(
            ty.to_string(),
            format!("result<list<{}>, option<diagnostic_error>>", name.as_str())
        );
    }
}

#[test]
fn invalid_types_are_closed_and_domains_do_not_coerce() {
    for value in [
        json!({"kind":"any"}),
        json!({"kind":"named","name":"float"}),
        json!({"kind":"named","name":"int","extra":1}),
        json!({"kind":"list"}),
        json!({"kind":"result","ok":{"kind":"named","name":"int"}}),
    ] {
        assert!(serde_json::from_value::<Type>(value).is_err());
    }
    assert_eq!(NamedType::from_name("Int"), None);
    assert_ne!(Type::named(NamedType::String), Type::named(NamedType::Path));
    assert_ne!(
        Type::named(NamedType::Bytes),
        Type::List {
            element: Box::new(Type::named(NamedType::Int))
        }
    );
}

#[test]
fn metadata_order_and_closed_spellings_match_contracts() {
    assert_eq!(Platform::ALL.map(Platform::as_str), ["windows", "ubuntu"]);
    assert_eq!(
        Capability::ALL.map(Capability::as_str),
        [
            "system",
            "processes",
            "network",
            "files",
            "events",
            "persistence",
            "drivers",
            "indicators"
        ]
    );
    assert!(Privilege::User < Privilege::Elevated && Privilege::Elevated < Privilege::LabOnly);
    assert!(serde_json::from_str::<Platform>("\"linux\"").is_err());
    assert!(serde_json::from_str::<Privilege>("\"root\"").is_err());
    assert!(serde_json::from_str::<Capability>("\"execute\"").is_err());
}
