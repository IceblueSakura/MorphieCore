//! Chat message/item mapping shared by request history, static output and event codecs.
use crate::protocol::openai::{CodecError, Profile, shape::*};
use crate::protocol::openai::{
    content::call::{reject_chat_orchestration, tool_call},
    content::intake::Items,
};
use crate::semantic::task::generation::*;
use serde_json::{Map, Value, json};
pub(in crate::protocol::openai) fn reject_moderation(
    o: &Map<String, Value>,
) -> Result<(), CodecError> {
    // A known safety report cannot follow the unknown-extension ignore policy.
    if o.get("moderation").is_some_and(|v| !v.is_null()) {
        return Err(CodecError::Unsupported("active moderation report".into()));
    }
    Ok(())
}
/// Request-only text arrays preserve ordered part ownership; canonical single parts
/// encode as strings. This does not admit response arrays or media placeholders.
pub(super) fn request_text_parts<'a>(
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
/// Which Chat surface a message comes from. `Output` is the response's own message;
/// `History` is a caller-supplied echo, so caller-only fields are validated too.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol::openai) enum MessageSide {
    History,
    Output,
}

/// One whole Chat message. The reasoning carrier is read first, so a rejected field
/// can never leave a half-built item behind; callers never sequence the two steps.
pub(in crate::protocol::openai) fn decode_whole(
    b: &mut Items,
    m: &Map<String, Value>,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    side: MessageSide,
) -> Result<(), CodecError> {
    let message = crate::protocol::openai::chat::reasoning::decode_static(b, m, adaptation)?;
    decode_message(b, &message, side)
}

fn decode_message(
    b: &mut Items,
    m: &Map<String, Value>,
    side: MessageSide,
) -> Result<(), CodecError> {
    let replay = side == MessageSide::History;
    let direction = b.direction;
    let role = string(m, "role")?;
    reject_chat_orchestration(m)?;
    // Chat lowering has no explicit-prefix carrier. Reject before permissive
    // client field filtering can discard the requested cache boundary.
    if replay
        && m.get("content")
            .and_then(Value::as_array)
            .is_some_and(|parts| {
                parts.iter().any(|part| {
                    part.as_object()
                        .is_some_and(|part| part.contains_key("prompt_cache_breakpoint"))
                })
            })
    {
        return Err(CodecError::Unsupported(
            "Chat prompt_cache_breakpoint".into(),
        ));
    }
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
            "name",
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
            "name",
            "content",
            "tool_calls",
            "refusal",
            "reasoning_content",
            "annotations",
            "audio",
        ],
        "tool" => &["role", "content", "tool_call_id"],
        "system" | "developer" | "user" => &["role", "name", "content"],
        _ => return Err(CodecError::Unsupported("message role".into())),
    };
    direction.fields(m, allowed)?;
    if (!replay || role == "tool") && m.get("name").is_some_and(|v| !v.is_null()) {
        return Err(CodecError::Unsupported("message name".into()));
    }
    let participant = if replay && role != "tool" {
        m.get("name")
            .map(|v| {
                crate::semantic::value::Text::allowing_empty(
                    v.as_str().ok_or(CodecError::Invalid("message name"))?,
                    "participant",
                    MAX_TEXT_BYTES,
                )
                .map_err(|_| CodecError::Limit)
            })
            .transpose()?
    } else {
        None
    };
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
                    participant,
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
                crate::protocol::openai::shape::admit_parsed(
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
                                replay: None,
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
                                replay: None,
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::video::read(part, direction)?,
                                )?),
                            }),
                            "file" => parts.push(Part {
                                replay: None,
                                cache: None,
                                id: b.part_id()?,
                                content: ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::file::read_chat(
                                        part, direction,
                                    )?,
                                )?),
                            }),
                            "input_audio" => parts.push(Part {
                                replay: None,
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
                (Some(Value::Array(values)), None)
                    if replay && role == "assistant" && !values.is_empty() =>
                {
                    let mut parts = Vec::with_capacity(values.len());
                    for value in values {
                        let part = object(value)?;
                        parts.push(match string(part, "type")? {
                            "text" => {
                                direction.fields(part, &["type", "text"])?;
                                b.part(string(part, "text")?)?
                            }
                            "refusal" if calls.is_none() => {
                                direction.fields(part, &["type", "refusal"])?;
                                b.refusal(
                                    crate::semantic::value::Text::allowing_empty(
                                        string(part, "refusal")?,
                                        "refusal",
                                        MAX_TEXT_BYTES,
                                    )
                                    .map_err(|_| CodecError::Limit)?,
                                )?
                            }
                            "refusal" => return Err(CodecError::Invalid("refusal")),
                            _ => {
                                return Err(CodecError::Unsupported("message content part".into()));
                            }
                        });
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
                    replay: None,
                    id: b.part_id()?,
                    content: crate::protocol::openai::chat::audio::read(audio, replay, direction)?,
                    cache: None,
                });
            }
            b.items.push((
                id,
                Item::Message(Message {
                    participant,
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
pub(in crate::protocol::openai) fn call_wire(c: &ToolCall) -> Value {
    let arguments = c.arguments.as_raw().expect("checked raw arguments");
    json!({
        "id": c.call_id.as_str(),
        "type": "function",
        "function": {"name": c.name.as_str(), "arguments": arguments},
    })
}
fn custom_call_wire(c: &CustomCall) -> Value {
    json!({
        "id": c.call_id.as_str(),
        "type": "custom",
        "custom": {"name": c.name.as_str(), "input": c.input},
    })
}
pub(in crate::protocol::openai) fn encode_items_with(
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
                let role = match i.authority {
                    InstructionAuthority::System => "system",
                    InstructionAuthority::Developer => "developer",
                };
                let mut message = json!({ "role": role, "content": content });
                if let Some(name) = &i.participant {
                    message["name"] = json!(name.as_str());
                }
                messages.push(message);
            }
            Item::Message(m) => {
                standalone_calls = false;
                let role = if m.role == MessageRole::User {
                    "user"
                } else {
                    "assistant"
                };
                let mut message = json!({ "role": role });
                if let Some(name) = &m.participant {
                    message["name"] = json!(name.as_str());
                }
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
                                    ContentPart::Refusal(text) =>
                                        json!({"type":"refusal","refusal":text.as_str()}),
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
            | Item::ToolChange(_)
            | Item::Compaction(_)
            | Item::Program(_)
            | Item::ProgramOutput(_) => {
                unreachable!("lowering rejects unsupported Chat items")
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                standalone_calls = false;
                let content = match &r.output {
                    ToolOutput::Text(s) => json!(s),
                    ToolOutput::Parts(parts) => json!(
                        parts
                            .iter()
                            .map(|p| {
                                let text = p
                                    .content
                                    .as_text()
                                    .expect("lowering admits text-only tool parts");
                                json!({"type": "text", "text": text})
                            })
                            .collect::<Vec<_>>()
                    ),
                    ToolOutput::Structured(_) => {
                        unreachable!("lowering rejects tool result semantics without a carrier")
                    }
                };
                let call_id = r.required_call_id().expect("validated request history");
                messages.push(json!({"role": "tool", "tool_call_id": call_id, "content": content}));
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
