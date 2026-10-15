//! Downstream Messages clients (Claude Code): requests to the IR, responses from it.
//! Unregistered fields and `cache_control` are ignored and named, as for OpenAI clients.
use super::{
    Block, OutputConfig, Role, Selection, TextContent as WireText, Tool, ToolChoice as WireChoice,
    semantic_intake::{Intake, text},
    shape::{label, object, parse, required, string},
};
use crate::{
    adapter::Request,
    protocol::{
        CodecError, DecodedRequest, DecodedResponse, fidelity::FidelityRecords, replay_envelope,
    },
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
/// Official fields (including beta) whose active value changes model input, tools or
/// output; ignoring them would be silent loss. Other unregistered fields are ignored by name.
const UNSUPPORTED: &[&str] = &[
    "stop_sequences",
    "container",
    "mcp_servers",
    "context_management",
    "compaction",
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
        Some("redacted_thinking") => &["type", "data"],
        Some("tool_use") => &["type", "id", "name", "input", "caller"],
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
    let o = object(&v)?;
    if let Some(key) = UNSUPPORTED.iter().find(|key| {
        o.get(**key)
            .is_some_and(|v| !v.is_null() && v != &json!([]))
    }) {
        return Err(CodecError::Unsupported((*key).into()));
    }
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
                        if ["defer_loading", "allowed_callers"]
                            .iter()
                            .any(|key| t.get(key).is_some())
                        {
                            return Err(CodecError::Unsupported("Anthropic tool dispatch".into()));
                        }
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
        let mut thinking = thinking.clone();
        keep(
            &mut thinking,
            &["type", "budget_tokens", "display"],
            &mut ignored,
        );
        let t = object(&thinking)?;
        match t.get("type").and_then(Value::as_str) {
            Some("enabled") => {
                let budget = t
                    .get("budget_tokens")
                    .and_then(Value::as_u64)
                    .filter(|n| *n >= 1024)
                    .ok_or(CodecError::Invalid("thinking budget"))?;
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
                settings.reasoning.budget_tokens = Some(budget);
            }
            Some("adaptive") => {
                if t.contains_key("budget_tokens") {
                    return Err(CodecError::Invalid("adaptive thinking budget"));
                }
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
            }
            Some("disabled") => {
                if t.contains_key("budget_tokens") || t.contains_key("display") {
                    return Err(CodecError::Invalid("disabled thinking controls"));
                }
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
            }
            _ => return Err(CodecError::Unsupported("Anthropic thinking mode".into())),
        }
        settings.reasoning.display =
            super::semantic::display(&super::shape::presence(t, "display", super::Display::read)?);
        if settings.reasoning.display == Presence::Null {
            return Err(CodecError::Invalid("thinking display"));
        }
    }
    if let Some(config) = o.get("output_config").filter(|v| !v.is_null()) {
        let mut config = config.clone();
        keep(&mut config, &["effort", "format"], &mut ignored);
        if let Some(format) = config.get_mut("format") {
            keep(format, &["type", "schema"], &mut ignored);
        }
        super::semantic::output_settings(&OutputConfig::read(&config)?, &mut settings);
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
    request.source = Some(crate::adapter::ClientProtocol::Messages);
    if let Some(stream) = o.get("stream").filter(|v| !v.is_null()) {
        request.delivery.stream =
            Presence::Value(stream.as_bool().ok_or(CodecError::Invalid("stream"))?);
    }
    request.ignored_fields = ignored.into_iter().collect();
    Ok(request)
}

/// Client values are sealed envelopes; the format tag keeps a redacted block's type.
pub(super) fn redacted(sealed: &str) -> bool {
    matches!(
        replay_envelope::open(sealed),
        Some((_, ReplayFormat::AnthropicMessagesRedactedThinking, _))
    )
}

/// Retained metadata and accounting reflect the actual Messages carrier.
pub(crate) fn project_snapshot(response: &mut DecodedResponse) -> Result<(), CodecError> {
    response.metadata.context.validate()?;
    response.metadata.context.settings = None;
    response.metadata.context.instruction_messages = None;
    response.metadata.instruction_fidelity = Default::default();
    response.metadata.context.system_fingerprint = Presence::Absent;
    let wire = super::client_events::usage_json(response.semantic.usage().as_ref())?;
    let count = |key: &str| -> Result<u64, CodecError> {
        wire[key]
            .as_u64()
            .ok_or(CodecError::Invalid("Messages projected usage"))
    };
    let selected = super::semantic::usage(&super::Usage {
        input_tokens: Presence::Value(count("input_tokens")?),
        output_tokens: count("output_tokens")?,
        cache_read_input_tokens: wire
            .get("cache_read_input_tokens")
            .map(|_| count("cache_read_input_tokens"))
            .transpose()?
            .map_or(Presence::Absent, Presence::Value),
        cache_creation_input_tokens: wire
            .get("cache_creation_input_tokens")
            .map(|_| count("cache_creation_input_tokens"))
            .transpose()?
            .map_or(Presence::Absent, Presence::Value),
    });
    response.semantic = response.semantic.clone().with_usage(Some(selected))?;
    Ok(())
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
                let signature = r.replay.as_ref().map_or("", ReplayValue::as_str);
                if redacted(signature) {
                    content.push(json!({"type":"redacted_thinking","data":signature}));
                    continue;
                }
                let thinking: String = r
                    .parts
                    .iter()
                    .map(|(_, p)| match p {
                        ReasoningContent::Summary(t) | ReasoningContent::Text(t) => t.as_str(),
                    })
                    .collect();
                content.push(json!({"type":"thinking","thinking":thinking,"signature":signature}));
            }
            Item::Message(m) if m.role == MessageRole::Assistant => {
                if m.phase.is_some() {
                    return Err(CodecError::Unsupported("Messages phase".into()));
                }
                for part in &m.parts {
                    match &part.content {
                        ContentPart::Text(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    "Messages probabilities".into(),
                                ));
                            }
                            if !t.annotations().is_empty() {
                                return Err(CodecError::Unsupported("Messages citations".into()));
                            }
                            content.push(json!({"type":"text","text":t.as_str()}))
                        }
                        ContentPart::Refusal(t) => {
                            if !t.logprobs().is_absent() {
                                return Err(CodecError::Unsupported(
                                    "Messages probabilities".into(),
                                ));
                            }
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
        super::client_events::usage_json(semantic.usage().as_ref())?,
    );
    Ok(Value::Object(v))
}
