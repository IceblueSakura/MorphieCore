//! Downstream Messages clients (Claude Code): requests to the IR, responses from it.
//! Unregistered fields and `cache_control` are ignored and named, as for OpenAI clients.
use super::{
    Block, Effort, Role, Selection, TextContent as WireText, Tool, ToolChoice as WireChoice,
    semantic_intake::{Intake, text},
    shape::{label, object, parse, required, string},
};
use crate::{
    adapter::Request,
    protocol::{CodecError, DecodedRequest, DecodedResponse, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const TOP: &[&str] = &[
    "model",
    "max_tokens",
    "messages",
    "system",
    "tools",
    "tool_choice",
    "thinking",
    "output_config",
    "temperature",
    "top_p",
    "stream",
];
const MAX_IGNORED: usize = 32;

/// Keep only `allowed` keys; the rest are ignored by name.
fn keep(v: &mut Value, allowed: &[&str], ignored: &mut BTreeSet<String>) {
    if let Some(o) = v.as_object_mut() {
        o.retain(|k, _| {
            let known = allowed.contains(&k.as_str());
            if !known && ignored.len() < MAX_IGNORED {
                ignored.insert(k.clone());
            }
            known
        });
    }
}
fn block_fields(kind: Option<&str>) -> &'static [&'static str] {
    match kind {
        // `citations` is kept so readers reject a non-empty list.
        Some("text") => &["type", "text", "citations"],
        Some("thinking") => &["type", "thinking", "signature"],
        Some("tool_use") => &["type", "id", "name", "input"],
        Some("tool_result") => &["type", "tool_use_id", "content", "is_error"],
        Some("image") => &["type", "source", "transformations"],
        // `context` and `citations` are kept so `Media::read` rejects them; they
        // change model-visible content.
        Some("document") => &["type", "source", "title", "context", "citations"],
        // Unknown block types stay closed and fail in `Block::read`.
        _ => &[
            "type",
            "text",
            "thinking",
            "signature",
            "id",
            "name",
            "input",
        ],
    }
}
fn blocks(v: &mut Value, ignored: &mut BTreeSet<String>) {
    if let Some(list) = v.as_array_mut() {
        for block in list {
            let kind = block.get("type").and_then(Value::as_str).map(str::to_owned);
            keep(block, block_fields(kind.as_deref()), ignored);
            if kind.as_deref() == Some("tool_result")
                && let Some(content) = block.get_mut("content")
            {
                blocks(content, ignored);
            }
        }
    }
}

/// Decode a client request; the model is a public label, never an upstream name.
pub fn decode_client_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let mut v = parse(bytes)?;
    let mut ignored = BTreeSet::new();
    keep(&mut v, TOP, &mut ignored);
    let o = object(&v)?.clone();
    let model = string(required(&o, "model")?)?;
    label(&model)?;
    let mut settings = GenerationSettings::default();
    settings.controls.max_output_tokens = Some(
        required(&o, "max_tokens")?
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or(CodecError::Invalid("max_tokens"))?,
    );
    for (key, apply) in [
        (
            "temperature",
            GenerationControls::with_temperature as fn(_, f64) -> _,
        ),
        ("top_p", GenerationControls::with_top_p),
    ] {
        if let Some(value) = o.get(key).filter(|v| !v.is_null()) {
            let value = value.as_f64().ok_or(CodecError::Invalid("sampling"))?;
            settings.controls = apply(settings.controls, value)?;
        }
    }
    let scope = LocalScope::ROOT;
    let mut intake = Intake::new(scope);
    if let Some(mut system) = o.get("system").filter(|v| !v.is_null()).cloned() {
        blocks(&mut system, &mut ignored);
        match WireText::read(&system)? {
            WireText::Text(s) => settings.instructions = Presence::Value(text(&s)?),
            WireText::Blocks(parts) if !parts.is_empty() => intake.items.push((
                ItemId::scoped(scope, 1),
                Item::Instruction(Instruction {
                    authority: InstructionAuthority::System,
                    status: None,
                    parts: parts
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            Ok((
                                PartId::scoped(scope, i as u64 + 1),
                                InputPart::Text(text(s)?),
                            ))
                        })
                        .collect::<Result<_, CodecError>>()?,
                }),
            )),
            WireText::Blocks(_) => {}
        }
    }
    if let Some(tools) = o.get("tools").filter(|v| !v.is_null()) {
        let tools = tools.as_array().ok_or(CodecError::Invalid("tools"))?;
        settings.tools =
            Some(
                tools
                    .iter()
                    .map(|t| {
                        let mut t = t.clone();
                        keep(
                            &mut t,
                            &["name", "description", "input_schema", "strict"],
                            &mut ignored,
                        );
                        let tool = Tool::read(&t)?;
                        tool.bytes()?;
                        Ok(ToolDefinition::Function(FunctionTool {
                            name: text(&tool.name)?,
                            description: tool.description.value().cloned(),
                            parameters: Some(tool.input_schema),
                            strict: tool.strict.value().map_or(
                                FunctionStrictness::Omitted(StrictDefault::NonStrict),
                                |b| FunctionStrictness::Explicit(*b),
                            ),
                            output_schema: None,
                            dispatch: Default::default(),
                        }))
                    })
                    .collect::<Result<_, CodecError>>()?,
            );
    }
    if let Some(choice) = o.get("tool_choice").filter(|v| !v.is_null()) {
        let choice = WireChoice::read(choice)?;
        settings.tool_choice = Some(match &choice.selection {
            Selection::Auto => ToolChoice::Auto,
            Selection::None => ToolChoice::None,
            Selection::Any => ToolChoice::Required,
            Selection::Tool(s) => ToolChoice::Specific(text(s)?),
        });
        settings.parallel_tool_calls = choice.disable_parallel_tool_use.value().map(|b| !b);
    }
    if let Some(thinking) = o.get("thinking").filter(|v| !v.is_null()) {
        let t = object(thinking)?;
        match t.get("type").and_then(Value::as_str) {
            Some("enabled") => {
                let budget = t
                    .get("budget_tokens")
                    .and_then(Value::as_u64)
                    .filter(|n| *n > 0)
                    .ok_or(CodecError::Invalid("thinking budget"))?;
                settings.reasoning = ReasoningRequest::present(None, None);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
                settings.reasoning.budget = Some(ReasoningBudget {
                    hard_limit: Some(budget),
                    soft_target: None,
                });
            }
            Some("adaptive") => {
                settings.reasoning = ReasoningRequest::present(None, None);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
            }
            Some("disabled") => {
                settings.reasoning = ReasoningRequest::present(Some(ReasoningEffort::None), None);
            }
            _ => return Err(CodecError::Unsupported("Anthropic thinking mode".into())),
        }
    }
    if let Some(config) = o.get("output_config").filter(|v| !v.is_null()) {
        let mut config = config.clone();
        keep(&mut config, &["effort"], &mut ignored);
        if config.get("effort").is_some() {
            settings.reasoning.presence = ReasoningPresence::Present;
            settings.reasoning.effort = Presence::Value(match Effort::read(&config)? {
                Effort::Low => ReasoningEffort::Low,
                Effort::Medium => ReasoningEffort::Medium,
                Effort::High => ReasoningEffort::High,
                Effort::XHigh => ReasoningEffort::XHigh,
                Effort::Max => ReasoningEffort::Max,
            });
        }
    }
    let messages = required(&o, "messages")?
        .as_array()
        .ok_or(CodecError::Invalid("messages"))?;
    for message in messages {
        let mut message = message.clone();
        keep(&mut message, &["role", "content"], &mut ignored);
        let m = object(&message)?;
        let role = Role::read(required(m, "role")?)?;
        let content = required(m, "content")?;
        let parsed = if content.is_string() {
            vec![Block::Text(string(content)?)]
        } else {
            let mut content = content.clone();
            blocks(&mut content, &mut ignored);
            content
                .as_array()
                .ok_or(CodecError::Invalid("content"))?
                .iter()
                .map(Block::read)
                .collect::<Result<_, _>>()?
        };
        intake.message(role, &parsed)?;
    }
    // Messages turns are not kept as envelopes: each target regroups items its own
    // way (native by role, Chat by assistant text and calls).
    // Calls carry their input as raw JSON text, as every client protocol's calls do.
    let items = intake
        .items
        .into_iter()
        .map(|(id, mut item)| {
            if let Item::ToolCall(call) = &mut item
                && let ToolArguments::Structured(input) = &call.arguments
            {
                call.arguments = ToolArguments::Raw(input.value().to_string());
            }
            // Thinking from a target without opaque values comes back unsigned.
            if let Item::Reasoning(r) = &mut item
                && r.replay.as_ref().is_some_and(|v| v.as_str().is_empty())
            {
                r.replay = None;
            }
            (id, item)
        })
        .collect();
    let semantic = GenerationRequest::from_resources(items, settings, intake.resources)?;
    let mut fidelity = FidelityRecords::default();
    fidelity.bind_replay_origin(&crate::execution::replay::CLIENT_ORIGIN)?;
    for part in intake.strict_images {
        fidelity.record_strict_image_size(part)?;
    }
    let mut request = Request::from_generation(DecodedRequest { semantic, fidelity }, &model)?;
    if let Some(stream) = o.get("stream").filter(|v| !v.is_null()) {
        request.delivery.stream =
            Presence::Value(stream.as_bool().ok_or(CodecError::Invalid("stream"))?);
    }
    request.ignored_fields = ignored.into_iter().collect();
    Ok(request)
}

/// Encode a sealed response for a Messages client under the public label.
pub fn encode_client_response(
    response: &DecodedResponse,
    model: &str,
) -> Result<Value, CodecError> {
    let semantic = &response.semantic;
    let mut content = vec![];
    let mut calls = false;
    for (_, item) in semantic.items() {
        match item {
            Item::Reasoning(r) => {
                let thinking: String = r
                    .parts
                    .iter()
                    .map(|(_, p)| match p {
                        ReasoningContent::Summary(t) | ReasoningContent::Text(t) => t.as_str(),
                    })
                    .collect();
                let signature = r.replay.as_ref().map_or("", ReplayValue::as_str);
                content.push(json!({"type":"thinking","thinking":thinking,"signature":signature}));
            }
            Item::Message(m) if m.role == MessageRole::Assistant => {
                for part in &m.parts {
                    match &part.content {
                        ContentPart::Text(t) => {
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        ContentPart::Refusal(t) => {
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        _ => return Err(CodecError::Unsupported("Messages content".into())),
                    }
                }
            }
            Item::ToolCall(call) => {
                calls = true;
                let input = match &call.arguments {
                    ToolArguments::Structured(v) => v.value().clone(),
                    ToolArguments::Raw(s) => serde_json::from_str(s)
                        .map_err(|_| CodecError::Invalid("tool arguments"))?,
                    _ => return Err(CodecError::Unsupported("Messages tool input".into())),
                };
                content.push(json!({"type":"tool_use","id":call.call_id.as_str(),
                    "name":call.name.as_str(),"input":input}));
            }
            _ => return Err(CodecError::Unsupported("Messages item".into())),
        }
    }
    let stop_reason = match (semantic.outcome(), &semantic.details().incomplete) {
        (Outcome::Completed, _) if calls => "tool_use",
        (Outcome::Completed, _) => "end_turn",
        (Outcome::Incomplete, Some(IncompleteReason::MaxOutputTokens)) => "max_tokens",
        (Outcome::Incomplete, Some(IncompleteReason::ContentFilter)) => "refusal",
        _ => return Err(CodecError::Unsupported("Messages terminal".into())),
    };
    let mut v = Map::new();
    v.insert("id".into(), json!(response.metadata.required_id()?));
    v.insert("type".into(), json!("message"));
    v.insert("role".into(), json!("assistant"));
    v.insert("model".into(), json!(model));
    v.insert("content".into(), json!(content));
    v.insert("stop_reason".into(), json!(stop_reason));
    v.insert("stop_sequence".into(), Value::Null);
    v.insert(
        "usage".into(),
        super::client_events::usage_json(semantic.usage().as_ref()),
    );
    Ok(Value::Object(v))
}
