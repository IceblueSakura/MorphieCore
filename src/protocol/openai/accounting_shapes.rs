//! Closed, source-bound accounting views, not substitutes for Generation usage.
//! Product response metadata is not authority to invoke programs or hosted tools.
use super::{
    CodecError,
    adapter_shapes::Extras,
    common::{fields, object},
};
use serde_json::{Map, Value, json};
fn take(o: &mut Map<String, Value>, key: &str, path: &str, extras: &mut Extras) {
    if let Some(value) = o.shift_remove(key) {
        extras.insert(path.into(), value);
    }
}
pub(super) fn product(o: &mut Map<String, Value>, extras: &mut Extras) -> Result<(), CodecError> {
    if let Some(value) = o.get("access_programs") {
        let programs = object(value)?;
        fields(programs, &["cyber"])?;
        if programs.get("cyber").is_some_and(|v| {
            !v.as_str().is_some_and(|s| {
                !s.is_empty()
                    && s.len() <= 64
                    && s.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            })
        }) {
            return Err(CodecError::Invalid("access program report"));
        }
        take(o, "access_programs", "/access_programs", extras);
    }
    if let Some(value) = o.get("moderation") {
        if !value.is_null() {
            return Err(CodecError::Unsupported("active moderation report".into()));
        }
        take(o, "moderation", "/moderation", extras);
    }
    if let Some(value) = o.get("tool_usage") {
        // No hosted-tool accounting semantics are admitted by the text slice.
        let expected = json!({"image_gen":{"input_tokens":0,"input_tokens_details":{"image_tokens":0,"text_tokens":0},"output_tokens":0,"output_tokens_details":{"image_tokens":0,"text_tokens":0},"total_tokens":0},"web_search":{"num_requests":0}});
        if value != &expected {
            return Err(CodecError::Unsupported("active tool accounting".into()));
        }
        take(o, "tool_usage", "/tool_usage", extras);
    }
    if let Some(usage) = o.get_mut("usage").and_then(Value::as_object_mut)
        && let Some(value) = usage.get("attribution")
    {
        attribution(value)?;
        take(usage, "attribution", "/usage/attribution", extras);
    }
    Ok(())
}
// Attribution uses upstream input/output coordinates, not local ItemId. Keep
// the entire closed view source-bound; never attach it to a new semantic owner
// or sum it into billed totals (context and billing can account differently).
fn attribution(value: &Value) -> Result<(), CodecError> {
    let attribution = object(value)?;
    fields(attribution, &["items"])?;
    let items = object(
        attribution
            .get("items")
            .ok_or(CodecError::Invalid("usage attribution"))?,
    )?;
    if items.len() > 64 {
        return Err(CodecError::Limit);
    }
    let counts = |value: &Value, item: bool| -> Result<(), CodecError> {
        let value = object(value)?;
        let keys = [
            "input_tokens",
            "output_tokens",
            "cached_tokens",
            "cache_write_tokens",
        ];
        let mut allowed = keys.to_vec();
        if item {
            allowed.push("content");
        }
        fields(value, &allowed)?;
        if keys
            .iter()
            .any(|key| value.get(*key).and_then(Value::as_u64).is_none())
        {
            return Err(CodecError::Invalid("attribution counter"));
        }
        Ok(())
    };
    for (id, item) in items {
        if id.is_empty() || id.len() > 128 || !id.bytes().all(|c| c.is_ascii_graphic()) {
            return Err(CodecError::Invalid("attribution coordinate"));
        }
        counts(item, true)?;
        let content = item
            .get("content")
            .and_then(Value::as_array)
            .ok_or(CodecError::Invalid("attribution content"))?;
        if content.len() > 64 {
            return Err(CodecError::Limit);
        }
        for part in content {
            counts(part, false)?;
        }
    }
    Ok(())
}
/// Billing ticks and context-window counters are not per-call billed token
/// counts. Retain exact integers in scoped fidelity; never sum or relabel them.
/// Source: https://docs.x.ai/developers/rest-api-reference/inference/responses
pub(super) fn context(o: &mut Map<String, Value>, extras: &mut Extras) -> Result<(), CodecError> {
    let Some(usage) = o.get_mut("usage").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for name in ["num_sources_used", "num_server_side_tools_used"] {
        if let Some(value) = usage.get(name) {
            if value.as_u64() != Some(0) {
                return Err(CodecError::Unsupported("active hosted tool usage".into()));
            }
            take(usage, name, &format!("/usage/{name}"), extras);
        }
    }
    if let Some(value) = usage.get("cost_in_usd_ticks") {
        if value.as_u64().is_none() {
            return Err(CodecError::Invalid("cost ticks"));
        }
        take(
            usage,
            "cost_in_usd_ticks",
            "/usage/cost_in_usd_ticks",
            extras,
        );
    }
    if let Some(value) = usage.get("context_details") {
        let details = object(value)?;
        fields(details, &["input_tokens", "output_tokens"])?;
        if details.values().any(|v| v.as_u64().is_none()) {
            return Err(CodecError::Invalid("context accounting"));
        }
        take(usage, "context_details", "/usage/context_details", extras);
    }
    Ok(())
}
