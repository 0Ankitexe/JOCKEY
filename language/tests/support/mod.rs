#![allow(dead_code)]
use jocky_language::{SourceFile, report::CheckReport, semantic::analyze};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
pub fn binary(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jockyc"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}
pub struct DenyRetrieval;
impl jsonschema::Retrieve for DenyRetrieval {
    fn retrieve(
        &self,
        _: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("unregistered schema resource".into())
    }
}
pub fn validator(schema: &str) -> jsonschema::Validator {
    let mut options = jsonschema::draft202012::options().with_retriever(DenyRetrieval);
    for resource in ["compiler-types.schema.json", "ast-v1.schema.json"] {
        let contents: Value = serde_json::from_slice(
            &std::fs::read(root().join("shared/compiler-contracts").join(resource)).unwrap(),
        )
        .unwrap();
        let id = contents["$id"].as_str().unwrap().to_owned();
        options = options.with_resource(id, jsonschema::Resource::from_contents(contents).unwrap());
    }
    let contents: Value = serde_json::from_slice(
        &std::fs::read(root().join("shared/compiler-contracts").join(schema)).unwrap(),
    )
    .unwrap();
    options.build(&contents).unwrap()
}
pub fn assert_report(bytes: &[u8]) -> Value {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    let value: Value = serde_json::from_slice(bytes).expect("exactly one JSON document");
    let validator = VALIDATOR.get_or_init(|| validator("check-report.schema.json"));
    let errors: Vec<_> = validator
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{value}");
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert!(!bytes.ends_with(b"\n\n"));
    value
}
pub fn report(name: &str, text: &str) -> Vec<u8> {
    let source = SourceFile::new(name, text);
    let result = analyze(
        source,
        jocky_forensic::contracts::builtin_registry().unwrap(),
    );
    match &result {
        Ok(checked) => CheckReport::from_checked(source, checked),
        Err(failure) => CheckReport::from_failure(source, failure),
    }
    .unwrap()
    .to_pretty_json()
    .unwrap()
}
