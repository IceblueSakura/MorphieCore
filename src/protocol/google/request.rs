//! Candidate-local generateContent request projection.
//! Source: <https://ai.google.dev/api/generate-content>.
use super::signature::{Anchor, Signed};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub(super) fn unsupported() -> CodecError {
    CodecError::Unsupported("Gemini semantic carrier".into())
}
/// Named degradable rules applied to one candidate's request copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    /// Gemini 3.8 Flash thinks at `low` at least; `none` and `minimal` become `low`.
    LowerEffort,
    /// `xhigh` and `max` become `high`, the highest Gemini level.
    CapEffort,
    /// A token budget becomes the nearest level.
    BudgetAsEffort,
    /// Gemini has one summary form, without concise or detailed variants.
    SummarizeReasoning,
    OmitReasoningContext,
    /// Opaque reasoning issued for another target or Google format is dropped.
    OmitForeignReplay,
    /// Reasoning without this target's value is sent as model text, as pi does.
    ReasoningAsText,
    /// A leading developer instruction (pi, Codex) is the system instruction.
    DeveloperAsSystem,
    /// An omitted `strict` defaulting to schema normalization (Responses) is sent
    /// as a plain declaration.
    DefaultStrictAsPlain,
    /// Gemini caches implicitly and has no cache-key carrier.
    OmitCacheKey,
    /// `functionResponse.response` holds one string; text parts are concatenated.
    JoinResultParts,
    /// Image detail, file detail and filename have no Gemini slot.
    OmitMediaOptions,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LowerEffort => "gemini.lower-effort.v1",
            Self::CapEffort => "gemini.cap-effort.v1",
            Self::BudgetAsEffort => "gemini.budget-as-effort.v1",
            Self::SummarizeReasoning => "gemini.summarize-reasoning.v1",
            Self::OmitReasoningContext => "gemini.omit-reasoning-context.v1",
            Self::OmitForeignReplay => "gemini.omit-foreign-replay.v1",
            Self::ReasoningAsText => "gemini.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "gemini.developer-as-system.v1",
            Self::DefaultStrictAsPlain => "gemini.default-strict-as-plain.v1",
            Self::OmitCacheKey => "gemini.omit-cache-key.v1",
            Self::JoinResultParts => "gemini.join-result-parts.v1",
            Self::OmitMediaOptions => "gemini.omit-media-options.v1",
        }
    }
}

pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let s = request.settings();
    if s.controls.top_logprobs.is_some()
        || !s.controls.logprobs.is_absent()
        || s.controls.truncation.is_some()
        || s.text != TextOptions::default()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
        || s.parallel_tool_calls == Some(false)
    {
        return Err(unsupported());
    }
    let mut body = Map::new();
    let mut system = match &s.instructions {
        Presence::Absent => None,
        Presence::Value(t) => Some(vec![t.as_str().to_owned()]),
        Presence::Null => return Err(unsupported()),
    };
    if let Some((_, Item::Instruction(i))) = request.items().first() {
        if i.status.is_some() || system.is_some() {
            return Err(unsupported());
        }
        if i.authority == InstructionAuthority::Developer {
            rules.insert(RequestRule::DeveloperAsSystem);
        }
        system = Some(i.parts.iter().map(|(_, t)| t.as_str().to_owned()).collect());
    }
    if let Some(parts) = system {
        body.insert(
            "systemInstruction".into(),
            json!({"parts": parts.iter().map(|t| json!({"text": t})).collect::<Vec<_>>()}),
        );
    }
    body.insert(
        "contents".into(),
        contents(request, fidelity, target, rules)?,
    );
    if s.tools.is_some() {
        let declarations = request
            .tools()
            .iter()
            .map(|tool| {
                let ToolDefinition::Function(t) = tool else {
                    return Err(unsupported());
                };
                if !t.dispatch.is_inactive() || t.output_schema.is_some() {
                    return Err(unsupported());
                }
                match t.strict {
                    FunctionStrictness::Omitted(StrictDefault::NonStrict)
                    | FunctionStrictness::Explicit(false) => {}
                    FunctionStrictness::Omitted(StrictDefault::NormalizeSchema) => {
                        rules.insert(RequestRule::DefaultStrictAsPlain);
                    }
                    FunctionStrictness::Explicit(true) => return Err(unsupported()),
                }
                let mut d = Map::new();
                d.insert("name".into(), t.name.as_str().into());
                if let Some(description) = &t.description {
                    d.insert("description".into(), description.as_str().into());
                }
                let parameters = t.parameters.as_ref().ok_or_else(unsupported)?;
                if !parameters.is_unversioned_local() {
                    return Err(unsupported());
                }
                d.insert("parametersJsonSchema".into(), parameters.root().clone());
                Ok(Value::Object(d))
            })
            .collect::<Result<Vec<_>, _>>()?;
        body.insert(
            "tools".into(),
            json!([{"functionDeclarations": declarations}]),
        );
    }
    if let Some(choice) = &s.tool_choice {
        let config = match choice {
            ToolChoice::Auto => json!({"mode": "AUTO"}),
            ToolChoice::None => json!({"mode": "NONE"}),
            ToolChoice::Required => json!({"mode": "ANY"}),
            ToolChoice::Specific(name) => {
                json!({"mode": "ANY", "allowedFunctionNames": [name.as_str()]})
            }
            _ => return Err(unsupported()),
        };
        body.insert(
            "toolConfig".into(),
            json!({"functionCallingConfig": config}),
        );
    }
    let mut generation = Map::new();
    if let Some(n) = s.controls.max_output_tokens {
        generation.insert("maxOutputTokens".into(), n.into());
    }
    if let Some(t) = s.controls.temperature() {
        generation.insert("temperature".into(), t.into());
    }
    if let Some(p) = s.controls.top_p() {
        generation.insert("topP".into(), p.into());
    }
    if let Some(thinking) = thinking(&s.reasoning, rules)? {
        generation.insert("thinkingConfig".into(), thinking);
    }
    if !generation.is_empty() {
        body.insert("generationConfig".into(), Value::Object(generation));
    }
    Ok(Value::Object(body))
}

fn thinking(
    reasoning: &ReasoningRequest,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Option<Value>, CodecError> {
    if reasoning.presence == ReasoningPresence::Null
        || !reasoning.display.is_absent() && !reasoning.summary.is_absent()
    {
        return Err(unsupported());
    }
    if !reasoning.context.is_absent() {
        rules.insert(RequestRule::OmitReasoningContext);
    }
    let include = match (&reasoning.display, &reasoning.summary) {
        (Presence::Value(ReasoningDisplay::Summary), _)
        | (Presence::Absent, Presence::Value(ReasoningSummary::Auto)) => true,
        (Presence::Absent, Presence::Value(ReasoningSummary::Concise))
        | (Presence::Absent, Presence::Value(ReasoningSummary::Detailed)) => {
            rules.insert(RequestRule::SummarizeReasoning);
            true
        }
        (Presence::Null, _) => return Err(unsupported()),
        _ => false,
    };
    match reasoning.mode {
        Presence::Absent | Presence::Value(ReasoningMode::Adaptive) => {}
        Presence::Value(ReasoningMode::Budgeted) => {
            rules.insert(RequestRule::BudgetAsEffort);
        }
        _ => return Err(unsupported()),
    }
    let effort = match (reasoning.effort.clone(), reasoning.budget_as_effort()) {
        (Presence::Absent, Some(effort)) => Presence::Value(effort),
        (effort, _) => effort,
    };
    let level = match effort {
        Presence::Absent => None,
        Presence::Value(ReasoningEffort::None | ReasoningEffort::Minimal) => {
            rules.insert(RequestRule::LowerEffort);
            Some("LOW")
        }
        Presence::Value(ReasoningEffort::Low) => Some("LOW"),
        Presence::Value(ReasoningEffort::Medium) => Some("MEDIUM"),
        Presence::Value(ReasoningEffort::High) => Some("HIGH"),
        Presence::Value(ReasoningEffort::XHigh | ReasoningEffort::Max) => {
            rules.insert(RequestRule::CapEffort);
            Some("HIGH")
        }
        Presence::Null => return Err(unsupported()),
    };
    let mut config = Map::new();
    if let Some(level) = level {
        config.insert("thinkingLevel".into(), level.into());
    }
    if include {
        config.insert("includeThoughts".into(), true.into());
    }
    Ok((!config.is_empty()).then_some(Value::Object(config)))
}

fn contents(
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
        let mut signed = vec![];
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
            item_parts(
                *owner,
                item,
                request,
                fidelity,
                target,
                rules,
                &mut parts,
                &mut signed,
            )?;
        }
        for value in signed {
            attach(&mut parts, value)?;
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

#[allow(clippy::too_many_arguments)]
fn item_parts(
    id: ItemId,
    item: &Item,
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
    parts: &mut Vec<Value>,
    signed: &mut Vec<Signed>,
) -> Result<(), CodecError> {
    match item {
        Item::Message(m) => {
            if m.status != ItemLifecycle::Completed || m.phase.is_some() {
                return Err(unsupported());
            }
            for p in &m.parts {
                match &p.content {
                    ContentPart::Text(t) if t.is_plain() => parts.push(json!({"text": t.as_str()})),
                    ContentPart::Resource(r) if m.role == MessageRole::User => {
                        parts.push(media(r.media(request.resources())?, false, rules)?)
                    }
                    _ => return Err(unsupported()),
                }
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
                    rules.insert(RequestRule::OmitForeignReplay);
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
            let value = Signed::decode(value.replay_token().ok_or_else(unsupported)?)?;
            if value.anchor == Anchor::Thought {
                let mut part = Map::new();
                part.insert("thought".into(), true.into());
                part.insert("text".into(), readable().into());
                if let Some(signature) = &value.signature {
                    part.insert("thoughtSignature".into(), signature.as_str().into());
                }
                parts.push(Value::Object(part));
            } else {
                if !r.parts.is_empty() {
                    return Err(CodecError::Invalid("Gemini signature owner"));
                }
                signed.push(value);
            }
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
            parts.push(json!({"functionCall": {
                "id": c.call_id.as_str(), "name": c.name.as_str(), "args": args}}));
        }
        Item::ToolResult(r) => {
            if !r.context.is_direct() || r.context.alias_domain.is_some() || r.status.is_some() {
                return Err(unsupported());
            }
            if let Some(execution) = &r.execution {
                let allowed = match execution {
                    ToolExecution::Succeeded => r.is_error != Some(true),
                    ToolExecution::NotExecuted | ToolExecution::Failed { code: None } => {
                        r.is_error == Some(true)
                    }
                    _ => false,
                };
                if !allowed {
                    return Err(unsupported());
                }
            }
            let mut media_parts = vec![];
            let output = match &r.output {
                ToolOutput::Text(s) => s.clone(),
                ToolOutput::Parts(list) => {
                    let mut text = vec![];
                    for (_, part) in list {
                        match part {
                            ToolResultPart::Text(t) => text.push(t.as_str()),
                            ToolResultPart::Resource(r) => {
                                media_parts.push(media(r.media(request.resources())?, true, rules)?)
                            }
                        }
                    }
                    if text.len() > 1 {
                        rules.insert(RequestRule::JoinResultParts);
                    }
                    text.concat()
                }
                ToolOutput::Structured(_) => return Err(unsupported()),
            };
            // `functionResponse.name` repeats the call's name; results link by call ID.
            let name = request
                .items()
                .iter()
                .find_map(|(_, i)| match i {
                    Item::ToolCall(c) if c.call_id == r.call_id => Some(c.name.as_str()),
                    _ => None,
                })
                .ok_or(CodecError::Invalid("Gemini result without call"))?;
            let key = if r.is_error == Some(true) {
                "error"
            } else {
                "output"
            };
            let mut response = json!({
                "id": r.call_id.as_str(), "name": name, "response": {key: output}});
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
/// carry only `inlineData` (`FunctionResponsePart`). `mimeType` is optional for `fileData`.
/// Source: docs/refs/multimodal.md.
fn media(
    resource: ResourceView<'_>,
    tool: bool,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    if match resource.description {
        ResourceDescription::Image { detail } => detail.is_some(),
        ResourceDescription::File(file) => file.filename.is_some() || file.detail.is_some(),
        _ => false,
    } {
        rules.insert(RequestRule::OmitMediaOptions);
    }
    match resource.location {
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => Ok(json!({"inlineData": {
            "mimeType": media_type.as_str(), "data": data_base64.as_str()}})),
        ResourceLocation::Url(url) if !tool => Ok(json!({"fileData": {"fileUri": url.as_str()}})),
        _ => Err(unsupported()),
    }
}

/// Restores a signature onto the part it was issued with, independent of item order.
fn attach(parts: &mut Vec<Value>, value: Signed) -> Result<(), CodecError> {
    let signature = value.signature.ok_or_else(unsupported)?;
    let slot = match &value.anchor {
        Anchor::Call(id) => parts
            .iter_mut()
            .find(|p| p["functionCall"]["id"].as_str() == Some(id.as_str()))
            .ok_or(CodecError::Invalid("Gemini signature without its call"))?,
        Anchor::Tail => match parts.last_mut() {
            Some(last)
                if last.get("text").is_some()
                    && last.get("thought").is_none()
                    && last.get("thoughtSignature").is_none() =>
            {
                last
            }
            _ => {
                parts.push(json!({"text": ""}));
                parts.last_mut().expect("pushed")
            }
        },
        Anchor::Thought => return Err(unsupported()),
    };
    if slot.get("thoughtSignature").is_some() {
        return Err(CodecError::Invalid("duplicate Gemini signature"));
    }
    slot["thoughtSignature"] = signature.into();
    Ok(())
}
