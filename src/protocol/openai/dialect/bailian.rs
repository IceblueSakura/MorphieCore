//! Documented Bailian request carriers and fail-closed model constraints.
//! OpenAI-compatible Chat/Responses, not the native DashScope RPC protocol.
use crate::{
    protocol::{
        GenerationRequestEnvelope as Request,
        failure::CarrierReason,
        openai::{CodecError, GenerationApi, function_tools, reasoning, shape::object},
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Map, Value, json};

pub(in crate::protocol::openai) const CONTROL_FIELDS: &[&str] =
    &["enable_thinking", "thinking_budget", "top_k"];
const UNOWNED: &[&str] = &[
    "extra_body",
    "parameters",
    "repetition_penalty",
    "tool_stream",
    "thinking",
    "chat_template_kwargs",
    "enable_search",
    "search_options",
    "enable_search_extension",
    "enable_code_interpreter",
    "use_multichannel",
    "vl_high_resolution_images",
    "ocr_options",
];

/// Registered controls cannot disappear under the unknown-field policy.
pub(in crate::protocol::openai) fn source(
    value: &Value,
    profile: GenerationApi,
    native: bool,
) -> Result<(), CodecError> {
    let o = object(value)?;
    if native {
        let model = o.get("model").and_then(Value::as_str).unwrap_or("");
        let effort = if profile == GenerationApi::Chat {
            o.get("reasoning_effort")
        } else {
            o.get("reasoning")
                .and_then(Value::as_object)
                .and_then(|r| r.get("effort"))
        };
        if matches!(model, "kimi-k3" | "glm-5.3" | "glm-5.3-prime")
            && effort.is_some_and(|v| v.as_str() == Some("none"))
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if profile == GenerationApi::Responses && o.contains_key("stream_options")
            || profile == GenerationApi::Chat
                && o.get("stream_options")
                    .and_then(Value::as_object)
                    .is_some_and(|o| o.contains_key("include_obfuscation"))
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    }
    for key in UNOWNED {
        if o.contains_key(*key) {
            return Err(CodecError::Unsupported(CarrierReason::UnknownField));
        }
    }
    for message in o
        .get(if profile == GenerationApi::Chat {
            "messages"
        } else {
            "input"
        })
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(message) = message.as_object() else {
            continue;
        };
        if message.contains_key("partial") || message.contains_key("prefix") {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        for part in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(part) = part.as_object() else {
                continue;
            };
            if [
                "cache_control",
                "min_pixels",
                "max_pixels",
                "total_pixels",
                "fps",
            ]
            .iter()
            .any(|k| part.contains_key(*k))
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            if native
                && part
                    .get("image_url")
                    .and_then(Value::as_object)
                    .is_some_and(|image| image.contains_key("detail"))
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        }
    }
    if native && profile == GenerationApi::Chat && o.get("max_tokens").is_some_and(|v| !v.is_null())
    {
        let model = o.get("model").and_then(Value::as_str).unwrap_or("");
        let disabled = o.get("enable_thinking") == Some(&json!(false))
            || o.get("reasoning_effort") == Some(&json!("none"));
        if !matches!(model, "glm-5.3" | "glm-5.3-prime") && !disabled {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    }
    Ok(())
}

pub(in crate::protocol::openai) fn read_controls(
    o: &Map<String, Value>,
    settings: &mut GenerationSettings,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    if let Some(value) = o.get("enable_thinking") {
        let enabled = value.as_bool().ok_or(CodecError::invalid())?;
        let activation = if enabled {
            ReasoningActivation::Enabled
        } else {
            ReasoningActivation::Disabled
        };
        if !settings.reasoning.activation.is_absent()
            && settings.reasoning.activation != Presence::Value(activation)
        {
            return Err(CodecError::invalid());
        }
        settings.reasoning.presence = ReasoningPresence::Present;
        settings.reasoning.activation = Presence::Value(activation);
    }
    if let Some(value) = o.get("thinking_budget") {
        if profile != GenerationApi::Chat {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let budget = value
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or(CodecError::invalid())?;
        if !settings.effort.is_absent() {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if settings
            .controls
            .max_output_tokens
            .is_some_and(|cap| budget > cap)
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        settings.reasoning.presence = ReasoningPresence::Present;
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.reasoning.budget_tokens = Some(budget);
    }
    if let Some(value) = o.get("top_k") {
        if profile != GenerationApi::Chat {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        settings.controls.top_k = Some(
            value
                .as_u64()
                .filter(|v| (1..=100).contains(v))
                .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))? as u32,
        );
    }
    settings.validate()?;
    Ok(())
}

/// Remove only fields with their own exact native carrier before the standard writer.
pub(in crate::protocol::openai) fn standard_reasoning(
    settings: &GenerationSettings,
    profile: GenerationApi,
) -> GenerationSettings {
    let mut standard = settings.clone();
    standard.reasoning.activation = Presence::Absent;
    if profile == GenerationApi::Chat {
        standard.reasoning.budget_tokens = None;
        if standard.reasoning.mode == Presence::Value(ReasoningMode::Budgeted) {
            standard.reasoning.mode = Presence::Absent;
        }
    }
    standard
}
pub(in crate::protocol::openai) fn extended(
    settings: &GenerationSettings,
    profile: GenerationApi,
    native: bool,
) -> bool {
    if native {
        reasoning::requires_extended_controls(&standard_reasoning(settings, profile))
    } else {
        reasoning::requires_extended_controls(settings)
    }
}
pub(in crate::protocol::openai) fn write_reasoning(
    settings: &GenerationSettings,
    o: &mut Map<String, Value>,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    if settings.reasoning.activation == Presence::Null || settings.effort == Presence::Null {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if let Some(activation) = settings.reasoning.activation.value() {
        o.insert(
            "enable_thinking".into(),
            json!(*activation == ReasoningActivation::Enabled),
        );
    }
    if profile == GenerationApi::Chat
        && let Some(budget) = settings.reasoning.budget_tokens
    {
        o.insert("thinking_budget".into(), json!(budget));
    }
    let standard = standard_reasoning(settings, profile);
    reasoning::write_request(&standard, o, profile)?;
    if profile == GenerationApi::Responses && o.get("reasoning") == Some(&json!({})) {
        o.shift_remove("reasoning");
    }
    Ok(())
}

use crate::protocol::reasoning::ReasoningEffortMapping;
const QWEN_FLASH: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "qwen3.8-flash",
    rule: "bailian.qwen3.8-flash.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::XHigh],
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::High, EffortLevel::XHigh),
        (EffortLevel::XHigh, EffortLevel::XHigh),
        (EffortLevel::Max, EffortLevel::XHigh),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::Medium),
        (EffortLevel::XHigh, EffortLevel::XHigh),
    ],
};
const QWEN_MAX: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "qwen3.8-max",
    rule: "bailian.qwen3.8-max.effort.v1",
    ..QWEN_FLASH
};
const GLM_CHAT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "glm-5.3",
    rule: "bailian.glm-5.3.chat.effort.v1",
    scope: EffortScope::Reasoning,
    native_levels: &[EffortLevel::Low, EffortLevel::High, EffortLevel::Max],
    encode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::Max, EffortLevel::Max),
    ],
    decode: &[
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::Max, EffortLevel::Max),
    ],
};
const KIMI_CHAT: ReasoningEffortMapping = ReasoningEffortMapping {
    canonical_model: "kimi-k3",
    rule: "bailian.kimi-k3.chat.effort.v1",
    ..GLM_CHAT
};
const GLM_RESPONSES: ReasoningEffortMapping = ReasoningEffortMapping {
    rule: "bailian.glm-5.3.responses.effort.v1",
    encode: &[
        (EffortLevel::Minimal, EffortLevel::Low),
        (EffortLevel::Low, EffortLevel::Low),
        (EffortLevel::Medium, EffortLevel::High),
        (EffortLevel::High, EffortLevel::High),
        (EffortLevel::XHigh, EffortLevel::Max),
        (EffortLevel::Max, EffortLevel::Max),
    ],
    ..GLM_CHAT
};
pub(crate) fn effort_mapping(
    model: &str,
    profile: GenerationApi,
) -> Option<&'static ReasoningEffortMapping> {
    match (model, profile) {
        ("qwen3.8-flash", _) => Some(&QWEN_FLASH),
        ("qwen3.8-max", _) => Some(&QWEN_MAX),
        ("glm-5.3" | "glm-5.3-prime", GenerationApi::Chat) => Some(&GLM_CHAT),
        ("glm-5.3" | "glm-5.3-prime", GenerationApi::Responses) => Some(&GLM_RESPONSES),
        ("kimi-k3", GenerationApi::Chat) => Some(&KIMI_CHAT),
        _ => None,
    }
}

pub(in crate::protocol::openai) fn check_native_effort(
    settings: &GenerationSettings,
    model: &str,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    if let (Some(mapping), Some(control)) =
        (effort_mapping(model, profile), settings.effort.value())
        && (!mapping.native_levels.contains(&control.level) || control.scope != mapping.scope)
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn responses_reasoning_carrier(reasoning: &ReasoningItem) -> bool {
    reasoning.replay.is_none()
        && reasoning
            .parts
            .iter()
            .all(|(_, part)| matches!(part, ReasoningContent::Summary(_)))
}

pub(in crate::protocol::openai) fn check(
    request: &Request,
    profile: GenerationApi,
    model: &str,
) -> Result<(), CodecError> {
    check_semantic(&request.task.semantic, profile, model)?;
    check_context(&request.context, &request.service_policy, profile)
}
pub(in crate::protocol::openai) fn check_context(
    ctx: &crate::semantic::context::ExecutionHints,
    service: &Presence<crate::semantic::context::ServicePolicy>,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    if !ctx.metadata.is_absent()
        || !ctx.max_tool_calls.is_absent()
        || !service.is_absent()
        || profile == GenerationApi::Responses
            && (!ctx.background.is_absent()
                || !ctx.previous_response_id.is_absent()
                || !ctx.conversation.is_absent()
                || !ctx.prompt.is_absent()
                || !ctx.moderation.is_absent()
                || !ctx.context_management.is_absent())
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn check_semantic(
    semantic: &GenerationRequest,
    profile: GenerationApi,
    model: &str,
) -> Result<(), CodecError> {
    let s = semantic.settings();
    let r = &s.reasoning;
    let c = &s.controls;
    if c.temperature().is_some_and(|v| !(0.0..2.0).contains(&v))
        || c.top_p().is_some_and(|v| v <= 0.0)
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let qwen = matches!(model, "qwen3.8-flash" | "qwen3.8-max");
    let glm = matches!(model, "glm-5.3" | "glm-5.3-prime");
    let disabled = r.activation.value() == Some(&ReasoningActivation::Disabled);
    if qwen && !disabled && c.temperature().is_some_and(|v| v < 0.6) {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if glm && disabled {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if r.budget_tokens.is_some() && (glm || model == "kimi-k3" || !s.effort.is_absent()) {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if c.top_k.is_some_and(|n| n > 100) || c.top_k.is_some() && model == "kimi-k3" {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if c.seed.is_some_and(|n| !(0..=2147483647).contains(&n))
        || c.top_logprobs.is_some_and(|n| n > 5)
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if (qwen || glm || model == "kimi-k3") && c.logprobs == Presence::Value(true) {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if s.effort == Presence::Null
        || r.activation == Presence::Null
        || r.presence == ReasoningPresence::Null
        || c.logprobs == Presence::Null
    {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if r.replay_mismatch.is_some()
        || !r.presentation.is_absent()
        || !r.context.is_absent()
        || !r.mode.is_absent() && r.mode != Presence::Value(ReasoningMode::Budgeted)
    {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if profile == GenerationApi::Chat {
        if (glm && !matches!(s.output(), OutputConstraint::Text))
            || model == "kimi-k3" && matches!(s.output(), OutputConstraint::JsonSchema { .. })
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        for tool in semantic.tools() {
            let ToolDefinition::Function(tool) = tool else {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            };
            if tool.description.is_none()
                || tool.output_schema.is_some()
                || !tool.dispatch.is_inactive()
                || !matches!(
                    tool.strict,
                    FunctionStrictness::Explicit(false)
                        | FunctionStrictness::Omitted(StrictDefault::NonStrict)
                )
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        }
        if matches!(
            s.tool_choice,
            Some(ToolChoice::Allowed { .. } | ToolChoice::Custom(_) | ToolChoice::Qualified(_))
        ) {
            return Err(CodecError::Unsupported(CarrierReason::Tools));
        }
        if c.frequency_penalty().is_some() || !s.text.verbosity.is_absent() {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if qwen && matches!(s.tool_choice, Some(ToolChoice::Required)) {
            return Err(CodecError::Unsupported(CarrierReason::Tools));
        }
        if !disabled && matches!(s.tool_choice, Some(ToolChoice::Specific(_))) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    } else {
        if c.has_extended_controls()
            || c.truncation.is_some()
            || !s.text.presence.is_absent()
            || s.parallel_tool_calls.is_some()
            || r.encrypted_output()
            || r.budget_tokens.is_some()
            || !r.read_history.is_absent()
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if c.max_output_tokens
            .is_some_and(|n| n < 16 || !model.is_empty() && !qwen && !disabled)
        {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        let tools = semantic.tools();
        for tool in tools {
            let ToolDefinition::Function(t) = tool else {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            };
            if t.description.is_none()
                || t.output_schema.is_some()
                || !t.dispatch.is_inactive()
                || !matches!(
                    t.strict,
                    FunctionStrictness::Explicit(false)
                        | FunctionStrictness::Omitted(StrictDefault::NonStrict)
                )
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        }
        let required = matches!(
            s.tool_choice,
            Some(
                ToolChoice::Required
                    | ToolChoice::Specific(_)
                    | ToolChoice::Allowed { required: true, .. }
            )
        );
        if required && tools.len() != 1 {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        for (_, item) in semantic.items() {
            match item {
                Item::ToolCall(c) if !c.context.is_direct() => {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                Item::ToolResult(result) => {
                    if !matches!(result.output, ToolOutput::Text(_)) {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                }
                Item::Message(m) if m.phase.is_some() => {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                Item::Reasoning(r) if !responses_reasoning_carrier(r) => {
                    return Err(CodecError::Unsupported(CarrierReason::UnknownField));
                }
                Item::CustomCall(_)
                | Item::CustomResult(_)
                | Item::ConfigurationUpdate(_)
                | Item::Compaction(_)
                | Item::Program(_)
                | Item::ProgramOutput(_) => {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Responses has no strict-schema guarantee. Never serialize a strict request
/// as a non-strict definition; the checker above requires explicit non-strict intent.
pub(in crate::protocol::openai) fn write_tools(
    s: &GenerationSettings,
    o: &mut Map<String, Value>,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    if let Some(tools) = &s.tools {
        let mut values = vec![];
        for tool in tools {
            let ToolDefinition::Function(t) = tool else {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            };
            if t.output_schema.is_some()
                || !t.dispatch.is_inactive()
                || !matches!(
                    t.strict,
                    FunctionStrictness::Explicit(false)
                        | FunctionStrictness::Omitted(StrictDefault::NonStrict)
                )
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            let mut value = json!({"type":"function","name":t.name.as_str(),"description":t.description.as_ref().ok_or(CodecError::invalid())?});
            if let Some(schema) = &t.parameters {
                value["parameters"] = schema.root().clone();
            }
            values.push(if profile == GenerationApi::Chat {
                value
                    .as_object_mut()
                    .expect("function")
                    .shift_remove("type");
                json!({"type":"function","function":value})
            } else {
                value
            });
        }
        o.insert("tools".into(), json!(values));
    }
    if let Some(choice) = &s.tool_choice {
        let value = match choice {
            ToolChoice::Specific(name) if profile == GenerationApi::Responses => {
                json!({"type":"allowed_tools","mode":"required","tools":[{"type":"function","name":name.as_str()}]})
            }
            _ => function_tools::write_choice(choice, profile)?,
        };
        o.insert("tool_choice".into(), value);
    }
    if profile == GenerationApi::Chat
        && let Some(parallel) = s.parallel_tool_calls
    {
        o.insert("parallel_tool_calls".into(), json!(parallel));
    }
    Ok(())
}
