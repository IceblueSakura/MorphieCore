//! Responses reasoning object and readable reasoning items.
use crate::protocol::openai::{
    CodecError, Profile,
    common::{DecodeDirection, object, put_presence, read_presence, string, text},
};
use crate::semantic::task::generation::{
    ConfigurationUpdate, EffortControl, EffortLevel, EffortScope, GenerationSettings,
    ItemLifecycle, MAX_TEXT_BYTES, PartId, ReasoningActivation, ReasoningContent, ReasoningContext,
    ReasoningItem, ReasoningMode, ReasoningPresence, ReasoningRequest, ReasoningSummary,
    ReplayFormat, ReplayValue,
};
use serde_json::{Map, Value, json};

/// Responses and Chat have no carrier for these independent thinking controls.
pub(super) fn requires_extended_controls(settings: &GenerationSettings) -> bool {
    let value = &settings.reasoning;
    value.budget_tokens.is_some()
        || !value.display.is_absent()
        || settings
            .effort
            .value()
            .is_some_and(|e| e.scope != EffortScope::Reasoning)
        || settings.effort == crate::semantic::value::Presence::Null
            && (value.presence == ReasoningPresence::Null
                || value.activation.value() == Some(&ReasoningActivation::Disabled))
        || value.activation == crate::semantic::value::Presence::Null
        || value.activation.value() == Some(&ReasoningActivation::Enabled)
            && !settings
                .effort
                .value()
                .is_some_and(|e| e.scope == EffortScope::Reasoning)
        || matches!(
            value.mode.value(),
            Some(ReasoningMode::Adaptive | ReasoningMode::Budgeted)
        )
}

pub(super) fn admit_configuration_mapping(
    update: &ConfigurationUpdate,
    mapping: Option<&crate::protocol::reasoning::ReasoningEffortMapping>,
    decode: bool,
) -> Result<(), CodecError> {
    if let (Some(mapping), Some(control)) = (mapping, update.effort) {
        if control.scope != mapping.scope {
            return Err(CodecError::Unsupported("reasoning mapping scope".into()));
        }
        let level = if decode {
            mapping.decode(control.level)?
        } else {
            mapping.encode(control.level)?
        };
        // Snapshot equality currently owns native configuration values. Do not
        // conceal conflicts by normalizing their many-to-one aliases first.
        if level != control.level {
            return Err(CodecError::Unsupported(
                "calibrated configuration history".into(),
            ));
        }
    }
    Ok(())
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
        _ => return Err(CodecError::Unsupported("include".into())),
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
        return Err(CodecError::Invalid("mode"));
    }
    let mut r = ReasoningRequest::present(None).with_encrypted_output(encrypted);
    read_effort(reasoning, &mut r, &mut settings.effort, "effort")?;
    // The standard nullable enum has no boolean disabled carrier.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/shared_params/reasoning.py
    r.summary = optional_label(reasoning, "summary", summary)?;
    if reasoning.contains_key("generate_summary") {
        let old = optional_label(reasoning, "generate_summary", summary)?;
        if !r.summary.is_absent() && r.summary != old {
            return Err(CodecError::Invalid("conflicting summary controls"));
        }
        r.summary = old;
    }
    r.context = optional_label(reasoning, "context", |s| {
        s.parse::<ReasoningContext>()
            .map_err(|_| CodecError::Unsupported("reasoning context".into()))
    })?;
    r.mode = optional_label(reasoning, "mode", |s| match s {
        "standard" => Ok(ReasoningMode::Standard),
        "pro" => Ok(ReasoningMode::Pro),
        _ => Err(CodecError::Unsupported("reasoning mode".into())),
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
    control: &mut crate::semantic::value::Presence<EffortControl>,
    key: &'static str,
) -> Result<(), CodecError> {
    use crate::semantic::value::Presence;
    match o.get(key) {
        None => {}
        Some(Value::Null) => *control = Presence::Null,
        Some(value) => {
            let update = effort(value.as_str().ok_or(CodecError::Invalid(key))?)?;
            reasoning.activation = update.activation.map_or(Presence::Absent, Presence::Value);
            *control = update.effort.map_or(Presence::Absent, Presence::Value);
        }
    }
    Ok(())
}
pub(super) fn write_request(
    settings: &GenerationSettings,
    o: &mut Map<String, Value>,
    profile: Profile,
) -> Result<(), CodecError> {
    settings.validate()?;
    let value = &settings.reasoning;
    if requires_extended_controls(settings) {
        return Err(CodecError::Unsupported("reasoning controls".into()));
    }
    if profile == Profile::Responses && value.summary() == Some(ReasoningSummary::Disabled) {
        // Typed control intent cannot be replaced by absence or null. This also
        // protects reported settings on events, before a final response exists.
        return Err(CodecError::Unsupported("reasoning summary".into()));
    }
    if value.encrypted_output() {
        o.insert("include".into(), json!(["reasoning.encrypted_content"]));
    }
    let level = effort_label(&ConfigurationUpdate {
        activation: value.activation.value().copied(),
        effort: settings.effort.value().copied(),
    })?;
    let put_effort = |o: &mut Map<String, Value>, key: &str| {
        if let Some(level) = level {
            o.insert(key.into(), json!(level));
        } else if settings.effort == crate::semantic::value::Presence::Null {
            o.insert(key.into(), Value::Null);
        }
    };
    if value.presence() == ReasoningPresence::Absent && settings.effort.is_absent() {
        return Ok(());
    }
    if profile == Profile::Chat {
        put_effort(o, "reasoning_effort");
        return Ok(());
    }
    if value.presence() == ReasoningPresence::Null {
        o.insert("reasoning".into(), Value::Null);
        return Ok(());
    }
    let mut reasoning = Map::new();
    put_effort(&mut reasoning, "effort");
    put_presence(&mut reasoning, "summary", &value.summary, |v| {
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
        _ => return Err(CodecError::Unsupported("reasoning status".into())),
    };
    let mut parts = Vec::new();
    for part in o
        .get("summary")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("reasoning summary"))?
    {
        let part = object(part)?;
        direction.fields(part, &["type", "text"])?;
        if string(part, "type")? != "summary_text" {
            return Err(CodecError::Unsupported("summary part".into()));
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
        for part in content
            .as_array()
            .ok_or(CodecError::Invalid("reasoning content"))?
        {
            let part = object(part)?;
            direction.fields(part, &["type", "text"])?;
            if string(part, "type")? != "reasoning_text" {
                return Err(CodecError::Unsupported("reasoning text".into()));
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
        _ => return Err(CodecError::Invalid("encrypted reasoning")),
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
    read_presence(o, key, |v| {
        parse(v.as_str().ok_or(CodecError::Invalid(key))?)
    })
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
                    .map_err(|_| CodecError::Unsupported("reasoning effort".into()))?,
            }),
        }
    })
}
fn summary(value: &str) -> Result<ReasoningSummary, CodecError> {
    Ok(match value {
        "auto" => ReasoningSummary::Auto,
        "concise" => ReasoningSummary::Concise,
        "detailed" => ReasoningSummary::Detailed,
        _ => return Err(CodecError::Unsupported("reasoning summary".into())),
    })
}
pub(super) fn effort_label(
    value: &ConfigurationUpdate,
) -> Result<Option<&'static str>, CodecError> {
    value.validate()?;
    if let Some(effort) = value.effort {
        if effort.scope != EffortScope::Reasoning {
            return Err(CodecError::Unsupported("response effort".into()));
        }
        return Ok(Some(effort.level.into()));
    }
    match value.activation {
        Some(ReasoningActivation::Disabled) => Ok(Some("none")),
        Some(ReasoningActivation::Enabled) => {
            Err(CodecError::Unsupported("reasoning activation".into()))
        }
        None => Ok(None),
    }
}
fn summary_label(value: ReasoningSummary) -> &'static str {
    match value {
        ReasoningSummary::Disabled => unreachable!("disabled has no Responses carrier"),
        ReasoningSummary::Auto => "auto",
        ReasoningSummary::Concise => "concise",
        ReasoningSummary::Detailed => "detailed",
    }
}
