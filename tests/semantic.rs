//! Offline semantic ownership, codec projection and failure contracts, grouped by domain.
#[path = "support/chat_profile.rs"]
mod chat_wire;
#[path = "support/semantic_events.rs"]
mod events_support;
#[path = "support/image_generation.rs"]
mod image_support;
#[path = "support/resources.rs"]
mod resources_support;
#[path = "support/properties.rs"]
mod test_properties;
#[path = "support/responses_profile.rs"]
mod wire;

#[path = "semantic/anthropic.rs"]
mod anthropic;
#[path = "semantic/chat.rs"]
mod chat;
#[path = "semantic/compaction.rs"]
mod compaction;
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
#[path = "semantic/text.rs"]
mod text;
#[path = "semantic/tool_calls.rs"]
mod tool_calls;
#[path = "semantic/usage.rs"]
mod usage;
