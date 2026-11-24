//! Provider-selected wire rules. A dialect adjusts the standard API; it never
//! replaces the codec, and a model or endpoint capability is a separate decision.
use crate::{
    openai::{
        GenerationApi,
        dialect::adaptation::{
            AccountingRules, Adaptation, ChatFinishReason, ChunkMetadata, HooksRules,
            MetadataRules, ReasoningRules, RequestRules, StreamRules, WireRules,
        },
    },
    semantic::value::ReplayOrigin,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    #[default]
    Standard,
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
    pub(in crate::openai) fn adaptation(
        self,
        profile: GenerationApi,
        scope: Option<ReplayOrigin>,
    ) -> Adaptation {
        let (profile_id, mut rules) = match self {
            Self::Standard => ("standard-v1", WireRules::default()),
            Self::Siwc => (
                "openai-responses",
                WireRules {
                    hooks: HooksRules {
                        responses_siwc: true,
                        ..Default::default()
                    },
                    metadata: MetadataRules {
                        responses_access_programs: true,
                        responses_inactive_reports: true,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        responses_zero_tool_accounting: true,
                        responses_usage_attribution: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        responses_event_owned_output: true,
                        responses_sse_without_content_type: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::Grok => (
                "grok-v1",
                WireRules {
                    metadata: MetadataRules {
                        response_extras: true,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        responses_context_accounting: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::MorphieCore => (
                "morphiecore-v1",
                WireRules {
                    request: RequestRules {
                        chat_video_url: true,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        chat_preserve_thinking: true,
                        thinking_controls: true,
                        structured_chat_reasoning: true,
                        accept_reasoning_alias: true,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        responses_image_usage: true,
                        responses_text_usage: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::DeepSeek => (
                "deepseek-v1",
                WireRules {
                    hooks: HooksRules {
                        deepseek_chat_requests: profile == GenerationApi::Chat,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        deepseek_responses_reasoning: profile == GenerationApi::Responses,
                        readable_reasoning: true,
                        ..Default::default()
                    },
                    request: RequestRules {
                        legacy_max_tokens: profile == GenerationApi::Chat,
                        system_instruction_messages: true,
                        responses_developer_is_user: profile == GenerationApi::Responses,
                        parallel_tools_always_enabled: true,
                        chat_original_image_detail: true,
                        ..Default::default()
                    },
                    metadata: MetadataRules {
                        chat_finish_reasons: &[
                            (
                                "insufficient_system_resource",
                                ChatFinishReason::ResourceFailure,
                            ),
                            ("aborted", ChatFinishReason::Interrupted),
                        ],
                        response_extras: true,
                        chunk: ChunkMetadata::StableCreatedAndFingerprint,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        chat_cache_usage_aliases: true,
                        default_cache_write: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::OpenRouter => (
                "openrouter-v1",
                WireRules {
                    metadata: MetadataRules {
                        chat_finish_reasons: &[("error", ChatFinishReason::ServerFailure)],
                        routing_extras: true,
                        response_extras: true,
                        responses_access_programs: true,
                        ..Default::default()
                    },
                    request: RequestRules {
                        chat_video_url: true,
                        responses_input_video: true,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        responses_reasoning_format: true,
                        accept_reasoning_alias: true,
                        reasoning_alias: true,
                        structured_chat_reasoning: true,
                        // Non-OpenAI models (`z-ai/glm-5.3`) stream `format: "unknown"` text.
                        unversioned_chat_reasoning_view: true,
                        ..Default::default()
                    },
                    hooks: HooksRules {
                        openrouter_session: true,
                        require_parameters: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        responses_done_marker: true,
                        responses_initial_failed: profile == GenerationApi::Responses,
                        repeated_finish_usage: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::OpenCodeGo => (
                "opencode-go-v1",
                WireRules {
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        accept_reasoning_alias: true,
                        reasoning_alias: true,
                        unversioned_chat_reasoning_view: true,
                        ..Default::default()
                    },
                    request: RequestRules {
                        reject_reasoning_controls: true,
                        legacy_max_tokens: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        repeated_finish_usage: true,
                        ..Default::default()
                    },
                    hooks: HooksRules {
                        opencode_go_headers: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            // Hosted Chat has nested activation and readable reasoning carriers;
            // no native Responses operation or numeric thinking budget is inferred.
            // https://github.com/OpenBMB/MiniCPM-V/blob/main/docs/api.md
            Self::ModelBest => (
                "modelbest-v1",
                WireRules {
                    request: RequestRules {
                        chat_video_url: true,
                        legacy_max_tokens: true,
                        reject_reasoning_controls: profile != GenerationApi::Chat,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        accept_reasoning_alias: true,
                        reasoning_alias: true,
                        chat_template_thinking: profile == GenerationApi::Chat,
                        ..Default::default()
                    },
                    metadata: MetadataRules {
                        chat_stop_diagnostics: true,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        chat_reasoning_usage_alias: true,
                        ..Default::default()
                    },
                    hooks: HooksRules {
                        chat_inference_response_shape: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::Nvidia => (
                "nvidia-v1",
                WireRules {
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        responses_reasoning_text_shorthand: true,
                        responses_terminal_reasoning: true,
                        ..Default::default()
                    },
                    request: RequestRules {
                        legacy_max_tokens: true,
                        ..Default::default()
                    },
                    metadata: MetadataRules {
                        response_extras: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        responses_done_marker: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::Bailian => (
                "bailian-v1",
                WireRules {
                    request: RequestRules {
                        system_instruction_messages: profile == GenerationApi::Chat,
                        chat_string_instructions: profile == GenerationApi::Chat,
                        chat_video_url: true,
                        ..Default::default()
                    },
                    metadata: MetadataRules {
                        // Both billing plans use the same upstream wire dialect.
                        responses_reported_schema_alias: true,
                        response_extras: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        empty_continuation_call_id: true,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        chat_preserve_thinking: true,
                        thinking_controls: true,
                        responses_summary_text_alias: true,
                        ..Default::default()
                    },
                    hooks: HooksRules {
                        bailian_requests: true,
                        ..Default::default()
                    },
                    accounting: AccountingRules {
                        responses_billing_view: true,
                        responses_text_usage: true,
                        responses_image_usage: true,
                        ..Default::default()
                    },
                },
            ),
            Self::Zhipu => (
                "zhipu-v1",
                WireRules {
                    metadata: MetadataRules {
                        chat_finish_reasons: &[
                            ("sensitive", ChatFinishReason::ContentFilter),
                            (
                                "model_context_window_exceeded",
                                ChatFinishReason::ContextWindow,
                            ),
                            ("network_error", ChatFinishReason::NetworkFailure),
                        ],
                        reported_request_id: true,
                        responses_inactive_state: true,
                        ..Default::default()
                    },
                    request: RequestRules {
                        // Chat and Responses have different role and message-shape contracts.
                        // https://docs.bigmodel.cn/api-reference/模型-api/对话补全
                        // https://docs.bigmodel.cn/api-reference/response/创建-response
                        system_instruction_messages: profile == GenerationApi::Chat,
                        chat_string_instructions: profile == GenerationApi::Chat,
                        responses_typed_messages: profile == GenerationApi::Responses,
                        chat_video_url: true,
                        reject_disabled_reasoning: true,
                        legacy_max_tokens: true,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        responses_unreported_text_logprobs: true,
                        // Optional transport trailer, never a replacement for a real terminal.
                        responses_done_marker: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            Self::Xiaomi => (
                "xiaomi-v1",
                WireRules {
                    metadata: MetadataRules {
                        chat_finish_reasons: &[(
                            "repetition_truncation",
                            ChatFinishReason::Repetition,
                        )],
                        chunk: ChunkMetadata::StableCreatedAndFingerprint,
                        ..Default::default()
                    },
                    request: RequestRules {
                        omit_image_detail: true,
                        ..Default::default()
                    },
                    reasoning: ReasoningRules {
                        readable_reasoning: true,
                        ..Default::default()
                    },
                    stream: StreamRules {
                        responses_unreported_text_logprobs: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
        };
        let mut images = match self {
            Self::MorphieCore | Self::Xiaomi => crate::image_constraints::ImageConstraints::all(),
            _ => crate::image_constraints::ImageConstraints::common(),
        };
        if matches!(self, Self::Xiaomi | Self::Bailian) {
            images.details.clear();
        }
        let mut cache = match self {
            // Public Responses carriers, independent of OAuth or product session headers.
            // https://docs.x.ai/developers/advanced-api-usage/prompt-caching/maximizing-cache-hits
            Self::Siwc | Self::Grok => crate::cache::CacheTargetPolicy {
                key: true,
                ..Default::default()
            },
            Self::Standard => crate::cache::CacheTargetPolicy {
                session_id: false,
                ..crate::cache::CacheTargetPolicy::all()
            },
            Self::MorphieCore => crate::cache::CacheTargetPolicy::all(),
            // Explicit sessions or declared conversations activate session stickiness.
            // Cache keys alone retain the separate upstream affinity fallback.
            // https://openrouter.ai/docs/guides/best-practices/prompt-caching
            // https://openrouter.ai/docs/client-sdks/typescript/models/responsesrequest
            Self::OpenRouter => crate::cache::CacheTargetPolicy {
                key: true,
                session_id: true,
                ..Default::default()
            },
            // Neutral request context supplies the internal session header;
            // cache keys remain independent and do not enter the Go body.
            // https://opencode.ai/docs/go/#where-can-i-use-it
            Self::OpenCodeGo => crate::cache::CacheTargetPolicy {
                session_id: true,
                ..Default::default()
            },
            _ => Default::default(),
        };
        // A Responses-only rule set never activates a Chat encoding, and the reverse.
        if self == Self::Grok && profile != GenerationApi::Responses {
            cache.key = false;
        }
        if self == Self::Bailian && profile != GenerationApi::Chat {
            rules.stream.empty_continuation_call_id = false;
        }
        Adaptation {
            rules,
            effort_mapping: None,
            reasoning_policy: None,
            images,
            files: match self {
                Self::Standard | Self::MorphieCore | Self::Siwc => {
                    crate::file_constraints::FileConstraints::all()
                }
                Self::OpenRouter => crate::file_constraints::FileConstraints {
                    urls: true,
                    ..crate::file_constraints::FileConstraints::pdf()
                },
                _ => crate::file_constraints::FileConstraints::none(),
            },
            cache,
            profile_id,
            scope,
            reasoning_scope: None,
        }
    }
}
