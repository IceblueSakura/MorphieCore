//! One bounded JSON authority, shared by tool arguments and results.
use super::{GenerationError, MAX_TEXT_BYTES};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuredValue(Value);
impl StructuredValue {
    pub fn new(value: Value) -> Result<Self, GenerationError> {
        let mut nodes = 0;
        structure(&value, 0, &mut nodes)?;
        crate::semantic::value::json_size(&value, MAX_TEXT_BYTES)
            .map_err(|_| GenerationError::Limit)?;
        Ok(Self(value))
    }
    pub fn from_bytes(input: &[u8]) -> Result<Self, GenerationError> {
        use crate::semantic::value::{JsonError, JsonLimits, parse_json};
        let value = parse_json(input, JsonLimits::STRUCTURED).map_err(|error| match error {
            JsonError::Limit => GenerationError::Limit,
            JsonError::Invalid => GenerationError::InvalidJsonValue,
        })?;
        Self::new(value)
    }
    pub fn value(&self) -> &Value {
        &self.0
    }
    pub(crate) fn bytes(&self) -> Result<usize, GenerationError> {
        crate::semantic::value::json_size(&self.0, MAX_TEXT_BYTES)
            .map_err(|_| GenerationError::Limit)
    }
}
fn structure(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), GenerationError> {
    *nodes += 1;
    if depth > 32 || *nodes > 8192 {
        return Err(GenerationError::Limit);
    }
    match value {
        Value::Array(values) => {
            for value in values {
                structure(value, depth + 1, nodes)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                *nodes += 1;
                structure(value, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
