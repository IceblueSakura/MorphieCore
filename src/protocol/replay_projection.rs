//! Public wire copies; execution authorizes hidden continuation before projection.
use crate::protocol::{CodecError, DecodedResponse, fidelity::FidelityRecords};
use crate::semantic::task::generation::*;

pub(crate) fn is_marker(value: &str) -> bool {
    value.starts_with("mca1.")
}

pub(crate) fn strip(
    response: &DecodedResponse,
    strip_phase: bool,
) -> Result<DecodedResponse, CodecError> {
    let mut copy = response.clone();
    let mut items = copy.semantic.items().to_vec();
    for (id, item) in &mut items {
        match item {
            Item::Message(m) => {
                if strip_phase {
                    m.phase = None;
                }
                for p in &mut m.parts {
                    p.replay = None;
                    copy.fidelity.remove_part_replay(*id, p.id);
                }
            }
            Item::ToolCall(c) => {
                if c.replay.take().is_some() {
                    copy.fidelity.remove_replay(*id);
                }
            }
            _ => {}
        }
    }
    let source = &response.semantic;
    copy.semantic =
        GenerationResponse::from_resources(items, source.outcome(), source.resources().clone())?
            .with_message_envelopes(source.message_envelopes().to_vec())?
            .with_progress(source.progress())?
            .with_details(source.details().clone())?
            .with_safety_reports(source.safety_reports().to_vec())?
            .with_usage_reports(source.usage_reports().clone())?;
    Ok(copy)
}
/// Only the wire view drops attachments. Execution must retain typed managed
/// Responses history; incompatible stateless delivery rejects before projection.
pub(crate) fn wire_event(event: &StreamEvent, source: &FidelityRecords) -> Option<StreamEvent> {
    if matches!(event, StreamEvent::PartReplay { .. }) {
        return None;
    }
    let mut event = event.clone();
    match &mut event {
        StreamEvent::ItemStarted {
            kind: ItemKind::ToolCall { .. },
            replay,
            ..
        } => *replay = None,
        StreamEvent::ItemFinished { item, replay, .. } if source.is_call_replay(*item) => {
            *replay = None
        }
        _ => {}
    }
    Some(event)
}
#[cfg(test)]
mod tests;
