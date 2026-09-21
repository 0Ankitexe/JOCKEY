#![allow(dead_code)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixturePair {
    pub name: String,
    pub source: PathBuf,
    pub oracle: PathBuf,
}

pub fn fixture_root(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
}

pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("language crate must be directly below the repository root")
        .to_path_buf()
}

pub fn discover_pairs(kind: &str, oracle_suffix: &str) -> Result<Vec<FixturePair>, Box<dyn Error>> {
    let root = fixture_root(kind);
    let mut sources = BTreeSet::new();
    let mut oracles = BTreeSet::new();

    for entry in fs::read_dir(&root)? {
        let path = entry?.path();
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("fixture path is not valid UTF-8: {}", path.display()))?;
        if let Some(name) = filename.strip_suffix(".jky") {
            sources.insert(name.to_owned());
        } else if let Some(name) = filename.strip_suffix(&format!(".{oracle_suffix}")) {
            oracles.insert(name.to_owned());
        }
    }

    if sources != oracles {
        return Err(format!(
            "fixture names differ in {}: sources={sources:?}, oracles={oracles:?}",
            root.display()
        )
        .into());
    }

    Ok(sources
        .into_iter()
        .map(|name| FixturePair {
            source: root.join(format!("{name}.jky")),
            oracle: root.join(format!("{name}.{oracle_suffix}")),
            name,
        })
        .collect())
}

pub fn read_utf8(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(path)?)
}

pub fn fixture_label(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(path
        .strip_prefix(repository_root())?
        .to_str()
        .ok_or_else(|| format!("fixture path is not valid UTF-8: {}", path.display()))?
        .replace('\\', "/"))
}

pub fn assert_exact_bytes(actual: &[u8], expected_path: &Path) -> Result<(), Box<dyn Error>> {
    let expected = fs::read(expected_path)?;
    assert_eq!(
        actual,
        expected,
        "byte mismatch for {}",
        expected_path.display()
    );
    Ok(())
}

pub fn normalized_json(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let value: Value = serde_json::from_slice(bytes)?;
    let mut normalized = serde_json::to_vec_pretty(&value)?;
    normalized.push(b'\n');
    Ok(normalized)
}

pub fn ast_schema() -> Result<Value, Box<dyn Error>> {
    let path = repository_root()
        .join("specs/001-language-parser-diagnostics/contracts/ast-json.schema.json");
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::normalized_json;

    #[test]
    fn normalizes_json_with_stable_indentation_and_final_lf() {
        assert_eq!(
            normalized_json(br#"{"b":2,"a":1}"#).expect("valid JSON"),
            b"{\n  \"a\": 1,\n  \"b\": 2\n}\n"
        );
    }
}
