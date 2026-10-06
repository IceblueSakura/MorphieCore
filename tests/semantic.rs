//! Offline semantic ownership, codec projection and failure contracts, grouped by domain.
#[path = "support/semantic_events.rs"]
mod events_support;
#[path = "support/image_generation.rs"]
mod image_support;
#[path = "support/responses_profile.rs"]
mod wire;

#[path = "semantic/adapters.rs"]
mod adapters;
#[path = "semantic/billing_modal_usage.rs"]
mod billing_modal_usage;
#[path = "semantic/cache_prefix.rs"]
mod cache_prefix;
#[path = "semantic/cache_projection.rs"]
mod cache_projection;
#[path = "semantic/chat_logprobs.rs"]
mod chat_logprobs;
#[path = "semantic/chat_media.rs"]
mod chat_media;
#[path = "semantic/chat_wire.rs"]
mod chat_wire;
#[path = "semantic/client_carrier.rs"]
mod client_carrier;
#[path = "semantic/continuation.rs"]
mod continuation;
#[path = "semantic/contract_ownership.rs"]
mod contract_ownership;
#[path = "semantic/extensions.rs"]
mod extensions;
#[path = "semantic/files.rs"]
mod files;
#[path = "semantic/function_events.rs"]
mod function_events;
#[path = "semantic/group_projection.rs"]
mod group_projection;
#[path = "semantic/history_continuation.rs"]
mod history_continuation;
#[path = "semantic/history_dependencies.rs"]
mod history_dependencies;
#[path = "semantic/image_binding.rs"]
mod image_binding;
#[path = "semantic/image_budgets.rs"]
mod image_budgets;
#[path = "semantic/image_collection.rs"]
mod image_collection;
#[path = "semantic/image_controls.rs"]
mod image_controls;
#[path = "semantic/image_generation.rs"]
mod image_generation;
#[path = "semantic/image_usage.rs"]
mod image_usage;
#[path = "semantic/images.rs"]
mod images;
#[path = "semantic/instructions.rs"]
mod instructions;
#[path = "semantic/interaction_progress.rs"]
mod interaction_progress;
#[path = "semantic/message_groups.rs"]
mod message_groups;
#[path = "semantic/model_constraints.rs"]
mod model_constraints;
#[path = "semantic/modelbest.rs"]
mod modelbest;
#[path = "semantic/openrouter_images.rs"]
mod openrouter_images;
#[path = "semantic/openrouter_speech.rs"]
mod openrouter_speech;
#[path = "semantic/openrouter_speech_binding.rs"]
mod openrouter_speech_binding;
#[path = "semantic/parsed_replay.rs"]
mod parsed_replay;
#[path = "semantic/phase.rs"]
mod phase;
#[path = "semantic/provider_profiles.rs"]
mod provider_profiles;
#[path = "semantic/reasoning.rs"]
mod reasoning;
#[path = "semantic/reasoning_boundary.rs"]
mod reasoning_boundary;
#[path = "semantic/replay_formats.rs"]
mod replay_formats;
#[path = "semantic/replay_ownership.rs"]
mod replay_ownership;
#[path = "semantic/response.rs"]
mod response;
#[path = "semantic/response_requirements.rs"]
mod response_requirements;
#[path = "semantic/router_adapter.rs"]
mod router_adapter;
#[path = "semantic/schema.rs"]
mod schema;
#[path = "semantic/scoped_usage.rs"]
mod scoped_usage;
#[path = "semantic/speech.rs"]
mod speech;
#[path = "semantic/speech_binding.rs"]
mod speech_binding;
#[path = "support/speech.rs"]
mod speech_support;
#[path = "semantic/string_enums.rs"]
mod string_enums;
#[path = "semantic/structured_events.rs"]
mod structured_events;
#[path = "semantic/subscription_accounting.rs"]
mod subscription_accounting;
#[path = "semantic/text_admission.rs"]
mod text_admission;
#[path = "semantic/text_events.rs"]
mod text_events;
#[path = "semantic/text_profile.rs"]
mod text_profile;
#[path = "semantic/timestamps.rs"]
mod timestamps;
#[path = "semantic/tokenplan_asr.rs"]
mod tokenplan_asr;
#[path = "semantic/tokenplan_audio_binding.rs"]
mod tokenplan_audio_binding;
#[path = "semantic/tokenplan_speech.rs"]
mod tokenplan_speech;
#[path = "semantic/tool_namespaces.rs"]
mod tool_namespaces;
#[path = "semantic/tool_results.rs"]
mod tool_results;
#[path = "semantic/tool_values.rs"]
mod tool_values;
#[path = "semantic/tools.rs"]
mod tools;
#[path = "semantic/turn_continuation.rs"]
mod turn_continuation;
#[path = "semantic/unreported_event_probabilities.rs"]
mod unreported_event_probabilities;
#[path = "semantic/usage.rs"]
mod usage;
#[path = "semantic/usage_views.rs"]
mod usage_views;
#[path = "semantic/vendor_shapes.rs"]
mod vendor_shapes;
