//! Generation event codecs. Protocol indexes are mappings, never semantic identities.
//! Payloads exclude SSE framing. Rejected streams are poisoned and cannot resume.
mod chat;
mod decode;
mod encode;
use super::{CodecError, Profile, ResponseMetadata, common::*};
use crate::lowering::generation::GenerationRepresentationContract;
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::{task::generation::*, value::ReplayOrigin},
};
pub use decode::EventDecoder;
pub use encode::EventEncoder;
use serde_json::{Map, Value};

fn event_fields(o: &Map<String, Value>, allowed: &[&str]) -> Result<(), CodecError> {
    if let Some(key) = o.keys().find(|k| {
        !matches!(k.as_str(), "sequence_number" | "obfuscation") && !allowed.contains(&k.as_str())
    }) {
        return Err(CodecError::Unsupported(key.clone()));
    }
    Ok(())
}
fn index(o: &Map<String, Value>, key: &'static str) -> Result<usize, CodecError> {
    o.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n < MAX_ITEMS as u64)
        .map(|n| n as usize)
        .ok_or(CodecError::Invalid(key))
}
fn lifecycle(value: &str) -> Result<ItemLifecycle, CodecError> {
    match value {
        "completed" => Ok(ItemLifecycle::Completed),
        "incomplete" => Ok(ItemLifecycle::Incomplete),
        _ => Err(CodecError::Invalid("item status")),
    }
}
fn replay(
    o: &Map<String, Value>,
    origin: Option<&ReplayOrigin>,
    final_value: bool,
) -> Result<Option<ReasoningReplay>, CodecError> {
    match o.get("encrypted_content") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let value = text(s, "encrypted reasoning", MAX_TEXT_BYTES)?;
            Ok(Some(ReasoningReplay {
                origin: origin.cloned(),
                value: if final_value {
                    ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, value)
                } else {
                    ReplayValue::partial(ReplayFormat::ResponsesEncrypted, value)
                },
            }))
        }
        _ => Err(CodecError::Invalid("encrypted reasoning")),
    }
}
fn kind_name(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Audio => unreachable!("audio has no Responses event carrier"),
        PartKind::Text => "output_text",
        PartKind::Refusal => "refusal",
        PartKind::Summary => "summary_text",
        PartKind::ReasoningText => "reasoning_text",
        PartKind::Arguments => "arguments",
        PartKind::StructuredArguments => unreachable!("profile rejects structured argument parts"),
        PartKind::CustomInput => "input",
    }
}
fn part_kind(part: &Map<String, Value>) -> Result<PartKind, CodecError> {
    match string(part, "type")? {
        "output_text" => Ok(PartKind::Text),
        "refusal" => Ok(PartKind::Refusal),
        "summary_text" => Ok(PartKind::Summary),
        "reasoning_text" => Ok(PartKind::ReasoningText),
        _ => Err(CodecError::Unsupported("part kind".into())),
    }
}
fn part_text(part: &Map<String, Value>, kind: PartKind) -> Result<&str, CodecError> {
    fields(
        part,
        if kind == PartKind::Refusal {
            &["type", "refusal"]
        } else if kind == PartKind::Text {
            &["type", "text", "annotations", "logprobs"]
        } else {
            &["type", "text"]
        },
    )?;
    string(
        part,
        if kind == PartKind::Refusal {
            "refusal"
        } else {
            "text"
        },
    )
}
fn part_wire(kind: PartKind, text: &str) -> Value {
    if kind == PartKind::Refusal {
        serde_json::json!({"type":"refusal","refusal":text})
    } else if kind == PartKind::Text {
        serde_json::json!({"type":"output_text","text":text,"annotations":[]})
    } else {
        serde_json::json!({"type":kind_name(kind),"text":text})
    }
}
fn sync_replays(
    state: &StreamState,
    fidelity: &mut FidelityRecords,
    owner: Option<ItemId>,
) -> Result<(), CodecError> {
    for item in state
        .items()
        .iter()
        .filter(|i| owner.is_none_or(|id| i.id == id))
    {
        if matches!(item.kind, ItemKind::Reasoning) {
            if let Some(replay) = &item.replay {
                let Item::Reasoning(semantic) = item.snapshot()? else {
                    unreachable!("reasoning owner")
                };
                fidelity.record_replay(item.id, &semantic, replay.origin.clone())?;
            } else {
                fidelity.remove_replay(item.id);
            }
        }
    }
    Ok(())
}
fn item_wire(
    state: &StreamState,
    id: ItemId,
    fidelity: &FidelityRecords,
) -> Result<Value, CodecError> {
    let item = state.item(id)?.snapshot()?;
    super::responses::encode_items(&[(id, item)], fidelity, true)
        .into_iter()
        .next()
        .ok_or(CodecError::Invalid("item snapshot"))
}
