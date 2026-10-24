//! Selected phase projection. Source history and protected semantic edits stay intact.
use crate::{lowering::generation::LossEvidence, semantic::task::generation::*};

pub(crate) fn has_phase(items: &[(ItemId, Item)]) -> bool {
    items
        .iter()
        .any(|(_, item)| matches!(item, Item::Message(m) if m.phase.is_some()))
}

pub(crate) fn omission(rule: &'static str) -> LossEvidence {
    LossEvidence {
        rule,
        field: "generation.message.phase",
        direction: "request.encode",
        action: "omit",
    }
}

pub(crate) fn delivery_omission(direction: &'static str) -> LossEvidence {
    LossEvidence {
        rule: "generation.omit-delivery-message-phase.v1",
        field: "generation.message.phase",
        direction,
        action: "omit",
    }
}

/// A target with no phase carrier receives visible history, not equivalent continuation.
/// Reconstruct a private representation; ordinary edits under an existing ID stay protected.
pub(crate) fn omit_request(
    source: &GenerationRequest,
) -> Result<Option<GenerationRequest>, GenerationError> {
    source.validate()?;
    if !has_phase(source.items()) {
        return Ok(None);
    }
    let mut items = source.items().to_vec();
    for (_, item) in &mut items {
        if let Item::Message(message) = item {
            message.phase = None;
        }
    }
    Ok(Some(
        GenerationRequest::from_resources(
            items,
            source.settings().clone(),
            source.resources().clone(),
        )?
        .with_cached_context(source.cached_context().cloned())?
        .with_message_envelopes(source.message_envelopes().to_vec())?,
    ))
}
