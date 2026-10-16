//! Anchored `thoughtSignature` payloads. Gemini validates a signature on the part that
//! carried it (the first `functionCall` of a step), so the value names that part instead
//! of relying on where a downstream carrier placed the reasoning item.
//! Source: <https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures>.
use crate::protocol::CodecError;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Anchor {
    /// The `functionCall` part with this call ID.
    Call(String),
    /// Its own `thought: true` part; the signature is optional.
    Thought,
    /// The last text part, bound to its content rather than its current position alone.
    Tail { text_hash: [u8; 32] },
}
pub(super) fn text_hash(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Signed {
    pub anchor: Anchor,
    pub signature: Option<String>,
}
impl Signed {
    pub fn encode(&self) -> String {
        let mut o = Map::new();
        match &self.anchor {
            Anchor::Call(id) => {
                o.insert("anchor".into(), "call".into());
                o.insert("call".into(), id.as_str().into());
            }
            Anchor::Thought => {
                o.insert("anchor".into(), "thought".into());
            }
            Anchor::Tail { text_hash } => {
                o.insert("anchor".into(), "tail".into());
                o.insert("text_hash".into(), serde_json::json!(text_hash));
            }
        }
        if let Some(signature) = &self.signature {
            o.insert("signature".into(), signature.as_str().into());
        }
        Value::Object(o).to_string()
    }
    pub fn decode(value: &str) -> Result<Self, CodecError> {
        let invalid = || CodecError::Invalid("Gemini signature payload");
        let value: Value = serde_json::from_str(value).map_err(|_| invalid())?;
        let o = value.as_object().ok_or_else(invalid)?;
        if o.keys()
            .any(|k| !matches!(k.as_str(), "anchor" | "call" | "signature" | "text_hash"))
        {
            return Err(invalid());
        }
        let string = |key: &str| match o.get(key) {
            None => Ok(None),
            Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
            Some(_) => Err(invalid()),
        };
        let signature = string("signature")?;
        let call = string("call")?;
        let text_hash = o
            .get("text_hash")
            .map(|value| serde_json::from_value::<[u8; 32]>(value.clone()).map_err(|_| invalid()))
            .transpose()?;
        let anchor = match (o.get("anchor").and_then(Value::as_str), call, text_hash) {
            (Some("call"), Some(id), None) if signature.is_some() => Anchor::Call(id),
            (Some("thought"), None, None) => Anchor::Thought,
            (Some("tail"), None, Some(text_hash)) if signature.is_some() => {
                Anchor::Tail { text_hash }
            }
            _ => return Err(invalid()),
        };
        Ok(Self { anchor, signature })
    }
}
