//! Bounded, strict JSON and embedded, deny-retrieval compiler schemas.
mod reader;
pub(crate) use reader::read_json;
pub(crate) use reader::read_json_metered;

use crate::{
    diagnostic::{DiagnosticCode as Code, Failure},
    limits,
    model::IrDocument,
};
use jsonschema::{Resource, Retrieve, Uri, Validator};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::{self, Write},
    sync::OnceLock,
};

#[derive(Clone, Copy)]
pub(crate) enum Schema {
    Ir,
    Values,
    Bundle,
    Report,
}
impl Schema {
    fn index(self) -> usize {
        match self {
            Self::Ir => 0,
            Self::Values => 1,
            Self::Bundle => 2,
            Self::Report => 3,
        }
    }
}

const SCHEMAS: [&str; 6] = [
    include_str!("../../shared/compiler-contracts/ir.schema.json"),
    include_str!("../../shared/compiler-contracts/fixture-values.schema.json"),
    include_str!("../../shared/compiler-contracts/fixture-bundle.schema.json"),
    include_str!("../../shared/compiler-contracts/fixture-report.schema.json"),
    include_str!("../../shared/compiler-contracts/compiler-types.schema.json"),
    include_str!("../../shared/compiler-contracts/module-registry.schema.json"),
];

struct Deny;
impl Retrieve for Deny {
    fn retrieve(&self, _: &Uri<String>) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("unknown compiler schema resource".into())
    }
}

fn validators() -> Result<&'static [Validator], Failure> {
    static CACHE: OnceLock<Result<Vec<Validator>, Failure>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let schemas = SCHEMAS
                .iter()
                .map(|text| {
                    serde_json::from_str::<Value>(text).map_err(|_| Failure::at(Code::Schema, ""))
                })
                .collect::<Result<Vec<_>, _>>()?;
            schemas[..4]
                .iter()
                .map(|schema| {
                    let resources = schemas
                        .iter()
                        .map(|v| {
                            let id = v["$id"]
                                .as_str()
                                .ok_or_else(|| Failure::at(Code::Schema, ""))?;
                            Ok((
                                id.to_owned(),
                                Resource::from_contents(v.clone())
                                    .map_err(|_| Failure::at(Code::Schema, ""))?,
                            ))
                        })
                        .collect::<Result<Vec<_>, Failure>>()?;
                    jsonschema::draft202012::options()
                        .with_retriever(Deny)
                        .with_resources(resources.into_iter())
                        .should_validate_formats(true)
                        .build(schema)
                        .map_err(|_| Failure::at(Code::Schema, ""))
                })
                .collect()
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

pub(crate) fn validate_schema(schema: Schema, value: &Value) -> Result<(), Failure> {
    validators()?[schema.index()]
        .validate(value)
        .map_err(|e| Failure::at(Code::Schema, e.instance_path.to_string()))
}

pub(crate) fn version(value: &Value) -> Result<(), Failure> {
    for (field, want) in [
        ("schema_version", crate::model::SCHEMA_VERSION),
        ("language_version", crate::model::LANGUAGE_VERSION),
    ] {
        if value
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|v| v != want)
        {
            return Err(Failure::at(Code::Version, format!("/{field}")));
        }
    }
    if value
        .pointer("/registry/schema_version")
        .and_then(Value::as_str)
        .is_some_and(|v| v != crate::model::REGISTRY_VERSION)
    {
        return Err(Failure::at(Code::Version, "/registry/schema_version"));
    }
    Ok(())
}

/// Strict decoding returns untrusted data, never a VerifiedProgram.
pub fn decode_ir(bytes: &[u8]) -> Result<IrDocument, Failure> {
    let value = read_json(bytes, limits::IR_BYTES)?;
    version(&value)?;
    validate_schema(Schema::Ir, &value)?;
    let document: IrDocument =
        serde_json::from_value(value).map_err(|_| Failure::at(Code::Schema, ""))?;
    // Avoid returning excessively deep typed trees, including hand-constructed
    // generic annotations which a recursive schema alone cannot depth-bound.
    crate::verify::census(&document)?;
    Ok(document)
}

pub(crate) struct BoundedWriter {
    pub bytes: Vec<u8>,
    maximum: usize,
    exhausted: bool,
}
impl BoundedWriter {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            exhausted: false,
        }
    }
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.maximum)
        {
            self.exhausted = true;
            return Err(io::Error::other("IR output bound"));
        }
        if self.bytes.try_reserve(bytes.len()).is_err() {
            self.exhausted = true;
            return Err(io::Error::other("IR output allocation"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn serialize_bounded(
    value: &impl Serialize,
    maximum: usize,
) -> Result<Vec<u8>, Failure> {
    serialize_reserved(value, maximum, Vec::new())
}

pub(crate) fn serialize_reserved(
    value: &impl Serialize,
    maximum: usize,
    mut buffer: Vec<u8>,
) -> Result<Vec<u8>, Failure> {
    buffer.clear();
    let mut writer = BoundedWriter::new(maximum);
    writer.bytes = buffer;
    if serde_json::to_writer_pretty(&mut writer, value).is_err() {
        return Err(if writer.exhausted {
            Failure::resource()
        } else {
            Failure::at(Code::Schema, "")
        });
    }
    writer.write_all(b"\n").map_err(|_| Failure::resource())?;
    Ok(writer.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_output_boundary_includes_lf_and_denies_unknown_resources() {
        assert_eq!(serialize_bounded(&true, 5).unwrap(), b"true\n");
        assert_eq!(
            serialize_bounded(&true, 4).unwrap_err().code(),
            Code::Resource
        );
        assert!(
            jsonschema::draft202012::options()
                .with_retriever(Deny)
                .build(&serde_json::json!({"$ref":"https://example.invalid/x"}))
                .is_err()
        );
        assert_eq!(validators().unwrap().len(), 4);
    }
}
