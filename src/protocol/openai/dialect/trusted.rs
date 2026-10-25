//! Provider-selected wire rules. A dialect adjusts the standard API; it never
//! replaces the codec, and a model or endpoint capability is a separate decision.
use crate::{
    protocol::openai::{
        Profile,
        dialect::adaptation::{Adaptation, WireRules},
    },
    semantic::value::ReplayOrigin,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Dialect {
    #[default]
    Standard,
    /// Explicit downstream readable-only Chat contract, never a Provider selector.
    ChatReasoning,
    Siwc,
    Grok,
    MorphieCore,
    DeepSeek,
    Xiaomi,
    OpenRouter,
    OpenCodeGo,
    ModelBest,
    Nvidia,
    Bailian,
    Zhipu,
}
impl Dialect {
    pub(in crate::protocol::openai) fn adaptation(
        self,
        profile: Profile,
        scope: Option<ReplayOrigin>,
    ) -> Adaptation {
        let (profile_id, mut rules) = match self {
            Self::Standard => ("standard-v1", WireRules::default()),
            Self::ChatReasoning => (
                "chat-readable-reasoning-v1",
                WireRules {
                    readable_reasoning: true,
                    readable_reasoning_text_only: true,
                    chat_preserve_thinking: true,
                    ..Default::default()
                },
            ),
            Self::Siwc => (
                "openai-responses",
                WireRules {
                    responses_forced_stream: true,
                    responses_siwc: true,
                    responses_access_programs: true,
                    responses_zero_tool_accounting: true,
                    responses_inactive_reports: true,
                    responses_usage_attribution: true,
                    responses_event_owned_output: true,
                    responses_sse_without_content_type: true,
                    ..Default::default()
                },
            ),
            Self::Grok => (
                "grok-v1",
                WireRules {
                    response_extras: true,
                    responses_context_accounting: true,
                    ..Default::default()
                },
            ),
            Self::MorphieCore => (
                "morphiecore-v1",
                WireRules {
                    chat_video_url: true,
                    readable_reasoning: true,
                    chat_preserve_thinking: true,
                    thinking_controls: true,
                    structured_chat_reasoning: true,
                    accept_reasoning_alias: true,
                    responses_image_usage: true,
                    responses_text_usage: true,
                    ..Default::default()
                },
            ),
            Self::DeepSeek => (
                "deepseek-v1",
                WireRules {
                    parallel_tools_always_enabled: true,
                    chat_cache_usage_aliases: true,
                    readable_reasoning: true,
                    chat_original_image_detail: true,
                    default_cache_write: true,
                    response_extras: true,
                    chunk_metadata_drift: true,
                    ..Default::default()
                },
            ),
            Self::OpenRouter => (
                "openrouter-v1",
                WireRules {
                    chat_video_url: true,
                    responses_input_video: true,
                    readable_reasoning: true,
                    routing_extras: true,
                    response_extras: true,
                    responses_reasoning_format: true,
                    responses_done_marker: true,
                    accept_reasoning_alias: true,
                    reasoning_alias: true,
                    structured_chat_reasoning: true,
                    // Non-OpenAI models (`z-ai/glm-5.3`) stream `format: "unknown"` text.
                    unversioned_chat_reasoning_view: true,
                    repeated_finish_usage: true,
                    require_parameters: true,
                    ..Default::default()
                },
            ),
            Self::OpenCodeGo => (
                "opencode-go-v1",
                WireRules {
                    readable_reasoning: true,
                    reject_reasoning_controls: true,
                    accept_reasoning_alias: true,
                    reasoning_alias: true,
                    unversioned_chat_reasoning_view: true,
                    repeated_finish_usage: true,
                    legacy_max_tokens: true,
                    opencode_go_headers: true,
                    ..Default::default()
                },
            ),
            // Hosted Chat grammar and readable reasoning are distinct from
            // reasoning control admission; no native Responses operation is inferred.
            // https://github.com/OpenBMB/MiniCPM-V/blob/main/docs/api.md
            Self::ModelBest => (
                "modelbest-v1",
                WireRules {
                    chat_video_url: true,
                    readable_reasoning: true,
                    accept_reasoning_alias: true,
                    reasoning_alias: true,
                    chat_stop_diagnostics: true,
                    chat_reasoning_usage_alias: true,
                    chunk_created_drift: true,
                    chat_inference_response_shape: true,
                    legacy_max_tokens: true,
                    reject_reasoning_controls: true,
                    ..Default::default()
                },
            ),
            Self::Nvidia => (
                "nvidia-v1",
                WireRules {
                    readable_reasoning: true,
                    legacy_max_tokens: true,
                    response_extras: true,
                    responses_reasoning_text_shorthand: true,
                    responses_terminal_reasoning: true,
                    responses_done_marker: true,
                    ..Default::default()
                },
            ),
            Self::Bailian => (
                "bailian-v1",
                WireRules {
                    chat_video_url: true,
                    // Both billing plans use the same upstream wire dialect.
                    chunk_created_drift: true,
                    empty_continuation_call_id: true,
                    responses_reported_schema_alias: true,
                    readable_reasoning: true,
                    chat_preserve_thinking: true,
                    thinking_controls: true,
                    bailian_requests: true,
                    response_extras: true,
                    responses_billing_view: true,
                    responses_text_usage: true,
                    responses_image_usage: true,
                    responses_queued_creation: true,
                    responses_summary_text_alias: true,
                    ..Default::default()
                },
            ),
            Self::Zhipu => (
                "zhipu-v1",
                WireRules {
                    chat_video_url: true,
                    readable_reasoning: true,
                    reject_disabled_reasoning: true,
                    legacy_max_tokens: true,
                    reported_request_id: true,
                    responses_inactive_state: true,
                    responses_unreported_text_logprobs: true,
                    // Optional transport trailer, never a replacement for a real terminal.
                    responses_done_marker: true,
                    ..Default::default()
                },
            ),
            Self::Xiaomi => (
                "xiaomi-v1",
                WireRules {
                    omit_image_detail: true,
                    readable_reasoning: true,
                    responses_unreported_text_logprobs: true,
                    chunk_metadata_drift: true,
                    ..Default::default()
                },
            ),
        };
        let mut images = match self {
            Self::MorphieCore | Self::Xiaomi => {
                crate::protocol::image_constraints::ImageConstraints::all()
            }
            _ => crate::protocol::image_constraints::ImageConstraints::common(),
        };
        if matches!(self, Self::Xiaomi | Self::Bailian) {
            images.details.clear();
        }
        let mut cache = match self {
            // Public Responses carriers, independent of OAuth or product session headers.
            // https://docs.x.ai/developers/advanced-api-usage/prompt-caching/maximizing-cache-hits
            Self::Siwc | Self::Grok => crate::protocol::cache::CacheProjection {
                key: true,
                ..Default::default()
            },
            Self::Standard | Self::ChatReasoning => crate::protocol::cache::CacheProjection {
                session_id: false,
                ..crate::protocol::cache::CacheProjection::all()
            },
            Self::MorphieCore => crate::protocol::cache::CacheProjection::all(),
            // The standard cache key is explicitly accepted as an affinity fallback.
            // session_id is a separate caller-supplied grouping value, never synthesized.
            // https://openrouter.ai/docs/guides/best-practices/prompt-caching
            // https://openrouter.ai/docs/client-sdks/typescript/models/responsesrequest
            Self::OpenRouter => crate::protocol::cache::CacheProjection {
                key: true,
                session_id: true,
                ..Default::default()
            },
            // Neutral request context supplies the internal session header;
            // cache keys remain independent and do not enter the Go body.
            // https://opencode.ai/docs/go/#where-can-i-use-it
            Self::OpenCodeGo => crate::protocol::cache::CacheProjection {
                session_id: true,
                ..Default::default()
            },
            _ => Default::default(),
        };
        // A Responses-only rule set never activates a Chat encoding, and the reverse.
        if self == Self::Grok && profile != Profile::Responses {
            cache.key = false;
        }
        if self == Self::Bailian && profile != Profile::Chat {
            rules.chunk_created_drift = false;
            rules.empty_continuation_call_id = false;
        }
        Adaptation {
            rules,
            effort_mapping: None,
            reasoning_policy: None,
            images,
            files: match self {
                Self::Standard | Self::ChatReasoning | Self::MorphieCore | Self::Siwc => {
                    crate::protocol::file_constraints::FileConstraints::all()
                }
                Self::OpenRouter => crate::protocol::file_constraints::FileConstraints {
                    urls: true,
                    ..crate::protocol::file_constraints::FileConstraints::pdf()
                },
                _ => crate::protocol::file_constraints::FileConstraints::none(),
            },
            cache,
            profile_id,
            scope,
        }
    }
}
