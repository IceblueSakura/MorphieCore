//! Scoped Responses-format reasoning on a Chat carrier, not opaque JSON passthrough.
use super::{CodecError, common::*};
use crate::{
    protocol::{adaptation::Adaptation, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};

pub(super) struct Details {
    pub part: Option<(PartKind, String)>,
    pub responses_encrypted: Option<(String, String)>,
}
/// One ordered readable part plus an optional final encrypted token. Fragment
/// indices belong to the carrier, not to separately invented semantic items.
pub(super) fn parse(
    m: &Map<String, Value>,
    a: &Adaptation,
    prior: Option<PartKind>,
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
                fields(o, &["type", "format", "index", field])?;
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
                fields(o, &["type", "format", "index", "id", "data"])?;
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
pub(super) fn decode_static(
    b: &mut Items,
    m: &Map<String, Value>,
    a: &Adaptation,
) -> Result<Map<String, Value>, CodecError> {
    if m.contains_key("reasoning_details") && string(m, "role")? != "assistant" {
        return Err(CodecError::Invalid("reasoning owner"));
    }
    let mut clean = m.clone();
    if let Some(details) = parse(m, a, None)? {
        if string(m, "role")? != "assistant" {
            return Err(CodecError::Invalid("reasoning owner"));
        }
        let id = b.id()?;
        let parts = if let Some((kind, value)) = details.part {
            let text = Text::allowing_empty(&value, "reasoning", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::Limit)?;
            vec![(
                b.part_id()?,
                if kind == PartKind::Summary {
                    ReasoningContent::Summary(text)
                } else {
                    ReasoningContent::Text(text)
                },
            )]
        } else {
            vec![]
        };
        let encrypted = if let Some((wire_id, data)) = details.responses_encrypted {
            b.fidelity.record_response_item_id(id, &wire_id)?;
            Some(ReplayValue::final_value(
                ReplayFormat::ResponsesEncrypted,
                text(&data, "encrypted reasoning", MAX_TEXT_BYTES)?,
            ))
        } else {
            None
        };
        let item = ReasoningItem {
            parts,
            status: ItemLifecycle::Completed,
            replay: encrypted,
        };
        b.fidelity.record_replay(id, &item, a.scope.clone())?;
        b.items.push((id, Item::Reasoning(item)));
        clean.shift_remove("reasoning");
        clean.shift_remove("reasoning_content");
    }
    if a.rules.structured_chat_reasoning || a.rules.unversioned_chat_reasoning_view {
        clean.shift_remove("reasoning_details");
    }
    if a.rules.unversioned_chat_reasoning_view && a.rules.reasoning_alias {
        super::adapter_shapes::decode_message(&mut clean)?;
    }
    Ok(clean)
}
pub(super) fn wire(id: ItemId, item: &ReasoningItem, fidelity: &FidelityRecords) -> Vec<Value> {
    let mut details = vec![];
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
        details.push(json!({"format":"openai-responses-v1","index":details.len(),"type":"reasoning.encrypted","data":token,"id":fidelity.response_item_id(id).map(str::to_owned).unwrap_or_else(||format!("item_{}",id.get()))}));
    }
    details
}
