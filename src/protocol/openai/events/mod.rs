//! Generation event codecs. Protocol indexes are mappings, never semantic identities.
//! Payloads exclude SSE framing. Rejected streams are poisoned and cannot resume.
mod chat;
mod decode;
mod encode;
mod responses;
use crate::protocol::openai::TargetCaps;
use crate::protocol::openai::{CodecError, Profile, ResponseMetadata, common::*};
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::{task::generation::*, value::ReplayOrigin},
};
pub use decode::EventDecoder;
pub use encode::EventEncoder;
use serde_json::{Map, Value};

fn event_fields(o: &Map<String, Value>, allowed: &[&str]) -> Result<(), CodecError> {
    DecodeDirection::Upstream.fields(o, allowed)
}
fn index(o: &Map<String, Value>, key: &'static str) -> Result<usize, CodecError> {
    o.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n < MAX_ITEMS as u64)
        .map(|n| n as usize)
        .ok_or(CodecError::Invalid(key))
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
        if matches!(item.kind, ItemKind::Compaction) {
            let replay = item
                .replay
                .as_ref()
                .ok_or(CodecError::Invalid("compaction content"))?;
            fidelity.record_compaction(item.id, &replay.value, replay.origin.clone())?;
        } else if matches!(item.kind, ItemKind::Reasoning) {
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
