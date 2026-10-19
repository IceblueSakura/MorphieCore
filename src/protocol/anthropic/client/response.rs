//! Client Messages delivery projection, named losses and JSON rendering.
use crate::protocol::{CodecError, DecodedResponse, anthropic as wire, replay_envelope};
use crate::semantic::{task::generation::*, value::Presence};
use serde_json::{Map, Value, json};

/// Client values are sealed envelopes; the format tag keeps a redacted block's type.
pub(in crate::protocol::anthropic) fn redacted(sealed: &str) -> bool {
    matches!(
        replay_envelope::open(sealed),
        Some((_, ReplayFormat::AnthropicMessagesRedactedThinking, _))
    )
}

/// Retained metadata and accounting reflect the actual Messages carrier.
pub(crate) fn project_snapshot(response: &mut DecodedResponse) -> Result<(), CodecError> {
    response.metadata.context.validate()?;
    if !response.semantic.safety_reports().is_empty()
        || response
            .metadata
            .context
            .settings
            .as_ref()
            .is_some_and(|settings| !settings.safety.is_empty())
    {
        return Err(CodecError::Unsupported("Messages safety report".into()));
    }
    if let Some(class) = response.metadata.context.service_class.value() {
        wire::service::class_wire(*class)?;
    }
    if response.metadata.context.execution != crate::semantic::context::ExecutionHints::default() {
        return Err(CodecError::Unsupported(
            "Messages reported execution context".into(),
        ));
    }
    response.metadata.context.settings = None;
    response.metadata.context.instruction_messages = None;
    response.metadata.instruction_fidelity = Default::default();
    response.metadata.context.system_fingerprint = Presence::Absent;
    let selected = wire::accounting::project_reports(response.semantic.usage_reports())?;
    if selected.operation().is_none() {
        return Err(CodecError::Invalid("Messages usage"));
    }
    response.semantic = response.semantic.clone().with_usage_reports(selected)?;
    Ok(())
}
pub(crate) fn metadata_losses(
    metadata: &crate::protocol::ResponseMetadata,
) -> Vec<crate::lowering::generation::LossEvidence> {
    let mut losses = vec![];
    if metadata.context.settings.is_some() || metadata.context.instruction_messages.is_some() {
        losses.push(crate::lowering::generation::LossEvidence {
            rule: "messages.omit-incidental-settings.v1",
            field: "response.reported.settings",
            direction: "response.encode",
            action: "omit",
        });
    }
    if !metadata.context.system_fingerprint.is_absent() {
        losses.push(crate::lowering::generation::LossEvidence {
            rule: "messages.omit-system-fingerprint.v1",
            field: "response.context.system_fingerprint",
            direction: "response.encode",
            action: "omit",
        });
    }
    losses
}
/// Messages has no refusal content block; `stop_reason: "refusal"` reports classifier
/// intervention, not a model-authored refusal message, so the text is kept as text.
pub(crate) fn refusal_loss(direction: &'static str) -> crate::lowering::generation::LossEvidence {
    crate::lowering::generation::LossEvidence {
        rule: "messages.refusal-as-text.v1",
        field: "generation.content.refusal",
        direction,
        action: "approximate",
    }
}
pub(crate) fn content_losses(
    response: &GenerationResponse,
) -> Vec<crate::lowering::generation::LossEvidence> {
    let mut losses = Vec::new();
    if wire::accounting::project_reports(response.usage_reports())
        .is_ok_and(|selected| selected != *response.usage_reports())
        && (response.usage_reports().has_details()
            || response
                .usage_reports()
                .operation()
                .is_some_and(|r| r.stage != UsageStage::Final))
    {
        losses.push(crate::protocol::usage::report_loss("response.encode"));
    }
    let refusal = response.items().iter().any(|(_, item)| {
        matches!(item, Item::Message(m)
            if m.parts.iter().any(|p| matches!(p.content, ContentPart::Refusal(_))))
    });
    losses.extend(refusal.then(|| refusal_loss("response.encode")));
    losses
}
/// Shared by JSON and SSE delivery. Lost report detail/scope and a changed scalar usage
/// view are independent receipts; a report without a selected view records only the former.
pub(crate) fn report_losses(
    source: &UsageReport,
    selected: Option<&UsageReport>,
    direction: &'static str,
) -> Vec<crate::lowering::generation::LossEvidence> {
    if selected == Some(source) {
        return vec![];
    }
    let detailed = source.details != Default::default()
        || source.scope != UsageScope::Operation
        || source.accounting != crate::semantic::task::generation::UsageAccounting::Direct;
    let mut losses = Vec::new();
    if detailed {
        losses.push(crate::protocol::usage::report_loss(direction));
    }
    if !detailed || selected.is_some_and(|s| s.scalar_summary() != source.scalar_summary()) {
        losses.push(crate::lowering::generation::LossEvidence {
            direction,
            ..usage_loss()
        });
    }
    losses
}
pub(crate) fn usage_loss() -> crate::lowering::generation::LossEvidence {
    crate::lowering::generation::LossEvidence {
        rule: "messages.usage-view.v1",
        field: "generation.usage",
        direction: "response.encode",
        action: "project",
    }
}
fn tool_input(call: &ToolCall) -> Result<Value, CodecError> {
    if !call.context.is_direct() || call.context.alias_domain.is_some() {
        return Err(CodecError::Unsupported("Messages call context".into()));
    }
    let input = match &call.arguments {
        ToolArguments::Structured(value) => value.clone(),
        ToolArguments::Raw(raw) => StructuredValue::from_bytes(raw.as_bytes())?,
        ToolArguments::StructuredPartial(_) => {
            return Err(CodecError::Unsupported("Messages tool input".into()));
        }
    };
    if !input.value().is_object() {
        return Err(CodecError::Unsupported("Messages object input".into()));
    }
    Ok(input.value().clone())
}
/// Reuse the existing EOF-validated materialization; never buffer streamed arguments again.
pub(crate) fn validate_tool_calls(response: &DecodedResponse) -> Result<(), CodecError> {
    for (_, item) in response.semantic.items() {
        if let Item::ToolCall(call) = item {
            if matches!(call.arguments, ToolArguments::StructuredPartial(_))
                && call.status == ItemLifecycle::Incomplete
                && response.semantic.outcome() == Outcome::Incomplete
                && response.semantic.details().incomplete == Some(IncompleteReason::MaxOutputTokens)
                && call.context.is_direct()
                && call.context.alias_domain.is_none()
            {
                continue;
            }
            tool_input(call)?;
        }
    }
    Ok(())
}
/// Encode a sealed response for a Messages client under the public label.
pub fn encode_client_response(
    response: &DecodedResponse,
    model: &str,
) -> Result<Value, CodecError> {
    encode_client_response_with(response, model, None)
}
/// The owned replay manifest trails the content it describes as an empty signed thinking block.
pub(crate) fn encode_client_response_with(
    response: &DecodedResponse,
    model: &str,
    attachment: Option<&crate::protocol::attachments::Carrier>,
) -> Result<Value, CodecError> {
    let semantic = &response.semantic;
    if !semantic.safety_reports().is_empty()
        || response
            .metadata
            .context
            .settings
            .as_ref()
            .is_some_and(|settings| !settings.safety.is_empty())
    {
        return Err(CodecError::Unsupported("Messages safety report".into()));
    }
    let mut content = vec![];
    let mut calls = false;
    for (_, item) in semantic.items() {
        match item {
            Item::Reasoning(r) => {
                let signature = r.replay.as_ref().map_or("", ReplayValue::as_str);
                if redacted(signature) {
                    content.push(json!({"type":"redacted_thinking","data":signature}));
                    continue;
                }
                let thinking: String = r
                    .parts
                    .iter()
                    .map(|(_, p)| match p {
                        ReasoningContent::Summary(t) | ReasoningContent::Text(t) => t.as_str(),
                    })
                    .collect();
                content.push(json!({"type":"thinking","thinking":thinking,"signature":signature}));
            }
            Item::Message(m) if m.role == MessageRole::Assistant => {
                if m.phase.is_some() {
                    return Err(CodecError::Unsupported("Messages phase".into()));
                }
                for part in &m.parts {
                    match &part.content {
                        ContentPart::Text(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    "Messages probabilities".into(),
                                ));
                            }
                            if !t.annotations().is_empty() {
                                return Err(CodecError::Unsupported("Messages citations".into()));
                            }
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        ContentPart::Refusal(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    "Messages probabilities".into(),
                                ));
                            }
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        _ => return Err(CodecError::Unsupported("Messages content".into())),
                    }
                }
            }
            Item::ToolCall(call) => {
                calls = true;
                let input = tool_input(call)?;
                content.push(json!({"type":"tool_use","id":call.call_id.as_str(),
                    "name":call.name.as_str(),"input":input}));
            }
            _ => return Err(CodecError::Unsupported("Messages item".into())),
        }
    }
    let (stop_reason, stop_sequence) = wire::terminal::encode(
        semantic.outcome(),
        semantic.progress(),
        calls,
        semantic.details(),
    )?;
    if let Some(carrier) = attachment {
        content.push(crate::protocol::attachments::messages_block(
            carrier.as_str(),
        ));
    }
    let mut v = Map::new();
    v.insert("id".into(), json!(response.metadata.required_id()?));
    v.insert("type".into(), json!("message"));
    v.insert("role".into(), json!("assistant"));
    v.insert("model".into(), json!(model));
    v.insert("content".into(), json!(content));
    v.insert("stop_reason".into(), json!(stop_reason));
    v.insert("stop_sequence".into(), json!(stop_sequence));
    let selected = wire::accounting::project_reports(semantic.usage_reports())?;
    let mut usage = wire::accounting::json(&selected)?;
    wire::service::write_class(
        usage.as_object_mut().expect("usage object"),
        &response.metadata.context.service_class,
    )?;
    v.insert("usage".into(), usage);
    let v = Value::Object(v);
    if attachment.is_some() {
        crate::semantic::value::json_size(&v, crate::semantic::task::generation::MAX_TOTAL_BYTES)
            .map_err(|_| CodecError::Limit)?;
    }
    Ok(v)
}
