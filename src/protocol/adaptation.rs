//! Explicit wire rules selected by trusted adapters, never by business JSON.
use crate::{
    protocol::openai::{CodecError, Profile},
    semantic::value::ReplayOrigin,
};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WireRules {
    /// Public SIWC is a restricted Responses operation, not a product-backend alias.
    /// https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations
    pub responses_siwc: bool,
    /// Product backend requires SSE regardless of downstream delivery.
    /// Source: https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/codex-api/src/common.rs
    pub responses_forced_stream: bool,
    /// Closed product access/zero-tool accounting, retained as scoped fidelity.
    pub responses_product_accounting: bool,
    /// Distinct billing ticks/context-window counters, never standard token usage.
    pub responses_context_accounting: bool,
    /// A completed stream may carry an empty terminal output summary after all
    /// item.done snapshots. Only those fully closed items supply the output.
    pub responses_event_owned_output: bool,
    /// A fixed SSE-only backend may omit Content-Type. This does not permit
    /// conflicting explicit media or bypass framing/terminal/EOF validation.
    pub responses_sse_without_content_type: bool,
    /// Chat standard has auto/low/high; this profile additionally represents original.
    /// Source: <https://api-docs.deepseek.com/guides/vision>.
    pub chat_original_image_detail: bool,
    /// Typed image-token fact in Chat input details, not an opaque vendor Usage.
    /// Source: <https://mimo.mi.com/docs/zh-CN/api/chat/openai-api>.
    pub chat_image_usage: bool,
    /// MorphieCore's explicit image-token extension on Responses input details;
    /// the fixed standard Responses schema has no such position.
    pub responses_image_usage: bool,
    /// Omitted text delta/done probabilities mean unreported, not a static empty
    /// probability fact. Explicit null/malformed values and missing final reports
    /// after reported probabilities still fail. Standard grammar stays strict.
    /// Source: <https://mimo.mi.com/docs/zh-CN/api/chat/responses>.
    pub responses_unreported_text_logprobs: bool,
    /// Assistant reasoning text has its own typed owner before its Chat carrier.
    pub readable_reasoning: bool,
    /// Preserve reasoning items while rejecting unverified active request controls.
    /// A readable history carrier does not establish reasoning-effort admission.
    pub reject_reasoning_controls: bool,
    /// Cache hit/miss aliases must agree with reported totals and cached counts.
    pub usage_aliases: bool,
    /// SGLang-shaped Chat reports reasoning tokens beside totals, not inside details.
    /// Normalize one typed count; conflicting aliases fail and unknown is not zero.
    /// Source: https://github.com/sgl-project/sglang/blob/main/python/sglang/srt/entrypoints/openai/protocol.py
    pub chat_reasoning_usage_alias: bool,
    /// Default absent/null cache-write to zero only inside valid usage. Never
    /// replace a reported/invalid count or invent whole usage; record provenance.
    /// This compatibility value is not evidence of cache activity or billing.
    pub default_cache_write: bool,
    /// Classified, bounded response facts bind final semantics and trusted scope.
    pub response_extras: bool,
    /// Validate the derived text view, then discard it; never restore edited text.
    pub derived_output_text: bool,
    /// Bind first timestamp/fingerprint values while keeping identity/model strict.
    pub chunk_metadata_drift: bool,
    /// Normalize timestamp drift only, without permitting fingerprint changes.
    pub chunk_created_drift: bool,
    /// Bounded, source-bound router and billing facts, never task semantics.
    pub routing_extras: bool,
    /// A readable alias must not conflict with the canonical reasoning carrier.
    pub reasoning_alias: bool,
    /// Scoped OpenAI Responses-format reasoning details on a Chat carrier.
    /// Final encrypted replay requires a completed owner and matching origin;
    /// it does not make the carrier response completed or permit partial replay.
    pub structured_chat_reasoning: bool,
    /// A closed format:unknown reasoning.text view owns readable text only.
    /// Never interprets its marker as a versioned or encrypted replay format.
    /// Source: <https://opencode.ai/docs/go/>.
    pub unversioned_chat_reasoning_view: bool,
    /// Accept only the checked redundant format marker, not arbitrary formats.
    pub responses_reasoning_format: bool,
    /// One data-only DONE may follow a validated terminal; never replace it.
    pub responses_done_marker: bool,
    /// Content-free usage tail may repeat the same finish, not add new output.
    pub repeated_finish_usage: bool,
    /// Trusted request policy; business JSON cannot override parameter admission.
    pub require_parameters: bool,
    /// Provider request spelling; downstream still uses max_completion_tokens.
    pub legacy_max_tokens: bool,
    /// Fixed client identity and an explicit session header for OpenCode Go.
    /// Never forwards inbound headers or invents a conversation identity.
    /// Source: <https://opencode.ai/docs/go/#where-can-i-use-it>.
    pub opencode_go_headers: bool,
    /// Advisory lastOne marker and source-bound numeric matched_stop diagnostics.
    pub chat_stop_diagnostics: bool,
    /// Source-bound diagnostic; never public response identity or task semantics.
    pub reported_request_id: bool,
    /// Normalize only explicit integer-zero unsupported usage details.
    pub zero_usage_details: bool,
    /// Validate an exact duplicate Chat-named input detail view on Responses.
    pub responses_usage_detail_view: bool,
    /// Only an explicit null billing placeholder is inactive. Real billing needs
    /// its own owner; this does not admit or discard vendor usage detail views.
    pub null_response_billing: bool,
    /// One response_api usage row: validate redundant totals and promote declared
    /// image/text facts. Conflicts, tool charges and unknown breakdowns are errors.
    /// Source: <https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses>.
    pub responses_billing_view: bool,
    /// Explicit typed text-token positions beyond the fixed Responses schema.
    /// MorphieCore uses canonical detail slots; billing-view profiles own their wire.
    pub responses_text_usage: bool,
    /// A created/queued snapshot emits Queued, not Started. Only a subsequent
    /// in_progress event starts it; absent progress cannot be synthesized.
    pub responses_queued_creation: bool,
    /// This wire names summary deltas/done reasoning_text and indexes them by
    /// content_index. Preserve Summary semantics and validate final snapshots.
    /// Source: <https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-responses>.
    pub responses_summary_text_alias: bool,
    /// Only reasoning.content entries containing exactly a text string may omit
    /// their reasoning_text tag. No other item/part discriminator is inferred.
    pub responses_reasoning_text_shorthand: bool,
    /// A real completed snapshot may close open readable reasoning only when
    /// identity, explicit completed status and all received content match exactly.
    /// Never supply text, close message/tool parts or finalize opaque replay here.
    pub responses_terminal_reasoning: bool,
    /// Empty previous_response_id and empty/null conversation_id are inactive
    /// response placeholders, never permission to resolve or replay remote state.
    /// Source: <https://docs.bigmodel.cn/cn/guide/develop/responses/introduction>.
    pub responses_inactive_state: bool,
    /// Documented null placeholders do not enable active media or legacy calls.
    pub inactive_chat_fields: bool,
    /// vLLM-shaped null-only debug slots and typed cache/image report aliases.
    /// Active unsupported values fail; no defaults or modality estimates are added.
    /// Source: https://github.com/vllm-project/vllm/blob/main/vllm/entrypoints/openai/chat_completion/protocol.py
    pub chat_inference_response_shape: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Adaptation {
    pub rules: WireRules,
    /// Profile image admission is intersected with independently configured Endpoint limits.
    pub images: super::image_constraints::ImageConstraints,
    pub files: super::file_constraints::FileConstraints,
    pub cache: super::cache::CacheProjection,
    /// Stable adapter contract identity, not a network or credential locator.
    pub profile_id: &'static str,
    pub scope: Option<ReplayOrigin>,
}
impl Adaptation {
    pub(crate) fn validate_response(
        &self,
        protocol: Profile,
        o: &Map<String, Value>,
    ) -> Result<(), CodecError> {
        if protocol == Profile::Responses {
            for name in crate::protocol::fidelity::RESPONSE_EXTRA_FIELDS {
                if o.contains_key(*name) && !self.rules.response_extras {
                    return Err(CodecError::Unsupported((*name).into()));
                }
            }
            if o.contains_key("output_text") && !self.rules.derived_output_text {
                return Err(CodecError::Unsupported("output_text".into()));
            }
        }
        if let Some(usage) = o.get("usage").and_then(Value::as_object) {
            for name in ["prompt_cache_hit_tokens", "prompt_cache_miss_tokens"] {
                if usage.contains_key(name)
                    && !(protocol == Profile::Chat && self.rules.usage_aliases)
                {
                    return Err(CodecError::Unsupported(name.into()));
                }
            }
        }
        if protocol == Profile::Chat
            && let Some(choices) = o.get("choices").and_then(Value::as_array)
        {
            for choice in choices {
                for field in ["message", "delta"] {
                    if let Some(message) = choice.get(field).and_then(Value::as_object) {
                        self.validate_message(message)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn validate_message(&self, message: &Map<String, Value>) -> Result<(), CodecError> {
        if message.contains_key("reasoning_content") && !self.rules.readable_reasoning {
            return Err(CodecError::Unsupported("reasoning_content".into()));
        }
        Ok(())
    }
}
