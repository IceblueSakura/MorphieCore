//! Direct IR → Interactions steps; no generateContent wire translation.
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords, native::ReplayTarget},
    semantic::{
        task::generation::*,
        value::{JsonLimits, Presence, parse_json},
    },
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    OmitVerbosity,
    Cache(crate::protocol::cache::CacheRule),
    SummarizeReasoning,
    OmitForeignReplay,
    ReasoningAsText,
    DeveloperAsSystem,
    OmitSchemaName,
    OmitCacheKey,
    JoinInstructionParts,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitVerbosity => "interactions.omit-verbosity.v1",
            Self::Cache(rule) => rule.name(),
            Self::SummarizeReasoning => "interactions.summarize-reasoning.v1",
            Self::OmitForeignReplay => "interactions.omit-foreign-replay.v1",
            Self::ReasoningAsText => "interactions.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "interactions.developer-as-system.v1",
            Self::OmitSchemaName => "interactions.omit-schema-name.v1",
            Self::OmitCacheKey => "interactions.omit-cache-key.v1",
            Self::JoinInstructionParts => "interactions.join-instruction-parts.v1",
        }
    }
}
fn unsupported() -> CodecError {
    CodecError::Unsupported("Interactions semantic carrier".into())
}

pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let s = request.settings();
    if !s.safety.is_empty() {
        return Err(unsupported());
    }
    if s.controls.top_k.is_some()
        || s.controls.presence_penalty().is_some()
        || s.controls.frequency_penalty().is_some()
    {
        return Err(unsupported());
    }
    if s.automatic_prefix_cache.is_some()
        || request
            .items()
            .iter()
            .any(|(_, i)| i.has_cache_breakpoints())
    {
        return Err(unsupported());
    }
    if s.controls.top_logprobs.is_some()
        || !s.controls.logprobs.is_absent()
        || s.controls.truncation.is_some()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
        || s.parallel_tool_calls == Some(false)
        || fidelity.has_strict_image_sizes(request.items())
    {
        return Err(unsupported());
    }
    match &s.text.format {
        Presence::Absent | Presence::Value(OutputConstraint::Text) => {}
        Presence::Value(OutputConstraint::JsonSchema {
            description: None, ..
        }) => {}
        _ => return Err(unsupported()),
    }
    if !s.text.verbosity.is_absent() {
        rules.insert(RequestRule::OmitVerbosity);
    }
    let mut body = Map::new();
    if let Presence::Value(OutputConstraint::JsonSchema { name, schema, .. }) = &s.text.format {
        if !super::schema::admits(schema.root()) {
            return Err(unsupported());
        }
        body.insert(
            "response_format".into(),
            json!({"type":"text","mime_type":"application/json","schema":schema.root()}),
        );
        if name.is_some() {
            rules.insert(RequestRule::OmitSchemaName);
        }
    }
    body.insert("model".into(), target.model().into());
    body.insert("store".into(), false.into());
    body.insert("stream".into(), true.into());
    let mut system = match &s.instructions {
        Presence::Absent => None,
        Presence::Value(t) => Some(t.as_str().to_owned()),
        Presence::Null => return Err(unsupported()),
    };
    if let Some((_, Item::Instruction(i))) = request.items().first() {
        if i.status.is_some() || system.is_some() {
            return Err(unsupported());
        }
        if i.authority == InstructionAuthority::Developer {
            rules.insert(RequestRule::DeveloperAsSystem);
        }
        // `system_instruction` is one string. Parts become paragraphs, as generateContent
        // renders separate system parts: https://ai.google.dev/api/caching#CachedContent
        if i.parts.len() > 1 {
            rules.insert(RequestRule::JoinInstructionParts);
        }
        system = Some(i.texts().map(|(_, t)| t).collect::<Vec<_>>().join("\n\n"));
    }
    if request
        .items()
        .iter()
        .skip(1)
        .any(|(_, i)| matches!(i, Item::Instruction(_)))
    {
        return Err(unsupported());
    }
    if let Some(system) = system {
        body.insert("system_instruction".into(), system.into());
    }
    body.insert("input".into(), steps(request, fidelity, target, rules)?);
    if s.tools.is_some() {
        let tools = request
            .tools()
            .iter()
            .map(|tool| {
                let ToolDefinition::Function(t) = tool else {
                    return Err(unsupported());
                };
                if !t.dispatch.is_inactive() || t.output_schema.is_some() || t.cache.is_some() {
                    return Err(unsupported());
                }
                match t.strict {
                    FunctionStrictness::Omitted(StrictDefault::NonStrict)
                    | FunctionStrictness::Explicit(false) => {}
                    FunctionStrictness::Omitted(StrictDefault::NormalizeSchema)
                    | FunctionStrictness::Explicit(true) => return Err(unsupported()),
                }
                // `parameters` is optional on the pinned Interactions Function type.
                let mut tool = json!({"type":"function","name":t.name.as_str()});
                if let Some(parameters) = &t.parameters {
                    tool["parameters"] = parameters.root().clone();
                }
                if let Some(description) = &t.description {
                    tool["description"] = description.as_str().into();
                }
                Ok(tool)
            })
            .collect::<Result<Vec<_>, _>>()?;
        body.insert("tools".into(), tools.into());
    }
    let mut config = Map::new();
    if let Some(n) = s.controls.max_output_tokens {
        config.insert("max_output_tokens".into(), n.into());
    }
    if let Some(t) = s.controls.temperature() {
        config.insert("temperature".into(), t.into());
    }
    if let Some(p) = s.controls.top_p() {
        config.insert("top_p".into(), p.into());
    }
    if let Some(choice) = &s.tool_choice {
        config.insert(
            "tool_choice".into(),
            match choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("any"),
                ToolChoice::Specific(name) => {
                    json!({"allowed_tools":{"mode":"any","tools":[name.as_str()]}})
                }
                ToolChoice::Allowed { required, tools }
                    if tools.iter().all(|tool| {
                        tool.kind == ToolKind::Function && tool.namespace.is_none()
                    }) =>
                {
                    json!({"allowed_tools":{"mode":if *required { "any" } else { "auto" },
                        "tools":tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>()}})
                }
                _ => return Err(unsupported()),
            },
        );
    }
    if !s.controls.stop_sequences.is_empty() {
        config.insert(
            "stop_sequences".into(),
            json!(
                s.controls
                    .stop_sequences
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
            ),
        );
    }
    if let Some(seed) = s.controls.seed {
        config.insert("seed".into(), seed.into());
    }
    thinking(s, &mut config, rules)?;
    if !config.is_empty() {
        body.insert("generation_config".into(), config.into());
    }
    Ok(body.into())
}

fn thinking(
    settings: &GenerationSettings,
    config: &mut Map<String, Value>,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<(), CodecError> {
    let r = &settings.reasoning;
    if r.presence == ReasoningPresence::Null
        || !r.display.is_absent() && !r.summary.is_absent()
        || r.budget_tokens.is_some()
        || r.activation == Presence::Null
        || r.activation.value() == Some(&ReasoningActivation::Disabled)
    {
        return Err(unsupported());
    }
    if !r.context.is_absent() {
        return Err(unsupported());
    }
    let summary = match (&r.display, &r.summary) {
        (Presence::Value(ReasoningDisplay::Summary), _)
        | (Presence::Absent, Presence::Value(ReasoningSummary::Auto)) => Some("auto"),
        (
            Presence::Absent,
            Presence::Value(ReasoningSummary::Concise | ReasoningSummary::Detailed),
        ) => {
            rules.insert(RequestRule::SummarizeReasoning);
            Some("auto")
        }
        (Presence::Value(ReasoningDisplay::Omitted), _)
        | (Presence::Absent, Presence::Value(ReasoningSummary::Disabled)) => Some("none"),
        (Presence::Absent, Presence::Absent) => None,
        _ => return Err(unsupported()),
    };
    if let Some(summary) = summary {
        config.insert("thinking_summaries".into(), summary.into());
    }
    match r.mode {
        Presence::Absent | Presence::Value(ReasoningMode::Adaptive) => {}
        _ => return Err(unsupported()),
    }
    let effort = settings.effort.value();
    if effort.is_some_and(|e| e.scope != EffortScope::Reasoning)
        || settings.effort == Presence::Null
    {
        return Err(unsupported());
    }
    let level = match effort {
        None => None,
        Some(EffortControl {
            level: EffortLevel::Minimal,
            ..
        }) => Some("minimal"),
        Some(EffortControl {
            level: EffortLevel::Low,
            ..
        }) => Some("low"),
        Some(EffortControl {
            level: EffortLevel::Medium,
            ..
        }) => Some("medium"),
        Some(EffortControl {
            level: EffortLevel::High,
            ..
        }) => Some("high"),
        Some(_) => return Err(unsupported()),
    };
    if let Some(level) = level {
        config.insert("thinking_level".into(), level.into());
    }
    Ok(())
}

fn steps(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    let mut steps = vec![];
    for group in request.message_envelopes() {
        let mut signed = vec![];
        let start = steps.len();
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
                            rules.insert(RequestRule::OmitForeignReplay);
                        }
                        if !content.is_empty() {
                            rules.insert(RequestRule::ReasoningAsText);
                            steps.push(json!({"type":"model_output","content":content}));
                        }
                        continue;
                    };
                    if !fidelity.replay_matches_request(*id, r, Some(&target.origin()), request) {
                        return Err(CodecError::Invalid("Interactions replay dependency"));
                    }
                    let value = super::signature::Signed::decode(
                        value.replay_token().ok_or_else(unsupported)?,
                    )?;
                    if let Some(call) = value.call {
                        if !r.parts.is_empty() {
                            return Err(CodecError::Invalid("Interactions signature owner"));
                        }
                        signed.push((call, value.signature.ok_or_else(unsupported)?));
                    } else {
                        let mut step = json!({"type":"thought"});
                        if !content.is_empty() {
                            step["summary"] = content.into();
                        }
                        if let Some(signature) = value.signature {
                            step["signature"] = signature.into();
                        }
                        steps.push(step);
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
                        ToolArguments::Raw(s) => {
                            parse_json(s.as_bytes(), JsonLimits::STRUCTURED)
                                .map_err(|_| CodecError::Invalid("Interactions arguments"))?
                        }
                        _ => return Err(unsupported()),
                    };
                    if !args.is_object() {
                        return Err(unsupported());
                    }
                    steps.push(json!({"type":"function_call","id":c.call_id.as_str(),
                        "name":c.name.as_str(),"arguments":args}));
                }
                Item::ToolResult(r) => {
                    if !r.context.is_direct()
                        || r.context.alias_domain.is_some()
                        || r.status.is_some()
                    {
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
                        .ok_or(CodecError::Invalid("Interactions result without call"))?;
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
        for (call, signature) in signed {
            let step = steps[start..]
                .iter_mut()
                .find(|s| s["type"] == "function_call" && s["id"].as_str() == Some(call.as_str()))
                .ok_or(CodecError::Invalid("Interactions signature without call"))?;
            if step.get("signature").is_some() {
                return Err(CodecError::Invalid("duplicate Interactions signature"));
            }
            step["signature"] = signature.into();
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
