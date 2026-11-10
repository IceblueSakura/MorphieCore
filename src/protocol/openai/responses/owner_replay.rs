//! Opt-in owner-local replay transport. Tokens stay opaque and never select targets.
use crate::{
    protocol::{CodecError, DecodedRequest, openai::shape::object, replay_envelope},
    semantic::{task::generation::*, value::Text},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PROFILE: &str = "owner-replay-v1";
pub const HEADER: &str = "x-morphiecore-responses-profile";
pub const FIELD: &str = "morphiecore_replay";

#[derive(Deserialize)]
struct PartToken {
    content_index: usize,
    token: String,
}
enum Attachment {
    Parts(Parts),
    Call(Call),
}
#[derive(Deserialize)]
struct Parts {
    parts: Vec<PartToken>,
}
#[derive(Deserialize)]
struct Call {
    token: String,
}

pub(in crate::protocol::openai) struct Pending {
    id: String,
    attachment: Attachment,
}

/// Remove registered wire fields before standard intake; retain only typed pending data.
pub(in crate::protocol::openai) fn take(value: &mut Value) -> Result<Vec<Pending>, CodecError> {
    let mut pending = vec![];
    if let Some(items) = value.get_mut("input").and_then(Value::as_array_mut) {
        for item in items {
            let Some(o) = item.as_object_mut() else {
                continue;
            };
            let Some(value) = o.remove(FIELD) else {
                continue;
            };
            let fields = object(&value)?;
            let attachment = match (fields.contains_key("parts"), fields.contains_key("token")) {
                (true, false) => Attachment::Parts(
                    serde_json::from_value(value).map_err(|_| CodecError::invalid())?,
                ),
                (false, true) => Attachment::Call(
                    serde_json::from_value(value).map_err(|_| CodecError::invalid())?,
                ),
                _ => return Err(CodecError::invalid()),
            };
            match (&attachment, o.get("type").and_then(Value::as_str)) {
                (Attachment::Parts(_), Some("message"))
                    if o.get("role").and_then(Value::as_str) == Some("assistant") => {}
                (Attachment::Call(_), Some("function_call")) => {}
                _ => return Err(CodecError::invalid()),
            }
            let id = o
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 256)
                .ok_or(CodecError::invalid())?
                .to_owned();
            if pending.len() >= MAX_ITEMS {
                return Err(CodecError::Limit);
            }
            pending.push(Pending { id, attachment });
        }
    }
    Ok(pending)
}

fn token(value: &str, call: bool) -> Result<ReplayValue, CodecError> {
    let (_, format, _) = replay_envelope::open(value).ok_or(CodecError::invalid())?;
    let value = ReplayValue::final_value(
        format,
        Text::new(value, "owner replay", MAX_TEXT_BYTES).map_err(|_| CodecError::Limit)?,
    );
    if call {
        value.validate_call_attachment()?;
    } else {
        value.validate_attachment()?;
    }
    Ok(value)
}

pub(in crate::protocol::openai) fn restore(
    request: &mut DecodedRequest,
    pending: Vec<Pending>,
) -> Result<(), CodecError> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut items = request.semantic.items().to_vec();
    let mut owners = BTreeMap::<String, Option<usize>>::new();
    for (index, (id, _)) in items.iter().enumerate() {
        if let Some(wire_id) = request.fidelity.response_item_id(*id) {
            owners
                .entry(wire_id.to_owned())
                .and_modify(|index| *index = None)
                .or_insert(Some(index));
        }
    }
    for pending in pending {
        let index = owners
            .remove(&pending.id)
            .flatten()
            .ok_or(CodecError::invalid())?;
        let (id, item) = &mut items[index];
        match (item, pending.attachment) {
            (Item::Message(message), Attachment::Parts(value)) => {
                if message.status == ItemLifecycle::InProgress
                    || value.parts.is_empty()
                    || value.parts.len() > message.parts.len()
                {
                    return Err(CodecError::invalid());
                }
                let mut indexes = BTreeSet::new();
                for value in value.parts {
                    if !indexes.insert(value.content_index) {
                        return Err(CodecError::invalid());
                    }
                    let part = message
                        .parts
                        .get_mut(value.content_index)
                        .ok_or(CodecError::invalid())?;
                    if !matches!(part.content, ContentPart::Text(_)) {
                        return Err(CodecError::invalid());
                    }
                    part.replay = Some(token(&value.token, false)?);
                    request.fidelity.record_part_replay(
                        *id,
                        message.status,
                        value.content_index,
                        part,
                        None,
                    )?;
                }
            }
            (Item::ToolCall(call), Attachment::Call(value)) => {
                if call.status == ItemLifecycle::InProgress {
                    return Err(CodecError::invalid());
                }
                call.replay = Some(token(&value.token, true)?);
                request.fidelity.record_call_replay(*id, call, None)?;
            }
            _ => return Err(CodecError::invalid()),
        }
    }
    request.semantic = request.semantic.clone().with_items(items)?;
    Ok(())
}

/// Publish only finalized tokens on their actual owners, never on a reasoning surrogate.
pub(crate) fn write(value: &mut Value, item: &Item) -> Result<(), CodecError> {
    let sealed = |value: &ReplayValue, call| -> Result<Value, CodecError> {
        let wire = value.replay_token().ok_or(CodecError::invalid())?;
        token(wire, call)?;
        Ok(json!(wire))
    };
    let attachment = match item {
        Item::Message(message) => {
            let mut parts = vec![];
            for (index, part) in message.parts.iter().enumerate() {
                if let Some(replay) = &part.replay {
                    parts.push(json!({"content_index":index,"token":sealed(replay, false)?}));
                }
            }
            (!parts.is_empty()).then(|| json!({"parts":parts}))
        }
        Item::ToolCall(call) => call
            .replay
            .as_ref()
            .map(|value| sealed(value, true).map(|token| json!({"token":token})))
            .transpose()?,
        _ => None,
    };
    if let Some(attachment) = attachment {
        object(value)?;
        value[FIELD] = attachment;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec};

    #[test]
    fn selected_profile_validates_attachment_purpose_position_and_final_owner() {
        let origin = replay_envelope::issuer("synthetic", "gemini");
        let part = replay_envelope::seal(
            &origin,
            ReplayFormat::GoogleGenerateContentSignature,
            "c2ln",
        )
        .unwrap();
        let reasoning =
            replay_envelope::seal(&origin, ReplayFormat::ResponsesEncrypted, "unrelated").unwrap();
        let message = json!({"type":"message","id":"msg_a","role":"assistant",
            "status":"completed","content":[{"type":"output_text","text":"OK"}],
            "morphiecore_replay":{"parts":[{"content_index":0,"token":part.as_str()}]}});
        let mut codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        codec.adaptation.rules.responses_owner_replay = true;
        let decode = |item| {
            codec.decode_client_request(
                json!({"model":"public","input":[{"role":"user","content":"go"},item]})
                    .to_string()
                    .as_bytes(),
            )
        };
        assert!(decode(message.clone()).is_ok());
        let mut extended = message.clone();
        extended[FIELD]["future_hint"] = json!(true);
        extended[FIELD]["parts"][0]["future_hint"] = json!({"ignored":true});
        assert!(decode(extended).is_ok());
        for change in [
            json!({"parts":[]}),
            json!({"parts":[{"content_index":1,"token":part.as_str()}]}),
            json!({"parts":[{"content_index":0,"token":part.as_str()},
                {"content_index":0,"token":part.as_str()}]}),
            json!({"parts":[{"content_index":0,"token":"mc1.broken"}]}),
            json!({"parts":[{"content_index":0,"token":reasoning.as_str()}]}),
            json!({"token":part.as_str()}),
            json!({"parts":[{"content_index":0,"token":part.as_str()}],
                "token":part.as_str()}),
        ] {
            let mut invalid = message.clone();
            invalid[FIELD] = change;
            assert!(decode(invalid).is_err());
        }
        for (key, value) in [
            ("status", json!("in_progress")),
            ("role", json!("user")),
            ("id", Value::Null),
        ] {
            let mut invalid = message.clone();
            invalid[key] = value;
            assert!(decode(invalid).is_err());
        }
    }
}
