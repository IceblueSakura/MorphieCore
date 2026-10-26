//! Client boundary for owner-local opaque state. Envelopes label compatibility, not
//! authenticity or target authority. Foreign history transfer remains candidate-local.
use crate::{
    adapter::Request,
    protocol::{CodecError, DecodedResponse, fidelity::FidelityRecords, replay_envelope},
    semantic::{
        task::generation::{Item, ItemKind, ItemReplay, ReplayFormat, ReplayValue, StreamEvent},
        value::ReplayOrigin,
    },
};
use std::{collections::BTreeMap, sync::LazyLock};

pub static CLIENT_ORIGIN: LazyLock<ReplayOrigin> =
    LazyLock::new(|| ReplayOrigin::new("client").expect("static origin"));

fn seal_as(
    value: &ReplayValue,
    origin: &ReplayOrigin,
    format: ReplayFormat,
) -> Result<ReplayValue, CodecError> {
    let sealed =
        replay_envelope::seal(origin, value.format(), value.as_str()).ok_or(CodecError::Limit)?;
    Ok(match value {
        ReplayValue::Partial { .. } => ReplayValue::partial(format, sealed),
        ReplayValue::Final { .. } => ReplayValue::final_value(format, sealed),
    })
}
fn seal(value: &ReplayValue, origin: &ReplayOrigin) -> Result<ReplayValue, CodecError> {
    let format = if value.format() == ReplayFormat::ResponsesCompaction {
        ReplayFormat::ResponsesCompaction
    } else {
        ReplayFormat::ResponsesEncrypted
    };
    seal_as(value, origin, format)
}
fn open_attachment(value: &ReplayValue) -> Result<(ReplayOrigin, ReplayValue), CodecError> {
    let (origin, format, payload) =
        replay_envelope::open(value.as_str()).ok_or(CodecError::invalid())?;
    let value = match value {
        ReplayValue::Partial { .. } => ReplayValue::partial(format, payload),
        ReplayValue::Final { .. } => ReplayValue::final_value(format, payload),
    };
    // The opened request and per-owner fidelity enforce part versus call purpose.
    value.validate()?;
    Ok((origin, value))
}

/// Validate source dependencies before replacing tokens on a private delivery copy.
/// Unbound reasoning keeps the existing omission policy; attachments and compaction fail.
pub fn seal_for_client(
    decoded: &mut DecodedResponse,
    client: &ReplayOrigin,
) -> Result<(), CodecError> {
    decoded
        .fidelity
        .check_attachment_bindings(decoded.semantic.items())?;
    let mut items = decoded.semantic.items().to_vec();
    let mut changed = false;
    for (id, item) in &mut items {
        match item {
            Item::Compaction(value) => {
                let origin = decoded
                    .fidelity
                    .replay_origin(*id)
                    .ok_or(CodecError::invalid())?;
                if !decoded
                    .fidelity
                    .compaction_matches(*id, value, Some(origin))
                {
                    return Err(CodecError::invalid());
                }
                *value = seal(value, origin)?;
                changed = true;
            }
            Item::Reasoning(reasoning) => {
                if let Some(value) = &reasoning.replay {
                    reasoning.replay = match decoded.fidelity.replay_origin(*id) {
                        Some(origin) => {
                            if !decoded
                                .fidelity
                                .replay_matches(*id, reasoning, Some(origin))
                            {
                                return Err(CodecError::invalid());
                            }
                            Some(seal(value, origin)?)
                        }
                        None => None,
                    };
                    changed = true;
                }
            }
            Item::ToolCall(call) => {
                if let Some(value) = &call.replay {
                    let origin = decoded
                        .fidelity
                        .replay_origin(*id)
                        .ok_or(CodecError::invalid())?;
                    if !decoded
                        .fidelity
                        .call_replay_matches(*id, call, Some(origin))
                    {
                        return Err(CodecError::invalid());
                    }
                    call.replay = Some(seal_as(value, origin, value.format())?);
                    changed = true;
                }
            }
            Item::Message(message) => {
                for (ordinal, part) in message.parts.iter_mut().enumerate() {
                    if let Some(value) = &part.replay {
                        let origin = decoded
                            .fidelity
                            .part_replay_origin(*id, part.id)
                            .ok_or(CodecError::invalid())?;
                        if !decoded.fidelity.part_replay_matches(
                            *id,
                            message.status,
                            ordinal,
                            part,
                            Some(origin),
                        ) {
                            return Err(CodecError::invalid());
                        }
                        part.replay = Some(seal_as(value, origin, value.format())?);
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    if !changed {
        return Ok(());
    }
    decoded.semantic = decoded.semantic.clone().with_items(items)?;
    for (id, item) in decoded.semantic.items() {
        match item {
            Item::Compaction(value) => {
                decoded.fidelity.remove_replay(*id);
                decoded
                    .fidelity
                    .record_compaction(*id, value, Some(client.clone()))?;
            }
            Item::Reasoning(reasoning) => {
                decoded.fidelity.remove_replay(*id);
                decoded
                    .fidelity
                    .record_replay(*id, reasoning, Some(client.clone()))?;
            }
            Item::ToolCall(call) if call.replay.is_some() => {
                decoded.fidelity.remove_replay(*id);
                decoded
                    .fidelity
                    .record_call_replay(*id, call, Some(client.clone()))?;
            }
            Item::Message(message) => {
                for (ordinal, part) in message.parts.iter().enumerate() {
                    if part.replay.is_some() {
                        decoded.fidelity.remove_part_replay(*id, part.id);
                        decoded.fidelity.record_part_replay(
                            *id,
                            message.status,
                            ordinal,
                            part,
                            Some(client.clone()),
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Streaming seal preserves actual attachment kinds; reasoning uses its existing carrier.
pub fn seal_event(
    mut event: StreamEvent,
    client: &ReplayOrigin,
    fidelity: &FidelityRecords,
) -> Result<StreamEvent, CodecError> {
    if let StreamEvent::PartReplay { replay, .. } = &mut event {
        let origin = replay.origin.as_ref().ok_or(CodecError::invalid())?;
        replay.value = seal_as(&replay.value, origin, replay.value.format())?;
        replay.origin = Some(client.clone());
        return Ok(event);
    }
    let call = match &event {
        StreamEvent::ItemStarted {
            kind: ItemKind::ToolCall { .. },
            ..
        } => true,
        StreamEvent::ItemFinished { item, .. } => fidelity.is_call_replay(*item),
        _ => false,
    };
    if let StreamEvent::ItemStarted { replay, .. } | StreamEvent::ItemFinished { replay, .. } =
        &mut event
        && let Some(current) = replay.take()
    {
        *replay = match &current.origin {
            Some(origin) => Some(ItemReplay {
                value: if call {
                    seal_as(&current.value, origin, current.value.format())?
                } else {
                    seal(&current.value, origin)?
                },
                origin: Some(client.clone()),
            }),
            None if call || current.value.format() == ReplayFormat::ResponsesCompaction => {
                return Err(CodecError::invalid());
            }
            None => None,
        };
    }
    Ok(event)
}

/// Invalid recognized attachments cannot fall back to plain history. The client codec
/// restores owner boundaries first; this step opens each owner's individual envelope.
pub fn open_from_client(request: &mut Request) -> Result<(), CodecError> {
    let mut items = request.task.semantic.items().to_vec();
    let mut origins = BTreeMap::new();
    let mut part_origins = BTreeMap::new();
    let mut changed = false;
    for (id, item) in &mut items {
        match item {
            Item::Compaction(value) => {
                let (origin, format, payload) = replay_envelope::open(value.as_str())
                    .filter(|(_, format, _)| *format == ReplayFormat::ResponsesCompaction)
                    .ok_or(CodecError::invalid())?;
                origins.insert(*id, origin);
                *value = ReplayValue::final_value(format, payload);
                changed = true;
            }
            Item::Reasoning(reasoning) => {
                if let Some(value) = &reasoning.replay {
                    if crate::protocol::replay_projection::is_marker(value.as_str()) {
                        return Err(CodecError::invalid());
                    }
                    reasoning.replay = match replay_envelope::open(value.as_str()) {
                        Some((origin, format, payload)) if value.replay_token().is_some() => {
                            origins.insert(*id, origin);
                            Some(ReplayValue::final_value(format, payload))
                        }
                        _ => None,
                    };
                    changed = true;
                }
            }
            Item::ToolCall(call) => {
                if let Some(value) = &call.replay {
                    let (origin, value) = open_attachment(value)?;
                    call.replay = Some(value);
                    origins.insert(*id, origin);
                    changed = true;
                }
            }
            Item::Message(message) => {
                for part in &mut message.parts {
                    if let Some(value) = &part.replay {
                        let (origin, value) = open_attachment(value)?;
                        part.replay = Some(value);
                        part_origins.insert((*id, part.id), origin);
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    if !changed {
        return Ok(());
    }
    items.retain(
        |(_, item)| !matches!(item,Item::Reasoning(r) if r.replay.is_none() && r.parts.is_empty()),
    );
    request.task.semantic = request.task.semantic.clone().with_items(items)?;
    request
        .task
        .fidelity
        .retain_owners(request.task.semantic.items());
    for (id, item) in request.task.semantic.items() {
        match item {
            Item::Compaction(value) => {
                let origin = origins.get(id).ok_or(CodecError::invalid())?;
                request.task.fidelity.remove_replay(*id);
                request
                    .task
                    .fidelity
                    .record_compaction(*id, value, Some(origin.clone()))?;
            }
            Item::Reasoning(reasoning) => {
                request.task.fidelity.remove_replay(*id);
                if let Some(origin) = origins.get(id) {
                    request
                        .task
                        .fidelity
                        .record_replay(*id, reasoning, Some(origin.clone()))?;
                }
            }
            Item::ToolCall(call) if call.replay.is_some() => {
                let origin = origins.get(id).ok_or(CodecError::invalid())?;
                request.task.fidelity.remove_replay(*id);
                request
                    .task
                    .fidelity
                    .record_call_replay(*id, call, Some(origin.clone()))?;
            }
            Item::Message(message) => {
                for (ordinal, part) in message.parts.iter().enumerate() {
                    if part.replay.is_some() {
                        let origin = part_origins
                            .get(&(*id, part.id))
                            .ok_or(CodecError::invalid())?;
                        request.task.fidelity.remove_part_replay(*id, part.id);
                        request.task.fidelity.record_part_replay(
                            *id,
                            message.status,
                            ordinal,
                            part,
                            Some(origin.clone()),
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
