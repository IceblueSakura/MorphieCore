//! Chat request mapping, including explicit assistant message ownership for tool calls.
pub use super::static_response::{decode_chat as decode_response, encode_chat as encode_response};
use super::{
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
];
pub fn decode_generation(v: &Value) -> Result<DecodedRequest, CodecError> {
    decode_generation_with(v, &Default::default())
}
pub(crate) fn decode_generation_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
) -> Result<DecodedRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    fields(o, FIELDS)?;
    let messages = o
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("messages"))?;
    let mut b = Items::default();
    for message in messages {
        adaptation.validate_message(object(message)?)?;
        let message = super::chat_reasoning::decode_static(&mut b, object(message)?, adaptation)?;
        decode_message(&mut b, &message, true)?;
    }
    let mut controls = controls(o, "max_completion_tokens")?;
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
    let format = read_presence(o, "response_format", read_response_format)?;
    let mut settings = GenerationSettings {
        controls,
        text: TextOptions {
            presence: !format.is_absent(),
            format,
            verbosity: Presence::Absent,
        },
        reasoning: super::reasoning::chat_request(o)?,
        ..Default::default()
    };
    super::chat_audio::settings(o, &mut settings)?;
    let r = GenerationRequest::from_settings(b.items, settings)?;
    Ok(DecodedRequest {
        semantic: function_tools::decode(r, o, Profile::Chat)?,
        fidelity: b.fidelity,
    })
}
/// Chat nests the json_schema body under `json_schema`; the flat Responses shell is parsed apart.
fn read_response_format(v: &Value) -> Result<OutputConstraint, CodecError> {
    let o = object(v)?;
    Ok(match string(o, "type")? {
        "text" => {
            fields(o, &["type"])?;
            OutputConstraint::Text
        }
        "json_object" => {
            fields(o, &["type"])?;
            OutputConstraint::JsonObject
        }
        "json_schema" => {
            fields(o, &["type", "json_schema"])?;
            let body = object(
                o.get("json_schema")
                    .ok_or(CodecError::Invalid("json_schema"))?,
            )?;
            fields(body, &["name", "description", "schema", "strict"])?;
            super::settings::read_schema_body(body)?
        }
        _ => return Err(CodecError::Unsupported("response format".into())),
    })
}
fn write_response_format(f: &OutputConstraint) -> Value {
    match f {
        OutputConstraint::Text => json!({"type":"text"}),
        OutputConstraint::JsonObject => json!({"type":"json_object"}),
        OutputConstraint::JsonSchema {
            name,
            description,
            schema,
            strict,
        } => {
            let mut body = Map::new();
            super::settings::write_schema_body(
                &mut body,
                name,
                description.as_ref(),
                schema,
                *strict,
            );
            json!({"type":"json_schema","json_schema":Value::Object(body)})
        }
    }
}
/// Request-only text arrays preserve ordered part ownership; canonical single parts
/// encode as strings. This does not admit response arrays or media placeholders.
fn request_text_parts(value: &Value) -> Result<Vec<&str>, CodecError> {
    match value {
        Value::String(text) => Ok(vec![text]),
        Value::Array(parts) if !parts.is_empty() => parts
            .iter()
            .map(|part| {
                let part = object(part)?;
                fields(part, &["type", "text"])?;
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
    let role = string(m, "role")?;
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
    fields(m, allowed)?;
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
                        request_text_parts(v)?
                    };
                    let mut parts = Vec::new();
                    for value in texts {
                        parts.push((
                            b.part_id()?,
                            ToolResultPart::Text(
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
            b.items.push((
                id,
                Item::ToolResult(ToolResult {
                    execution: None,
                    call_id: text(string(m, "tool_call_id")?, "call_id", 256)?,
                    output,
                    status: None,
                    context: CallContext::default(),
                }),
            ));
        }
        "system" | "developer" => {
            if m.contains_key("tool_calls") || m.contains_key("tool_call_id") {
                return Err(CodecError::Invalid("instruction tool fields"));
            }
            let mut parts = Vec::new();
            for text in request_text_parts(m.get("content").ok_or(CodecError::Invalid("content"))?)?
            {
                parts.push((
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
                super::common::admit_parsed(
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
                                fields(part, &["type", "text"])?;
                                parts.push(b.part(string(part, "text")?)?);
                            }
                            "image_url" => parts.push(Part {
                                id: b.part_id()?,
                                content: ContentPart::Resource(super::image::read(
                                    part,
                                    Profile::Chat,
                                )?),
                            }),
                            _ => return Err(CodecError::Unsupported("user content part".into())),
                        }
                    }
                    parts
                }
                (Some(value @ Value::Array(_)), None) if replay => request_text_parts(value)?
                    .into_iter()
                    .map(|text| b.part(text))
                    .collect::<Result<Vec<_>, _>>()?,
                (None | Some(Value::Null), None) if role == "assistant" => Vec::new(),
                _ => return Err(CodecError::Unsupported("message content".into())),
            };
            super::chat_annotations::attach(&mut parts, m.get("annotations"))?;
            if let Some(audio) = m.get("audio").filter(|v| !v.is_null()) {
                if parts
                    .iter()
                    .any(|p| matches!(p.content, ContentPart::Refusal(_)))
                {
                    return Err(CodecError::Invalid("audio with refusal"));
                }
                parts.push(Part {
                    id: b.part_id()?,
                    content: super::chat_audio::read(audio, replay)?,
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
                    let call = tool_call(call, Profile::Chat, Some(id), None, replay)?;
                    let cid = b.id()?;
                    b.items.push((cid, Item::ToolCall(call)));
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
    check_item_carriers(target.semantic.items())?;
    if target.semantic.items().iter().any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result) if result.execution.is_some() || !result.output.is_text_only())) {
        return Err(CodecError::Unsupported("tool result semantics".into()));
    }
    let mut messages = encode_items_with(
        target.semantic.items(),
        target.fidelity,
        target.adaptation.rules.structured_chat_reasoning,
        false,
    );
    if let Some(t) = target.semantic.instructions().value() {
        messages.insert(0, json!({"role":"developer","content":t.as_str()}));
    }
    let mut v = json!({"messages": messages});
    let o = v.as_object_mut().expect("object literal");
    write_controls(
        target.semantic,
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
    put_presence(o, "logprobs", &target.semantic.controls().logprobs, |v| {
        json!(v)
    });
    if let Some(n) = target.semantic.controls().top_logprobs {
        o.insert("top_logprobs".into(), json!(n));
    }
    put_presence(
        o,
        "response_format",
        &target.semantic.text_options().format,
        write_response_format,
    );
    super::chat_audio::write_settings(target.semantic.settings(), o);
    function_tools::encode(target.semantic, Profile::Chat, o)?;
    super::reasoning::write_request(target.semantic.reasoning(), o, Profile::Chat)?;
    bounded(&v)?;
    Ok(v)
}
pub(super) fn call_wire(c: &ToolCall) -> Value {
    json!({"id":c.call_id.as_str(),"type":"function","function":{"name":c.name.as_str(),"arguments":c.arguments.as_raw().expect("checked raw arguments")}})
}
pub(super) fn encode_items_with(
    items: &[(ItemId, Item)],
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    structured: bool,
    response: bool,
) -> Vec<Value> {
    let mut messages = Vec::<Value>::new();
    let mut standalone_calls = false;
    let mut reasoning: Option<String> = None;
    let mut details: Option<Vec<Value>> = None;
    for (id, item) in items {
        match item {
            Item::Reasoning(r) => {
                // Chat carries readable reasoning text on its carrier message.
                standalone_calls = false;
                if structured
                    && (r.replay.is_some()
                        || r.parts
                            .iter()
                            .any(|(_, p)| matches!(p, ReasoningContent::Summary(_))))
                {
                    details = Some(super::chat_reasoning::wire(*id, r, fidelity));
                    continue;
                }
                let text = match r.parts.as_slice() {
                    [(_, ReasoningContent::Text(t))] if !t.as_str().is_empty() => t.as_str(),
                    _ => unreachable!("lowering rejects non-Chat reasoning shapes"),
                };
                reasoning = Some(text.to_owned());
            }
            Item::Instruction(i) => {
                standalone_calls = false;
                let content = if i.parts.len() == 1 {
                    json!(i.parts[0].1.as_str())
                } else {
                    json!(
                        i.parts
                            .iter()
                            .map(|(_, t)| json!({"type":"text","text":t.as_str()}))
                            .collect::<Vec<_>>()
                    )
                };
                messages.push(json!({"role":match i.authority { InstructionAuthority::System => "system", InstructionAuthority::Developer => "developer" },"content":content}));
            }
            Item::Message(m) => {
                standalone_calls = false;
                let mut message =
                    json!({"role":if m.role == MessageRole::User { "user" } else { "assistant" }});
                if let Some(text) = reasoning.take() {
                    message["reasoning_content"] = json!(text);
                }
                if let Some(value) = details.take() {
                    message["reasoning_details"] = json!(value);
                }
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
                                super::chat_audio::write(audio)
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
                                    super::chat_annotations::write(t.annotations());
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
                            message["content"] =
                                json!([super::image::write(resource, Profile::Chat)])
                        }
                    },
                    parts => {
                        message["content"] = json!(
                            parts
                                .iter()
                                .map(|part| match &part.content {
                                    ContentPart::Text(text) =>
                                        json!({"type":"text","text":text.as_str()}),
                                    ContentPart::Resource(resource) =>
                                        super::image::write(resource, Profile::Chat),
                                    _ => unreachable!("lowering rejects non-input request arrays"),
                                })
                                .collect::<Vec<_>>()
                        )
                    }
                }
                messages.push(message);
            }
            Item::ToolCall(c) => {
                if c.message.is_none() && !standalone_calls {
                    let mut message = json!({"role":"assistant","content":null});
                    if let Some(text) = reasoning.take() {
                        message["reasoning_content"] = json!(text);
                    }
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
                calls.as_array_mut().expect("calls").push(call_wire(c));
                standalone_calls = c.message.is_none();
            }
            Item::CustomCall(_)
            | Item::CustomResult(_)
            | Item::ConfigurationUpdate(_)
            | Item::Program(_)
            | Item::ProgramOutput(_) => {
                unreachable!("lowering rejects unsupported Chat items")
            }
            Item::ToolResult(r) => {
                standalone_calls = false;
                messages.push(
                    json!({"role":"tool","tool_call_id":r.call_id.as_str(),"content":match &r.output {
                        ToolOutput::Text(s)=>json!(s),
                        ToolOutput::Parts(parts)=>json!(parts.iter().map(|(_,t)|json!({"type":"text","text":t.as_text().expect("lowering admits text-only tool parts")})).collect::<Vec<_>>()),
                        ToolOutput::Structured(_) => unreachable!("lowering rejects tool result semantics without a carrier"),
                    }}),
                );
            }
        }
    }
    messages
}
