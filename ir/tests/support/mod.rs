#![allow(dead_code)]

pub mod programs;

use jsonschema::{Resource, Retrieve, Uri, Validator};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

pub const HAND_AUTHORED: &[u8] = include_bytes!("../fixtures/valid/hand-authored.ir.json");

pub fn schemas() -> BTreeMap<&'static str, Value> {
    [
        (
            "ir.schema.json",
            include_str!("../../../shared/compiler-contracts/ir.schema.json"),
        ),
        (
            "fixture-values.schema.json",
            include_str!("../../../shared/compiler-contracts/fixture-values.schema.json"),
        ),
        (
            "fixture-bundle.schema.json",
            include_str!("../../../shared/compiler-contracts/fixture-bundle.schema.json"),
        ),
        (
            "fixture-report.schema.json",
            include_str!("../../../shared/compiler-contracts/fixture-report.schema.json"),
        ),
        (
            "compiler-types.schema.json",
            include_str!("../../../shared/compiler-contracts/compiler-types.schema.json"),
        ),
        (
            "module-registry.schema.json",
            include_str!("../../../shared/compiler-contracts/module-registry.schema.json"),
        ),
    ]
    .into_iter()
    .map(|(name, text)| (name, serde_json::from_str(text).unwrap()))
    .collect()
}

pub struct Deny;
impl Retrieve for Deny {
    fn retrieve(&self, _: &Uri<String>) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("unknown schema retrieval denied".into())
    }
}

pub fn validator(name: &str) -> &'static Validator {
    static VALIDATORS: OnceLock<BTreeMap<&'static str, Validator>> = OnceLock::new();
    &VALIDATORS.get_or_init(|| {
        let schemas = schemas();
        schemas
            .iter()
            .map(|(name, schema)| {
                let resources = schemas.values().map(|v| {
                    (
                        v["$id"].as_str().unwrap().to_owned(),
                        Resource::from_contents(v.clone()).unwrap(),
                    )
                });
                let validator = jsonschema::draft202012::options()
                    .with_retriever(Deny)
                    .with_resources(resources)
                    .should_validate_formats(true)
                    .build(schema)
                    .unwrap();
                (*name, validator)
            })
            .collect()
    })[name]
}

pub fn raw() -> Value {
    serde_json::from_slice(HAND_AUTHORED).unwrap()
}

pub fn read(text: &str) -> jocky_ir::IrDocument {
    jocky_ir::decode_ir(text.as_bytes()).unwrap()
}

pub fn document() -> jocky_ir::IrDocument {
    jocky_ir::decode_ir(HAND_AUTHORED).unwrap()
}

pub fn verify(
    document: jocky_ir::IrDocument,
) -> Result<jocky_ir::VerifiedProgram, jocky_ir::VerifyFailure> {
    jocky_ir::verify(
        document,
        jocky_forensic::contracts::builtin_registry().unwrap(),
    )
}

pub fn reidentify(document: &mut jocky_ir::IrDocument) {
    jocky_ir::identity::assign_instruction_ids(document).unwrap();
}
