//! Standard Chat delivery copies omit uncarried reasoning owners and opaque replay.
use crate::{
    protocol::{
        conversion::generation::{Change, ConversionBoundary, Effect, Field, Owner},
        openai::{GenerationApi, dialect::adaptation::WireRules},
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
