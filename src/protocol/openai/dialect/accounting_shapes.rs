//! Closed, source-bound accounting views, not substitutes for Generation usage.
//! Product response metadata is not authority to invoke programs or hosted tools.
use crate::protocol::openai::{
    CodecError, dialect::adapter_shapes::Extras, shape::DecodeDirection,
};
use serde_json::{Map, Value, json};
fn take(o: &mut Map<String, Value>, key: &str, path: &str, extras: &mut Extras) {
    if let Some(value) = o.shift_remove(key) {
        extras.insert(path.into(), value);
    }
}
pub(in crate::protocol::openai) fn programs(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    if let Some(value) = o.get_mut("access_programs") {
        let programs = value
            .as_object_mut()
            .ok_or(CodecError::Invalid("access program report"))?;
        direction.fields(programs, &["cyber"])?;
        programs.retain(|name, _| name == "cyber");
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
    Ok(())
}
pub(in crate::protocol::openai) fn inactive_reports(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
) -> Result<(), CodecError> {
    for field in ["frequency_penalty", "presence_penalty"] {
        if let Some(value) = o.get(field) {
            if value.as_f64() != Some(0.0) {
                return Err(CodecError::Unsupported("active penalty report".into()));
            }
            take(o, field, &format!("/{field}"), extras);
        }
    }
    null_moderation(o, extras)
}
fn null_moderation(o: &mut Map<String, Value>, extras: &mut Extras) -> Result<(), CodecError> {
    if let Some(value) = o.get("moderation") {
        if !value.is_null() {
            return Err(CodecError::Unsupported("active moderation report".into()));
        }
        take(o, "moderation", "/moderation", extras);
    }
    Ok(())
}
pub(in crate::protocol::openai) fn product(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    programs(o, extras, direction)?;
    null_moderation(o, extras)?;
    zero_tools(o, extras)?;
    usage_attribution(o, extras, direction)
}
pub(in crate::protocol::openai) fn usage_attribution(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    if let Some(usage) = o.get_mut("usage").and_then(Value::as_object_mut)
        && let Some(value) = usage.get_mut("attribution")
    {
        attribution(value, direction)?;
        take(usage, "attribution", "/usage/attribution", extras);
    }
    Ok(())
}
pub(in crate::protocol::openai) fn zero_tools(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
) -> Result<(), CodecError> {
    if let Some(value) = o.get("tool_usage") {
        // No hosted-tool accounting semantics are admitted by the text slice.
        let expected = json!({"image_gen":{"input_tokens":0,"input_tokens_details":{"image_tokens":0,"text_tokens":0},"output_tokens":0,"output_tokens_details":{"image_tokens":0,"text_tokens":0},"total_tokens":0},"web_search":{"num_requests":0}});
        if value != &expected {
            return Err(CodecError::Unsupported("active tool accounting".into()));
        }
        take(o, "tool_usage", "/tool_usage", extras);
    }
    Ok(())
}
// Attribution uses upstream input/output coordinates, not local ItemId. Keep
// the entire closed view source-bound; never attach it to a new semantic owner
// or sum it into billed totals (context and billing can account differently).
fn attribution(value: &mut Value, direction: DecodeDirection) -> Result<(), CodecError> {
    let attribution = value
        .as_object_mut()
        .ok_or(CodecError::Invalid("usage attribution"))?;
    direction.fields(attribution, &["items", "request_fields"])?;
    attribution.retain(|name, _| matches!(name.as_str(), "items" | "request_fields"));
    let items = attribution
        .get_mut("items")
        .and_then(Value::as_object_mut)
        .ok_or(CodecError::Invalid("usage attribution"))?;
    if items.len() > 64 {
        return Err(CodecError::Limit);
    }
    let counts = |value: &mut Value, item: bool| -> Result<(), CodecError> {
        let value = value
            .as_object_mut()
            .ok_or(CodecError::Invalid("attribution counter"))?;
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
        direction.fields(value, &allowed)?;
        value.retain(|name, _| allowed.contains(&name.as_str()));
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
        let Some(content) = item.get_mut("content") else {
            continue;
        };
        let content = content
            .as_array_mut()
            .ok_or(CodecError::Invalid("attribution content"))?;
        if content.len() > 64 {
            return Err(CodecError::Limit);
        }
        for part in content {
            counts(part, false)?;
        }
    }
    if let Some(request_fields) = attribution.get_mut("request_fields") {
        let request_fields = request_fields
            .as_object_mut()
            .ok_or(CodecError::Invalid("attribution request fields"))?;
        direction.fields(request_fields, &["tools"])?;
        request_fields.retain(|name, _| name == "tools");
        for value in request_fields.values_mut() {
            counts(value, false)?;
        }
    }
    Ok(())
}
/// Billing ticks and context-window counters are not per-call billed token
/// counts. Retain exact integers in scoped fidelity; never sum or relabel them.
/// Source: https://docs.x.ai/developers/rest-api-reference/inference/responses
pub(in crate::protocol::openai) fn context(
    o: &mut Map<String, Value>,
    extras: &mut Extras,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    for name in ["frequency_penalty", "presence_penalty"] {
        if o.get(name)
            .is_some_and(|v| !v.as_f64().is_some_and(f64::is_finite))
        {
            return Err(CodecError::Invalid("reported penalty"));
        }
    }
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
    if let Some(value) = usage.get_mut("context_details") {
        let details = value
            .as_object_mut()
            .ok_or(CodecError::Invalid("context accounting"))?;
        direction.fields(details, &["input_tokens", "output_tokens"])?;
        details.retain(|name, _| matches!(name.as_str(), "input_tokens" | "output_tokens"));
        if details.values().any(|v| v.as_u64().is_none()) {
            return Err(CodecError::Invalid("context accounting"));
        }
        take(usage, "context_details", "/usage/context_details", extras);
    }
    Ok(())
}
