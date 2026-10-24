//! Request-history content projection for generateContent.
use super::{
    replay::Signed,
    request::{RequestRule, unsupported},
};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::task::generation::*,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub(super) fn contents(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let mut contents = vec![];
    let mut included = BTreeSet::new();
    for group in request.message_envelopes() {
        let role = match group.role() {
            MessageEnvelopeRole::User => "user",
            MessageEnvelopeRole::Assistant => "model",
            _ => return Err(unsupported()),
        };
        let mut parts = vec![];
        for owner in group.members() {
            if !included.insert(*owner) {
                return Err(unsupported());
            }
            let item = &request
                .items()
                .iter()
                .find(|(id, _)| id == owner)
                .ok_or_else(unsupported)?
                .1;
            item_parts(*owner, item, request, fidelity, target, rules, &mut parts)?;
        }
        if parts.is_empty() {
            return Err(unsupported());
        }
        contents.push(json!({"role": role, "parts": parts}));
    }
    let expected: Vec<_> = request
        .items()
        .iter()
        .filter(|(_, item)| !matches!(item, Item::Instruction(_)))
        .map(|(id, _)| *id)
        .collect();
    let actual: Vec<_> = request
        .message_envelopes()
        .iter()
        .flat_map(|g| g.members())
        .copied()
        .collect();
    if actual != expected
        || request
            .items()
            .iter()
            .skip_while(|(_, item)| matches!(item, Item::Instruction(_)))
            .any(|(_, i)| matches!(i, Item::Instruction(_)))
    {
        return Err(unsupported());
    }
    Ok(Value::Array(contents))
}

fn item_parts(
    id: ItemId,
    item: &Item,
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
    parts: &mut Vec<Value>,
) -> Result<(), CodecError> {
    match item {
        Item::Message(m) => {
            if m.status != ItemLifecycle::Completed || m.phase.is_some() {
                return Err(unsupported());
            }
            for p in &m.parts {
                let mut part = match &p.content {
                    ContentPart::Text(t) if t.is_plain() => json!({"text": t.as_str()}),
                    ContentPart::Resource(r) if m.role == MessageRole::User => {
                        media(r.media(request.resources())?)?
                    }
                    _ => return Err(unsupported()),
                };
                if let Some(value) = &p.replay {
                    part["thoughtSignature"] = value.replay_token().ok_or_else(unsupported)?.into();
                }
                parts.push(part);
            }
        }
        Item::Reasoning(r) => {
            if r.status != ItemLifecycle::Completed {
                return Err(unsupported());
            }
            let readable = || {
                r.parts
                    .iter()
                    .map(|(_, part)| {
                        let (ReasoningContent::Summary(t) | ReasoningContent::Text(t)) = part;
                        t.as_str()
                    })
                    .collect::<String>()
            };
            let own = r
                .replay
                .as_ref()
                .filter(|v| v.format() == ReplayFormat::GoogleGenerateContentPart);
            let Some(value) = own else {
                // Every turn converts alike, keeping the projection append-only.
                if r.replay.is_some() {
                    return Err(CodecError::Invalid(
                        crate::protocol::failure::ShapeReason::InvalidValue,
                    ));
                }
                let text = readable();
                if !text.trim().is_empty() {
                    rules.insert(RequestRule::ReasoningAsText);
                    parts.push(json!({"text": text}));
                }
                return Ok(());
            };
            if !fidelity.replay_matches_request(id, r, Some(&target.origin()), request) {
                return Err(CodecError::Invalid(
                    crate::protocol::failure::ShapeReason::InvalidValue,
                ));
            }
            let token = value.replay_token().ok_or_else(unsupported)?;
            if r.parts.is_empty() && super::replay::is_legacy(token) {
                rules.insert(RequestRule::OmitLegacySignature);
                return Ok(());
            }
            let value = Signed::decode(token)?;
            let mut part = Map::new();
            part.insert("thought".into(), true.into());
            part.insert("text".into(), readable().into());
            if let Some(signature) = &value.signature {
                part.insert("thoughtSignature".into(), signature.as_str().into());
            }
            parts.push(Value::Object(part));
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
                ToolArguments::Raw(s) => crate::semantic::value::parse_json(
                    s.as_bytes(),
                    crate::semantic::value::JsonLimits::STRUCTURED,
                )
                .map_err(|_| {
                    CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue)
                })?,
                ToolArguments::StructuredPartial(_) => return Err(unsupported()),
            };
            if !args.is_object() {
                return Err(unsupported());
            }
            let mut part = json!({"functionCall": {
                "id": c.call_id.as_str(), "name": c.name.as_str(), "args": args}});
            if let Some(value) = &c.replay {
                part["thoughtSignature"] = value.replay_token().ok_or_else(unsupported)?.into();
            }
            parts.push(part);
        }
        Item::ToolResult(r) => {
            if !r.context.is_direct()
                || r.context.alias_domain.is_some()
                || r.status.is_some()
                || r.execution.is_some()
            {
                return Err(unsupported());
            }
            let output = match &r.output {
                ToolOutput::Text(s) => json!(s),
                ToolOutput::Parts(list) => {
                    let mut output = Vec::with_capacity(list.len());
                    for part in list {
                        output.push(match &part.content {
                            InputPart::Text(t) => json!(t.as_str()),
                            // Gemini API has no established occurrence-association
                            // carrier; never fabricate displayName/$ref linkage.
                            InputPart::Resource(_) => return Err(unsupported()),
                        });
                    }
                    Value::Array(output)
                }
                ToolOutput::Structured(value) => value.value().clone(),
            };
            // `functionResponse.name` repeats the call's name; results link by call ID.
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
            let key = if r.is_error == Some(true) {
                "error"
            } else {
                "output"
            };
            let response = json!({
                "id": r.required_call_id()?, "name": name, "response": {key: output}});
            parts.push(json!({"functionResponse": response}));
        }
        _ => return Err(unsupported()),
    }
    Ok(())
}

/// One inert user resource as `inlineData` or `fileData`.
/// Tool-result media has no selected occurrence-association carrier.
/// Source: docs/refs/multimodal.md.
fn media(resource: ResourceView<'_>) -> Result<Value, CodecError> {
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
            // https://ai.google.dev/gemini-api/docs/image-understanding
            let allowed = !media_type
                .as_str()
                .to_ascii_lowercase()
                .starts_with("image/")
                || matches!(
                    media_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif"
                );
            if !allowed {
                return Err(unsupported());
            }
            Ok(json!({"inlineData": {
                "mimeType": media_type.as_str(), "data": data_base64.as_str()}}))
        }
        ResourceLocation::Url(url) => {
            crate::protocol::google::check_public_media_url(url.as_str())?;
            Ok(json!({"fileData": {"fileUri": url.as_str()}}))
        }
        _ => Err(unsupported()),
    }
}
