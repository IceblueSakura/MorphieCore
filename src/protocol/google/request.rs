//! Candidate-local generateContent request projection.
//! Source: <https://ai.google.dev/api/generate-content>.
use super::signature::Signed;
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
    OmitVerbosity,
    OmitMessagePhase,
    Hint(crate::protocol::cache::HintRule),
    /// Gemini has one summary form, without concise or detailed variants.
    SummarizeReasoning,
    /// Incompatible complete reasoning is omitted, retaining visible history.
    TransferVisibleHistory,
    /// Explicit opt-in for unattributed readable reasoning.
    ReasoningAsText,
    /// A leading developer instruction (pi, Codex) is the system instruction.
    DeveloperAsSystem,
    OmitSchemaName,
    /// Gemini caches implicitly and has no cache-key carrier.
    OmitCacheKey,
    /// `functionResponse.response` holds one string; text parts are concatenated.
    JoinResultParts,
    /// A pre-ownership surrogate signature returned by an older gateway is dropped.
    OmitLegacySignature,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitVerbosity => "gemini.omit-verbosity.v1",
            Self::OmitMessagePhase => "gemini.omit-message-phase.v1",
            Self::Hint(rule) => rule.name(),
            Self::SummarizeReasoning => "gemini.summarize-reasoning.v1",
            Self::TransferVisibleHistory => "gemini.transfer-visible-history.v1",
            Self::ReasoningAsText => "gemini.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "gemini.developer-as-system.v1",
            Self::OmitSchemaName => "gemini.omit-schema-name.v1",
            Self::OmitCacheKey => "gemini.omit-cache-key.v1",
            Self::JoinResultParts => "gemini.join-result-parts.v1",
            Self::OmitLegacySignature => "gemini.omit-legacy-signature.v1",
        }
    }
}

pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
    identity: &crate::semantic::context::ClientIdentityHints,
) -> Result<Value, CodecError> {
    let s = request.settings();
    if s.automatic_prefix_cache.is_some()
        || request
            .items()
            .iter()
            .any(|(_, i)| i.has_cache_breakpoints())
    {
        return Err(unsupported());
    }
    if s.controls.truncation.is_some()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
        || s.parallel_tool_calls == Some(false)
        || fidelity.has_strict_image_sizes(request.items())
    {
        return Err(unsupported());
    }
    if !s.text.verbosity.is_absent() {
        rules.insert(RequestRule::OmitVerbosity);
    }
    let mut body = Map::new();
    if !s.safety.is_empty() {
        body.insert(
            "safetySettings".into(),
            super::safety::settings(&s.safety).into(),
        );
    }
    if let Some(value) = identity.safety_identifier.value() {
        body.insert("labels".into(), json!({"safety_identifier":value}));
    }
    if let Some(reference) = request.cached_context() {
        // Cached resources may fix these controls. Do not invent an overlay.
        // https://ai.google.dev/api/caching#CachedContent
        if !s.instructions.is_absent()
            || s.tools.is_some()
            || s.tool_choice.is_some()
            || request
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::Instruction(_)))
        {
            return Err(unsupported());
        }
        let name = reference.id();
        let id = name
            .strip_prefix("cachedContents/")
            .ok_or_else(unsupported)?;
        if id.is_empty()
            || id
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#'))
        {
            return Err(unsupported());
        }
        body.insert("cachedContent".into(), name.into());
    }
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
        system = Some(i.texts().map(|(_, t)| t.to_owned()).collect());
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
                if !t.dispatch.is_inactive() || t.output_schema.is_some() || t.cache.is_some() {
                    return Err(unsupported());
                }
                match t.strict {
                    FunctionStrictness::Omitted(StrictDefault::NonStrict)
                    | FunctionStrictness::Explicit(false) => {}
                    FunctionStrictness::Omitted(StrictDefault::NormalizeSchema)
                    | FunctionStrictness::Explicit(true) => return Err(unsupported()),
                }
                let mut d = Map::new();
                d.insert("name".into(), t.name.as_str().into());
                if let Some(description) = &t.description {
                    d.insert("description".into(), description.as_str().into());
                }
                // An omitted schema declares a function without parameters, as in the source.
                // https://ai.google.dev/api/caching#FunctionDeclaration
                if let Some(parameters) = &t.parameters {
                    // This carrier describes named parameters, not a scalar/union input.
                    // Do not infer or inject an object constraint into the source schema.
                    if parameters.root().get("type").and_then(Value::as_str) != Some("object") {
                        return Err(unsupported());
                    }
                    d.insert("parametersJsonSchema".into(), parameters.root().clone());
                }
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
            ToolChoice::Allowed {
                required: true,
                tools,
            } if tools
                .iter()
                .all(|tool| tool.kind == ToolKind::Function && tool.namespace.is_none()) =>
            {
                let names: Vec<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
                json!({"mode": "ANY", "allowedFunctionNames": names})
            }
            _ => return Err(unsupported()),
        };
        body.insert(
            "toolConfig".into(),
            json!({"functionCallingConfig": config}),
        );
    }
    let mut generation = Map::new();
    match &s.text.format {
        Presence::Absent | Presence::Value(OutputConstraint::Text) => {}
        Presence::Value(OutputConstraint::JsonSchema {
            name,
            description: None,
            schema,
            ..
        }) if super::schema::admits(schema.root()) => {
            generation.insert("responseMimeType".into(), json!("application/json"));
            generation.insert("responseJsonSchema".into(), schema.root().clone());
            if name.is_some() {
                rules.insert(RequestRule::OmitSchemaName);
            }
        }
        _ => return Err(unsupported()),
    }
    match &s.controls.logprobs {
        Presence::Absent => {}
        Presence::Value(value) => {
            generation.insert("responseLogprobs".into(), json!(value));
        }
        Presence::Null => return Err(unsupported()),
    }
    if let Some(count) = s.controls.top_logprobs {
        if s.controls.logprobs != Presence::Value(true) {
            return Err(unsupported());
        }
        generation.insert("logprobs".into(), count.into());
    }
    if let Some(n) = s.controls.max_output_tokens {
        generation.insert("maxOutputTokens".into(), n.into());
    }
    if let Some(t) = s.controls.temperature() {
        generation.insert("temperature".into(), t.into());
    }
    if let Some(p) = s.controls.top_p() {
        generation.insert("topP".into(), p.into());
    }
    if let Some(k) = s.controls.top_k {
        generation.insert(
            "topK".into(),
            i32::try_from(k).map_err(|_| unsupported())?.into(),
        );
    }
    if s.controls.stop_sequences.len() > 5 {
        return Err(unsupported());
    }
    if !s.controls.stop_sequences.is_empty() {
        generation.insert(
            "stopSequences".into(),
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
        generation.insert(
            "seed".into(),
            i32::try_from(seed).map_err(|_| unsupported())?.into(),
        );
    }
    if let Some(penalty) = s.controls.presence_penalty() {
        generation.insert("presencePenalty".into(), penalty.into());
    }
    if let Some(penalty) = s.controls.frequency_penalty() {
        generation.insert("frequencyPenalty".into(), penalty.into());
    }
    if let Some(thinking) = thinking(s, rules)? {
        generation.insert("thinkingConfig".into(), thinking);
    }
    if !generation.is_empty() {
        body.insert("generationConfig".into(), Value::Object(generation));
    }
    Ok(Value::Object(body))
}

fn thinking(
    settings: &GenerationSettings,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Option<Value>, CodecError> {
    let reasoning = &settings.reasoning;
    if reasoning.presence == ReasoningPresence::Null
        || !reasoning.display.is_absent() && !reasoning.summary.is_absent()
        || reasoning
            .budget_tokens
            .is_some_and(|budget| budget > i64::MAX as u64)
        || reasoning.activation == Presence::Null
        || reasoning.activation.value() == Some(&ReasoningActivation::Disabled)
    {
        return Err(unsupported());
    }
    if !reasoning.context.is_absent() || !reasoning.read_history.is_absent() {
        return Err(unsupported());
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
        Presence::Value(ReasoningMode::Budgeted) if reasoning.budget_tokens.is_some() => {}
        _ => return Err(unsupported()),
    }
    let effort = settings.effort.value();
    if effort.is_some_and(|e| e.scope != EffortScope::Reasoning)
        || settings.effort == Presence::Null
        || effort.is_some() && reasoning.budget_tokens.is_some()
    {
        return Err(unsupported());
    }
    let level = match effort {
        None => None,
        Some(EffortControl {
            level: EffortLevel::Minimal,
            ..
        }) => Some("MINIMAL"),
        Some(EffortControl {
            level: EffortLevel::Low,
            ..
        }) => Some("LOW"),
        Some(EffortControl {
            level: EffortLevel::Medium,
            ..
        }) => Some("MEDIUM"),
        Some(EffortControl {
            level: EffortLevel::High,
            ..
        }) => Some("HIGH"),
        Some(_) => return Err(unsupported()),
    };
    let mut config = Map::new();
    if let Some(budget) = reasoning.budget_tokens {
        config.insert("thinkingBudget".into(), budget.into());
    }
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
            if r.parts.is_empty() && super::signature::is_legacy(token) {
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
                    for part in list {
                        match &part.content {
                            InputPart::Text(t) => text.push(t.as_str()),
                            InputPart::Resource(r) => {
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
            super::check_public_media_url(url.as_str())?;
            Ok(json!({"fileData": {"fileUri": url.as_str()}}))
        }
        _ => Err(unsupported()),
    }
}
