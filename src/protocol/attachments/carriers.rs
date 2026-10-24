//! Client wire recognition and construction for gateway attachment manifests.
use super::*;

pub(crate) fn check_chat_support(
    profile: crate::protocol::openai::Profile,
    structured: bool,
) -> Result<(), CodecError> {
    if profile == crate::protocol::openai::Profile::Chat && !structured {
        return Err(CodecError::Unsupported("Chat attachment carrier".into()));
    }
    Ok(())
}
pub(crate) fn is_marker(value: &str) -> bool {
    value.starts_with("mca")
}
fn label(fields: &serde_json::Map<String, Value>) -> bool {
    fields.get("id").is_none_or(|id| {
        id.as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 256)
    }) && fields.get("index").is_none_or(|n| n.as_u64().is_some())
}
/// A recognized gateway carrier. An altered or misplaced one is dropped with its hidden
/// state rather than rejecting the visible history that the client returned.
pub(crate) enum Carried {
    Manifest(String),
    Unusable,
}
/// Recognized transport records are removed before ordinary reasoning decoding.
pub(crate) fn take_chat(message: &mut Value) -> Option<Carried> {
    let assistant = message["role"] == "assistant";
    let details = message
        .get_mut("reasoning_details")
        .and_then(Value::as_array_mut)?;
    let carrier = |d: &Value| {
        d["type"] == "reasoning.encrypted"
            && d.get("data").and_then(Value::as_str).is_some_and(is_marker)
    };
    let mut found = details.iter().filter(|d| carrier(d));
    let first = found.next()?;
    let usable = assistant
        && found.next().is_none()
        && first.as_object().is_some_and(|fields| {
            label(fields)
                && fields
                    .get("format")
                    .is_none_or(|f| f.as_str() == Some("openai-responses-v1"))
        });
    let value = first["data"].as_str().unwrap_or_default().to_owned();
    details.retain(|d| !carrier(d));
    Some(if usable {
        Carried::Manifest(value)
    } else {
        Carried::Unusable
    })
}
fn carrier_id(token: &str) -> String {
    format!("rs_mca_{}", &digest(&[token])[..32])
}
pub(crate) fn chat_detail(token: &str) -> Value {
    json!({
        "type": "reasoning.encrypted",
        "id": carrier_id(token),
        "format": "openai-responses-v1",
        "data": token,
    })
}
