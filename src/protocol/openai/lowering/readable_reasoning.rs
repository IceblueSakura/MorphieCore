//! Standard Chat delivery copies omit unsigned reasoning, never opaque replay.
use crate::{
    protocol::{
        conversion::generation::{Change, ConversionBoundary, Effect, Field, Owner},
        openai::{GenerationApi, dialect::adaptation::WireRules, lowering::RepresentationError},
    },
    semantic::task::generation::*,
};

pub(crate) const RULE: &str = "chat.omit-readable-reasoning.v1";

pub(crate) fn selected(profile: GenerationApi, rules: &WireRules) -> bool {
    profile == GenerationApi::Chat && !rules.readable_reasoning && !rules.structured_chat_reasoning
}

pub(crate) fn loss(item: ItemId, boundary: ConversionBoundary) -> Effect {
    Effect::new(RULE, Field::DeliveredProjection, boundary, Change::Omit).at(Owner::item(item))
}

pub(crate) fn project(
    source: &GenerationResponse,
    profile: GenerationApi,
    rules: &WireRules,
) -> Result<Option<(GenerationResponse, Vec<Effect>)>, RepresentationError> {
    if !selected(profile, rules) {
        return Ok(None);
    }
    let mut effects = vec![];
    for (id, item) in source.items() {
        if let Item::Reasoning(reasoning) = item {
            if reasoning.replay.is_some() {
                return Err(RepresentationError::ReplayFormat);
            }
            effects.push(loss(*id, ConversionBoundary::ResponseProject));
        }
    }
    if effects.is_empty() {
        return Ok(None);
    }
    let items = source
        .items()
        .iter()
        .filter(|(_, item)| !matches!(item, Item::Reasoning(_)))
        .cloned()
        .collect();
    Ok(Some((source.clone().with_items(items)?, effects)))
}

/// Owner identity, not fragment spelling, determines which events are omitted.
pub(crate) fn owner(event: &StreamEvent) -> Option<ItemId> {
    match event {
        StreamEvent::ItemStarted { item, .. }
        | StreamEvent::ToolResultOutput { item, .. }
        | StreamEvent::PartStarted { item, .. }
        | StreamEvent::Delta { item, .. }
        | StreamEvent::AudioDelta { item, .. }
        | StreamEvent::LogprobsSnapshot { item, .. }
        | StreamEvent::AnnotationAdded { item, .. }
        | StreamEvent::TextMetadata { item, .. }
        | StreamEvent::ValueFinished { item, .. }
        | StreamEvent::PartReplay { item, .. }
        | StreamEvent::PartFinished { item, .. }
        | StreamEvent::PartInterrupted { item, .. }
        | StreamEvent::CompactionProgress { item }
        | StreamEvent::ItemFinished { item, .. } => Some(*item),
        _ => None,
    }
}

pub(crate) fn carries_replay(event: &StreamEvent) -> bool {
    matches!(
        event,
        StreamEvent::ItemStarted {
            replay: Some(_),
            ..
        } | StreamEvent::ItemFinished {
            replay: Some(_),
            ..
        } | StreamEvent::PartReplay { .. }
    )
}
