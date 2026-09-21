mod common;

use std::collections::BTreeSet;
use std::error::Error;

use jocky_language::{AstDocument, SourceFile, parse};
use serde_json::Value;

#[test]
fn every_valid_source_matches_its_ast_golden_and_schema() -> Result<(), Box<dyn Error>> {
    let pairs = common::discover_pairs("valid", "ast.json")?;
    assert!(
        pairs.len() >= 12,
        "expected at least 12 valid fixture pairs"
    );

    let schema = common::ast_schema()?;
    let validator = jsonschema::draft202012::options().build(&schema)?;
    let mut kinds = BTreeSet::new();

    for pair in pairs {
        let source = common::read_utf8(&pair.source)?;
        let program = parse(SourceFile::new(pair.source.to_str().unwrap(), &source))
            .unwrap_or_else(|diagnostics| panic!("{} failed: {diagnostics:?}", pair.name));
        let bytes = AstDocument::new(program).to_pretty_json()?;
        common::assert_exact_bytes(&bytes, &pair.oracle)?;

        let value: Value = serde_json::from_slice(&bytes)?;
        let errors = validator
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{} violates AST schema: {errors:?}",
            pair.name
        );
        collect_kinds(&value, &mut kinds);
    }

    for required in [
        "named",
        "list",
        "option",
        "result",
        "let",
        "call",
        "if",
        "for",
        "return",
        "identifier",
        "string",
        "integer",
        "boolean",
        "duration",
        "windows",
        "ubuntu",
    ] {
        assert!(
            kinds.contains(required),
            "missing AST alternative {required}"
        );
    }
    Ok(())
}

#[test]
fn semantic_anomalies_remain_syntax_valid() -> Result<(), Box<dyn Error>> {
    for name in [
        "semantic-undefined-name",
        "semantic-duplicate-functions",
        "semantic-wrong-argument-count",
        "semantic-apparent-type-mismatch",
        "semantic-apparent-target-mismatch",
        "semantic-questionable-return",
    ] {
        let path = common::fixture_root("valid").join(format!("{name}.jky"));
        let source = common::read_utf8(&path)?;
        assert!(
            parse(SourceFile::new("nonexistent-label.jky", &source)).is_ok(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn every_valid_source_is_accepted_before_oracle_comparison() -> Result<(), Box<dyn Error>> {
    let root = common::fixture_root("valid");
    let mut sources = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    sources.sort();
    sources.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("jky"));
    assert!(sources.len() >= 12);

    for path in sources {
        let source = common::read_utf8(&path)?;
        if let Err(diagnostics) = parse(SourceFile::new(path.to_str().unwrap(), &source)) {
            panic!("{} failed: {diagnostics:?}", path.display());
        }
    }
    Ok(())
}

fn collect_kinds(value: &Value, kinds: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(kind)) = object.get("kind") {
                kinds.insert(kind.clone());
            }
            for child in object.values() {
                collect_kinds(child, kinds);
            }
        }
        Value::Array(array) => {
            for child in array {
                collect_kinds(child, kinds);
            }
        }
        _ => {}
    }
}
