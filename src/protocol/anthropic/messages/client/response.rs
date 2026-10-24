//! Messages client response rendering over the shared delivery projection.
use super::projection::{redacted, tool_input};
use crate::protocol::{CodecError, DecodedResponse, anthropic::messages as wire};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};

pub fn encode_client_response(
    response: &DecodedResponse,
    model: &str,
) -> Result<Value, CodecError> {
    response.metadata.validate()?;
    crate::protocol::reports::validate_single(&response.metadata.context.reports)?;
    let semantic = &response.semantic;
    if semantic
        .items()
        .iter()
        .any(|(_, item)| item.has_replay_attachment())
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Replay,
        ));
    }
    if !semantic.safety_reports().is_empty()
        || response
            .metadata
            .context
            .reports
            .iter()
            .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Safety,
        ));
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
                    return Err(CodecError::Unsupported(
                        crate::protocol::failure::CarrierReason::Unavailable,
                    ));
                }
                for part in &m.parts {
                    match &part.content {
                        ContentPart::Text(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    crate::protocol::failure::CarrierReason::Unavailable,
                                ));
                            }
                            if !t.annotations().is_empty() {
                                return Err(CodecError::Unsupported(
                                    crate::protocol::failure::CarrierReason::Unavailable,
                                ));
                            }
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        ContentPart::Refusal(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    crate::protocol::failure::CarrierReason::Unavailable,
                                ));
                            }
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        _ => {
                            return Err(CodecError::Unsupported(
                                crate::protocol::failure::CarrierReason::Unavailable,
                            ));
                        }
                    }
                }
            }
            Item::ToolCall(call) => {
                calls = true;
                let input = tool_input(call)?;
                content.push(json!({"type":"tool_use","id":call.call_id.as_str(),
                    "name":call.name.as_str(),"input":input}));
            }
            _ => {
                return Err(CodecError::Unsupported(
                    crate::protocol::failure::CarrierReason::Unavailable,
                ));
            }
        }
    }
    let (stop_reason, stop_sequence) = wire::terminal::encode(
        semantic.outcome(),
        semantic.progress(),
        calls,
        semantic.details(),
    )?;
    let mut v = Map::new();
    v.insert("id".into(), json!(response.metadata.required_id()?));
    v.insert("type".into(), json!("message"));
    v.insert("role".into(), json!("assistant"));
    v.insert("model".into(), json!(model));
    v.insert("content".into(), json!(content));
    v.insert("stop_reason".into(), json!(stop_reason.wire()));
    v.insert("stop_sequence".into(), json!(stop_sequence));
    wire::wire::envelope::write_refusal(&mut v, &semantic.details().refusal);
    let fallback = response.metadata.delivery.as_ref().and_then(|d| d.usage);
    let selected = match wire::accounting::project_reports(semantic.usage_reports()) {
        Ok(selected) => selected,
        Err(CodecError::Unsupported(_)) if fallback.is_some() => UsageReports::default(),
        Err(error) => return Err(error),
    };
    let mut usage = wire::accounting::json_with_fallback(&selected, fallback)?;
    wire::service::write_class(
        usage.as_object_mut().expect("usage object"),
        response.metadata.context.service_class(),
    )?;
    v.insert("usage".into(), usage);
    Ok(Value::Object(v))
}
