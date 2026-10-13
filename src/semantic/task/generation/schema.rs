//! One authoritative JSON-object schema, kept exactly. Vocabularies and subsets are
//! target constraints checked at protocol boundaries; this is not a schema validator.
use crate::semantic::task::generation::{GenerationError, MAX_TEXT_BYTES};
use serde_json::Value;

const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_NODES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaDocument(Value);
impl From<Value> for SchemaDocument {
    /// Raw input. Owning request/settings validation remains mandatory.
    fn from(root: Value) -> Self {
        Self(root)
    }
}
impl SchemaDocument {
    pub fn root(&self) -> &Value {
        &self.0
    }
    pub fn root_mut(&mut self) -> &mut Value {
        &mut self.0
    }
    /// Every official API types a schema as a JSON object. Bound the data before
    /// callers serialize or traverse it.
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if !self.0.is_object() {
            return Err(GenerationError::InvalidSchema);
        }
        let mut nodes = 0;
        preflight(&self.0, 0, &mut nodes)?;
        crate::semantic::value::json_size(&self.0, MAX_TEXT_BYTES)
            .map_err(|_| GenerationError::Limit)
    }
}
fn preflight(v: &Value, depth: usize, nodes: &mut usize) -> Result<(), GenerationError> {
    charge(nodes)?;
    match v {
        Value::Object(o) => {
            if depth >= MAX_JSON_DEPTH {
                return Err(GenerationError::Limit);
            }
            for child in o.values() {
                // Keys are nodes too: an object of empty values is not free.
                charge(nodes)?;
                preflight(child, depth + 1, nodes)?;
            }
        }
        Value::Array(a) => {
            if depth >= MAX_JSON_DEPTH {
                return Err(GenerationError::Limit);
            }
            for child in a {
                preflight(child, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn charge(nodes: &mut usize) -> Result<(), GenerationError> {
    *nodes += 1;
    if *nodes > MAX_JSON_NODES {
        return Err(GenerationError::Limit);
    }
    Ok(())
}
