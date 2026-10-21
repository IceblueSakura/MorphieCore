//! Candidate-local generateContent request projection.
//! Source: <https://ai.google.dev/api/generate-content>.
use super::items::contents;
use crate::{
    protocol::{
        CodecError,
        fidelity::FidelityRecords,
        native::{ReplayTarget, group_client_history},
    },
    semantic::{
        task::generation::*,
        value::{Presence, json_size},
    },
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
                if !t.dispatch.is_inactive()
                    || t.output_schema.is_some()
                    || t.cache.is_some()
                    || t.input_examples.is_some()
                {
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

/// Pure IR → `streamGenerateContent` body for one target, with the rules applied.
pub fn encode_generation_request(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    resource_time: Option<std::time::SystemTime>,
) -> Result<(Value, Vec<RequestRule>), CodecError> {
    encode_with_identity(
        request,
        fidelity,
        target,
        resource_time,
        &Default::default(),
    )
}
pub(super) fn encode_with_identity(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    resource_time: Option<std::time::SystemTime>,
    identity: &crate::semantic::context::ClientIdentityHints,
) -> Result<(Value, Vec<RequestRule>), CodecError> {
    let (request, fidelity, transferred) = crate::protocol::native::native_reasoning_history(
        request,
        fidelity,
        target,
        &[
            crate::semantic::task::generation::ReplayFormat::GoogleGenerateContentPart,
            crate::semantic::task::generation::ReplayFormat::GoogleGenerateContentSignature,
        ],
    )?;
    let request = group_client_history(&request, false)?;
    request.validate()?;
    if let Some(reference) = request.cached_context() {
        fidelity.check_cached_context(reference, &target.origin(), resource_time)?;
    }
    let mut rules = std::collections::BTreeSet::new();
    if transferred {
        rules.insert(RequestRule::TransferVisibleHistory);
    }
    let phase = crate::protocol::phase::omit_request(&request)?;
    if phase.is_some() {
        rules.insert(RequestRule::OmitMessagePhase);
    }
    let value = project(
        phase.as_ref().unwrap_or(&request),
        &fidelity,
        target,
        &mut rules,
        identity,
    )?;
    json_size(&value, crate::semantic::task::generation::MAX_TOTAL_BYTES)
        .map_err(|_| CodecError::Limit)?;
    Ok((value, rules.into_iter().collect()))
}
