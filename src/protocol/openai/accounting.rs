//! Chat and Responses token accounting over one typed `Usage`.
//! The two profiles name the same counters differently; aliases are named rules.
use crate::{
    protocol::{
        failure::{CarrierReason, ShapeReason},
        openai::{CodecError, GenerationApi, shape::*},
    },
    semantic::task::generation::*,
};
use serde_json::{Map, Value, json};

pub(super) fn usage(
    value: Option<&Value>,
    profile: GenerationApi,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    evidence: &mut crate::protocol::conversion::generation::EvidenceDelta,
    boundary: crate::protocol::conversion::generation::ConversionBoundary,
) -> Result<Option<Usage>, CodecError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let usage = object(value)?;
    let (input_key, output_key, input_details, output_details) = match profile {
        GenerationApi::Chat => (
            "prompt_tokens",
            "completion_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        ),
        GenerationApi::Responses => (
            "input_tokens",
            "output_tokens",
            "input_tokens_details",
            "output_tokens_details",
        ),
    };
    let mut allowed = vec![
        input_key,
        output_key,
        "total_tokens",
        input_details,
        output_details,
    ];
    let cache_aliases = profile == GenerationApi::Chat && adaptation.rules.chat_cache_usage_aliases;
    if cache_aliases {
        allowed.extend(["prompt_cache_hit_tokens", "prompt_cache_miss_tokens"]);
    }
    if profile == GenerationApi::Chat && adaptation.rules.chat_reasoning_usage_alias {
        allowed.push("reasoning_tokens");
    }
    DecodeDirection::Upstream.fields(usage, &allowed)?;
    let mut parsed = Usage {
        output_relation: crate::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
        input_relation: crate::semantic::task::generation::InputTokenRelation::IncludesCache,
        total_relation: crate::semantic::task::generation::TotalTokenRelation::InputAndOutput,
        input_tokens: Some(count(usage, input_key)?),
        output_tokens: Some(count(usage, output_key)?),
        total_tokens: Some(count(usage, "total_tokens")?),
        cached_input_tokens: detail(usage, input_details, "cached_tokens")?,
        input_image_tokens: if profile == GenerationApi::Chat
            || adaptation.rules.responses_image_usage
        {
            detail(usage, input_details, "image_tokens")?
        } else {
            None
        },
        input_audio_tokens: if profile == GenerationApi::Chat {
            detail(usage, input_details, "audio_tokens")?
        } else {
            None
        },
        output_audio_tokens: if profile == GenerationApi::Chat {
            detail(usage, output_details, "audio_tokens")?
        } else {
            None
        },
        input_cache_write_tokens: detail(usage, input_details, "cache_write_tokens")?,
        reasoning_tokens: detail(usage, output_details, "reasoning_tokens")?,
        input_text_tokens: if profile == GenerationApi::Chat
            || adaptation.rules.responses_text_usage
        {
            detail(usage, input_details, "text_tokens")?
        } else {
            None
        },
        output_text_tokens: if profile == GenerationApi::Chat
            || adaptation.rules.responses_text_usage
        {
            detail(usage, output_details, "text_tokens")?
        } else {
            None
        },
        accepted_prediction_tokens: if profile == GenerationApi::Chat {
            detail(usage, output_details, "accepted_prediction_tokens")?
        } else {
            None
        },
        rejected_prediction_tokens: if profile == GenerationApi::Chat {
            detail(usage, output_details, "rejected_prediction_tokens")?
        } else {
            None
        },
    };
    if profile == GenerationApi::Chat
        && adaptation.rules.chat_reasoning_usage_alias
        && let Some(value) = usage.get("reasoning_tokens")
    {
        let reported = if value.is_null() {
            None
        } else {
            Some(
                value
                    .as_u64()
                    .ok_or(CodecError::Invalid(ShapeReason::Usage))?,
            )
        };
        if parsed
            .reasoning_tokens
            .zip(reported)
            .is_some_and(|(left, right)| left != right)
        {
            return Err(CodecError::Invalid(ShapeReason::Usage));
        }
        parsed.reasoning_tokens = parsed.reasoning_tokens.or(reported);
    }
    // DeepSeek Chat reports `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens`
    // as aliases of the standard details. Normalize the hit count into
    // `cached_input_tokens` and validate both aliases against the reported
    // totals: no count is lost, none is estimated, and disagreement is rejected.
    if cache_aliases && let Some(hit) = usage.get("prompt_cache_hit_tokens") {
        let hit = hit
            .as_u64()
            .ok_or(CodecError::Invalid(ShapeReason::Usage))?;
        if parsed
            .cached_input_tokens
            .is_some_and(|cached| cached != hit)
        {
            return Err(CodecError::Invalid(ShapeReason::Usage));
        }
        parsed.cached_input_tokens = Some(hit);
    }
    if cache_aliases && let Some(miss) = usage.get("prompt_cache_miss_tokens") {
        let miss = miss
            .as_u64()
            .ok_or(CodecError::Invalid(ShapeReason::Usage))?;
        let cached = parsed
            .cached_input_tokens
            .ok_or(CodecError::Unsupported(CarrierReason::Usage))?;
        if parsed
            .input_tokens
            .and_then(|input| input.checked_sub(cached))
            != Some(miss)
        {
            return Err(CodecError::Invalid(ShapeReason::Usage));
        }
    }
    if parsed.input_cache_write_tokens.is_none() && adaptation.rules.default_cache_write {
        parsed.input_cache_write_tokens = Some(0);
        evidence.record_cache_write_default(adaptation.profile_id, boundary);
    }
    if parsed
        .input_tokens
        .zip(parsed.output_tokens)
        .and_then(|(input, output)| input.checked_add(output))
        != parsed.total_tokens
    {
        return Err(CodecError::Invalid(ShapeReason::Usage));
    }
    Ok(Some(parsed))
}
fn count(o: &Map<String, Value>, key: &'static str) -> Result<u64, CodecError> {
    o.get(key)
        .and_then(Value::as_u64)
        .ok_or(CodecError::Invalid(ShapeReason::Usage))
}
fn detail(
    o: &Map<String, Value>,
    key: &'static str,
    field: &'static str,
) -> Result<Option<u64>, CodecError> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let details = object(value)?;
            // Closed, protocol-specific schemas: new Chat facts must never be
            // smuggled into the standard Responses detail objects.
            let allowed: &[&str] = match key {
                "prompt_tokens_details" => &[
                    "audio_tokens",
                    "cached_tokens",
                    "cache_write_tokens",
                    "text_tokens",
                    "image_tokens",
                ],
                "completion_tokens_details" => &[
                    "audio_tokens",
                    "reasoning_tokens",
                    "text_tokens",
                    "accepted_prediction_tokens",
                    "rejected_prediction_tokens",
                ],
                "input_tokens_details" => &[
                    "cached_tokens",
                    "cache_write_tokens",
                    "image_tokens",
                    "text_tokens",
                ],
                "output_tokens_details" => &["reasoning_tokens", "text_tokens"],
                _ => unreachable!("fixed usage detail schema"),
            };
            DecodeDirection::Upstream.fields(details, allowed)?;
            match details.get(field) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => Ok(Some(
                    value
                        .as_u64()
                        .ok_or(CodecError::Invalid(ShapeReason::Usage))?,
                )),
            }
        }
    }
}

pub(super) fn encode_usage(
    usage: Usage,
    profile: GenerationApi,
    rules: &crate::protocol::openai::dialect::adaptation::WireRules,
) -> Value {
    let mut value = match profile {
        GenerationApi::Chat => json!({
            "prompt_tokens": usage.input_tokens,
            "completion_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }),
        GenerationApi::Responses => json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }),
    };
    let object = value.as_object_mut().expect("usage object");
    if let Some(cached) = usage.cached_input_tokens {
        let key = if profile == GenerationApi::Chat {
            "prompt_tokens_details"
        } else {
            "input_tokens_details"
        };
        object.insert(key.into(), json!({"cached_tokens": cached}));
    }
    if let Some(written) = usage.input_cache_write_tokens {
        let details = object
            .entry(if profile == GenerationApi::Chat {
                "prompt_tokens_details"
            } else {
                "input_tokens_details"
            })
            .or_insert_with(|| json!({}));
        details[if profile == GenerationApi::Chat && rules.chat_inference_response_shape {
            "created_cache_tokens"
        } else {
            "cache_write_tokens"
        }] = json!(written);
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        if profile == GenerationApi::Chat && rules.chat_reasoning_usage_alias {
            object.insert("reasoning_tokens".into(), json!(reasoning));
        } else {
            let key = if profile == GenerationApi::Chat {
                "completion_tokens_details"
            } else {
                "output_tokens_details"
            };
            object.insert(key.into(), json!({"reasoning_tokens": reasoning}));
        }
    }
    if let Some(images) = usage.input_image_tokens {
        // Chat owns this standard detail; Responses requires an explicit extension.
        let key = if profile == GenerationApi::Chat {
            "prompt_tokens_details"
        } else {
            "input_tokens_details"
        };
        let details = object.entry(key).or_insert_with(|| json!({}));
        if profile == GenerationApi::Chat && rules.chat_inference_response_shape {
            details["multimodal_tokens"] = json!({"image":images});
        } else {
            details["image_tokens"] = json!(images);
        }
    }
    for (details, count) in [
        (
            if profile == GenerationApi::Chat {
                "prompt_tokens_details"
            } else {
                "input_tokens_details"
            },
            usage.input_text_tokens,
        ),
        (
            if profile == GenerationApi::Chat {
                "completion_tokens_details"
            } else {
                "output_tokens_details"
            },
            usage.output_text_tokens,
        ),
    ] {
        if let Some(count) = count {
            object.entry(details).or_insert_with(|| json!({}))["text_tokens"] = json!(count);
        }
    }
    if profile == GenerationApi::Chat {
        for (details, count) in [
            ("prompt_tokens_details", usage.input_audio_tokens),
            ("completion_tokens_details", usage.output_audio_tokens),
        ] {
            if let Some(count) = count {
                object.entry(details).or_insert_with(|| json!({}))["audio_tokens"] = json!(count);
            }
        }
        for (details, name, count) in [
            (
                "completion_tokens_details",
                "accepted_prediction_tokens",
                usage.accepted_prediction_tokens,
            ),
            (
                "completion_tokens_details",
                "rejected_prediction_tokens",
                usage.rejected_prediction_tokens,
            ),
        ] {
            if let Some(count) = count {
                object.entry(details).or_insert_with(|| json!({}))[name] = json!(count);
            }
        }
    }
    if profile == GenerationApi::Responses
        && rules.responses_billing_view
        && (usage.input_image_tokens.is_some()
            || usage.input_text_tokens.is_some()
            || usage.output_text_tokens.is_some())
    {
        crate::protocol::openai::dialect::adapter_shapes::encode_billing_usage(&mut value);
    }
    value
}
