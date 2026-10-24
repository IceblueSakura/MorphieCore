//! Request-history content projection for Interactions.
use super::request::{RequestRule, unsupported};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{
        task::generation::*,
        value::{JsonLimits, parse_json},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn steps(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let mut steps = vec![];
    for group in request.message_envelopes() {
        if group.members().is_empty() {
            return Err(unsupported());
        }
        for id in group.members() {
            let item = &request
                .items()
                .iter()
                .find(|(owner, _)| owner == id)
                .ok_or_else(unsupported)?
                .1;
            match item {
                Item::Message(m) => {
                    if m.status != ItemLifecycle::Completed || m.phase.is_some() {
                        return Err(unsupported());
                    }
                    let content = m
                        .parts
                        .iter()
                        .map(|p| match &p.content {
                            ContentPart::Text(t) if t.is_plain() => {
                                Ok(json!({"type":"text","text":t.as_str()}))
                            }
                            ContentPart::Resource(r) if m.role == MessageRole::User => {
                                media(r.media(request.resources())?, false, rules)
                            }
                            _ => Err(unsupported()),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let kind = match m.role {
                        MessageRole::User => "user_input",
                        MessageRole::Assistant => "model_output",
                    };
                    steps.push(json!({"type":kind,"content":content}));
                }
                Item::Reasoning(r) => {
                    if r.status != ItemLifecycle::Completed {
                        return Err(unsupported());
                    }
                    let content = r
                        .parts
                        .iter()
                        .map(|(_, p)| {
                            let (ReasoningContent::Summary(t) | ReasoningContent::Text(t)) = p;
                            json!({"type":"text","text":t.as_str()})
                        })
                        .collect::<Vec<_>>();
                    let own = r
                        .replay
                        .as_ref()
                        .filter(|v| v.format() == ReplayFormat::GoogleInteractionsV1Thought);
                    let Some(value) = own else {
                        if r.replay.is_some() {
                            return Err(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ));
                        }
                        if !content.is_empty() {
                            rules.insert(RequestRule::ReasoningAsText);
                            steps.push(json!({"type":"model_output","content":content}));
                        }
                        continue;
                    };
                    if !fidelity.replay_matches_request(*id, r, Some(&target.origin()), request) {
                        return Err(CodecError::Invalid(
                            crate::protocol::failure::ShapeReason::InvalidValue,
                        ));
                    }
                    let value = super::replay::Signed::decode(
                        value.replay_token().ok_or_else(unsupported)?,
                        ReplayFormat::GoogleInteractionsV1Thought,
                    )?;
                    let mut step = json!({"type":"thought"});
                    if !content.is_empty() {
                        step["summary"] = content.into();
                    }
                    if let Some(signature) = value.signature() {
                        step["signature"] = signature.into();
                    }
                    steps.push(step);
                }
                Item::ToolCall(c) => {
                    if c.status != ItemLifecycle::Completed
                        || !c.context.is_direct()
                        || c.context.alias_domain.is_some()
                    {
                        return Err(unsupported());
                    }
                    let args = match &c.arguments {
                        ToolArguments::Structured(v) => v.value().clone(),
                        ToolArguments::Raw(s) => parse_json(s.as_bytes(), JsonLimits::STRUCTURED)
                            .map_err(|_| {
                            CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
                        })?,
                        _ => return Err(unsupported()),
                    };
                    if !args.is_object() {
                        return Err(unsupported());
                    }
                    let mut step = json!({"type":"function_call","id":c.call_id.as_str(),
                        "name":c.name.as_str(),"arguments":args});
                    if let Some(value) = &c.replay {
                        if value.format() != ReplayFormat::GoogleInteractionsV1FunctionCall
                            || !fidelity.call_replay_matches(*id, c, Some(&target.origin()))
                        {
                            return Err(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ));
                        }
                        let signed = super::replay::Signed::decode(
                            value.replay_token().ok_or_else(unsupported)?,
                            ReplayFormat::GoogleInteractionsV1FunctionCall,
                        )?;
                        step["signature"] = signed.signature().ok_or_else(unsupported)?.into();
                    }
                    steps.push(step);
                }
                Item::ToolResult(r) => {
                    if !r.context.is_direct()
                        || r.context.alias_domain.is_some()
                        || r.status.is_some()
                        || r.execution.is_some()
                    {
                        return Err(unsupported());
                    }
                    let result = match &r.output {
                        ToolOutput::Text(t) => json!(t),
                        ToolOutput::Parts(parts) => Value::Array(
                            parts
                                .iter()
                                .map(|p| match &p.content {
                                    InputPart::Text(t) => {
                                        Ok(json!({"type":"text","text":t.as_str()}))
                                    }
                                    InputPart::Resource(r) => {
                                        media(r.media(request.resources())?, true, rules)
                                    }
                                })
                                .collect::<Result<_, CodecError>>()?,
                        ),
                        ToolOutput::Structured(value) if value.value().is_object() => {
                            value.value().clone()
                        }
                        ToolOutput::Structured(_) => return Err(unsupported()),
                    };
                    let name = request
                        .items()
                        .iter()
                        .find_map(|(_, i)| match i {
                            Item::ToolCall(c) if r.call_id.as_ref() == Some(&c.call_id) => {
                                Some(c.name.as_str())
                            }
                            _ => None,
                        })
                        .ok_or(CodecError::Invalid(
                            crate::protocol::failure::ShapeReason::InvalidValue,
                        ))?;
                    let mut step = json!({"type":"function_result","call_id":r.required_call_id()?,
                        "name":name,"result":result});
                    if let Some(error) = r.is_error {
                        step["is_error"] = error.into();
                    }
                    steps.push(step);
                }
                _ => return Err(unsupported()),
            }
        }
    }
    Ok(steps.into())
}

/// One inert resource as v1 content. v1 has no video content, and `function_result`
/// carries only text and images. Source: docs/refs/multimodal.md.
fn media(
    resource: ResourceView<'_>,
    tool: bool,
    _rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let kind = match resource.kind() {
        ResourceKind::Image => "image",
        ResourceKind::Audio if !tool => "audio",
        ResourceKind::File if !tool => "document",
        _ => return Err(unsupported()),
    };
    if match resource.description {
        ResourceDescription::Image {
            detail,
            size_policy,
        } => detail.is_some() || *size_policy == ImageSizePolicy::RejectOversized,
        ResourceDescription::File(file) => {
            file.filename.is_some() || file.detail.is_some() || !file.context.is_absent()
        }
        _ => false,
    } {
        return Err(unsupported());
    }
    match resource.location {
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => {
            if kind == "document" && !matches!(media_type.as_str(), "application/pdf" | "text/csv")
            {
                return Err(unsupported());
            }
            Ok(json!({"type":kind,"data":data_base64.as_str(),"mime_type":media_type.as_str()}))
        }
        ResourceLocation::Url(url) => {
            crate::protocol::google::check_public_media_url(url.as_str())?;
            Ok(json!({"type":kind,"uri":url.as_str()}))
        }
        _ => Err(unsupported()),
    }
}
