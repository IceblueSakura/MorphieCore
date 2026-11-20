//! Selected phase projection. Source history and protected semantic edits stay intact.
use crate::semantic::task::generation::*;

pub fn has_phase(items: &[(ItemId, Item)]) -> bool {
    items
        .iter()
        .any(|(_, item)| matches!(item, Item::Message(m) if m.phase.is_some()))
}

/// A target with no phase carrier receives visible history, not equivalent continuation.
/// Reconstruct a private representation; ordinary edits under an existing ID stay protected.
pub fn omit_request(
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
        GenerationRequest::from_resources_and_envelopes(
            items,
            source.settings().clone(),
            source.resources().clone(),
            source.message_envelopes().to_vec(),
        )?
        .with_cached_context(source.cached_context().cloned())?,
    ))
}
