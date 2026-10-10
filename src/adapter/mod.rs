//! Trusted bidirectional boundary adapters composed from shared protocol codecs.
//! No registry lookup, credentials, network or arbitrary JSON body hooks live here.
//! This module owns profile-to-rule selection; rule semantics live in `WireRules`
//! and owning codecs. Dialect existence alone does not register or enable a model.
//! Official lookup sources: `docs/references/providers/README.md`.
use crate::{
    lowering::generation::{GenerationRepresentationContract, RepresentationError, lower_response},
    protocol::{
        CodecError, DecodedResponse,
        adaptation::{Adaptation, WireRules},
        openai::{self, Profile, events::EventDecoder},
    },
    semantic::value::ReplayOrigin,
};
use serde_json::Value;
pub mod embeddings;
pub mod images;
mod request;
mod siwc;
pub mod speech;
pub mod transcription;
mod upstream;
pub use request::Request;
pub use upstream::UpstreamAdapter;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Dialect {
    #[default]
    Standard,
    Siwc,
    Codex,
    Grok,
    MorphieCore,
    DeepSeek,
    Xiaomi,
    OpenRouter,
    OpenCodeGo,
    ModelBest,
    Nvidia,
    Bailian,
    BailianTokenPlan,
    Zhipu,
}
impl Dialect {
    fn adaptation(self, scope: Option<ReplayOrigin>) -> Adaptation {
        let (profile_id, rules) = match self {
            Self::Standard => ("standard-v1", WireRules::default()),
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
            Self::Codex => (
                "codex-v1",
                WireRules {
                    response_extras: true,
                    responses_product_accounting: true,
                    responses_event_owned_output: true,
                    responses_forced_stream: true,
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
                    readable_reasoning: true,
                    structured_chat_reasoning: true,
                    chat_image_usage: true,
                    responses_image_usage: true,
                    responses_text_usage: true,
                    ..Default::default()
                },
            ),
            Self::DeepSeek => (
                "deepseek-v1",
                WireRules {
                    readable_reasoning: true,
                    chat_original_image_detail: true,
                    usage_aliases: true,
                    default_cache_write: true,
                    response_extras: true,
                    chunk_metadata_drift: true,
                    ..Default::default()
                },
            ),
            Self::OpenRouter => (
                "openrouter-v1",
                WireRules {
                    readable_reasoning: true,
                    routing_extras: true,
                    response_extras: true,
                    responses_reasoning_format: true,
                    responses_done_marker: true,
                    reasoning_alias: true,
                    structured_chat_reasoning: true,
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
                    reasoning_alias: true,
                    unversioned_chat_reasoning_view: true,
                    zero_usage_details: true,
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
                    readable_reasoning: true,
                    reasoning_alias: true,
                    chat_stop_diagnostics: true,
                    chat_reasoning_usage_alias: true,
                    chunk_created_drift: true,
                    inactive_chat_fields: true,
                    chat_inference_response_shape: true,
                    chat_image_usage: true,
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
                    null_response_billing: true,
                    responses_reasoning_text_shorthand: true,
                    responses_terminal_reasoning: true,
                    responses_done_marker: true,
                    ..Default::default()
                },
            ),
            Self::Bailian | Self::BailianTokenPlan => (
                if self == Self::BailianTokenPlan {
                    "bailian-tokenplan-v1"
                } else {
                    "bailian-v1"
                },
                WireRules {
                    // Scope timestamp normalization to the plan's Chat carrier.
                    chunk_created_drift: self == Self::BailianTokenPlan,
                    empty_continuation_call_id: self == Self::BailianTokenPlan,
                    responses_reported_schema_alias: self == Self::BailianTokenPlan,
                    readable_reasoning: true,
                    legacy_max_tokens: true,
                    inactive_chat_fields: true,
                    response_extras: true,
                    responses_billing_view: true,
                    responses_text_usage: true,
                    responses_image_usage: true,
                    null_response_billing: true,
                    responses_queued_creation: true,
                    responses_summary_text_alias: true,
                    // Modality counters are reported facts, never inferred from images.
                    // https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions
                    chat_image_usage: true,
                    ..Default::default()
                },
            ),
            Self::Zhipu => (
                "zhipu-v1",
                WireRules {
                    readable_reasoning: true,
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
                    readable_reasoning: true,
                    responses_unreported_text_logprobs: true,
                    chat_image_usage: true,
                    derived_output_text: true,
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
        if self == Self::Xiaomi {
            images.details.clear();
        }
        let cache = match self {
            // Public Responses carriers, independent of OAuth or product session headers.
            // https://docs.x.ai/developers/advanced-api-usage/prompt-caching/maximizing-cache-hits
            Self::Siwc | Self::Grok => crate::protocol::cache::CacheProjection {
                key: true,
                ..Default::default()
            },
            Self::Standard => crate::protocol::cache::CacheProjection {
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
        Adaptation {
            rules,
            images,
            files: match self {
                Self::Standard | Self::MorphieCore | Self::Siwc => {
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Adapter {
    pub protocol: Profile,
    pub adaptation: Adaptation,
}
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
}
impl Adapter {
    pub fn new(protocol: Profile, dialect: Dialect, scope: Option<ReplayOrigin>) -> Self {
        let mut adaptation = dialect.adaptation(scope);
        if dialect == Dialect::Grok && protocol != Profile::Responses {
            adaptation.cache.key = false;
        }
        if dialect == Dialect::BailianTokenPlan && protocol != Profile::Chat {
            adaptation.rules.chunk_created_drift = false;
            adaptation.rules.empty_continuation_call_id = false;
        }
        Self {
            protocol,
            adaptation,
        }
    }
    pub fn contract(
        &self,
        contract: &GenerationRepresentationContract,
    ) -> GenerationRepresentationContract {
        let mut result = contract.clone();
        result.adaptation = self.adaptation.clone();
        result.images.intersect(&self.adaptation.images);
        result.files.intersect(&self.adaptation.files);
        result.cache.intersect(self.adaptation.cache);
        result
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        let value = openai::json::decode(bytes)?;
        let mut decoded = match self.protocol {
            Profile::Chat => {
                openai::chat_envelope::validate_response(&value)?;
                openai::static_response::decode_chat_with(&value, &self.adaptation)?
            }
            Profile::Responses => {
                openai::envelope::validate_response_snapshot_with(&value, &self.adaptation)?;
                openai::static_response::decode_responses_with(&value, &self.adaptation)?
            }
        };
        if let Some(scope) = &self.adaptation.scope {
            decoded.fidelity.bind_replay_origin(scope)?;
        }
        for (_, item) in decoded.semantic.items() {
            if let crate::semantic::task::generation::Item::Message(m) = item {
                for p in &m.parts {
                    if matches!(
                        p.content,
                        crate::semantic::task::generation::ContentPart::Audio(_)
                            | crate::semantic::task::generation::ContentPart::AudioReference(_)
                    ) {
                        decoded.fidelity.record_audio(
                            p.id,
                            &p.content,
                            self.adaptation.scope.clone(),
                        )?;
                    }
                }
            }
        }
        Ok(decoded)
    }
    pub fn encode_response(
        &self,
        response: &DecodedResponse,
        contract: &GenerationRepresentationContract,
    ) -> Result<Value, AdapterError> {
        let target = lower_response(
            &response.semantic,
            &response.fidelity,
            &response.metadata,
            self.protocol,
            self.contract(contract),
        )?;
        Ok(match self.protocol {
            Profile::Chat => openai::chat_envelope::encode_response(&target)?,
            Profile::Responses => openai::envelope::encode_response(&target)?,
        })
    }
    pub fn event_decoder(&self) -> EventDecoder {
        let decoder = EventDecoder::new(self.protocol).with_adaptation(self.adaptation.clone());
        match &self.adaptation.scope {
            Some(scope) => decoder.with_replay_origin(scope.clone()),
            None => decoder,
        }
    }
}
