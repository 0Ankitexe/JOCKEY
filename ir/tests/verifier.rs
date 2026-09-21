mod support;

use jocky_ir::{
    DiagnosticCode as Code,
    model::{Operation, SlotKind},
};
use jocky_shared::compiler::{NamedType, Type};

#[test]
fn frozen_malformed_corpus_has_independent_expected_codes_and_pointers() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/invalid/verifier/manifest.json")).unwrap();
    assert!(cases.as_array().unwrap().len() >= 20);
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/invalid/verifier");
    for case in cases.as_array().unwrap() {
        let file = case["file"].as_str().unwrap();
        let bytes = std::fs::read(root.join(file)).unwrap();
        let failure = jocky_ir::decode_ir(&bytes)
            .and_then(support::verify)
            .unwrap_err();
        assert!(
            failure
                .diagnostics()
                .iter()
                .any(|d| d.code.as_str() == case["code"]
                    && d.pointer.as_deref() == case["pointer"].as_str()),
            "{file}: {failure:?}"
        );
    }
}

#[test]
fn hand_authored_valid_program_round_trips_and_retains_private_success() {
    let verified = support::verify(support::document()).unwrap();
    assert_eq!(verified.document().entry, 0);
    assert!(verified.document().module.span.is_none());
    let bytes = verified.to_json().unwrap();
    assert!(
        support::validator("ir.schema.json").is_valid(&serde_json::from_slice(&bytes).unwrap())
    );
    for _ in 0..10 {
        assert_eq!(
            support::verify(jocky_ir::decode_ir(&bytes).unwrap())
                .unwrap()
                .to_json()
                .unwrap(),
            bytes
        );
    }
}

#[test]
fn identity_reference_type_control_initialization_and_metadata_fail_closed() {
    type Mutation = (Code, fn(&mut jocky_ir::IrDocument));
    let cases: Vec<Mutation> = vec![
        (Code::Reference, |d| d.entry = 99),
        (Code::Identity, |d| {
            d.functions[0].regions[0].instructions[0].id =
                "00000000-0000-8000-8000-000000000000".into()
        }),
        (Code::Reference, |d| d.functions[0].slots[1].region = 99),
        (Code::Reference, |d| {
            d.functions[0].regions[0].instructions[0].operation = Operation::Const {
                destination: 999,
                constant: 0,
            }
        }),
        (Code::Reference, |d| {
            d.functions[0].regions[0].instructions[0].operation = Operation::Const {
                destination: 1,
                constant: 999,
            }
        }),
        (Code::Type, |d| {
            d.functions[0].slots[1].r#type = Type::named(NamedType::Bool)
        }),
        (Code::Return, |d| {
            d.functions[0].regions[0].instructions[1].operation = Operation::Return { value: 0 }
        }),
        (Code::Control, |d| {
            d.functions[0].regions.push(jocky_ir::model::Region {
                instructions: vec![],
            });
        }),
        (Code::Initialization, |d| {
            d.functions[0].slots[1].kind = SlotKind::Parameter;
            d.functions[0].parameters.push(1);
        }),
        (Code::Initialization, |d| {
            d.functions[0].regions[0].instructions[0].operation = Operation::Copy {
                destination: 1,
                source: 1,
            };
        }),
        (Code::Metadata, |d| d.entry_metadata.read_only = false),
        (Code::Metadata, |d| d.functions[0].metadata.lab_only = true),
        (Code::Contract, |d| {
            d.registry.fingerprint = "0".repeat(64);
            support::reidentify(d);
        }),
    ];
    for (expected, change) in cases {
        let mut document = support::document();
        change(&mut document);
        let error = support::verify(document).unwrap_err();
        assert!(
            error.diagnostics().iter().any(|d| d.code == expected),
            "expected {expected:?}: {error:?}"
        );
    }
    let mut no_return = support::document();
    no_return.functions[0].regions[0].instructions.pop();
    assert_eq!(support::verify(no_return).unwrap_err().code(), Code::Return);
    let mut before = support::document();
    before.functions[0].regions[0].instructions.swap(0, 1);
    support::reidentify(&mut before);
    assert_eq!(
        support::verify(before).unwrap_err().code(),
        Code::Initialization
    );
}

#[test]
fn programmatic_invalid_spans_versions_and_bounds_are_checked_too() {
    let mut span = support::document();
    span.module.span = Some(jocky_ir::model::Span { start: 2, end: 1 });
    assert!(support::verify(span).is_err());
    let mut version = support::document();
    version.language_version = "wrong".into();
    assert_eq!(support::verify(version).unwrap_err().code(), Code::Version);
    let mut label = support::document();
    label.source.label = "🙂".repeat(1025);
    assert_eq!(support::verify(label).unwrap_err().code(), Code::Resource);
}
