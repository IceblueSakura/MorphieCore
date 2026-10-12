//! A signature belongs to its thought or to a call ID, never its carrier position.
use crate::{
    protocol::CodecError,
    semantic::value::{JsonLimits, parse_json},
};
use serde_json::Value;

pub(super) struct Signed {
    pub call: Option<String>,
    pub signature: Option<String>,
}
impl Signed {
    pub fn encode(&self) -> String {
        let mut value = match &self.call {
            Some(call) => serde_json::json!({"anchor":"call","call":call}),
            None => serde_json::json!({"anchor":"thought"}),
        };
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
            .any(|k| !matches!(k.as_str(), "anchor" | "call" | "signature"))
        {
            return Err(invalid());
        }
        let field = |key| match o.get(key) {
            None => Ok(None),
            Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
            _ => Err(invalid()),
        };
        let call = field("call")?;
        let signature = field("signature")?;
        match (o.get("anchor").and_then(Value::as_str), &call, &signature) {
            (Some("thought"), None, _) | (Some("call"), Some(_), Some(_)) => {}
            _ => return Err(invalid()),
        }
        Ok(Self { call, signature })
    }
}
