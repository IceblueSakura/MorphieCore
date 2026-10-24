//! Bounded native step signatures with distinct thought and function-call owners.
use crate::{
    protocol::CodecError,
    semantic::{
        task::generation::ReplayFormat,
        value::{JsonLimits, parse_json},
    },
};
use serde_json::Value;

pub(super) enum Signed {
    Thought(Option<String>),
    Call(String),
}
impl Signed {
    pub fn format(&self) -> ReplayFormat {
        match self {
            Self::Thought(_) => ReplayFormat::GoogleInteractionsV1Thought,
            Self::Call(_) => ReplayFormat::GoogleInteractionsV1FunctionCall,
        }
    }
    pub fn signature(&self) -> Option<&str> {
        match self {
            Self::Thought(value) => value.as_deref(),
            Self::Call(value) => Some(value.as_str()),
        }
    }
    pub fn encode(&self) -> String {
        let anchor = match self {
            Self::Thought(_) => "thought",
            Self::Call(_) => "function_call",
        };
        let mut value = serde_json::json!({"anchor":anchor});
        if let Some(signature) = self.signature() {
            value["signature"] = signature.into();
        }
        value.to_string()
    }
    pub fn decode(raw: &str, format: ReplayFormat) -> Result<Self, CodecError> {
        let invalid =
            || CodecError::Invalid(crate::protocol::failure::ShapeReason::SignaturePayload);
        let value = parse_json(raw.as_bytes(), JsonLimits::STRUCTURED).map_err(|_| invalid())?;
        let o = value.as_object().ok_or_else(invalid)?;
        if o.keys()
            .any(|k| !matches!(k.as_str(), "anchor" | "signature"))
        {
            return Err(invalid());
        }
        let signature = match o.get("signature") {
            None => None,
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => return Err(invalid()),
        };
        match (format, o.get("anchor").and_then(Value::as_str)) {
            (ReplayFormat::GoogleInteractionsV1Thought, Some("thought")) => {
                Ok(Self::Thought(signature))
            }
            (ReplayFormat::GoogleInteractionsV1FunctionCall, Some("function_call")) => {
                Ok(Self::Call(signature.ok_or_else(invalid)?))
            }
            _ => Err(invalid()),
        }
    }
}
