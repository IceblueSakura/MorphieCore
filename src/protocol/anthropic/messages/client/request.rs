//! Untrusted client Messages: registered fields, protected controls and IR admission.
use crate::{
    lowering::disposition::{OMIT_REPLAY_MISMATCH, Rule},
    protocol::{
        CodecError, DecodedRequest, GenerationRequestEnvelope as Request,
        anthropic::messages::{
            Block, OutputConfig, Role, Selection, TextContent as WireText, Tool,
            ToolChoice as WireChoice,
            items::{Intake, text},
            service, settings,
            shape::{self, label, object, parse, required, string},
            wire,
        },
        conversion::generation::{ConversionBoundary, EvidenceDelta},
        failure::CarrierReason,
        fidelity::FidelityRecords,
    },
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
        Some("tool_addition" | "tool_removal") => &["type", "tool", "cache_control"],
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
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            let kind = block.get("type").and_then(Value::as_str).map(str::to_owned);
            keep(block, block_fields(kind.as_deref()), ignored);
            if let Some(cache) = block.get_mut("cache_control") {
                keep(cache, &["type", "ttl"], ignored);
            }
            if matches!(kind.as_deref(), Some("tool_addition" | "tool_removal"))
                && let Some(tool) = block.get_mut("tool")
            {
                match tool.get("type").and_then(Value::as_str) {
                    Some("tool_reference") => keep(tool, &["type", "name"], ignored),
                    Some("tool_definition") => {
                        keep(tool, &["type", "definition"], ignored);
                        if let Some(definition) = tool.get_mut("definition") {
                            tool_fields(definition, ignored)?;
                        }
                    }
                    _ => {}
                }
            }
            if kind.as_deref() == Some("tool_result")
                && let Some(content) = block.get_mut("content")
            {
                blocks(content, ignored)?;
            }
        }
    }
    Ok(())
}
/// Close a tool definition without filtering examples or other known model input.
fn tool_fields(t: &mut Value, ignored: &mut BTreeSet<String>) -> Result<(), CodecError> {
    keep(t, wire::request::TOOL_FIELDS, ignored);
    if let Some(cache) = t.get_mut("cache_control") {
        keep(cache, &["type", "ttl"], ignored);
    }
    Ok(())
}

/// Check exact JSON digits without rounding or expanding untrusted exponents.
fn sampling_number(value: &Value, integral: bool) -> bool {
    let Some(number) = value.as_number() else {
        return false;
    };
    let raw = number.as_str();
    let unsigned = raw.strip_prefix('-').unwrap_or(raw);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    let point = mantissa.find('.').unwrap_or(mantissa.len());
    let digits = mantissa.replace('.', "");
    let significant = digits.trim_start_matches('0');
    let leading = digits.len() - significant.len();
    let significant = significant.trim_end_matches('0');
    if significant.is_empty() {
        return true;
    }
    if raw.starts_with('-') {
        return false;
    }
    let sign = if exponent.starts_with('-') { -1 } else { 1 };
    // Saturation retains exact comparisons with these small bounds even when
    // a valid JSON exponent exceeds the machine integer range.
    let exponent = exponent
        .trim_start_matches(['-', '+'])
        .bytes()
        .fold(0_i64, |n, digit| {
            n.saturating_mul(10)
                .saturating_add(i64::from(digit - b'0') * sign)
        });
    let order = (point as i64 - leading as i64).saturating_add(exponent);
    if integral && significant.len() as i64 > order {
        return false;
    }
    let maximum = if integral {
        "18446744073709551615"
    } else {
        "1"
    };
    // Both significands have no trailing zeros, so lexical order is exact
    // when their decimal orders agree.
    order < maximum.len() as i64 || order == maximum.len() as i64 && significant <= maximum
}

/// Decode a client request; the model is a public label, never an upstream name.
pub fn decode_client_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let mut v = parse(bytes)?;
    let mut evidence = EvidenceDelta::default();
    // Legacy sampling is advisory; malformed known values still reject.
    let mut removed_sampling = false;
    for name in ["temperature", "top_p", "top_k"] {
        if let Some(value) = v.get(name).filter(|v| !v.is_null()) {
            if !sampling_number(value, name == "top_k") {
                return Err(CodecError::invalid());
            }
            removed_sampling = true;
        }
        if let Some(o) = v.as_object_mut() {
            o.remove(name);
        }
    }
    if removed_sampling {
        evidence
            .push(Rule::OMIT_MESSAGES_REMOVED_SAMPLING.effect(ConversionBoundary::RequestDecode));
    }
    let o = object(&v)?;
    wire::request::validate_request_controls(o)?;
    for field in [
        "system",
        "tools",
        "tool_choice",
        "thinking",
        "output_config",
        "stream",
    ] {
        if o.get(field).is_some_and(Value::is_null) {
            return Err(CodecError::invalid());
        }
    }
    if let Some(_key) = UNSUPPORTED.iter().find(|key| {
        o.get(**key)
            .is_some_and(|v| !v.is_null() && !(**key == "mcp_servers" && v == &json!([])))
    }) {
        return Err(CodecError::Unsupported(CarrierReason::UnknownField));
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
            .ok_or(CodecError::invalid())?,
    );
    let scope = LocalScope::ROOT;
    let mut intake = Intake::new(scope);
    if let Some(mut system) = o.get("system").cloned() {
        blocks(&mut system, &mut ignored)?;
        settings.instructions = Presence::Value(
            crate::protocol::anthropic::messages::items::root_instructions(WireText::read(
                &system,
            )?)?,
        );
    }
    if let Some(tools) = o.get("tools") {
        let tools = tools.as_array().ok_or(CodecError::invalid())?;
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
    if let Some(choice) = o.get("tool_choice") {
        let mut choice = choice.clone();
        keep(
            &mut choice,
            &["type", "name", "disable_parallel_tool_use"],
            &mut ignored,
        );
        let choice = WireChoice::read(&choice)?;
        settings.tool_choice = Some(match &choice.selection {
            Selection::Auto => ToolChoice::Auto,
            Selection::None => ToolChoice::None,
            Selection::Any => ToolChoice::Required,
            Selection::Tool(s) => ToolChoice::Specific(text(s)?),
        });
        settings.parallel_tool_calls = choice.disable_parallel_tool_use.value().map(|b| !b);
    }
    if let Some(thinking) = o.get("thinking") {
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
        if t.contains_key("block_binding") {
            if t.get("type").and_then(Value::as_str) != Some("adaptive") {
                return Err(CodecError::invalid());
            }
            match wire::request::PrefixMismatch::read(t.get("block_binding"))? {
                Some(wire::request::PrefixMismatch::Error) => {
                    return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                }
                Some(wire::request::PrefixMismatch::DropBlock) => {
                    evidence.push(OMIT_REPLAY_MISMATCH.effect(ConversionBoundary::RequestDecode));
                }
                None => {}
            }
        }
        match t.get("type").and_then(Value::as_str) {
            Some("enabled") => {
                let budget = t
                    .get("budget_tokens")
                    .and_then(Value::as_u64)
                    .filter(|n| *n >= 1024)
                    .ok_or(CodecError::invalid())?;
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
                settings.reasoning.budget_tokens = Some(budget);
                if settings
                    .controls
                    .max_output_tokens
                    .is_some_and(|cap| budget >= cap)
                {
                    return Err(CodecError::invalid());
                }
            }
            Some("adaptive") => {
                if t.contains_key("budget_tokens") {
                    return Err(CodecError::invalid());
                }
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Enabled);
                settings.reasoning.mode = Presence::Value(ReasoningMode::Adaptive);
            }
            Some("disabled") => {
                if t.contains_key("budget_tokens") || t.contains_key("display") {
                    return Err(CodecError::invalid());
                }
                settings.reasoning = ReasoningRequest::present(None);
                settings.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
            }
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning));
            }
        }
        settings.reasoning.presentation.visibility = settings::display(&shape::presence(
            t,
            "display",
            wire::request::Display::read,
        )?);
        if settings.reasoning.presentation.visibility == Presence::Null {
            return Err(CodecError::invalid());
        }
    }
    if let Some(config) = o.get("output_config") {
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
        .ok_or(CodecError::invalid())?;
    let mut roles = Vec::with_capacity(messages.len());
    let mut parsed_messages = Vec::with_capacity(messages.len());
    for message in messages {
        let mut message = message.clone();
        // Turn-scoped clearing changes the model contract.
        if message.get("clear_at").is_some_and(|v| !v.is_null()) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if message.get("output_config").is_some() {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        keep(&mut message, &["role", "content"], &mut ignored);
        let m = object(&message)?;
        let role = Role::read(required(m, "role")?)?;
        let content = required(m, "content")?;
        let effort = None;
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
                    .ok_or(CodecError::invalid())?
                    .iter()
                    .map(|v| wire::request::CacheMarked::read(v, Block::read))
                    .collect::<Result<_, _>>()?
            };
            roles.push((role, !parsed.is_empty()));
            wire::request::input_block_bytes(role, &parsed)?;
            intake.system_message(&parsed, effort)?;
            parsed_messages.push(wire::request::InputMessage {
                role,
                content: wire::request::MessageContent::Blocks(parsed),
                effort,
            });
            continue;
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
                .ok_or(CodecError::invalid())?
                .iter()
                .map(|v| wire::request::CacheMarked::read(v, Block::read))
                .collect::<Result<_, _>>()?
        };
        if !wire::request::input_block_bytes(role, &parsed)?.1 {
            return Err(CodecError::invalid());
        }
        if parsed.iter().any(|block| match &block.content {
            Block::Thinking { signature, .. } | Block::RedactedThinking(signature) => {
                crate::protocol::replay_projection::is_marker(signature)
            }
            _ => false,
        }) {
            return Err(CodecError::Unsupported(CarrierReason::Replay));
        }
        intake.request_message(role, &parsed)?;
        parsed_messages.push(wire::request::InputMessage {
            role,
            content: wire::request::MessageContent::Blocks(parsed),
            effort,
        });
    }
    wire::request::validate_system_placement(&roles)?;
    let selection = settings.tool_choice.as_ref().map(|c| match c {
        ToolChoice::Required => Selection::Any,
        ToolChoice::Specific(name) => Selection::Tool(name.as_str().into()),
        ToolChoice::None => Selection::None,
        _ => Selection::Auto,
    });
    wire::request::validate_turns(
        &parsed_messages,
        settings.reasoning.mode == Presence::Value(ReasoningMode::Budgeted),
        selection.as_ref(),
        false,
    )?;
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
            (id, item)
        })
        .collect();
    let mut fidelity = FidelityRecords::default();
    let semantic = GenerationRequest::from_resources(items, settings, intake.resources)?;
    crate::protocol::anthropic::messages::items::validate_cache(&semantic)?;
    fidelity.bind_replay_origin(&crate::protocol::replay_envelope::CLIENT_ORIGIN)?;
    let mut request = Request::from_generation(
        DecodedRequest {
            evidence,
            semantic,
            fidelity,
        },
        &model,
    )?;
    request.context.identity.user = wire::request::read_user_id(&o)?;
    request.service_policy = service::read_policy(&o)?;
    request.source = Some(crate::protocol::ClientProtocol::Messages);
    if let Some(stream) = o.get("stream") {
        request.delivery.stream = Presence::Value(stream.as_bool().ok_or(CodecError::invalid())?);
    }
    request.ignored_fields = ignored.into_iter().collect();
    Ok(request)
}
