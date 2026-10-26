//! Responses-only event part, item-snapshot and replay-value helpers; no stream state.
use super::*;
use crate::protocol::failure::{CarrierReason, ShapeReason};

pub(super) fn lifecycle(value: &str) -> Result<ItemLifecycle, CodecError> {
    match value {
        "completed" => Ok(ItemLifecycle::Completed),
        "incomplete" => Ok(ItemLifecycle::Incomplete),
        _ => Err(CodecError::Invalid(ShapeReason::Item)),
    }
}
pub(super) fn replay(
    o: &Map<String, Value>,
    origin: Option<&ReplayOrigin>,
    final_value: bool,
) -> Result<Option<ItemReplay>, CodecError> {
    if o.get("type").and_then(Value::as_str) == Some("compaction") {
        return Ok(Some(ItemReplay {
            origin: origin.cloned(),
            value: crate::protocol::openai::responses::items::compaction_content(o, final_value)?,
        }));
    }
    match o.get("encrypted_content") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let value = text(s, "encrypted reasoning", MAX_TEXT_BYTES)?;
            Ok(Some(ItemReplay {
                origin: origin.cloned(),
                value: if final_value {
                    ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, value)
                } else {
                    ReplayValue::partial(ReplayFormat::ResponsesEncrypted, value)
                },
            }))
        }
        _ => Err(CodecError::invalid()),
    }
}
fn kind_name(kind: PartKind) -> &'static str {
    match kind {
        PartKind::ResultText | PartKind::ResultJson => {
            unreachable!("Provider result carrier rejected")
        }
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
pub(super) fn part_kind(part: &Map<String, Value>) -> Result<PartKind, CodecError> {
    match string(part, "type")? {
        "output_text" => Ok(PartKind::Text),
        "refusal" => Ok(PartKind::Refusal),
        "summary_text" => Ok(PartKind::Summary),
        "reasoning_text" => Ok(PartKind::ReasoningText),
        _ => Err(CodecError::Unsupported(CarrierReason::Unavailable)),
    }
}
pub(super) fn part_text(part: &Map<String, Value>, kind: PartKind) -> Result<&str, CodecError> {
    DecodeDirection::Upstream.fields(
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
pub(super) fn part_wire(kind: PartKind, text: &str) -> Value {
    if kind == PartKind::Refusal {
        serde_json::json!({"type":"refusal","refusal":text})
    } else if kind == PartKind::Text {
        serde_json::json!({"type":"output_text","text":text,"annotations":[]})
    } else {
        serde_json::json!({"type":kind_name(kind),"text":text})
    }
}
pub(super) fn item_wire(
    state: &StreamState,
    id: ItemId,
    fidelity: &FidelityRecords,
) -> Result<Value, CodecError> {
    let item = state.item(id)?.snapshot()?;
    crate::protocol::openai::responses::items::encode_items(
        &[(id, item)],
        fidelity,
        true,
        state.resources(),
    )?
    .into_iter()
    .next()
    .ok_or(CodecError::Invalid(ShapeReason::ItemSnapshot))
}
