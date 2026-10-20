//! The selected Interactions signature carrier belongs to a thought step.
use crate::{
    protocol::CodecError,
    semantic::value::{JsonLimits, parse_json},
};
use serde_json::Value;

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
    pub fn decode(raw: &str) -> Result<Self, CodecError> {
        let invalid = || CodecError::Invalid("Interactions signature payload");
        let value = parse_json(raw.as_bytes(), JsonLimits::STRUCTURED).map_err(|_| invalid())?;
        let o = value.as_object().ok_or_else(invalid)?;
        if o.keys()
            .any(|k| !matches!(k.as_str(), "anchor" | "signature"))
        {
            return Err(invalid());
        }
        let field = |key| match o.get(key) {
            None => Ok(None),
            Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
            _ => Err(invalid()),
        };
        let signature = field("signature")?;
        if o.get("anchor").and_then(Value::as_str) != Some("thought") {
            return Err(invalid());
        }
        Ok(Self { signature })
    }
}
