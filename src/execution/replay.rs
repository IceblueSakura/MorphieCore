//! Client boundary for upstream-issued opaque reasoning and compacted context.
//! Values leave in a tagged envelope (`protocol::replay_envelope`) bound to the client origin,
//! and return labelled with the Provider/model claimed by the envelope. Each candidate then
//! drops foreign reasoning but rejects foreign compacted context.
use crate::{
    adapter::Request,
    protocol::{CodecError, DecodedResponse, replay_envelope},
    semantic::{
        task::generation::{Item, ItemReplay, ReplayFormat, ReplayValue, StreamEvent},
        value::ReplayOrigin,
    },
};
use std::sync::LazyLock;

/// Origin of every sealed value as seen by downstream codecs.
pub static CLIENT_ORIGIN: LazyLock<ReplayOrigin> =
    LazyLock::new(|| ReplayOrigin::new("client").expect("static origin"));

fn seal(value: &ReplayValue, origin: &ReplayOrigin) -> Result<ReplayValue, CodecError> {
    let sealed =
        replay_envelope::seal(origin, value.format(), value.as_str()).ok_or(CodecError::Limit)?;
    let format = if value.format() == ReplayFormat::ResponsesCompaction {
        ReplayFormat::ResponsesCompaction
    } else {
        ReplayFormat::ResponsesEncrypted
    };
    Ok(match value {
        ReplayValue::Partial { .. } => ReplayValue::partial(format, sealed),
        ReplayValue::Final { .. } => ReplayValue::final_value(format, sealed),
    })
}

/// Seal opaque items. Unbound reasoning is omitted; unbound compaction must fail.
pub fn seal_for_client(
    decoded: &mut DecodedResponse,
    client: &ReplayOrigin,
) -> Result<(), CodecError> {
    let mut items = decoded.semantic.items().to_vec();
    let mut changed = false;
    for (id, item) in &mut items {
        if let Item::Compaction(value) = item {
            let origin = decoded
                .fidelity
                .replay_origin(*id)
                .ok_or(CodecError::Invalid("unbound compaction"))?;
            if !decoded
                .fidelity
                .compaction_matches(*id, value, Some(origin))
            {
                return Err(CodecError::Invalid("compaction dependency"));
            }
            *value = seal(value, origin)?;
            changed = true;
            continue;
        }
        let Item::Reasoning(reasoning) = item else {
            continue;
        };
        let Some(value) = &reasoning.replay else {
            continue;
        };
        reasoning.replay = match decoded.fidelity.replay_origin(*id) {
            Some(origin) => {
                if !decoded
                    .fidelity
                    .replay_matches(*id, reasoning, Some(origin))
                {
                    return Err(CodecError::Invalid("reasoning dependency"));
                }
                Some(seal(value, origin)?)
            }
            None => None,
        };
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    decoded.semantic = decoded.semantic.clone().with_items(items)?;
    for (id, item) in decoded.semantic.items() {
        if let Item::Compaction(value) = item {
            decoded.fidelity.remove_replay(*id);
            decoded
                .fidelity
                .record_compaction(*id, value, Some(client.clone()))?;
        }
        if let Item::Reasoning(reasoning) = item {
            decoded.fidelity.remove_replay(*id);
            decoded
                .fidelity
                .record_replay(*id, reasoning, Some(client.clone()))?;
        }
    }
    Ok(())
}

/// Streaming counterpart of [`seal_for_client`].
pub fn seal_event(
    mut event: StreamEvent,
    client: &ReplayOrigin,
) -> Result<StreamEvent, CodecError> {
    if let StreamEvent::ItemStarted { replay, .. } | StreamEvent::ItemFinished { replay, .. } =
        &mut event
        && let Some(current) = replay.take()
    {
        *replay = match &current.origin {
            Some(origin) => Some(ItemReplay {
                value: seal(&current.value, origin)?,
                origin: Some(client.clone()),
            }),
            None if current.value.format() == ReplayFormat::ResponsesCompaction => {
                return Err(CodecError::Invalid("unbound compaction"));
            }
            None => None,
        };
    }
    Ok(event)
}

/// Unwrap client envelopes. Labels are compatibility hints, not authenticated claims.
/// Invalid reasoning is dropped; invalid compaction cannot safely lose its context.
pub fn open_from_client(request: &mut Request) -> Result<(), CodecError> {
    let semantic = &request.task.semantic;
    let mut items = semantic.items().to_vec();
    let mut origins = vec![];
    let mut changed = false;
    for (id, item) in &mut items {
        if let Item::Compaction(value) = item {
            let (origin, format, payload) = replay_envelope::open(value.as_str())
                .filter(|(_, format, _)| *format == ReplayFormat::ResponsesCompaction)
                .ok_or(CodecError::Invalid("compaction envelope"))?;
            origins.push((*id, origin));
            *value = ReplayValue::final_value(format, payload);
            changed = true;
            continue;
        }
        let Item::Reasoning(reasoning) = item else {
            continue;
        };
        let Some(value) = &reasoning.replay else {
            continue;
        };
        reasoning.replay = match replay_envelope::open(value.as_str()) {
            Some((origin, format, payload)) if value.replay_token().is_some() => {
                origins.push((*id, origin));
                Some(ReplayValue::final_value(format, payload))
            }
            _ => None,
        };
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    items.retain(
        |(_, item)| !matches!(item, Item::Reasoning(r) if r.replay.is_none() && r.parts.is_empty()),
    );
    request.task.semantic = semantic.clone().with_items(items)?;
    request
        .task
        .fidelity
        .retain_owners(request.task.semantic.items());
    for (id, item) in request.task.semantic.items() {
        if let Item::Compaction(value) = item {
            let origin = origins
                .iter()
                .find(|(owner, _)| owner == id)
                .ok_or(CodecError::Invalid("compaction origin"))?;
            request.task.fidelity.remove_replay(*id);
            request
                .task
                .fidelity
                .record_compaction(*id, value, Some(origin.1.clone()))?;
        }
        if let Item::Reasoning(reasoning) = item {
            request.task.fidelity.remove_replay(*id);
            if let Some((_, origin)) = origins.iter().find(|(owner, _)| owner == id) {
                request
                    .task
                    .fidelity
                    .record_replay(*id, reasoning, Some(origin.clone()))?;
            }
        }
    }
    Ok(())
}
