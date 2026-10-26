//! Thought-owned native state; ordinary text/call signatures live on their actual IR owners.
use crate::{
    protocol::CodecError,
    semantic::value::{JsonLimits, parse_json},
};
use serde_json::{Map, Value};

pub(super) struct Signed {
    pub signature: Option<String>,
}
impl Signed {
    pub fn encode(&self) -> String {
        let mut value = serde_json::json!({"anchor":"thought"});
        if let Some(signature) = &self.signature {
            value["signature"] = signature.as_str().into();
        }
        value.to_string()
    }
    pub fn decode(value: &str) -> Result<Self, CodecError> {
        let invalid = || CodecError::invalid();
        let value = parse_json(value.as_bytes(), JsonLimits::STRUCTURED).map_err(|_| invalid())?;
        let fields: &Map<String, Value> = value.as_object().ok_or_else(invalid)?;
        if fields
            .keys()
            .any(|k| !matches!(k.as_str(), "anchor" | "signature"))
            || fields.get("anchor").and_then(Value::as_str) != Some("thought")
        {
            return Err(invalid());
        }
        let signature = match fields.get("signature") {
            None => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            _ => return Err(invalid()),
        };
        Ok(Self { signature })
    }
}
/// A signature-only payload from the removed `call`/`tail` surrogate format. Its owner
/// cannot be identified safely in returned history, so callers drop it with a receipt.
pub(super) fn is_legacy(value: &str) -> bool {
    parse_json(value.as_bytes(), JsonLimits::STRUCTURED).is_ok_and(|value| {
        matches!(
            value.get("anchor").and_then(Value::as_str),
            Some("call" | "tail")
        )
    })
}
