//! Responses reasoning object and readable reasoning items.
use crate::{
    protocol::{
        failure::CarrierReason,
        openai::{
            CodecError, GenerationApi,
            shape::{DecodeDirection, object, put_presence, read_presence, string, text},
        },
    },
    semantic::task::generation::{
        ConfigurationUpdate, EffortControl, EffortLevel, EffortScope, GenerationSettings,
        ItemLifecycle, MAX_TEXT_BYTES, PartId, ReasoningActivation, ReasoningContent,
        ReasoningContext, ReasoningDisplay, ReasoningItem, ReasoningMode, ReasoningPresence,
        ReasoningRequest, ReasoningSummary, ReplayFormat, ReplayValue,
    },
};
use serde_json::{Map, Value, json};

/// Responses and Chat have no carrier for these independent thinking controls.
pub(super) fn requires_extended_controls(settings: &GenerationSettings) -> bool {
    let value = &settings.reasoning;
    value.budget_tokens.is_some()
        || value.replay_mismatch == Some(crate::semantic::task::generation::ReplayMismatch::Reject)
        || value.presentation.visibility == crate::semantic::value::Presence::Null
        || settings
            .effort
            .as_ref()
            .is_some_and(|e| e.scope != EffortScope::Reasoning)
            && (value.presence == ReasoningPresence::Null
                || value.activation.value() == Some(&ReasoningActivation::Disabled))
        || value.activation == crate::semantic::value::Presence::Null
        || value.activation.value() == Some(&ReasoningActivation::Enabled)
            && !settings
                .effort
                .as_ref()
                .is_some_and(|e| e.scope == EffortScope::Reasoning)
        || matches!(
            value.mode.value(),
            Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
        )
}

/// `reported` distinguishes response-echo admission from request admission: the
/// pinned create type marks `mode` non-nullable, the response model does not.
pub(super) fn request(
    o: &Map<String, Value>,
    reported: bool,
    direction: DecodeDirection,
    settings: &mut GenerationSettings,
) -> Result<(), CodecError> {
    let encrypted = match o.get("include") {
        None | Some(Value::Null) => false,
        Some(Value::Array(values))
            if values.iter().all(|v| {
                matches!(
                    v.as_str(),
                    Some("reasoning.encrypted_content" | "message.output_text.logprobs")
                )
            }) =>
        {
            values.iter().any(|v| v == "reasoning.encrypted_content")
        }
        _ => {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    };
    let Some(value) = o.get("reasoning") else {
        settings.reasoning = ReasoningRequest::absent().with_encrypted_output(encrypted);
        return Ok(());
    };
    if value.is_null() {
        let mut r = ReasoningRequest::absent().with_encrypted_output(encrypted);
        r.presence = ReasoningPresence::Null;
        settings.reasoning = r;
        return Ok(());
    }
    let reasoning = object(value)?;
    direction.fields(
        reasoning,
        &["effort", "summary", "generate_summary", "context", "mode"],
    )?;
    if !reported && reasoning.get("mode").is_some_and(Value::is_null) {
        return Err(CodecError::invalid());
    }
    let mut r = ReasoningRequest::present(None).with_encrypted_output(encrypted);
    read_effort(reasoning, &mut r, &mut settings.effort, "effort")?;
    // The standard nullable enum has no boolean disabled carrier.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared_params/reasoning.py
    r.presentation.summary = optional_label(reasoning, "summary", summary)?;
    if reasoning.contains_key("generate_summary") {
        let old = optional_label(reasoning, "generate_summary", summary)?;
        if !r.presentation.summary.is_absent() && r.presentation.summary != old {
            return Err(CodecError::invalid());
        }
        r.presentation.summary = old;
    }
    if r.presentation.summary.value().is_some() {
        r.presentation.visibility =
            crate::semantic::value::Presence::Value(ReasoningDisplay::Summary);
    }
    r.context = optional_label(reasoning, "context", |s| {
        s.parse::<ReasoningContext>()
            .map_err(|_| CodecError::Unsupported(CarrierReason::Reasoning))
    })?;
    r.mode = optional_label(reasoning, "mode", |s| match s {
        "standard" => Ok(ReasoningMode::Standard),
        "pro" => Ok(ReasoningMode::Pro),
        _ => Err(CodecError::Unsupported(CarrierReason::Reasoning)),
    })?;
    r.validate()?;
    settings.reasoning = r;
    Ok(())
}
pub(super) fn chat_request(
    o: &Map<String, Value>,
    settings: &mut GenerationSettings,
) -> Result<(), CodecError> {
    if !o.contains_key("reasoning_effort") {
        return Ok(());
    };
    let mut reasoning = ReasoningRequest::present(None);
    read_effort(o, &mut reasoning, &mut settings.effort, "reasoning_effort")?;
    settings.reasoning = reasoning;
    Ok(())
}
fn read_effort(
    o: &Map<String, Value>,
    reasoning: &mut ReasoningRequest,
    control: &mut Option<EffortControl>,
    key: &'static str,
) -> Result<(), CodecError> {
    use crate::semantic::value::Presence;
    match o.get(key) {
        None => {}
        Some(Value::Null) => *control = None,
        Some(value) => {
            let update = effort(value.as_str().ok_or(CodecError::invalid())?)?;
            reasoning.activation = update.activation.map_or(Presence::Absent, Presence::Value);
            *control = update.effort;
        }
    }
    Ok(())
}
pub(super) fn write_request(
    settings: &GenerationSettings,
    o: &mut Map<String, Value>,
    profile: GenerationApi,
) -> Result<(), CodecError> {
    settings.validate()?;
    let value = &settings.reasoning;
    if requires_extended_controls(settings) || !value.read_history.is_absent() {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if profile == GenerationApi::Responses
        && value.presentation.visibility.value() == Some(&ReasoningDisplay::Omitted)
    {
        // Typed control intent cannot be replaced by absence or null. This also
        // protects reported settings on events, before a final response exists.
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    if value.encrypted_output() {
        o.insert("include".into(), json!(["reasoning.encrypted_content"]));
    }
    let level = effort_label(&ConfigurationUpdate {
        activation: value.activation.value().copied(),
        effort: settings.effort.as_ref().copied(),
    })?;
    let put_effort = |o: &mut Map<String, Value>, key: &str| {
        if let Some(level) = level {
            o.insert(key.into(), json!(level));
        }
    };
    if value.presence() == ReasoningPresence::Absent && settings.effort.is_none() {
        return Ok(());
    }
    if profile == GenerationApi::Chat {
        put_effort(o, "reasoning_effort");
        return Ok(());
    }
    if value.presence() == ReasoningPresence::Null {
        o.insert("reasoning".into(), Value::Null);
        return Ok(());
    }
    let mut reasoning = Map::new();
    put_effort(&mut reasoning, "effort");
    let summary = if value.presentation.summary.is_absent()
        && value.presentation.visibility.value() == Some(&ReasoningDisplay::Summary)
    {
        crate::semantic::value::Presence::Value(ReasoningSummary::Auto)
    } else {
        value.presentation.summary.clone()
    };
    put_presence(&mut reasoning, "summary", &summary, |v| {
        json!(summary_label(*v))
    });
    put_presence(&mut reasoning, "context", &value.context, |v| {
        let label: &'static str = (*v).into();
        json!(label)
    });
    put_presence(&mut reasoning, "mode", &value.mode, |v| {
        let label: &'static str = (*v).into();
        json!(label)
    });
    o.insert("reasoning".into(), Value::Object(reasoning));
    Ok(())
}
pub(super) fn decode_item(
    o: &Map<String, Value>,
    next_part: &mut impl FnMut() -> Result<PartId, CodecError>,
    allow_incomplete: bool,
    direction: DecodeDirection,
) -> Result<ReasoningItem, CodecError> {
    direction.fields(
        o,
        &[
            "id",
            "type",
            "summary",
            "content",
            "encrypted_content",
            "status",
        ],
    )?;
    let status = match o.get("status") {
        None | Some(Value::Null) => ItemLifecycle::Completed,
        Some(Value::String(s)) if s == "completed" => ItemLifecycle::Completed,
        Some(Value::String(s)) if s == "incomplete" && allow_incomplete => {
            ItemLifecycle::Incomplete
        }
        Some(Value::String(s)) if s == "in_progress" && allow_incomplete => {
            ItemLifecycle::InProgress
        }
        _ => {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning));
        }
    };
    let mut parts = Vec::new();
    for part in o
        .get("summary")
        .and_then(Value::as_array)
        .ok_or(CodecError::invalid())?
    {
        let part = object(part)?;
        direction.fields(part, &["type", "text"])?;
        if string(part, "type")? != "summary_text" {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        parts.push((
            next_part()?,
            ReasoningContent::Summary(
                crate::semantic::value::Text::allowing_empty(
                    string(part, "text")?,
                    "summary",
                    MAX_TEXT_BYTES,
                )
                .map_err(|_| CodecError::Limit)?,
            ),
        ));
    }
    if let Some(content) = o.get("content").filter(|value| !value.is_null()) {
        for part in content.as_array().ok_or(CodecError::invalid())? {
            let part = object(part)?;
            direction.fields(part, &["type", "text"])?;
            if string(part, "type")? != "reasoning_text" {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning));
            }
            parts.push((
                next_part()?,
                ReasoningContent::Text(
                    crate::semantic::value::Text::allowing_empty(
                        string(part, "text")?,
                        "reasoning text",
                        MAX_TEXT_BYTES,
                    )
                    .map_err(|_| CodecError::Limit)?,
                ),
            ));
        }
    }
    let encrypted = match o.get("encrypted_content") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if !value.is_empty() => {
            let value = text(value, "encrypted reasoning", MAX_TEXT_BYTES)?;
            Some(if status == ItemLifecycle::InProgress {
                ReplayValue::partial(ReplayFormat::ResponsesEncrypted, value)
            } else {
                ReplayValue::final_value(ReplayFormat::ResponsesEncrypted, value)
            })
        }
        _ => {
            return Err(CodecError::invalid());
        }
    };
    Ok(ReasoningItem {
        parts,
        status,
        replay: encrypted,
    })
}
pub(crate) fn encode_item(item: &ReasoningItem, response: bool) -> Value {
    let summary: Vec<_> = item
        .parts
        .iter()
        .filter_map(|(_, part)| match part {
            ReasoningContent::Summary(text) => {
                Some(json!({"type":"summary_text","text":text.as_str()}))
            }
            ReasoningContent::Text(_) => None,
        })
        .collect();
    let content: Vec<_> = item
        .parts
        .iter()
        .filter_map(|(_, part)| match part {
            ReasoningContent::Text(text) => {
                Some(json!({"type":"reasoning_text","text":text.as_str()}))
            }
            ReasoningContent::Summary(_) => None,
        })
        .collect();
    let mut value = json!({"type":"reasoning","summary":summary});
    if !content.is_empty() {
        value["content"] = json!(content);
    }
    let encrypted = if response {
        item.replay.as_ref().map(ReplayValue::as_str)
    } else {
        item.replay.as_ref().and_then(ReplayValue::replay_token)
    };
    if let Some(encrypted) = encrypted {
        value["encrypted_content"] = json!(encrypted);
    }
    if response {
        value["status"] = json!(match item.status {
            ItemLifecycle::Completed => "completed",
            ItemLifecycle::Incomplete => "incomplete",
            ItemLifecycle::InProgress => "in_progress",
        });
    }
    value
}
fn optional_label<T>(
    o: &Map<String, Value>,
    key: &'static str,
    parse: fn(&str) -> Result<T, CodecError>,
) -> Result<crate::semantic::value::Presence<T>, CodecError> {
    read_presence(o, key, |v| parse(v.as_str().ok_or(CodecError::invalid())?))
}
pub(super) fn effort(value: &str) -> Result<ConfigurationUpdate, CodecError> {
    Ok(if value == "none" {
        ConfigurationUpdate {
            activation: Some(ReasoningActivation::Disabled),
            effort: None,
        }
    } else {
        ConfigurationUpdate {
            activation: None,
            effort: Some(EffortControl {
                scope: EffortScope::Reasoning,
                level: value
                    .parse::<EffortLevel>()
                    .map_err(|_| CodecError::Unsupported(CarrierReason::Reasoning))?,
            }),
        }
    })
}
fn summary(value: &str) -> Result<ReasoningSummary, CodecError> {
    Ok(match value {
        "auto" => ReasoningSummary::Auto,
        "concise" => ReasoningSummary::Concise,
        "detailed" => ReasoningSummary::Detailed,
        _ => {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning));
        }
    })
}
pub(super) fn effort_label(
    value: &ConfigurationUpdate,
) -> Result<Option<&'static str>, CodecError> {
    value.validate()?;
    if let Some(effort) = value.effort {
        if effort.scope != EffortScope::Reasoning {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        return Ok(Some(effort.level.into()));
    }
    match value.activation {
        Some(ReasoningActivation::Disabled) => Ok(Some("none")),
        Some(ReasoningActivation::Enabled) => {
            Err(CodecError::Unsupported(CarrierReason::Reasoning))
        }
        None => Ok(None),
    }
}
fn summary_label(value: ReasoningSummary) -> &'static str {
    match value {
        ReasoningSummary::Auto => "auto",
        ReasoningSummary::Concise => "concise",
        ReasoningSummary::Detailed => "detailed",
    }
}
