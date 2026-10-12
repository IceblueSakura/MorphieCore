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
    LowerEffort,
    CapEffort,
    BudgetAsEffort,
    SummarizeReasoning,
    OmitReasoningContext,
    OmitForeignReplay,
    ReasoningAsText,
    DeveloperAsSystem,
    DefaultStrictAsPlain,
}
impl RequestRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::LowerEffort => "interactions.lower-effort.v1",
            Self::CapEffort => "interactions.cap-effort.v1",
            Self::BudgetAsEffort => "interactions.budget-as-effort.v1",
            Self::SummarizeReasoning => "interactions.summarize-reasoning.v1",
            Self::OmitReasoningContext => "interactions.omit-reasoning-context.v1",
            Self::OmitForeignReplay => "interactions.omit-foreign-replay.v1",
            Self::ReasoningAsText => "interactions.reasoning-as-text.v1",
            Self::DeveloperAsSystem => "interactions.developer-as-system.v1",
            Self::DefaultStrictAsPlain => "interactions.default-strict-as-plain.v1",
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
    if s.controls.top_logprobs.is_some()
        || !s.controls.logprobs.is_absent()
        || s.controls.truncation.is_some()
        || s.text != TextOptions::default()
        || !s.output_modalities.is_absent()
        || !s.audio.is_absent()
        || s.parallel_tool_calls == Some(false)
        || !request.replay_groups().is_empty()
        || !request.call_derivations().is_empty()
    {
        return Err(unsupported());
    }
    let mut body = Map::new();
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
        system = Some(
            i.parts
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
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
                let parameters = t.parameters.as_ref().ok_or_else(unsupported)?;
                if !parameters.is_unversioned_local() {
                    return Err(unsupported());
                }
                let mut tool = json!({"type":"function","name":t.name.as_str(),
                "parameters":parameters.root()});
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
                _ => return Err(unsupported()),
            },
        );
    }
    thinking(&s.reasoning, &mut config, rules)?;
    if !config.is_empty() {
        body.insert("generation_config".into(), config.into());
    }
    Ok(body.into())
}

fn thinking(
    r: &ReasoningRequest,
    config: &mut Map<String, Value>,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<(), CodecError> {
    if r.presence == ReasoningPresence::Null || !r.display.is_absent() && !r.summary.is_absent() {
        return Err(unsupported());
    }
    if !r.context.is_absent() {
        rules.insert(RequestRule::OmitReasoningContext);
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
        Presence::Value(ReasoningMode::Budgeted) => {
            rules.insert(RequestRule::BudgetAsEffort);
        }
        _ => return Err(unsupported()),
    }
    let effort = match (r.effort.clone(), r.budget_as_effort()) {
        (Presence::Absent, Some(effort)) => Presence::Value(effort),
        (effort, _) => effort,
    };
    let level = match effort {
        Presence::Absent => None,
        Presence::Value(ReasoningEffort::None) => {
            rules.insert(RequestRule::LowerEffort);
            Some("minimal")
        }
        Presence::Value(ReasoningEffort::Minimal) => Some("minimal"),
        Presence::Value(ReasoningEffort::Low) => Some("low"),
        Presence::Value(ReasoningEffort::Medium) => Some("medium"),
        Presence::Value(ReasoningEffort::High) => Some("high"),
        Presence::Value(ReasoningEffort::XHigh | ReasoningEffort::Max) => {
            rules.insert(RequestRule::CapEffort);
            Some("high")
        }
        Presence::Null => return Err(unsupported()),
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
                        .map(|p| {
                            let ContentPart::Text(t) = &p.content else {
                                return Err(unsupported());
                            };
                            if !t.is_plain() || p.replay.is_some() {
                                return Err(unsupported());
                            }
                            Ok(json!({"type":"text","text":t.as_str()}))
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
                        || c.context.replay.is_some()
                        || c.context.definition.is_some()
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
                        || r.context.definition.is_some()
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
                        ToolOutput::Parts(parts) => Value::Array(parts.iter().map(|(_, p)| {
                            Ok(json!({"type":"text","text":p.as_text().ok_or_else(unsupported)?}))
                        }).collect::<Result<_, CodecError>>()?),
                        ToolOutput::Structured(_) => return Err(unsupported()),
                    };
                    let name = request
                        .items()
                        .iter()
                        .find_map(|(_, i)| match i {
                            Item::ToolCall(c) if c.call_id == r.call_id => Some(c.name.as_str()),
                            _ => None,
                        })
                        .ok_or(CodecError::Invalid("Interactions result without call"))?;
                    let mut step = json!({"type":"function_result","call_id":r.call_id.as_str(),
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
