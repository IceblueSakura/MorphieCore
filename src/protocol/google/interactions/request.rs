//! Direct IR → Interactions steps; no generateContent wire translation.
use super::items::steps;
use crate::{
    protocol::{
        CodecError, failure::CarrierReason, fidelity::FidelityRecords, native::ReplayTarget,
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RequestRule {
    OmitSampling,
    OmitPrediction,
    OmitVerbosity,
    OmitMessagePhase,
    Hint(crate::protocol::cache::HintRule),
    SummarizeReasoning,
    TransferVisibleHistory,
    ReasoningAsText,
    OmitSchemaName,
    OmitCacheKey,
    JoinInstructionParts,
}
impl RequestRule {
    pub(crate) fn effect(
        self,
        request: &GenerationRequest,
    ) -> crate::protocol::conversion::generation::Effect {
        use crate::protocol::conversion::generation::{
            Change, ConversionBoundary as B, Effect, Field, Owner,
        };
        let (field, change) = match self {
            Self::Hint(hint) => return hint.effect(),
            Self::JoinInstructionParts => (Field::InstructionAuthority, Change::Approximate),
            Self::OmitCacheKey => (Field::Cache, Change::Omit),
            Self::OmitMessagePhase => (Field::MessagePhase, Change::Omit),
            Self::OmitPrediction => (Field::Prediction, Change::Omit),
            Self::OmitSampling => (Field::Sampling, Change::Omit),
            Self::OmitSchemaName => (Field::SchemaName, Change::Omit),
            Self::OmitVerbosity => (Field::Verbosity, Change::Omit),
            Self::ReasoningAsText => (Field::ReportedReasoning, Change::Approximate),
            Self::SummarizeReasoning => (Field::ReasoningSummary, Change::Approximate),
            Self::TransferVisibleHistory => (Field::Replay, Change::Omit),
        };
        match self {
            Self::SummarizeReasoning => Effect::Summary {
                rule: self.name().into(),
                owner: Owner::Root,
                input: request.reasoning().presentation.summary.clone(),
                output: ReasoningDisplay::Summary,
            },
            _ => Effect::new(self.name(), field, B::RequestProject, change),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::OmitSampling => "interactions.omit-ignored-sampling.v1",
            Self::OmitPrediction => "generation.omit-prediction.v1",
            Self::OmitVerbosity => "interactions.omit-verbosity.v1",
            Self::OmitMessagePhase => "interactions.omit-message-phase.v1",
            Self::Hint(rule) => rule.name(),
            Self::SummarizeReasoning => "interactions.summarize-reasoning.v1",
            Self::TransferVisibleHistory => "interactions.transfer-visible-history.v1",
            Self::ReasoningAsText => "interactions.reasoning-as-text.v1",
            Self::OmitSchemaName => "interactions.omit-schema-name.v1",
            Self::OmitCacheKey => "interactions.omit-cache-key.v1",
            Self::JoinInstructionParts => "interactions.join-instruction-parts.v1",
        }
    }
}
pub(super) fn unsupported() -> CodecError {
    CodecError::Unsupported(CarrierReason::Unavailable)
}

pub(super) fn project(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Value, CodecError> {
    if request
        .items()
        .iter()
        .any(|(_, i)| i.participant().is_some())
    {
        return Err(unsupported());
    }
    let s = request.settings();
    if s.prediction.is_some() {
        rules.insert(RequestRule::OmitPrediction);
    }
    if !s.safety.is_empty() {
        return Err(unsupported());
    }
    if s.controls.temperature().is_some()
        || s.controls.top_p().is_some()
        || s.controls.top_k.is_some()
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
        || crate::protocol::image_constraints::rejects_downsize(request.items())
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
        Presence::Value(root) => {
            if root.parts.iter().any(|part| part.cache.is_some()) {
                return Err(unsupported());
            }
            Some(
                root.parts
                    .iter()
                    .map(|part| part.text.as_str().to_owned())
                    .collect(),
            )
        }
        Presence::Null => return Err(unsupported()),
    };
    for (_, item) in request
        .items()
        .iter()
        .take_while(|(_, item)| matches!(item, Item::Instruction(_)))
    {
        let Item::Instruction(i) = item else {
            unreachable!()
        };
        if i.status.is_some() {
            return Err(unsupported());
        }
        system
            .get_or_insert_with(Vec::new)
            .extend(i.texts().map(|(_, t)| t.to_owned()));
    }
    if request
        .items()
        .iter()
        .skip_while(|(_, item)| matches!(item, Item::Instruction(_)))
        .any(|(_, i)| matches!(i, Item::Instruction(_)))
    {
        return Err(unsupported());
    }
    if let Some(system) = system {
        // The native string renders ordered parts as paragraphs, not reordered history.
        if system.len() > 1 {
            rules.insert(RequestRule::JoinInstructionParts);
        }
        body.insert("system_instruction".into(), system.join("\n\n").into());
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
        || r.budget_tokens.is_some()
        || r.activation == Presence::Null
        || r.activation.value() == Some(&ReasoningActivation::Disabled)
    {
        return Err(unsupported());
    }
    if !r.context.is_absent() || !r.read_history.is_absent() {
        return Err(unsupported());
    }
    let summary = match (&r.presentation.visibility, &r.presentation.summary) {
        (Presence::Value(ReasoningDisplay::Summary), granularity) => {
            if matches!(
                granularity,
                Presence::Value(ReasoningSummary::Concise | ReasoningSummary::Detailed)
            ) {
                rules.insert(RequestRule::SummarizeReasoning);
            }
            Some("auto")
        }
        (Presence::Value(ReasoningDisplay::Omitted), _) => Some("none"),
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

/// Full history as steps, always streamed and never stored by the upstream.
pub fn encode_generation_request(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
) -> Result<(Value, Vec<RequestRule>), CodecError> {
    let (request, fidelity, transferred) = crate::protocol::native::native_reasoning_history(
        request,
        fidelity,
        target,
        &[
            crate::semantic::task::generation::ReplayFormat::GoogleInteractionsV1Thought,
            crate::semantic::task::generation::ReplayFormat::GoogleInteractionsV1FunctionCall,
        ],
    )?;
    let request = crate::protocol::native::group_client_history(&request, false)?;
    request.validate()?;
    if request.cached_context().is_some() {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
    )?;
    crate::semantic::value::json_size(&value, crate::semantic::task::generation::MAX_TOTAL_BYTES)
        .map_err(|_| CodecError::Limit)?;
    Ok((value, rules.into_iter().collect()))
}
