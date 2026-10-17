//! Chat request mapping, including explicit assistant message ownership for tool calls.
pub(in crate::protocol::openai) mod annotations;
pub(in crate::protocol::openai) mod audio;
pub mod envelope;
pub(crate) mod logprobs;
pub(in crate::protocol::openai) mod owners;
pub(in crate::protocol::openai) mod reasoning;
pub mod sse;
pub use crate::protocol::openai::static_response::{
    decode_chat as decode_response, encode_chat as encode_response,
};
use crate::protocol::openai::{
    CodecError, DecodedRequest, Profile, RequestRepresentation, common::*, function_tools,
};
use crate::semantic::task::generation::*;
use crate::semantic::value::Presence;
use serde_json::{Map, Value, json};

pub(super) const FIELDS: &[&str] = &[
    "messages",
    "temperature",
    "top_p",
    "max_completion_tokens",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "reasoning_effort",
    "response_format",
    "logprobs",
    "top_logprobs",
    "audio",
    "modalities",
    "verbosity",
    "stop",
    "functions",
    "function_call",
    "web_search_options",
    "prediction",
    "seed",
    "frequency_penalty",
    "presence_penalty",
    "logit_bias",
];
/// Official fields whose active value changes tools or output; ignoring them would be
/// silent loss. Other unregistered client fields are ignored by name.
const UNSUPPORTED: &[&str] = &["functions", "function_call", "web_search_options"];
pub fn decode_generation(v: &Value) -> Result<DecodedRequest, CodecError> {
    decode_generation_with(v, &Default::default(), DecodeDirection::Request)
}
pub(crate) fn decode_generation_with(
    v: &Value,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    direction: DecodeDirection<'_>,
) -> Result<DecodedRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    direction.fields(o, FIELDS)?;
    if let Some(key) = ["logit_bias"].into_iter().find(|key| o.contains_key(*key)) {
        return Err(CodecError::Unsupported(key.into()));
    }
    if let Some(key) = UNSUPPORTED.iter().find(|key| {
        o.get(**key)
            .is_some_and(|v| !v.is_null() && v != &json!([]) && v != &json!(""))
    }) {
        return Err(CodecError::Unsupported((*key).into()));
    }
    let messages = o
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("messages"))?;
    let mut b = Items::new(direction);
    if let Some(prediction) = o.get("prediction").filter(|v| !v.is_null()) {
        let prediction = object(prediction)?;
        direction.fields(prediction, &["type", "content"])?;
        if string(prediction, "type")? != "content" {
            return Err(CodecError::Invalid("prediction type"));
        }
        let content = prediction
            .get("content")
            .ok_or(CodecError::Invalid("prediction content"))?;
        match content {
            Value::String(_) => {
                raw_string(prediction, "content")?;
            }
            Value::Array(_) => {
                request_text_parts(content, direction)?;
            }
            _ => return Err(CodecError::Invalid("prediction content")),
        }
        b.fidelity.record_prediction_omission();
    }
    for message in messages {
        let message = crate::protocol::openai::chat::reasoning::decode_static(
            &mut b,
            object(message)?,
            adaptation,
        )?;
        decode_message(&mut b, &message, true)?;
    }
    let mut controls = controls(o, "max_completion_tokens")?;
    controls.stop_sequences = crate::protocol::generation_controls::stops(o.get("stop"), true, 4)?;
    controls.seed = o
        .get("seed")
        .filter(|v| !v.is_null())
        .map(|v| v.as_i64().ok_or(CodecError::Invalid("seed")))
        .transpose()?;
    for (key, apply) in [
        (
            "presence_penalty",
            GenerationControls::with_presence_penalty as fn(_, f64) -> _,
        ),
        (
            "frequency_penalty",
            GenerationControls::with_frequency_penalty,
        ),
    ] {
        if let Some(value) = o.get(key).filter(|v| !v.is_null()) {
            let value = value
                .as_f64()
                .filter(|v| (-2.0..=2.0).contains(v))
                .ok_or(CodecError::Invalid("penalty"))?;
            controls = apply(controls, value)?;
        }
    }
    if let Some(v) = o.get("top_p").filter(|v| !v.is_null()) {
        controls = controls.with_top_p(v.as_f64().ok_or(CodecError::Invalid("top_p"))?)?;
    }
    controls.logprobs = read_presence(o, "logprobs", |v| {
        v.as_bool().ok_or(CodecError::Invalid("logprobs"))
    })?;
    controls.top_logprobs = o
        .get("top_logprobs")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_u64()
                .filter(|n| *n <= 20)
                .map(|n| n as u8)
                .ok_or(CodecError::Invalid("top_logprobs"))
        })
        .transpose()?;
    if controls.top_logprobs.is_some() && controls.logprobs != Presence::Value(true) {
        return Err(CodecError::Invalid("top_logprobs without logprobs"));
    }
    let format = read_presence(o, "response_format", |v| read_response_format(v, direction))?;
    let verbosity = read_presence(o, "verbosity", crate::protocol::openai::settings::verbosity)?;
    let mut settings = GenerationSettings {
        controls,
        text: TextOptions {
            presence: !format.is_absent() || !verbosity.is_absent(),
            format,
            verbosity,
        },
        ..Default::default()
    };
    crate::protocol::openai::reasoning::chat_request(o, &mut settings)?;
    crate::protocol::openai::chat::audio::settings(o, &mut settings, direction)?;
    let groups = owners::envelopes(&b.message_owners)?;
    let r = GenerationRequest::from_resources(b.items, settings, b.resources)?
        .with_message_envelopes(groups)?;
    Ok(DecodedRequest {
        semantic: function_tools::decode(r, o, Profile::Chat, direction)?,
        fidelity: b.fidelity,
    })
}
/// Chat nests the json_schema body under `json_schema`; the flat Responses shell is parsed apart.
fn read_response_format(
    v: &Value,
    direction: DecodeDirection<'_>,
) -> Result<OutputConstraint, CodecError> {
    let o = object(v)?;
    Ok(match string(o, "type")? {
        "text" => {
            direction.fields(o, &["type"])?;
            OutputConstraint::Text
        }
        "json_object" => {
            direction.fields(o, &["type"])?;
            OutputConstraint::JsonObject
        }
        "json_schema" => {
            direction.fields(o, &["type", "json_schema"])?;
            let body = object(
                o.get("json_schema")
                    .ok_or(CodecError::Invalid("json_schema"))?,
            )?;
            direction.fields(body, &["name", "description", "schema", "strict"])?;
            crate::protocol::openai::settings::read_schema_body(body)?
        }
        _ => return Err(CodecError::Unsupported("response format".into())),
    })
}
fn write_response_format(f: &OutputConstraint) -> Result<Value, CodecError> {
    Ok(match f {
        OutputConstraint::Text => json!({"type":"text"}),
        OutputConstraint::JsonObject => json!({"type":"json_object"}),
        OutputConstraint::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let mut body = Map::new();
            crate::protocol::openai::settings::write_schema_body(
                &mut body,
                name.as_ref(),
                description.as_ref(),
                schema,
                *strict,
            )?;
            json!({"type":"json_schema","json_schema":Value::Object(body)})
        }
    })
}
/// Request-only text arrays preserve ordered part ownership; canonical single parts
/// encode as strings. This does not admit response arrays or media placeholders.
fn request_text_parts<'a>(
    value: &'a Value,
    direction: DecodeDirection<'_>,
) -> Result<Vec<&'a str>, CodecError> {
    match value {
        Value::String(text) => Ok(vec![text]),
        Value::Array(parts) if !parts.is_empty() => parts
            .iter()
            .map(|part| {
                let part = object(part)?;
                direction.fields(part, &["type", "text"])?;
                if string(part, "type")? != "text" {
                    return Err(CodecError::Unsupported("message content part".into()));
                }
                string(part, "text")
            })
            .collect(),
        _ => Err(CodecError::Invalid("content")),
    }
}
pub(super) fn decode_message(
    b: &mut Items,
    m: &Map<String, Value>,
    replay: bool,
) -> Result<(), CodecError> {
    let direction = b.direction;
    let role = string(m, "role")?;
    reject_chat_orchestration(m)?;
    if m.get("function_call").is_some_and(|v| !v.is_null()) {
        return Err(CodecError::Unsupported("legacy function_call".into()));
    }
    if role != "assistant"
        && ["audio", "annotations", "refusal", "tool_calls"]
            .iter()
            .any(|key| m.get(*key).is_some_and(|v| !v.is_null()))
    {
        return Err(CodecError::Invalid("assistant-only message content"));
    }
    // A shared field superset would silently erase assistant-only values on
    // instruction/tool messages. Validate the role's shell before normalization.
    let allowed: &[&str] = match role {
        "assistant" if replay => &[
            "role",
            "content",
            "tool_calls",
            "refusal",
            "reasoning_content",
            "parsed",
            "annotations",
            "audio",
        ],
        "assistant" => &[
            "role",
            "content",
            "tool_calls",
            "refusal",
            "reasoning_content",
            "annotations",
            "audio",
        ],
        "tool" => &["role", "content", "tool_call_id"],
        "system" | "developer" | "user" => &["role", "content"],
        _ => return Err(CodecError::Unsupported("message role".into())),
    };
    direction.fields(m, allowed)?;
    if replay && role != "assistant" && m.contains_key("parsed") {
        // The pinned SDK attaches its parsed view to assistant messages only.
        return Err(CodecError::Invalid("parsed"));
    }
    // Readable provider thinking text on an assistant message maps to a
    // reasoning item ordered immediately ahead of its carrier message. Absent,
    // null and empty carry no reasoning; anything else must be a string.
    let reasoning = match (role, m.get("reasoning_content")) {
        (_, None) | (_, Some(Value::Null)) => None,
        ("assistant", Some(Value::String(s))) if s.is_empty() => None,
        ("assistant", Some(Value::String(s))) => Some(
            crate::semantic::value::Text::new(s, "reasoning_content", MAX_TEXT_BYTES)
                .map_err(|_| CodecError::Limit)?,
        ),
        _ => return Err(CodecError::Invalid("reasoning_content")),
    };
    if let Some(text) = reasoning {
        let part_id = b.part_id()?;
        let rid = b.id()?;
        b.items.push((
            rid,
            Item::Reasoning(ReasoningItem {
                parts: vec![(part_id, ReasoningContent::Text(text))],
                status: ItemLifecycle::Completed,
                replay: None,
            }),
        ));
    }
    let id = b.id()?;
    match role {
        "tool" => {
            if m.contains_key("tool_calls") {
                return Err(CodecError::Invalid("tool result"));
            }
            // Tool results own a string or an ordered text array, including an
            // explicit empty array. Never concatenate parts or parse them as arguments.
            let output = match m.get("content") {
                Some(Value::String(_)) => raw_string(m, "content")?.into(),
                Some(v @ Value::Array(parts)) => {
                    let texts = if parts.is_empty() {
                        vec![]
                    } else {
                        request_text_parts(v, direction)?
                    };
                    let mut parts = Vec::new();
                    for value in texts {
                        parts.push(Part::new(
                            b.part_id()?,
                            InputPart::Text(
                                crate::semantic::value::Text::allowing_empty(
                                    value,
                                    "tool output",
                                    MAX_TEXT_BYTES,
                                )
                                .map_err(|_| CodecError::Limit)?,
                            ),
                        ));
                    }
                    ToolOutput::Parts(parts)
                }
                _ => return Err(CodecError::Invalid("tool content")),
            };
            let call_id = text(string(m, "tool_call_id")?, "call_id", 256)?;
            // The result shell has no kind; its preceding typed call owns that fact.
            let custom = b.items.iter().any(|(_, item)| {
                matches!(item,
                Item::CustomCall(call) if call.call_id == call_id)
            });
            let result = ToolResult {
                cache: None,
                is_error: None,
                execution: None,
                call_id: Some(call_id),
                name: None,
                output,
                status: None,
                context: CallContext::default(),
            };
            b.items.push((
                id,
                if custom {
                    Item::CustomResult(result)
                } else {
                    Item::ToolResult(result)
                },
            ));
        }
        "system" | "developer" => {
            if m.contains_key("tool_calls") || m.contains_key("tool_call_id") {
                return Err(CodecError::Invalid("instruction tool fields"));
            }
            let mut parts = Vec::new();
            for text in request_text_parts(
                m.get("content").ok_or(CodecError::Invalid("content"))?,
                direction,
            )? {
                parts.push(Part::new(
                    b.part_id()?,
                    crate::semantic::value::Text::allowing_empty(
                        text,
                        "instruction",
                        MAX_TEXT_BYTES,
                    )
                    .map_err(|_| CodecError::Limit)?,
                ));
            }
            b.items.push((
                id,
                Item::Instruction(Instruction {
                    authority: if role == "system" {
                        InstructionAuthority::System
                    } else {
                        InstructionAuthority::Developer
                    },
                    parts,
                    status: None,
                }),
            ));
        }
        "user" | "assistant" => {
            if m.contains_key("tool_call_id") || (role == "user" && m.contains_key("tool_calls")) {
                return Err(CodecError::Invalid("message tool fields"));
            }
            if replay {
                crate::protocol::openai::common::admit_parsed(
                    "parsed",
                    m.get("parsed"),
                    match m.get("content") {
                        Some(Value::String(s)) => Some(s.as_str()),
                        _ => None,
                    },
                )?;
            }
            let calls = m
                .get("tool_calls")
                .filter(|v| !v.is_null())
                .map(|v| v.as_array().ok_or(CodecError::Invalid("tool_calls")))
                .transpose()?
                // An explicitly empty list declares no calls and normalizes to
                // absent, matching the streaming path (ADR 0008).
                .filter(|calls| !calls.is_empty());
            let refusal = match m.get("refusal") {
                None | Some(Value::Null) => None,
                Some(Value::String(value)) if role == "assistant" => Some(
                    crate::semantic::value::Text::allowing_empty(value, "refusal", MAX_TEXT_BYTES)
                        .map_err(|_| CodecError::Limit)?,
                ),
                _ => return Err(CodecError::Invalid("refusal")),
            };
            let mut parts = match (m.get("content"), refusal) {
                (_, Some(_)) if calls.is_some() => {
                    return Err(CodecError::Invalid("refusal"));
                }
                (Some(value), Some(_)) if !value.is_null() && value != "" => {
                    return Err(CodecError::Invalid("refusal"));
                }
                (_, Some(refusal)) => vec![b.refusal(refusal)?],
                (Some(Value::String(s)), None) => vec![b.part(s)?],
                (Some(Value::Array(values)), None) if replay && role == "user" => {
                    if values.is_empty() || values.len() > MAX_ITEMS {
                        return Err(CodecError::Invalid("user content"));
                    }
                    let mut parts = Vec::new();
                    for value in values {
                        let part = object(value)?;
                        match string(part, "type")? {
                            "text" => {
                                direction.fields(part, &["type", "text"])?;
                                parts.push(b.part(string(part, "text")?)?);
                            }
                            "image_url" => parts.push(Part {
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::image::read(
                                        part,
                                        Profile::Chat,
                                        direction,
                                    )?,
                                )?),
                            }),
                            "video_url" => parts.push(Part {
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::video::read(part, direction)?,
                                )?),
                            }),
                            "file" => parts.push(Part {
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::file::read_chat(
                                        part, direction,
                                    )?,
                                )?),
                            }),
                            "input_audio" => parts.push(Part {
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::input_audio::read(
                                        part, direction,
                                    )?,
                                )?),
                            }),
                            _ => return Err(CodecError::Unsupported("user content part".into())),
                        }
                    }
                    parts
                }
                (Some(value @ Value::Array(_)), None) if replay => {
                    request_text_parts(value, direction)?
                        .into_iter()
                        .map(|text| b.part(text))
                        .collect::<Result<Vec<_>, _>>()?
                }
                (None | Some(Value::Null), None) if role == "assistant" => Vec::new(),
                _ => return Err(CodecError::Unsupported("message content".into())),
            };
            crate::protocol::openai::chat::annotations::attach(
                &mut parts,
                m.get("annotations"),
                b,
            )?;
            if let Some(audio) = m.get("audio").filter(|v| !v.is_null()) {
                if parts
                    .iter()
                    .any(|p| matches!(p.content, ContentPart::Refusal(_)))
                {
                    return Err(CodecError::Invalid("audio with refusal"));
                }
                parts.push(Part {
                    id: b.part_id()?,
                    content: crate::protocol::openai::chat::audio::read(audio, replay, direction)?,
                    cache: None,
                });
            }
            b.items.push((
                id,
                Item::Message(Message {
                    role: if role == "user" {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    parts,
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ));
            if let Some(calls) = calls {
                for (position, c) in calls.iter().enumerate() {
                    let call = object(c)?;
                    // The SDK's chat stream accumulation leaks the chunk index into
                    // message-level calls and providers echo it; whenever present it
                    // must agree with the message position.
                    match call.get("index") {
                        None | Some(Value::Null) => {}
                        Some(v) if v.as_u64() == Some(position as u64) => {}
                        _ => return Err(CodecError::Invalid("tool index")),
                    }
                    let item = if string(call, "type")? == "custom" {
                        reject_chat_orchestration(call)?;
                        direction.fields(call, &["id", "type", "custom", "index"])?;
                        if call.contains_key("function") {
                            return Err(CodecError::Invalid("mixed tool call"));
                        }
                        let custom = object(
                            call.get("custom")
                                .ok_or(CodecError::Invalid("custom call"))?,
                        )?;
                        reject_chat_orchestration(custom)?;
                        direction.fields(custom, &["name", "input"])?;
                        Item::CustomCall(CustomCall {
                            call_id: text(string(call, "id")?, "call id", 256)?,
                            name: text(string(custom, "name")?, "custom name", 128)?,
                            input: raw_string(custom, "input")?,
                            context: Default::default(),
                        })
                    } else {
                        if call.contains_key("custom") {
                            return Err(CodecError::Invalid("mixed tool call"));
                        }
                        Item::ToolCall(tool_call(call, Profile::Chat, None, replay, direction)?)
                    };
                    let cid = b.id()?;
                    b.message_owners.push((cid, id));
                    b.items.push((cid, item));
                }
            }
        }
        _ => return Err(CodecError::Unsupported("message role".into())),
    }
    Ok(())
}
pub fn encode_generation(target: &RequestRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Chat {
        return Err(CodecError::ProfileMismatch);
    }
    check_message_carriers(
        target.semantic.items(),
        target.semantic.message_envelopes(),
        Profile::Chat,
    )?;
    check_item_carriers(target.semantic.items())?;
    crate::protocol::openai::content::citations::check_items(
        target.semantic.items(),
        target.semantic.resources(),
        Profile::Chat,
    )?;
    target
        .fidelity
        .check_wire_item_ids(target.semantic.items(), false)?;
    if target.semantic.items().iter().any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result) if result.execution.is_some() || result.is_error.is_some() || !result.output.is_text_only())) {
        return Err(CodecError::Unsupported("tool result semantics".into()));
    }
    let mut messages = encode_items_with(
        target.semantic.items(),
        &owners::owners(target.semantic.items(), target.semantic.message_envelopes()),
        target.fidelity,
        target.adaptation.rules.structured_chat_reasoning,
        false,
        target.semantic.resources(),
    );
    if let Some(t) = target.semantic.instructions().value() {
        let role = if target.adaptation.rules.chat_system_instructions {
            "system"
        } else {
            "developer"
        };
        messages.insert(0, json!({"role":role,"content":t.as_str()}));
    }
    let mut v = json!({"messages": messages});
    let o = v.as_object_mut().expect("object literal");
    write_controls(
        &target.semantic,
        o,
        if target.adaptation.rules.legacy_max_tokens {
            "max_tokens"
        } else {
            "max_completion_tokens"
        },
    );
    if let Some(p) = target.semantic.controls().top_p() {
        o.insert("top_p".into(), json!(p));
    }
    let controls = target.semantic.controls();
    if !controls.stop_sequences.is_empty() {
        o.insert(
            "stop".into(),
            json!(
                controls
                    .stop_sequences
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
            ),
        );
    }
    if let Some(seed) = controls.seed {
        o.insert("seed".into(), seed.into());
    }
    if let Some(penalty) = controls.presence_penalty() {
        o.insert("presence_penalty".into(), penalty.into());
    }
    if let Some(penalty) = controls.frequency_penalty() {
        o.insert("frequency_penalty".into(), penalty.into());
    }
    put_presence(o, "logprobs", &target.semantic.controls().logprobs, |v| {
        json!(v)
    });
    if let Some(n) = target.semantic.controls().top_logprobs {
        o.insert("top_logprobs".into(), json!(n));
    }
    put_presence(
        o,
        "verbosity",
        &target.semantic.text_options().verbosity,
        |v| {
            json!(match v {
                Verbosity::Low => "low",
                Verbosity::Medium => "medium",
                Verbosity::High => "high",
            })
        },
    );
    match &target.semantic.text_options().format {
        crate::semantic::value::Presence::Absent => {}
        crate::semantic::value::Presence::Null => {
            o.insert("response_format".into(), Value::Null);
        }
        crate::semantic::value::Presence::Value(format) => {
            o.insert("response_format".into(), write_response_format(format)?);
        }
    }
    crate::protocol::openai::chat::audio::write_settings(target.semantic.settings(), o);
    function_tools::encode(&target.semantic, Profile::Chat, o)?;
    crate::protocol::openai::reasoning::write_request(
        target.semantic.settings(),
        o,
        Profile::Chat,
    )?;
    bounded(&v)?;
    Ok(v)
}
pub(super) fn call_wire(c: &ToolCall) -> Value {
    json!({"id":c.call_id.as_str(),"type":"function","function":{"name":c.name.as_str(),"arguments":c.arguments.as_raw().expect("checked raw arguments")}})
}
fn custom_call_wire(c: &CustomCall) -> Value {
    json!({"id":c.call_id.as_str(),"type":"custom","custom":{"name":c.name.as_str(),"input":c.input}})
}
pub(super) fn encode_items_with(
    items: &[(ItemId, Item)],
    owners: &std::collections::BTreeMap<ItemId, ItemId>,
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    structured: bool,
    response: bool,
    resources: &ResourceTable,
) -> Vec<Value> {
    let mut messages = Vec::<Value>::new();
    let mut standalone_calls = false;
    // Reasoning items wait for the message that carries them.
    let mut reasoning: Vec<(ItemId, &ReasoningItem)> = vec![];
    for (id, item) in items {
        match item {
            Item::Reasoning(r) => {
                standalone_calls = false;
                reasoning.push((*id, r));
            }
            Item::Instruction(i) => {
                standalone_calls = false;
                let content = if let [part] = i.parts.as_slice() {
                    json!(part.content.as_str())
                } else {
                    json!(
                        i.texts()
                            .map(|(_, t)| json!({"type":"text","text":t}))
                            .collect::<Vec<_>>()
                    )
                };
                messages.push(json!({"role":match i.authority { InstructionAuthority::System => "system", InstructionAuthority::Developer => "developer" },"content":content}));
            }
            Item::Message(m) => {
                standalone_calls = false;
                let mut message =
                    json!({"role":if m.role == MessageRole::User { "user" } else { "assistant" }});
                crate::protocol::openai::chat::reasoning::write_carrier(
                    &mut message,
                    &std::mem::take(&mut reasoning),
                    fidelity,
                    structured,
                );
                let text_parts: Vec<_> = m
                    .parts
                    .iter()
                    .filter(|p| {
                        !matches!(
                            p.content,
                            ContentPart::Audio(_) | ContentPart::AudioReference(_)
                        )
                    })
                    .collect();
                for p in &m.parts {
                    match &p.content {
                        ContentPart::Audio(audio) => {
                            message["audio"] = if response {
                                crate::protocol::openai::chat::audio::write(audio)
                            } else {
                                json!({"id":audio.reference().id()})
                            }
                        }
                        ContentPart::AudioReference(reference) => {
                            message["audio"] = json!({"id":reference.id()})
                        }
                        _ => {}
                    }
                }
                match text_parts.as_slice() {
                    [] => message["content"] = Value::Null,
                    [part] => match &part.content {
                        ContentPart::Text(t) => {
                            message["content"] = json!(t.as_str());
                            if !t.annotations().is_empty() {
                                message["annotations"] =
                                    crate::protocol::openai::chat::annotations::write(
                                        t.annotations(),
                                        resources,
                                    );
                            }
                        }
                        ContentPart::Refusal(t) => {
                            message["content"] = Value::Null;
                            message["refusal"] = json!(t.as_str());
                        }
                        ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                            unreachable!("filtered audio")
                        }
                        ContentPart::Resource(resource) => {
                            message["content"] = json!([write_input_resource(
                                resource.media(resources).expect("validated resource")
                            )])
                        }
                    },
                    parts => {
                        message["content"] = json!(
                            parts
                                .iter()
                                .map(|part| match &part.content {
                                    ContentPart::Text(text) =>
                                        json!({"type":"text","text":text.as_str()}),
                                    ContentPart::Resource(resource) => write_input_resource(
                                        resource.media(resources).expect("validated resource")
                                    ),
                                    _ => unreachable!("lowering rejects non-input request arrays"),
                                })
                                .collect::<Vec<_>>()
                        )
                    }
                }
                messages.push(message);
            }
            Item::ToolCall(_) | Item::CustomCall(_) => {
                if !owners.contains_key(id) && !standalone_calls {
                    let mut message = json!({"role":"assistant","content":null});
                    crate::protocol::openai::chat::reasoning::write_carrier(
                        &mut message,
                        &std::mem::take(&mut reasoning),
                        fidelity,
                        structured,
                    );
                    messages.push(message);
                }
                let message = messages
                    .last_mut()
                    .expect("validated call owner or standalone message");
                let calls = message
                    .as_object_mut()
                    .expect("message")
                    .entry("tool_calls")
                    .or_insert_with(|| json!([]));
                calls.as_array_mut().expect("calls").push(match item {
                    Item::ToolCall(call) => call_wire(call),
                    Item::CustomCall(call) => custom_call_wire(call),
                    _ => unreachable!("call"),
                });
                standalone_calls = !owners.contains_key(id);
            }
            Item::ProviderTool(_)
            | Item::ConfigurationUpdate(_)
            | Item::Compaction(_)
            | Item::Program(_)
            | Item::ProgramOutput(_) => {
                unreachable!("lowering rejects unsupported Chat items")
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                standalone_calls = false;
                messages.push(
                    json!({"role":"tool","tool_call_id":r.required_call_id().expect("validated request history"),"content":match &r.output {
                        ToolOutput::Text(s)=>json!(s),
                        ToolOutput::Parts(parts)=>json!(parts.iter().map(|p|json!({"type":"text","text":p.content.as_text().expect("lowering admits text-only tool parts")})).collect::<Vec<_>>()),
                        ToolOutput::Structured(_) => unreachable!("lowering rejects tool result semantics without a carrier"),
                    }}),
                );
            }
        }
    }
    messages
}

fn write_input_resource(resource: ResourceView<'_>) -> Value {
    match resource.kind() {
        ResourceKind::Audio => crate::protocol::openai::content::input_audio::write(resource),
        ResourceKind::Image => {
            crate::protocol::openai::content::image::write(resource, Profile::Chat)
        }
        ResourceKind::Video => crate::protocol::openai::content::video::write(resource),
        ResourceKind::File => crate::protocol::openai::content::file::write_chat(resource),
    }
}
