//! Scoped Responses-format reasoning on a Chat carrier, not opaque JSON passthrough.
use crate::protocol::openai::{CodecError, common::*};
use crate::{
    protocol::{fidelity::FidelityRecords, openai::dialect::adaptation::Adaptation},
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};

pub(in crate::protocol::openai) struct Details {
    pub part: Option<(PartKind, String)>,
    pub responses_encrypted: Option<(String, String)>,
}
/// One stream delta: one ordered readable part plus an optional final encrypted
/// token. Fragment indices belong to the carrier, not to invented semantic items.
pub(in crate::protocol::openai) fn parse(
    m: &Map<String, Value>,
    a: &Adaptation,
    prior: Option<PartKind>,
    direction: DecodeDirection,
) -> Result<Option<Details>, CodecError> {
    let Some(value) = m.get("reasoning_details") else {
        return Ok(None);
    };
    if !a.rules.structured_chat_reasoning && !a.rules.unversioned_chat_reasoning_view {
        return Err(CodecError::Unsupported("reasoning details".into()));
    }
    if value.is_null() {
        return Ok(None);
    }
    let values = value
        .as_array()
        .ok_or(CodecError::Invalid("reasoning details"))?;
    if values.is_empty() {
        return Ok(None);
    }
    if values.len() > 2 {
        return Err(CodecError::Unsupported("reasoning detail grouping".into()));
    }
    let mut result = Details {
        part: None,
        responses_encrypted: None,
    };
    for value in values {
        let o = object(value)?;
        let format = string(o, "format")?;
        let kind = string(o, "type")?;
        // An unversioned readable view cannot carry summaries, identities or ciphertext.
        let readable_view = a.rules.unversioned_chat_reasoning_view
            && format == "unknown"
            && kind == "reasoning.text";
        if !readable_view && (!a.rules.structured_chat_reasoning || format != "openai-responses-v1")
        {
            return Err(CodecError::Unsupported("reasoning format".into()));
        }
        match kind {
            "reasoning.summary" | "reasoning.text" => {
                let field = if kind == "reasoning.summary" {
                    "summary"
                } else {
                    "text"
                };
                direction.fields(o, &["type", "format", "index", field])?;
                let part_kind = if kind == "reasoning.summary" {
                    PartKind::Summary
                } else {
                    PartKind::ReasoningText
                };
                if result.part.is_some()
                    || result.responses_encrypted.is_some()
                    || prior.is_some_and(|p| p != part_kind)
                    || o.get("index").and_then(Value::as_u64) != Some(0)
                {
                    return Err(CodecError::Invalid("reasoning detail order"));
                }
                let text = string(o, field)?;
                if text.len() > MAX_TEXT_BYTES {
                    return Err(CodecError::Limit);
                }
                result.part = Some((part_kind, text.into()));
            }
            "reasoning.encrypted" => {
                direction.fields(o, &["type", "format", "index", "id", "data"])?;
                let expected = u64::from(prior.is_some() || result.part.is_some());
                if result.responses_encrypted.is_some()
                    || o.get("index").and_then(Value::as_u64) != Some(expected)
                {
                    return Err(CodecError::Invalid("reasoning detail order"));
                }
                let id = text(string(o, "id")?, "reasoning identity", 256)?;
                let data = text(string(o, "data")?, "encrypted reasoning", MAX_TEXT_BYTES)?;
                result.responses_encrypted = Some((id.as_str().into(), data.as_str().into()));
            }
            _ => return Err(CodecError::Unsupported("reasoning detail kind".into())),
        }
    }
    for key in ["reasoning", "reasoning_content"] {
        if let Some(view) = m.get(key).filter(|v| !v.is_null()) {
            let view = view.as_str().ok_or(CodecError::Invalid("reasoning view"))?;
            if view != result.part.as_ref().map(|(_, s)| s.as_str()).unwrap_or("") {
                return Err(CodecError::Invalid("reasoning view disagreement"));
            }
        }
    }
    Ok(Some(result))
}
/// One reasoning item on a Chat message; a replay value closes it.
#[derive(Default)]
struct Group {
    parts: Vec<(PartKind, String)>,
    /// The wire item ID (encrypted details only) and the value.
    replay: Option<(Option<String>, String)>,
}
/// Ordered reasoning items on one static message. Client input does not check index
/// values or `format` labels: pi merges streamed deltas and keeps the first index, and
/// client values are envelopes or are dropped. Other directions require positions.
fn parse_message(
    m: &Map<String, Value>,
    a: &Adaptation,
    direction: DecodeDirection,
) -> Result<Vec<Group>, CodecError> {
    let Some(value) = m.get("reasoning_details") else {
        return Ok(vec![]);
    };
    if !a.rules.structured_chat_reasoning && !a.rules.unversioned_chat_reasoning_view {
        return Err(CodecError::Unsupported("reasoning details".into()));
    }
    let values = match value {
        Value::Null => return Ok(vec![]),
        Value::Array(values) => values,
        _ => return Err(CodecError::Invalid("reasoning details")),
    };
    let client = direction.client();
    let mut groups: Vec<Group> = vec![];
    for (position, value) in values.iter().enumerate() {
        let o = object(value)?;
        let format = string(o, "format")?;
        let kind = string(o, "type")?;
        // An unversioned readable view cannot carry summaries, identities or ciphertext.
        let readable_view = a.rules.unversioned_chat_reasoning_view
            && format == "unknown"
            && kind == "reasoning.text";
        if !client
            && !readable_view
            && (!a.rules.structured_chat_reasoning || format != "openai-responses-v1")
        {
            return Err(CodecError::Unsupported("reasoning format".into()));
        }
        if !o.get("index").map_or(client, |i| {
            i.as_u64().is_some_and(|i| client || i == position as u64)
        }) {
            return Err(CodecError::Invalid("reasoning detail order"));
        }
        if groups.last().is_none_or(|group| group.replay.is_some()) {
            groups.push(Group::default());
        }
        let group = groups.last_mut().expect("open group");
        match kind {
            "reasoning.summary" => {
                direction.fields(o, &["type", "format", "index", "summary"])?;
                group
                    .parts
                    .push((PartKind::Summary, readable(o, "summary")?));
            }
            "reasoning.text" => {
                direction.fields(o, &["type", "format", "index", "text", "signature"])?;
                group
                    .parts
                    .push((PartKind::ReasoningText, readable(o, "text")?));
                // OpenRouter's Anthropic shape signs the text detail itself.
                if !readable_view
                    && let Some(signature) = o.get("signature").filter(|v| !v.is_null())
                {
                    let signature = signature
                        .as_str()
                        .ok_or(CodecError::Invalid("reasoning signature"))?;
                    let signature = text(signature, "reasoning signature", MAX_TEXT_BYTES)?;
                    group.replay = Some((None, signature.as_str().into()));
                }
            }
            "reasoning.encrypted" => {
                direction.fields(o, &["type", "format", "index", "id", "data"])?;
                let id = text(string(o, "id")?, "reasoning identity", 256)?;
                let data = text(string(o, "data")?, "encrypted reasoning", MAX_TEXT_BYTES)?;
                group.replay = Some((Some(id.as_str().into()), data.as_str().into()));
            }
            _ => return Err(CodecError::Unsupported("reasoning detail kind".into())),
        }
    }
    let view: String = groups
        .iter()
        .flat_map(|group| group.parts.iter().map(|(_, text)| text.as_str()))
        .collect();
    // An empty view says nothing: pi sends `reasoning_content: ""` beside the
    // details when its DeepSeek compat requires the field.
    for key in ["reasoning", "reasoning_content"] {
        if let Some(value) = m
            .get(key)
            .filter(|v| !v.is_null() && v.as_str() != Some(""))
            && value
                .as_str()
                .ok_or(CodecError::Invalid("reasoning view"))?
                != view
        {
            return Err(CodecError::Invalid("reasoning view disagreement"));
        }
    }
    Ok(groups)
}
fn readable(o: &Map<String, Value>, field: &'static str) -> Result<String, CodecError> {
    let text = string(o, field)?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(CodecError::Limit);
    }
    Ok(text.into())
}
pub(in crate::protocol::openai) fn decode_static(
    b: &mut Items,
    m: &Map<String, Value>,
    a: &Adaptation,
) -> Result<Map<String, Value>, CodecError> {
    let groups = parse_message(m, a, b.direction)?;
    if m.contains_key("reasoning_details") && string(m, "role")? != "assistant" {
        return Err(CodecError::Invalid("reasoning owner"));
    }
    let mut clean = m.clone();
    if !groups.is_empty() {
        clean.shift_remove("reasoning");
        clean.shift_remove("reasoning_content");
    }
    for group in groups {
        let id = b.id()?;
        let mut parts = vec![];
        for (kind, value) in group.parts {
            let text = Text::allowing_empty(&value, "reasoning", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::Limit)?;
            parts.push((
                b.part_id()?,
                if kind == PartKind::Summary {
                    ReasoningContent::Summary(text)
                } else {
                    ReasoningContent::Text(text)
                },
            ));
        }
        let replay = match group.replay {
            Some((wire_id, data)) => {
                if let Some(wire_id) = wire_id {
                    b.fidelity.record_response_item_id(id, &wire_id)?;
                }
                Some(ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text(&data, "encrypted reasoning", MAX_TEXT_BYTES)?,
                ))
            }
            None => None,
        };
        let item = ReasoningItem {
            parts,
            status: ItemLifecycle::Completed,
            replay,
        };
        b.fidelity.record_replay(id, &item, a.scope.clone())?;
        b.items.push((id, Item::Reasoning(item)));
    }
    if a.rules.structured_chat_reasoning || a.rules.unversioned_chat_reasoning_view {
        clean.shift_remove("reasoning_details");
    }
    if a.rules.unversioned_chat_reasoning_view && a.rules.reasoning_alias {
        crate::protocol::openai::dialect::adapter_shapes::decode_message(&mut clean)?;
    }
    Ok(clean)
}
/// Fields for the reasoning items ahead of one Chat carrier message. The view is the
/// concatenation of readable parts. Details keep item boundaries: each replay value
/// closes its item, and indices are positions in the message.
pub(in crate::protocol::openai) fn write_carrier(
    message: &mut Value,
    items: &[(ItemId, &ReasoningItem)],
    fidelity: &FidelityRecords,
    structured: bool,
) {
    let view: String = items
        .iter()
        .flat_map(|(_, r)| &r.parts)
        .map(|(_, part)| match part {
            ReasoningContent::Summary(t) | ReasoningContent::Text(t) => t.as_str(),
        })
        .collect();
    if !view.is_empty() {
        message["reasoning_content"] = json!(view);
    }
    if structured
        && items.iter().any(|(_, r)| {
            r.replay.is_some()
                || r.parts
                    .iter()
                    .any(|(_, p)| matches!(p, ReasoningContent::Summary(_)))
        })
    {
        let mut details = vec![];
        for (id, item) in items {
            wire(*id, item, fidelity, &mut details);
        }
        message["reasoning_details"] = json!(details);
    }
}
fn wire(id: ItemId, item: &ReasoningItem, fidelity: &FidelityRecords, details: &mut Vec<Value>) {
    for (_, part) in &item.parts {
        let mut value = json!({"format":"openai-responses-v1","index":details.len()});
        match part {
            ReasoningContent::Summary(t) => {
                value["type"] = json!("reasoning.summary");
                value["summary"] = json!(t.as_str());
            }
            ReasoningContent::Text(t) => {
                value["type"] = json!("reasoning.text");
                value["text"] = json!(t.as_str());
            }
        }
        details.push(value);
    }
    if let Some(token) = item.replay.as_ref().and_then(ReplayValue::replay_token) {
        details.push(json!({"format":"openai-responses-v1","index":details.len(),"type":"reasoning.encrypted","data":token,"id":fidelity.response_item_id(id).map(str::to_owned).unwrap_or_else(||crate::protocol::fidelity::generated_item_id(id))}));
    }
}
