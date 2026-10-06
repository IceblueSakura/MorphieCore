//! Single-candidate text and function response closure, including non-success terminals.
use super::{
    CodecError, DecodedResponse, Profile, ResponseMetadata, ResponseRepresentation, chat,
    common::*, responses, terminal::*,
};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};

pub(super) fn metadata(
    o: &Map<String, Value>,
    profile: Profile,
) -> Result<ResponseMetadata, CodecError> {
    let created_key = if profile == Profile::Chat {
        "created"
    } else {
        "created_at"
    };
    let id = text(string(o, "id")?, "response id", 256)?
        .as_str()
        .to_owned();
    let model = text(string(o, "model")?, "response model", 256)?
        .as_str()
        .to_owned();
    let created = super::envelope::timestamp(
        o.get(created_key)
            .ok_or(CodecError::Invalid("created time"))?,
    )?;
    let (context, instruction_fidelity) = if profile == Profile::Responses {
        crate::semantic::context::ResponseContext::read(o)?
    } else {
        (
            crate::semantic::context::ResponseContext {
                execution: crate::semantic::context::ExecutionHints::read(o)?,
                system_fingerprint: read_presence(o, "system_fingerprint", |v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or(CodecError::Invalid("system fingerprint"))
                })?,
                ..Default::default()
            },
            Default::default(),
        )
    };
    context.validate()?;
    if profile == Profile::Chat {
        super::chat_envelope::validate_context(&context.execution)?;
    }
    Ok(ResponseMetadata {
        id,
        model,
        created,
        context,
        instruction_fidelity,
    })
}
pub(super) fn usage(
    value: Option<&Value>,
    profile: Profile,
    adaptation: &crate::protocol::adaptation::Adaptation,
    fidelity: &mut crate::protocol::fidelity::FidelityRecords,
) -> Result<Option<Usage>, CodecError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let usage = object(value)?;
    if !adaptation.rules.usage_aliases || profile != Profile::Chat {
        for key in ["prompt_cache_hit_tokens", "prompt_cache_miss_tokens"] {
            if usage.contains_key(key) {
                return Err(CodecError::Unsupported(key.into()));
            }
        }
    }
    let (input_key, output_key, input_details, output_details) = match profile {
        Profile::Chat => (
            "prompt_tokens",
            "completion_tokens",
            "prompt_tokens_details",
            "completion_tokens_details",
        ),
        Profile::Responses => (
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
        "prompt_cache_hit_tokens",
        "prompt_cache_miss_tokens",
    ];
    if profile == Profile::Chat && adaptation.rules.chat_reasoning_usage_alias {
        allowed.push("reasoning_tokens");
    }
    fields(usage, &allowed)?;
    let image_usage = match profile {
        Profile::Chat => adaptation.rules.chat_image_usage,
        Profile::Responses => adaptation.rules.responses_image_usage,
    };
    if !image_usage
        && usage
            .get(input_details)
            .and_then(Value::as_object)
            .is_some_and(|details| details.contains_key("image_tokens"))
    {
        return Err(CodecError::Unsupported("image_tokens".into()));
    }
    if profile == Profile::Responses
        && !adaptation.rules.responses_text_usage
        && [input_details, output_details].iter().any(|key| {
            usage
                .get(*key)
                .and_then(Value::as_object)
                .is_some_and(|d| d.contains_key("text_tokens"))
        })
    {
        return Err(CodecError::Unsupported("text_tokens".into()));
    }
    let mut parsed = Usage {
        scope: crate::semantic::task::generation::UsageScope::Operation,
        basis: crate::semantic::task::generation::UsageBasis::Final,
        output_relation: crate::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
        total_relation: crate::semantic::task::generation::TotalTokenRelation::InputAndOutput,
        input_tokens: Some(count(usage, input_key)?),
        output_tokens: Some(count(usage, output_key)?),
        total_tokens: Some(count(usage, "total_tokens")?),
        cached_input_tokens: detail(usage, input_details, "cached_tokens")?,
        input_image_tokens: detail(usage, input_details, "image_tokens")?,
        input_audio_tokens: if profile == Profile::Chat {
            detail(usage, input_details, "audio_tokens")?
        } else {
            None
        },
        output_audio_tokens: if profile == Profile::Chat {
            detail(usage, output_details, "audio_tokens")?
        } else {
            None
        },
        input_cache_write_tokens: detail(usage, input_details, "cache_write_tokens")?,
        reasoning_tokens: detail(usage, output_details, "reasoning_tokens")?,
        input_text_tokens: detail(usage, input_details, "text_tokens")?,
        output_text_tokens: detail(usage, output_details, "text_tokens")?,
        accepted_prediction_tokens: if profile == Profile::Chat {
            detail(usage, output_details, "accepted_prediction_tokens")?
        } else {
            None
        },
        rejected_prediction_tokens: if profile == Profile::Chat {
            detail(usage, output_details, "rejected_prediction_tokens")?
        } else {
            None
        },
    };
    if profile == Profile::Chat
        && adaptation.rules.chat_reasoning_usage_alias
        && let Some(value) = usage.get("reasoning_tokens")
    {
        let reported = if value.is_null() {
            None
        } else {
            Some(
                value
                    .as_u64()
                    .ok_or(CodecError::Invalid("reasoning usage alias"))?,
            )
        };
        if parsed
            .reasoning_tokens
            .zip(reported)
            .is_some_and(|(left, right)| left != right)
        {
            return Err(CodecError::Invalid("reasoning usage alias conflict"));
        }
        parsed.reasoning_tokens = parsed.reasoning_tokens.or(reported);
    }
    // DeepSeek Chat reports `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens`
    // as aliases of the standard details. Normalize the hit count into
    // `cached_input_tokens` and validate both aliases against the reported
    // totals: no count is lost, none is estimated, and disagreement is rejected.
    if let Some(hit) = usage.get("prompt_cache_hit_tokens") {
        let hit = hit.as_u64().ok_or(CodecError::Invalid("usage detail"))?;
        if parsed
            .cached_input_tokens
            .is_some_and(|cached| cached != hit)
        {
            return Err(CodecError::Invalid("usage alias"));
        }
        parsed.cached_input_tokens = Some(hit);
    }
    if let Some(miss) = usage.get("prompt_cache_miss_tokens") {
        let miss = miss.as_u64().ok_or(CodecError::Invalid("usage detail"))?;
        let cached = parsed.cached_input_tokens.unwrap_or(0);
        if parsed
            .input_tokens
            .and_then(|input| input.checked_sub(cached))
            != Some(miss)
        {
            return Err(CodecError::Invalid("usage alias"));
        }
    }
    if parsed.input_cache_write_tokens.is_none() && adaptation.rules.default_cache_write {
        parsed.input_cache_write_tokens = Some(0);
        fidelity.record_cache_write_default(adaptation.profile_id);
    }
    if parsed
        .input_tokens
        .zip(parsed.output_tokens)
        .and_then(|(input, output)| input.checked_add(output))
        != parsed.total_tokens
    {
        return Err(CodecError::Invalid("usage total"));
    }
    Ok(Some(parsed))
}
fn count(o: &Map<String, Value>, key: &'static str) -> Result<u64, CodecError> {
    o.get(key)
        .and_then(Value::as_u64)
        .ok_or(CodecError::Invalid(key))
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
            fields(details, allowed)?;
            match details.get(field) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => Ok(Some(
                    value.as_u64().ok_or(CodecError::Invalid("usage detail"))?,
                )),
            }
        }
    }
}
pub(super) fn encode_usage(
    usage: Usage,
    profile: Profile,
    rules: &crate::protocol::adaptation::WireRules,
) -> Value {
    let mut value = match profile {
        Profile::Chat => json!({
            "prompt_tokens": usage.input_tokens,
            "completion_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }),
        Profile::Responses => json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "total_tokens": usage.total_tokens
        }),
    };
    let object = value.as_object_mut().expect("usage object");
    if let Some(cached) = usage.cached_input_tokens {
        let key = if profile == Profile::Chat {
            "prompt_tokens_details"
        } else {
            "input_tokens_details"
        };
        object.insert(key.into(), json!({"cached_tokens": cached}));
    }
    if let Some(written) = usage.input_cache_write_tokens {
        let details = object
            .entry(if profile == Profile::Chat {
                "prompt_tokens_details"
            } else {
                "input_tokens_details"
            })
            .or_insert_with(|| json!({}));
        details[if profile == Profile::Chat && rules.chat_inference_response_shape {
            "created_cache_tokens"
        } else {
            "cache_write_tokens"
        }] = json!(written);
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        if profile == Profile::Chat && rules.chat_reasoning_usage_alias {
            object.insert("reasoning_tokens".into(), json!(reasoning));
        } else {
            let key = if profile == Profile::Chat {
                "completion_tokens_details"
            } else {
                "output_tokens_details"
            };
            object.insert(key.into(), json!({"reasoning_tokens": reasoning}));
        }
    }
    if let Some(images) = usage.input_image_tokens {
        // Lowering has already checked the explicitly selected extension slot.
        let key = if profile == Profile::Chat {
            "prompt_tokens_details"
        } else {
            "input_tokens_details"
        };
        let details = object.entry(key).or_insert_with(|| json!({}));
        if profile == Profile::Chat && rules.chat_inference_response_shape {
            details["multimodal_tokens"] = json!({"image":images});
        } else {
            details["image_tokens"] = json!(images);
        }
    }
    for (details, count) in [
        (
            if profile == Profile::Chat {
                "prompt_tokens_details"
            } else {
                "input_tokens_details"
            },
            usage.input_text_tokens,
        ),
        (
            if profile == Profile::Chat {
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
    if profile == Profile::Chat {
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
    if profile == Profile::Responses
        && rules.responses_billing_view
        && (usage.input_image_tokens.is_some()
            || usage.input_text_tokens.is_some()
            || usage.output_text_tokens.is_some())
    {
        super::adapter_shapes::encode_billing_usage(&mut value);
    }
    value
}
pub fn decode_chat(v: &Value) -> Result<DecodedResponse, CodecError> {
    decode_chat_with(v, &Default::default())
}
pub(crate) fn decode_chat_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
) -> Result<DecodedResponse, CodecError> {
    let (v, extras) = super::adapter_shapes::decode(v, Profile::Chat, adaptation)?;
    let o = object(&v)?;
    adaptation.validate_response(Profile::Chat, o)?;
    fields(
        o,
        &[
            "id",
            "object",
            "created",
            "model",
            "choices",
            "usage",
            "system_fingerprint",
            "service_tier",
            "metadata",
        ],
    )?;
    if string(o, "object")? != "chat.completion" {
        return Err(CodecError::Invalid("response object"));
    }
    let choices = o
        .get("choices")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        .ok_or(CodecError::Unsupported("candidate count".into()))?;
    let c = object(&choices[0])?;
    fields(c, &["index", "message", "finish_reason", "logprobs"])?;
    if c.get("index").and_then(Value::as_u64) != Some(0) {
        return Err(CodecError::Unsupported("candidate index".into()));
    }
    let outcome = match string(c, "finish_reason")? {
        "stop" | "tool_calls" => Outcome::Completed,
        "length" | "content_filter" => Outcome::Incomplete,
        _ => {
            return Err(CodecError::Unsupported("finish reason".into()));
        }
    };
    let message = object(c.get("message").ok_or(CodecError::Invalid("message"))?)?;
    let mut b = Items::default();
    let message = super::chat_reasoning::decode_static(&mut b, message, adaptation)?;
    chat::decode_message(&mut b, &message, false)?;
    super::chat_logprobs::attach(&mut b.items, c.get("logprobs"))?;
    if matches!(outcome, Outcome::Incomplete) {
        for (_, item) in &mut b.items {
            match item {
                Item::ToolCall(call) => call.status = ItemLifecycle::Incomplete,
                Item::Message(m) => m.status = ItemLifecycle::Incomplete,
                // A complete encrypted detail already closed its owner. The
                // carrier can truncate later without making that token partial.
                Item::Reasoning(r)
                    if r.replay
                        .as_ref()
                        .and_then(ReplayValue::replay_token)
                        .is_none() =>
                {
                    r.status = ItemLifecycle::Incomplete;
                }
                _ => {}
            }
        }
    }
    let usage = usage(o.get("usage"), Profile::Chat, adaptation, &mut b.fidelity)?;
    let semantic = response_with_usage(b.items, outcome, usage)?.with_details(
        if outcome == Outcome::Incomplete {
            TerminalDetails {
                error: None,
                incomplete: Some(if c["finish_reason"] == "content_filter" {
                    IncompleteReason::ContentFilter
                } else {
                    IncompleteReason::MaxOutputTokens
                }),
            }
        } else {
            TerminalDetails::default()
        },
    )?;
    // Chat's finish label must agree with content even though response outcome
    // no longer encodes a call/no-call distinction.
    if chat_finish(&semantic)? != string(c, "finish_reason")? {
        return Err(CodecError::Invalid("finish reason conflicts with output"));
    }
    let metadata = metadata(o, Profile::Chat)?;
    b.fidelity.capture_routing_extras(
        Profile::Chat,
        adaptation,
        extras,
        &semantic,
        &metadata.id,
    )?;
    Ok(DecodedResponse {
        semantic,
        fidelity: b.fidelity,
        metadata,
    })
}
pub fn decode_responses(v: &Value) -> Result<DecodedResponse, CodecError> {
    decode_responses_with(v, &Default::default())
}
pub(crate) fn decode_responses_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
) -> Result<DecodedResponse, CodecError> {
    let (v, extras) = super::adapter_shapes::decode(v, Profile::Responses, adaptation)?;
    let o = object(&v)?;
    adaptation.validate_response(Profile::Responses, o)?;
    super::envelope::response_fields(o)?;
    if string(o, "object")? != "response" {
        return Err(CodecError::Invalid("response object"));
    }
    let outcome = match string(o, "status")? {
        "completed" => Outcome::Completed,
        "incomplete" => Outcome::Incomplete,
        "failed" => Outcome::Failed,
        "cancelled" => Outcome::Cancelled,
        _ => return Err(CodecError::Unsupported("response status".into())),
    };
    let output = o
        .get("output")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("output"))?;
    let mut b = Items::default();
    let item_status = if outcome == Outcome::Completed {
        "completed"
    } else {
        "incomplete"
    };
    responses::decode_items(&mut b, output, true, item_status)?;
    let usage = usage(
        o.get("usage"),
        Profile::Responses,
        adaptation,
        &mut b.fidelity,
    )?;
    let semantic =
        response_with_usage(b.items, outcome, usage)?.with_details(decode_details(o)?)?;
    super::envelope::record_vendor_shapes(
        Profile::Responses,
        adaptation,
        o,
        &mut b.fidelity,
        Some(&semantic),
    )?;
    let metadata = metadata(o, Profile::Responses)?;
    b.fidelity.capture_routing_extras(
        Profile::Responses,
        adaptation,
        extras,
        &semantic,
        &metadata.id,
    )?;
    Ok(DecodedResponse {
        semantic,
        fidelity: b.fidelity,
        metadata,
    })
}
fn response_with_usage(
    items: Vec<(ItemId, Item)>,
    outcome: Outcome,
    usage: Option<Usage>,
) -> Result<GenerationResponse, CodecError> {
    let response = GenerationResponse::new(items, outcome)?;
    Ok(match usage {
        Some(usage) => response.with_usage(usage)?,
        None => response,
    })
}
pub fn encode_chat(target: &ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Chat {
        return Err(CodecError::ProfileMismatch);
    }
    check_response_carriers(target.semantic)?;
    check_item_carriers(target.semantic.items())?;
    let finish = chat_finish(target.semantic)?;
    let mut messages = chat::encode_items_with(
        target.semantic.items(),
        target.fidelity,
        target.adaptation.rules.structured_chat_reasoning,
        true,
    );
    if target.adaptation.rules.reasoning_alias {
        for message in &mut messages {
            super::adapter_shapes::encode_message(message);
        }
    }
    let message = messages
        .first()
        .ok_or(CodecError::Invalid("empty Chat candidate"))?;
    let m = target.metadata;
    let mut value = json!({"id":m.id,"object":"chat.completion","created":m.created,"model":m.model,
        "choices":[{"index":0,"message":message,"finish_reason":finish}],
        "usage":target.semantic.usage().map(|usage| encode_usage(usage, Profile::Chat, &target.adaptation.rules))});
    if let Some(probabilities) = super::chat_logprobs::choice(target.semantic.items())? {
        value["choices"][0]["logprobs"] = probabilities;
    }
    put_presence(
        value.as_object_mut().expect("object"),
        "metadata",
        &m.context.execution.metadata,
        |v| json!(v),
    );
    put_presence(
        value.as_object_mut().expect("object"),
        "system_fingerprint",
        &m.context.system_fingerprint,
        |v| json!(v),
    );
    put_presence(
        value.as_object_mut().expect("object"),
        "service_tier",
        &m.context.execution.service_tier,
        |v| json!(v),
    );
    super::envelope::write_response_extras(
        target.fidelity,
        target.profile,
        &target.adaptation,
        target.semantic,
        &target.metadata.id,
        value.as_object_mut().expect("object"),
    );
    bounded(&value)?;
    Ok(value)
}
pub fn encode_responses(target: &ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    check_response_carriers(target.semantic)?;
    check_item_carriers(target.semantic.items())?;
    let status = match target.semantic.outcome() {
        Outcome::Completed => "completed",
        Outcome::Incomplete => "incomplete",
        Outcome::Failed => "failed",
        Outcome::Cancelled => "cancelled",
    };
    let output = responses::encode_items(target.semantic.items(), target.fidelity, true);
    let m = target.metadata;
    let mut value = json!({"id":m.id,"object":"response","created_at":m.created,"model":m.model,"status":status,
        "output":output,"usage":target.semantic.usage().map(|usage| encode_usage(usage, Profile::Responses, &target.adaptation.rules))});
    super::envelope::write_metadata(
        m,
        value.as_object_mut().expect("object"),
        status == "completed",
    )?;
    super::envelope::write_response_extras(
        target.fidelity,
        target.profile,
        &target.adaptation,
        target.semantic,
        &target.metadata.id,
        value.as_object_mut().expect("object"),
    );
    if target.semantic.details().error.is_some() {
        value["error"] = encode_error(target.semantic.details().error.as_ref());
    }
    if target.semantic.details().incomplete.is_some() {
        value["incomplete_details"] =
            encode_incomplete(target.semantic.details().incomplete.as_ref());
    }
    bounded(&value)?;
    Ok(value)
}
