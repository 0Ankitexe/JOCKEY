//! Bounded JSON decoding which rejects duplicate keys before map insertion.
use super::{RegistryError, RegistryErrorKind};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

pub(super) fn read(text: &str) -> Result<Value, RegistryError> {
    if text.len() > 1_048_576 {
        return Err(RegistryError::new(
            RegistryErrorKind::ResourceLimit,
            "registry-bytes",
        ));
    }
    let mut problem = None;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let result = Seed {
        depth: 0,
        problem: &mut problem,
    }
    .deserialize(&mut decoder);
    if let Some(error) = problem {
        return Err(error);
    }
    let value =
        result.map_err(|_| RegistryError::new(RegistryErrorKind::InvalidJson, "invalid-json"))?;
    decoder
        .end()
        .map_err(|_| RegistryError::new(RegistryErrorKind::InvalidJson, "trailing-json"))?;
    Ok(value)
}

struct Seed<'a> {
    depth: usize,
    problem: &'a mut Option<RegistryError>,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        decoder.deserialize_any(self)
    }
}
impl Seed<'_> {
    fn container<E: de::Error>(&mut self) -> Result<(), E> {
        if self.depth >= 96 {
            *self.problem = Some(RegistryError::new(
                RegistryErrorKind::ResourceLimit,
                "json-nesting",
            ));
            return Err(E::custom("registry limit"));
        }
        Ok(())
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("invalid number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<Value, A::Error> {
        self.container()?;
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            depth: self.depth + 1,
            problem: self.problem,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Value, A::Error> {
        self.container()?;
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                *self.problem = Some(RegistryError::new(
                    RegistryErrorKind::DuplicateJsonKey,
                    "duplicate-json-key",
                ));
                return Err(de::Error::custom("duplicate key"));
            }
            let value = map.next_value_seed(Seed {
                depth: self.depth + 1,
                problem: self.problem,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicates_at_any_depth_and_trailing_input_fail() {
        assert_eq!(
            read(r#"{"a":{"x":1,"x":2}}"#).unwrap_err().kind,
            RegistryErrorKind::DuplicateJsonKey
        );
        assert_eq!(
            read("null true").unwrap_err().kind,
            RegistryErrorKind::InvalidJson
        );
        assert_eq!(
            read(r#"[null,1,-2,1.2,true,"s"]"#)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }

    #[test]
    fn raw_nesting_allows_exactly_ninety_six_containers() {
        assert!(read(&format!("{}0{}", "[".repeat(96), "]".repeat(96))).is_ok());
        assert_eq!(
            read(&format!("{}0{}", "[".repeat(97), "]".repeat(97)))
                .unwrap_err()
                .kind,
            RegistryErrorKind::ResourceLimit
        );
    }
}
