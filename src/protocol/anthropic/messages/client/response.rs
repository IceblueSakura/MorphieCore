//! Messages client response rendering over the shared delivery projection.
use super::projection::{redacted, tool_input};
use crate::protocol::{CodecError, DecodedResponse, anthropic::messages as wire};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};

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
    response.metadata.validate()?;
    crate::protocol::reports::validate_single(&response.metadata.context.reports)?;
    let semantic = &response.semantic;
    if !semantic.safety_reports().is_empty()
        || response
            .metadata
            .context
            .reports
            .iter()
            .any(|r| r.settings.as_ref().is_some_and(|s| !s.safety.is_empty()))
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
        response.metadata.context.service_class(),
    )?;
    v.insert("usage".into(), usage);
    let v = Value::Object(v);
    if attachment.is_some() {
        crate::semantic::value::json_size(&v, crate::semantic::task::generation::MAX_TOTAL_BYTES)
            .map_err(|_| CodecError::Limit)?;
    }
    Ok(v)
}
