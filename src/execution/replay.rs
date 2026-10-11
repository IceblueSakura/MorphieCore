//! Client boundary for upstream-issued opaque reasoning. Values leave the gateway in
//! a tagged envelope (`protocol::replay_envelope`) bound to the local client origin,
//! and return labelled with the Provider/model claimed by the envelope. Each candidate then
//! drops values from any other issuer (`Request::without_foreign_replay`).
use crate::{
    adapter::Request,
    protocol::{CodecError, DecodedResponse, replay_envelope},
    semantic::{
        task::generation::{Item, ReasoningReplay, ReplayFormat, ReplayValue, StreamEvent},
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
    Ok(match value {
        ReplayValue::Partial { .. } => {
            ReplayValue::partial(ReplayFormat::ResponsesEncrypted, sealed)
        }
        ReplayValue::Final { .. } => {
            ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, sealed)
        }
    })
}

/// Replace each reasoning replay value with its sealed carrier. A value without a
/// known issuer cannot be returned later, so it is not delivered.
pub fn seal_for_client(
    decoded: &mut DecodedResponse,
    client: &ReplayOrigin,
) -> Result<(), CodecError> {
    let mut items = decoded.semantic.items().to_vec();
    let mut changed = false;
    for (id, item) in &mut items {
        let Item::Reasoning(reasoning) = item else {
            continue;
        };
        let Some(value) = &reasoning.replay else {
            continue;
        };
        reasoning.replay = match decoded.fidelity.replay_origin(*id) {
            Some(origin) => Some(seal(value, origin)?),
            None => None,
        };
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    decoded.semantic = decoded.semantic.clone().with_items(items)?;
    for (id, item) in decoded.semantic.items() {
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
            Some(origin) => Some(ReasoningReplay {
                value: seal(&current.value, origin)?,
                origin: Some(client.clone()),
            }),
            None => None,
        };
    }
    Ok(event)
}

/// Unwrap client envelopes. Labels are compatibility hints, not authenticated claims.
/// Malformed and bare values cannot identify a compatible issuer and are dropped.
pub fn open_from_client(request: &mut Request) -> Result<(), CodecError> {
    let semantic = &request.task.semantic;
    let mut items = semantic.items().to_vec();
    let mut origins = vec![];
    let mut changed = false;
    for (id, item) in &mut items {
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
