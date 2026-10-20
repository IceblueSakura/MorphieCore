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
            .skip(1)
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
                        media(r.media(request.resources())?, false, rules)?
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
                    return Err(CodecError::Invalid("Gemini replay format"));
                }
                let text = readable();
                if !text.trim().is_empty() {
                    rules.insert(RequestRule::ReasoningAsText);
                    parts.push(json!({"text": text}));
                }
                return Ok(());
            };
            if !fidelity.replay_matches_request(id, r, Some(&target.origin()), request) {
                return Err(CodecError::Invalid("Gemini replay dependency"));
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
                .map_err(|_| CodecError::Invalid("Gemini object arguments"))?,
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
            let mut media_parts = vec![];
            let output = match &r.output {
                ToolOutput::Text(s) => json!(s),
                ToolOutput::Parts(list) => {
                    let mut output = Vec::with_capacity(list.len());
                    for (index, part) in list.iter().enumerate() {
                        output.push(match &part.content {
                            InputPart::Text(t) => json!(t.as_str()),
                            InputPart::Resource(r) => {
                                // Reference labels belong to occurrences, not resources or
                                // filenames. Result-local positions stay stable as history grows.
                                let name = format!("morphie_tool_media_{index}");
                                let mut value = media(r.media(request.resources())?, true, rules)?;
                                value["inlineData"]["displayName"] = json!(name);
                                media_parts.push(value);
                                json!({"$ref": name})
                            }
                        });
                    }
                    Value::Array(output)
                }
                ToolOutput::Structured(_) => return Err(unsupported()),
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
                .ok_or(CodecError::Invalid("Gemini result without call"))?;
            let key = if r.is_error == Some(true) {
                "error"
            } else {
                "output"
            };
            let mut response = json!({
                "id": r.required_call_id()?, "name": name, "response": {key: output}});
            if !media_parts.is_empty() {
                response["parts"] = media_parts.into();
            }
            parts.push(json!({"functionResponse": response}));
        }
        _ => return Err(unsupported()),
    }
    Ok(())
}

/// One inert resource as `inlineData` or, outside tool results, `fileData`. Tool results
/// carry only `inlineData` (`FunctionResponsePart`) of the documented types, so no audio
/// or video. `mimeType` is optional for `fileData`.
/// Source: docs/refs/multimodal.md.
fn media(
    resource: ResourceView<'_>,
    tool: bool,
    _rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    if match resource.description {
        ResourceDescription::Image { detail } => detail.is_some(),
        ResourceDescription::File(file) => file.filename.is_some() || file.detail.is_some(),
        _ => false,
    } {
        return Err(unsupported());
    }
    match resource.location {
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => {
            // Function responses admit a closed type list; user images have their own.
            // https://ai.google.dev/gemini-api/docs/generate-content/function-calling
            // https://ai.google.dev/gemini-api/docs/image-understanding
            let allowed = if tool {
                matches!(
                    media_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp" | "application/pdf" | "text/plain"
                )
            } else {
                !media_type
                    .as_str()
                    .to_ascii_lowercase()
                    .starts_with("image/")
                    || matches!(
                        media_type.as_str(),
                        "image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif"
                    )
            };
            if !allowed {
                return Err(unsupported());
            }
            Ok(json!({"inlineData": {
                "mimeType": media_type.as_str(), "data": data_base64.as_str()}}))
        }
        ResourceLocation::Url(url) if !tool => {
            crate::protocol::google::check_public_media_url(url.as_str())?;
            Ok(json!({"fileData": {"fileUri": url.as_str()}}))
        }
        _ => Err(unsupported()),
    }
}
