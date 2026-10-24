//! Untrusted client Messages: registered fields, protected controls and IR admission.
use crate::protocol::anthropic::messages::{
    Block, OutputConfig, Role, Selection, TextContent as WireText, Tool, ToolChoice as WireChoice,
    items::{Intake, text},
    service, settings,
    shape::{self, label, object, parse, required, string},
    wire,
};
use crate::{
    adapter::Request,
    protocol::{CodecError, DecodedRequest, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const TOP: &[&str] = &[
    "model",
    "service_tier",
    "max_tokens",
    "messages",
    "system",
    "tools",
    "tool_choice",
    "thinking",
    "output_config",
    "temperature",
    "top_p",
    "top_k",
    "cache_control",
    "stop_sequences",
    "stream",
    "metadata",
];
/// Official fields (including beta) whose active value changes model input, tools or
/// output; ignoring them would be silent loss. Other unregistered fields are ignored by name.
const UNSUPPORTED: &[&str] = &[
    "container",
    "mcp_servers",
    "context_management",
    "compaction",
    "inference_geo",
    "speed",
    "fallbacks",
    "fallback_credit_token",
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
        Some("text") => &["type", "text", "citations", "cache_control"],
        Some("thinking") => &["type", "thinking", "signature"],
        Some("redacted_thinking") => &["type", "data"],
        Some("tool_use") => &[
            "type",
            "id",
            "name",
            "input",
            "caller",
            "toolset_name",
            "cache_control",
        ],
        Some("tool_result") => &[
            "type",
            "tool_use_id",
            "content",
            "is_error",
            "toolset_name",
            "cache_control",
        ],
        Some("image") => &["type", "source", "transformations", "cache_control"],
        Some("tool_addition" | "tool_removal") => &["type", "tool", "cache_control"],
        // `context` has a per-use owner; keep `citations` so active citation
        // requests reject instead of silently changing model-visible content.
        Some("document") => &[
            "type",
            "source",
            "title",
            "context",
            "citations",
            "cache_control",
        ],
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
fn blocks(v: &mut Value, ignored: &mut BTreeSet<String>) -> Result<(), CodecError> {
    if let Some(list) = v.as_array_mut() {
        for block in list {
            if block.get("cache_control").is_some()
                && !matches!(
                    block.get("type").and_then(Value::as_str),
                    Some(
                        "text"
                            | "image"
                            | "document"
                            | "tool_use"
                            | "tool_result"
                            | "tool_addition"
                            | "tool_removal"
                    )
                )
            {
                return Err(CodecError::Unsupported(
                    "Anthropic content cache breakpoint".into(),
                ));
            }
            let kind = block.get("type").and_then(Value::as_str).map(str::to_owned);
            keep(block, block_fields(kind.as_deref()), ignored);
            if let Some(cache) = block.get_mut("cache_control") {
                keep(cache, &["type", "ttl"], ignored);
            }
            if kind.as_deref() == Some("tool_result")
                && let Some(content) = block.get_mut("content")
            {
                blocks(content, ignored)?;
            }
            if let Some(tool) = block.get_mut("tool") {
                keep(tool, &["type", "name", "definition"], ignored);
                if let Some(definition) = tool.get_mut("definition") {
                    tool_fields(definition, ignored)?;
                }
            }
        }
    }
    Ok(())
}
/// Close a tool definition without filtering examples or other known model input.
fn tool_fields(t: &mut Value, ignored: &mut BTreeSet<String>) -> Result<(), CodecError> {
    if t.get("allowed_callers").is_some() {
        return Err(CodecError::Unsupported("Anthropic tool dispatch".into()));
    }
    keep(t, wire::request::TOOL_FIELDS, ignored);
    if let Some(cache) = t.get_mut("cache_control") {
        keep(cache, &["type", "ttl"], ignored);
    }
    Ok(())
}

/// Decode a client request; the model is a public label, never an upstream name.
pub fn decode_client_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let mut v = parse(bytes)?;
    let o = object(&v)?;
    if let Some(key) = UNSUPPORTED.iter().find(|key| {
        o.get(**key)
            .is_some_and(|v| !v.is_null() && !(**key == "mcp_servers" && v == &json!([])))
    }) {
        return Err(CodecError::Unsupported((*key).into()));
    }
    let mut ignored = BTreeSet::new();
    keep(&mut v, TOP, &mut ignored);
    if let Some(cache) = v.get_mut("cache_control") {
        keep(cache, &["type", "ttl"], &mut ignored);
    }
    if let Some(metadata) = v.get_mut("metadata") {
        keep(metadata, &["user_id"], &mut ignored);
    }
    let o = object(&v)?.clone();
    let model = string(required(&o, "model")?)?;
    label(&model)?;
    let mut settings = GenerationSettings::default();
    settings.controls.stop_sequences =
        crate::protocol::generation_controls::stops(o.get("stop_sequences"), false, MAX_ITEMS)?;
    if let Some(cache) = o.get("cache_control") {
        settings.automatic_prefix_cache = Some(wire::request::CacheControl::read(cache)?.ttl());
    }
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
        if let Some(value) = wire::request::sampling(&o, key, 1.0)? {
            settings.controls = apply(settings.controls, value)?;
        }
    }
    // The IR owns top_k; Messages targets that reject it do so at projection.
    if let Some(value) = o.get("top_k").filter(|v| !v.is_null()) {
        settings.controls.top_k = Some(
            value
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(CodecError::Invalid("top_k"))?,
        );
    }
    let scope = LocalScope::ROOT;
    let mut intake = Intake::new(scope);
    if let Some(mut system) = o.get("system").filter(|v| !v.is_null()).cloned() {
        blocks(&mut system, &mut ignored)?;
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
                            Ok(Part {
                                replay: None,
                                id: PartId::scoped(scope, i as u64 + 1),
                                content: text(&s.content)?,
                                cache: s.cache,
                            })
                        })
                        .collect::<Result<_, CodecError>>()?,
                }),
            )),
            WireText::Blocks(_) => {}
        }
    }
    if let Some(tools) = o.get("tools").filter(|v| !v.is_null()) {
        let tools = tools.as_array().ok_or(CodecError::Invalid("tools"))?;
        settings.tools = Some(
            tools
                .iter()
                .map(|t| {
                    let mut t = t.clone();
                    tool_fields(&mut t, &mut ignored)?;
                    let tool = Tool::read(&t)?;
                    tool.bytes()?;
                    crate::protocol::anthropic::messages::items::function_tool(&tool)
                        .map(ToolDefinition::Function)
                })
                .collect::<Result<_, CodecError>>()?,
        );
        wire::request::validate_cache_breakpoints(settings.tools.iter().flatten().filter_map(
            |tool| match tool {
                ToolDefinition::Function(tool) => tool.cache,
                _ => None,
            },
        ))?;
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
            &["type", "budget_tokens", "display", "block_binding"],
            &mut ignored,
        );
        if let Some(binding) = thinking.get_mut("block_binding") {
            keep(binding, &["prefix_mismatch_behavior"], &mut ignored);
        }
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
                if settings
                    .controls
                    .max_output_tokens
                    .is_some_and(|cap| budget >= cap)
                {
                    return Err(CodecError::Invalid("thinking budget"));
                }
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
        settings.reasoning.presentation.visibility = settings::display(&shape::presence(
            t,
            "display",
            wire::request::Display::read,
        )?);
        if settings.reasoning.presentation.visibility == Presence::Null {
            return Err(CodecError::Invalid("thinking display"));
        }
        let binding = wire::request::PrefixMismatch::read(t.get("block_binding"))?;
        if binding.is_some() && settings.reasoning.mode != Presence::Value(ReasoningMode::Adaptive)
        {
            return Err(CodecError::Invalid("thinking.block_binding"));
        }
        settings.reasoning.replay_mismatch =
            binding.map(crate::protocol::anthropic::messages::settings::replay_mismatch);
    }
    if let Some(config) = o.get("output_config").filter(|v| !v.is_null()) {
        let mut config = config.clone();
        keep(
            &mut config,
            &["effort", "format", "task_budget"],
            &mut ignored,
        );
        if let Some(format) = config.get_mut("format") {
            keep(format, &["type", "schema"], &mut ignored);
        }
        crate::protocol::anthropic::messages::settings::output_settings(
            &OutputConfig::read(&config)?,
            &mut settings,
        );
    }
    let messages = required(&o, "messages")?
        .as_array()
        .ok_or(CodecError::Invalid("messages"))?;
    let mut roles = Vec::with_capacity(messages.len());
    let mut attachments = vec![];
    for message in messages {
        let mut message = message.clone();
        // Turn-scoped clearing changes the model contract.
        if message.get("clear_at").is_some_and(|v| !v.is_null()) {
            return Err(CodecError::Unsupported("message clear_at".into()));
        }
        keep(
            &mut message,
            &["role", "content", "output_config"],
            &mut ignored,
        );
        if let Some(config) = message.get_mut("output_config") {
            keep(config, &["effort"], &mut ignored);
        }
        let m = object(&message)?;
        let role = Role::read(required(m, "role")?)?;
        let content = required(m, "content")?;
        let effort = wire::request::read_message_effort(m)?;
        if role == Role::System {
            // An empty string renders nothing, as an empty block list does.
            let parsed: Vec<_> = if content.is_string() {
                let s = string(content)?;
                (!s.is_empty())
                    .then(|| wire::request::CacheMarked {
                        content: Block::Text(s),
                        cache: None,
                    })
                    .into_iter()
                    .collect()
            } else {
                let mut content = content.clone();
                blocks(&mut content, &mut ignored)?;
                content
                    .as_array()
                    .ok_or(CodecError::Invalid("content"))?
                    .iter()
                    .map(|v| wire::request::CacheMarked::read(v, Block::read))
                    .collect::<Result<_, _>>()?
            };
            roles.push((role, !parsed.is_empty()));
            intake.system_message(&parsed, effort)?;
            continue;
        }
        if effort.is_some() {
            return Err(CodecError::Invalid("Anthropic message output_config"));
        }
        roles.push((role, true));
        let parsed = if content.is_string() {
            vec![wire::request::CacheMarked {
                content: Block::Text(string(content)?),
                cache: None,
            }]
        } else {
            let mut content = content.clone();
            blocks(&mut content, &mut ignored)?;
            content
                .as_array()
                .ok_or(CodecError::Invalid("content"))?
                .iter()
                .map(|v| wire::request::CacheMarked::read(v, Block::read))
                .collect::<Result<_, _>>()?
        };
        let start = intake.items.len();
        let mut marker = None;
        let mut content = Vec::new();
        for block in parsed {
            if let Some(carried) = crate::protocol::attachments::messages_marker(&block.content) {
                use crate::protocol::attachments::Carried;
                marker = Some(
                    if role != Role::Assistant || marker.is_some() || block.cache.is_some() {
                        Carried::Unusable
                    } else {
                        carried
                    },
                );
            } else {
                content.push(block);
            }
        }
        // A carrier-only assistant message has no visible content of its own.
        if marker.is_none() || role != Role::Assistant || !content.is_empty() {
            intake.request_message(role, &content)?;
        }
        if let Some(marker) = marker {
            attachments.push(crate::protocol::attachments::Marker::bounded(
                marker,
                start,
                intake.items.len(),
            ));
        }
    }
    wire::request::validate_system_placement(&roles)?;
    // Messages turns are not kept as envelopes: each target regroups items its own
    // way (native by role, Chat by assistant text and calls).
    // Calls carry their input as raw JSON text, as every client protocol's calls do.
    let mut items = intake
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
    let mut fidelity = FidelityRecords::default();
    let mut groups = vec![];
    crate::protocol::attachments::restore(&mut items, &mut groups, &mut fidelity, attachments)?;
    let semantic = GenerationRequest::from_resources(items, settings, intake.resources)?
        .with_message_envelopes(groups)?;
    crate::protocol::anthropic::messages::items::validate_cache(&semantic)?;
    fidelity.bind_replay_origin(&crate::execution::replay::CLIENT_ORIGIN)?;
    let mut request = Request::from_generation(DecodedRequest { semantic, fidelity }, &model)?;
    request.context.identity.user = wire::request::read_user_id(&o)?;
    request.service_policy = service::read_policy(&o)?;
    request.source = Some(crate::adapter::ClientProtocol::Messages);
    if let Some(stream) = o.get("stream").filter(|v| !v.is_null()) {
        request.delivery.stream =
            Presence::Value(stream.as_bool().ok_or(CodecError::Invalid("stream"))?);
    }
    request.ignored_fields = ignored.into_iter().collect();
    Ok(request)
}
