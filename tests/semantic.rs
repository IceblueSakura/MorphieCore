//! Offline semantic ownership, codec projection and failure contracts, grouped by domain.
pub(crate) use morphiecore_test_harness::{
    image_generation as image_support, resources as resources_support,
    semantic_events as events_support,
};
pub(crate) use morphiecore_test_support::{
    chat_profile as chat_wire, messages_stream, properties as test_properties,
    responses_profile as wire,
};

#[path = "semantic/anthropic.rs"]
mod anthropic;
#[path = "semantic/attribution.rs"]
mod attribution;
#[path = "semantic/chat.rs"]
mod chat;
#[path = "semantic/compaction.rs"]
mod compaction;
#[path = "semantic/embedding.rs"]
mod embedding;
#[path = "semantic/google.rs"]
mod google;
#[path = "semantic/history.rs"]
mod history;
#[path = "semantic/interactions.rs"]
mod interactions;
#[path = "semantic/media.rs"]
mod media;
#[path = "semantic/messages.rs"]
mod messages;
#[path = "semantic/provider_observations.rs"]
mod provider_observations;
#[path = "semantic/providers.rs"]
mod providers;
#[path = "semantic/reasoning_replay.rs"]
mod reasoning_replay;
#[path = "semantic/schema.rs"]
mod schema;
#[path = "semantic/standard_contracts.rs"]
mod standard_contracts;
#[path = "semantic/text.rs"]
mod text;
#[path = "semantic/tool_calls.rs"]
mod tool_calls;
#[path = "semantic/usage.rs"]
mod usage;
