//! Responses item mapping shared by input history, static output and event codecs.
use crate::{
    protocol::{
        failure::{CarrierReason, ShapeReason},
        fidelity::FidelityRecords,
        openai::{
            CodecError, GenerationApi,
            content::{call::tool_call, intake::Items},
            shape::*,
        },
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};
pub(in crate::protocol::openai) fn read_phase(
    o: &Map<String, Value>,
) -> Result<Option<Phase>, CodecError> {
    match o.get("phase") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => s
            .parse::<Phase>()
            .map(Some)
            .map_err(|_| CodecError::invalid()),
        _ => Err(CodecError::invalid()),
    }
}
pub(in crate::protocol::openai) fn status(
    o: &Map<String, Value>,
    default: ItemLifecycle,
) -> Result<ItemLifecycle, CodecError> {
    match o.get("status") {
        None => Ok(default),
        Some(Value::String(s)) => match s.as_str() {
            "completed" => Ok(ItemLifecycle::Completed),
            "incomplete" => Ok(ItemLifecycle::Incomplete),
            "in_progress" => Ok(ItemLifecycle::InProgress),
            _ => Err(CodecError::Invalid(ShapeReason::Item)),
        },
        _ => Err(CodecError::Invalid(ShapeReason::Item)),
    }
}
pub(in crate::protocol::openai) fn status_label(s: ItemLifecycle) -> &'static str {
    match s {
        ItemLifecycle::Completed => "completed",
        ItemLifecycle::Incomplete => "incomplete",
        ItemLifecycle::InProgress => "in_progress",
    }
}
pub(in crate::protocol::openai) fn call_context(
    o: &Map<String, Value>,
    direction: DecodeDirection,
) -> Result<CallContext, CodecError> {
    let namespace = match o.get("namespace") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(text(s, "tool namespace", 128)?),
        _ => {
            return Err(CodecError::invalid());
        }
    };
    let async_call = match o.get("async") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        _ => {
            return Err(CodecError::invalid());
        }
    };
    let caller = match o.get("caller") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let caller = object(value)?;
            direction.fields(caller, &["type", "caller_id"])?;
            match string(caller, "type")? {
                "direct" => {
                    if caller.contains_key("caller_id") {
                        return Err(CodecError::invalid());
                    }
                    None
                }
                "program" => Some(CallOrigin::Program {
                    caller_id: text(string(caller, "caller_id")?, "caller id", 256)?,
                }),
                _ => {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
            }
        }
    };
    Ok(CallContext {
        namespace,
        async_call,
        caller,
        alias_domain: None,
    })
}
pub(in crate::protocol::openai) fn write_call_context(
    context: &CallContext,
    o: &mut Map<String, Value>,
) {
    if let Some(namespace) = &context.namespace {
        o.insert("namespace".into(), json!(namespace.as_str()));
    }
    if context.async_call {
        o.insert("async".into(), json!(true));
    }
    if let Some(CallOrigin::Program { caller_id }) = &context.caller {
        o.insert(
            "caller".into(),
            json!({"type":"program","caller_id":caller_id.as_str()}),
        );
    }
}
fn output(b: &mut Items, v: &Value, response: bool) -> Result<ToolOutput, CodecError> {
    if let Some(s) = v.as_str() {
        if s.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        return Ok(ToolOutput::Text(s.into()));
    }
    let a = v.as_array().ok_or(CodecError::invalid())?;
    Ok(ToolOutput::Parts(input_parts(b, a, response)?))
}
/// Text and media input content of tool results.
fn input_parts(
    b: &mut Items,
    a: &[Value],
    response: bool,
) -> Result<Vec<Part<InputPart>>, CodecError> {
    let direction = b.direction;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let mut p = vec![];
    for v in a {
        let o = object(v)?;
        let part = match string(o, "type")? {
            "input_text" => {
                direction.fields(o, &["type", "text", "prompt_cache_breakpoint"])?;
                let id = b.part_id()?;
                b.fidelity.record_input_text(id)?;
                (
                    id,
                    InputPart::Text(
                        Text::allowing_empty(string(o, "text")?, "tool output", MAX_TEXT_BYTES)
                            .map_err(|_| CodecError::Limit)?,
                    ),
                )
            }
            // The pinned function/custom result union includes standard image
            // content. File IDs still require an issuer/resource lifecycle.
            // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_call_output_item_list_param.py
            "input_image" => {
                let resource = crate::protocol::openai::content::image::read(
                    o,
                    GenerationApi::Responses,
                    direction,
                )?;
                let id = b.part_id()?;
                (id, InputPart::Resource(b.resource(resource)?))
            }
            "input_file" => {
                let resource = crate::protocol::openai::content::file::read(o, direction)?;
                let id = b.part_id()?;
                (id, InputPart::Resource(b.resource(resource)?))
            }
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        let mut part = Part::new(part.0, part.1);
        part.cache = read_breakpoint(o, response)?;
        p.push(part);
    }
    Ok(p)
}
fn record_input_form(
    b: &mut Items,
    id: PartId,
    o: &Map<String, Value>,
) -> Result<Option<PrefixCacheBoundary>, CodecError> {
    b.fidelity.record_input_text(id)?;
    read_breakpoint(o, false)
}
fn read_breakpoint(
    o: &Map<String, Value>,
    response: bool,
) -> Result<Option<PrefixCacheBoundary>, CodecError> {
    match o.get("prompt_cache_breakpoint") {
        None => Ok(None),
        Some(Value::Null) if response => Ok(None),
        Some(v)
            if v.as_object()
                .and_then(|o| o.get("mode"))
                .and_then(Value::as_str)
                == Some("explicit") =>
        {
            Ok(Some(PrefixCacheBoundary::RequestLifetime))
        }
        _ => Err(CodecError::invalid()),
    }
}
fn input_part(text: &str, cache: Option<PrefixCacheBoundary>) -> Value {
    let mut v = json!({"type":"input_text","text":text});
    if cache == Some(PrefixCacheBoundary::RequestLifetime) {
        v["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
    }
    v
}
pub(in crate::protocol::openai) fn configuration_update(
    o: &Map<String, Value>,
    response: bool,
    direction: DecodeDirection,
) -> Result<ConfigurationUpdate, CodecError> {
    match o.get("reasoning") {
        None => Ok(ConfigurationUpdate::default()),
        Some(Value::Null) if response => Ok(ConfigurationUpdate::default()),
        Some(Value::Null) => Err(CodecError::invalid()),
        Some(value) => {
            let reasoning = object(value)?;
            direction.fields(reasoning, &["effort"])?;
            match reasoning.get("effort") {
                None | Some(Value::Null) => Ok(ConfigurationUpdate::default()),
                Some(value) => crate::protocol::openai::reasoning::effort(
                    value.as_str().ok_or(CodecError::invalid())?,
                ),
            }
        }
    }
}
pub(in crate::protocol::openai) fn tool_result(
    b: &mut Items,
    o: &Map<String, Value>,
    response: bool,
    custom: bool,
) -> Result<ToolResult, CodecError> {
    // Function output snapshots may omit call_id; replay still requires it.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_tool_call_output_item.py
    b.direction.fields(
        o,
        &[
            "type",
            "id",
            "call_id",
            "output",
            "status",
            "name",
            "namespace",
            "caller",
        ],
    )?;
    let optional = |key, limit| {
        o.get(key)
            .filter(|v| !v.is_null())
            .map(|v| text(v.as_str().ok_or(CodecError::invalid())?, key, limit))
            .transpose()
    };
    let call_id = optional("call_id", 256)?;
    if (!response || custom) && call_id.is_none() {
        return Err(CodecError::invalid());
    }
    if response {
        text(string(o, "id")?, "result id", 256)?;
        string(o, "status")?;
    }
    Ok(ToolResult {
        call_id,
        cache: None,
        name: optional("name", 128)?,
        output: output(b, o.get("output").ok_or(CodecError::invalid())?, response)?,
        status: if o.contains_key("status") {
            Some(status(o, ItemLifecycle::Completed)?)
        } else {
            None
        },
        is_error: None,
        execution: None,
        context: call_context(o, b.direction)?,
    })
}

/// Which Responses surface supplies the array. `Output` carries the response's own
/// item lifecycle; `History` is an input echo with no per-item status field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol::openai) enum ItemSide {
    History,
    Output,
}

impl ItemSide {
    fn response(self) -> bool {
        self == Self::Output
    }
}

pub(in crate::protocol::openai) fn decode_items(
    b: &mut Items,
    input: &[Value],
    side: ItemSide,
    item_status: &str,
    input_video: bool,
) -> Result<(), CodecError> {
    let response = side.response();
    let direction = b.direction;
    for value in input {
        let o = object(value)?;
        if !response
            && matches!(direction, DecodeDirection::Client(_))
            && o.get("type").and_then(Value::as_str) == Some("reasoning")
            && o.get("encrypted_content")
                .and_then(Value::as_str)
                .is_some_and(crate::protocol::replay_projection::is_marker)
        {
            return Err(CodecError::Unsupported(CarrierReason::Replay));
        }
        let id = b.id()?;
        let typ = o
            .get("type")
            .map(|v| v.as_str().ok_or(CodecError::invalid()))
            .transpose()?;
        let item = match typ {
            Some("function_call") => Item::ToolCall(tool_call(
                o,
                GenerationApi::Responses,
                response.then_some(item_status),
                !response,
                direction,
            )?),
            Some("custom_tool_call") => {
                direction.fields(
                    o,
                    &[
                        "type",
                        "id",
                        "call_id",
                        "name",
                        "input",
                        "namespace",
                        "caller",
                        "async",
                    ],
                )?;
                if !response {
                    for key in ["id", "namespace", "async"] {
                        if o.get(key).is_some_and(Value::is_null) {
                            return Err(CodecError::invalid());
                        }
                    }
                }
                Item::CustomCall(CustomCall {
                    call_id: text(string(o, "call_id")?, "call id", 256)?,
                    name: text(string(o, "name")?, "custom name", 128)?,
                    input: raw_string(o, "input")?,
                    context: call_context(o, direction)?,
                })
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                let r = tool_result(b, o, response, typ == Some("custom_tool_call_output"))?;
                if typ == Some("custom_tool_call_output") {
                    Item::CustomResult(r)
                } else {
                    Item::ToolResult(r)
                }
            }
            Some("configuration_update") => {
                direction.fields(o, &["type", "id", "reasoning"])?;
                if response {
                    text(string(o, "id")?, "configuration update id", 256)?;
                } else if o
                    .get("id")
                    .is_some_and(|v| !v.is_null() && v.as_str().is_none())
                {
                    return Err(CodecError::invalid());
                }
                Item::ConfigurationUpdate(configuration_update(o, response, direction)?)
            }
            Some("compaction") => {
                direction.fields(o, &["type", "id", "encrypted_content"])?;
                if response {
                    text(string(o, "id")?, "compaction id", 256)?;
                }
                let value = compaction_content(o, true)?;
                b.fidelity.record_compaction(id, &value, None)?;
                Item::Compaction(value)
            }
            Some("program") => {
                direction.fields(o, &["type", "id", "call_id", "code", "fingerprint"])?;
                text(string(o, "id")?, "program id", 256)?;
                Item::Program(Program {
                    call_id: text(string(o, "call_id")?, "call id", 256)?,
                    code: raw_string(o, "code")?,
                    fingerprint: raw_string(o, "fingerprint")?,
                })
            }
            Some("program_output") => {
                direction.fields(o, &["type", "id", "call_id", "result", "status"])?;
                text(string(o, "id")?, "program output id", 256)?;
                if response {
                    accept_status(o, item_status)?;
                }
                Item::ProgramOutput(ProgramOutput {
                    call_id: text(string(o, "call_id")?, "call id", 256)?,
                    result: raw_string(o, "result")?,
                    status: match string(o, "status")? {
                        "completed" => ItemLifecycle::Completed,
                        "incomplete" => ItemLifecycle::Incomplete,
                        _ => {
                            return Err(CodecError::invalid());
                        }
                    },
                })
            }
            Some("reasoning") => {
                // Wire reasoning items carry required identity; it is never invented.
                text(string(o, "id")?, "reasoning id", 256)?;
                let r = crate::protocol::openai::reasoning::decode_item(
                    o,
                    &mut || b.part_id(),
                    !response || item_status != "completed",
                    direction,
                )?;
                b.fidelity.record_replay(id, &r, None)?;
                Item::Reasoning(r)
            }
            Some("message") | None => {
                direction.fields(
                    o,
                    if typ.is_none() {
                        &["role", "content", "phase"]
                    } else {
                        &["type", "id", "role", "content", "status", "phase"]
                    },
                )?;
                let role = string(o, "role")?;
                if o.contains_key("phase") && role != "assistant" {
                    // The standard labels assistant messages only.
                    return Err(CodecError::invalid());
                }
                let phase = read_phase(o)?;
                let content = o.get("content").ok_or(CodecError::invalid())?;
                if !response
                    && role == "assistant"
                    && content.as_array().is_some_and(|parts| {
                        parts.iter().any(|p| {
                            matches!(
                                p.get("type").and_then(Value::as_str),
                                Some("output_text" | "refusal")
                            )
                        })
                    })
                {
                    // Complete output-message history is not the easy-input union branch.
                    if typ != Some("message") {
                        return Err(CodecError::invalid());
                    }
                    text(string(o, "id")?, "message id", 256)?;
                    string(o, "status")?;
                }
                let values = if let Some(s) = content.as_str() {
                    if response {
                        return Err(CodecError::invalid());
                    }
                    vec![if role == "assistant" {
                        // Shorthand has no annotation slot; the typed part still requires the array.
                        json!({"type":"output_text","text":s,"annotations":[]})
                    } else {
                        json!({"type":"input_text","text":s})
                    }]
                } else {
                    content.as_array().ok_or(CodecError::invalid())?.clone()
                };
                if values.len() > MAX_ITEMS {
                    return Err(CodecError::Limit);
                }
                if role == "system" || role == "developer" {
                    let instruction_status = o
                        .get("status")
                        .map(|_| status(o, ItemLifecycle::Completed))
                        .transpose()?;
                    let mut parts = Vec::with_capacity(values.len());
                    for value in &values {
                        let part = object(value)?;
                        if string(part, "type")? != "input_text" {
                            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                        }
                        direction.fields(part, &["type", "text", "prompt_cache_breakpoint"])?;
                        let id = b.part_id()?;
                        let cache = record_input_form(b, id, part)?;
                        let mut part = Part::new(
                            id,
                            Text::allowing_empty(
                                string(part, "text")?,
                                "instruction",
                                MAX_TEXT_BYTES,
                            )
                            .map_err(|_| CodecError::Limit)?,
                        );
                        part.cache = cache;
                        parts.push(part);
                    }
                    Item::Instruction(Instruction {
                        participant: None,
                        parts,
                        status: instruction_status,
                    })
                } else {
                    let role = match role {
                        "user" => MessageRole::User,
                        "assistant" => MessageRole::Assistant,
                        _ => {
                            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                        }
                    };
                    let mut parts = vec![];
                    for v in &values {
                        let p = object(v)?;
                        let kind = string(p, "type")?;
                        let content = match kind {
                            "refusal" if role == MessageRole::Assistant => {
                                direction.fields(p, &["type", "refusal"])?;
                                ContentPart::Refusal(
                                    Text::allowing_empty(
                                        string(p, "refusal")?,
                                        "refusal",
                                        MAX_TEXT_BYTES,
                                    )
                                    .map_err(|_| CodecError::Limit)?
                                    .into(),
                                )
                            }
                            "input_text" if !response => {
                                direction
                                    .fields(p, &["type", "text", "prompt_cache_breakpoint"])?;
                                ContentPart::Text(
                                    Text::allowing_empty(
                                        string(p, "text")?,
                                        "text",
                                        MAX_TEXT_BYTES,
                                    )
                                    .map_err(|_| CodecError::Limit)?
                                    .into(),
                                )
                            }
                            "input_image" if !response && role == MessageRole::User => {
                                ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::image::read(
                                        p,
                                        GenerationApi::Responses,
                                        direction,
                                    )?,
                                )?)
                            }
                            "input_file" if !response && role == MessageRole::User => {
                                ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::file::read(p, direction)?,
                                )?)
                            }
                            "input_video"
                                if !response && role == MessageRole::User && input_video =>
                            {
                                ContentPart::Resource(b.resource(
                                    crate::protocol::openai::content::video::read_responses(
                                        p, direction,
                                    )?,
                                )?)
                            }
                            "output_text" if role == MessageRole::Assistant => ContentPart::Text(
                                crate::protocol::openai::content::text::read(p, b)?,
                            ),
                            _ => {
                                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                            }
                        };
                        let part_id = b.part_id()?;
                        let cache = match kind {
                            // The SDK union admits assistant `input_text`, but the API accepts
                            // only `output_text`; it is the same assistant text, re-spelled.
                            "input_text" if role == MessageRole::Assistant => {
                                if p.contains_key("prompt_cache_breakpoint") {
                                    return Err(CodecError::Unsupported(
                                        CarrierReason::Unavailable,
                                    ));
                                }
                                None
                            }
                            "input_text" => record_input_form(b, part_id, p)?,
                            "input_image" | "input_file" => read_breakpoint(p, false)?,
                            _ => None,
                        };
                        parts.push(Part {
                            replay: None,
                            cache,
                            id: part_id,
                            content,
                        });
                    }
                    let status = status(o, ItemLifecycle::Completed)?;
                    if response && item_status == "completed" && status != ItemLifecycle::Completed
                    {
                        return Err(CodecError::invalid());
                    }
                    Item::Message(Message {
                        participant: None,
                        role,
                        parts,
                        status,
                        phase,
                    })
                }
            }
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
        };
        // Report IDs and the explicitly nullable input-item IDs use their own contracts.
        if !((response || matches!(typ, Some("compaction" | "function_call_output")))
            && o.get("id").is_some_and(Value::is_null))
        {
            b.record_id(id, o)?;
        }
        if response {
            b.fidelity
                .observe_created_by(id, &item, &b.resources, o.get("created_by"))?;
        }
        b.items.push((id, item));
    }
    Ok(())
}
/// Compaction snapshots have no readable parts or native lifecycle field.
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_compaction_item.py
pub(in crate::protocol::openai) fn compaction_content(
    o: &Map<String, Value>,
    final_value: bool,
) -> Result<ReplayValue, CodecError> {
    let content = Text::allowing_empty(
        string(o, "encrypted_content")?,
        "compaction content",
        MAX_TEXT_BYTES,
    )
    .map_err(|_| CodecError::Limit)?;
    let value = if final_value {
        ReplayValue::final_value(ReplayFormat::ResponsesCompaction, content)
    } else {
        ReplayValue::partial(ReplayFormat::ResponsesCompaction, content)
    };
    value.validate_compaction()?;
    Ok(value)
}
fn media_part(
    resource: &ResourceUse,
    cache: Option<PrefixCacheBoundary>,
    resources: &ResourceTable,
) -> Value {
    let resource = resource.media(resources).expect("validated resource");
    let mut v = match resource.kind() {
        ResourceKind::Image => {
            crate::protocol::openai::content::image::write(resource, GenerationApi::Responses)
        }
        ResourceKind::File => crate::protocol::openai::content::file::write(resource),
        ResourceKind::Video => crate::protocol::openai::content::video::write_responses(resource),
        ResourceKind::Audio => unreachable!("lowering rejects audio without a Responses carrier"),
    };
    if cache == Some(PrefixCacheBoundary::RequestLifetime) {
        v["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
    }
    v
}
fn input_wire(parts: &[Part<InputPart>], resources: &ResourceTable) -> Value {
    json!(
        parts
            .iter()
            .map(|part| match &part.content {
                InputPart::Text(text) => input_part(text.as_str(), part.cache),
                InputPart::Resource(resource) => media_part(resource, part.cache, resources),
            })
            .collect::<Vec<_>>()
    )
}
pub(in crate::protocol::openai) fn output_wire(o: &ToolOutput, resources: &ResourceTable) -> Value {
    match o {
        ToolOutput::Text(s) => json!(s),
        ToolOutput::Parts(p) => input_wire(p, resources),
        ToolOutput::Structured(_) => {
            unreachable!("lowering rejects tool result semantics without a carrier")
        }
    }
}
pub(in crate::protocol::openai) fn encode_items(
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    response: bool,
    resources: &ResourceTable,
) -> Result<Vec<Value>, CodecError> {
    if items.iter().any(|(_, i)| i.participant().is_some()) {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let mut out = vec![];
    for (id, item) in items {
        let mut v = match item {
            Item::Instruction(i) => {
                let content = if let [part] = i.parts.as_slice()
                    && part.cache.is_none()
                    && i.status.is_none()
                {
                    json!(part.content.as_str())
                } else {
                    json!(
                        i.parts
                            .iter()
                            .map(|p| input_part(p.content.as_str(), p.cache))
                            .collect::<Vec<_>>()
                    )
                };
                let role = "developer";
                let mut v = json!({ "role": role, "content": content });
                if let Some(status) = i.status {
                    v["type"] = json!("message");
                    v["status"] = json!(status_label(status));
                }
                v
            }
            // Empty owners still carry identity, phase and lifecycle. Omitting
            // one here would also contradict its already emitted SSE item.
            Item::Message(m) => {
                let user = m.role == MessageRole::User;
                let content = m
                    .parts
                    .iter()
                    .map(|p| match &p.content {
                        ContentPart::Text(t) if !response && user => {
                            input_part(t.as_str(), p.cache)
                        }
                        ContentPart::Text(t) => crate::protocol::openai::content::text::write(
                            t,
                            "output_text",
                            resources,
                        ),
                        ContentPart::Refusal(t) => {
                            json!({"type": "refusal", "refusal": t.as_str()})
                        }
                        ContentPart::Resource(value) => media_part(value, p.cache, resources),
                        ContentPart::Audio(_) | ContentPart::AudioReference(_) => {
                            unreachable!("lowering rejects audio without a Responses carrier")
                        }
                    })
                    .collect::<Vec<_>>();
                let role = if user { "user" } else { "assistant" };
                let mut v = json!({"type": "message", "role": role, "content": content});
                if let Some(p) = m.phase {
                    v["phase"] = json!(p.label());
                }
                v
            }
            Item::ToolCall(c) => {
                let mut v = json!({
                    "type": "function_call",
                    "call_id": c.call_id.as_str(),
                    "name": c.name.as_str(),
                    "arguments": c.arguments.as_raw().expect("checked raw arguments"),
                });
                write_call_context(&c.context, v.as_object_mut().expect("object"));
                v
            }
            Item::CustomCall(c) => {
                let mut v = json!({
                    "type": "custom_tool_call",
                    "call_id": c.call_id.as_str(),
                    "name": c.name.as_str(),
                    "input": c.input,
                });
                write_call_context(&c.context, v.as_object_mut().expect("object"));
                v
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                let kind = if matches!(item, Item::CustomResult(_)) {
                    "custom_tool_call_output"
                } else {
                    "function_call_output"
                };
                let output = output_wire(&r.output, resources);
                let mut v = json!({ "type": kind, "output": output });
                if let Some(call_id) = &r.call_id {
                    v["call_id"] = json!(call_id.as_str());
                }
                if let Some(name) = &r.name {
                    v["name"] = json!(name.as_str());
                }
                if let Some(s) = r.status {
                    v["status"] = json!(status_label(s));
                }
                write_call_context(&r.context, v.as_object_mut().expect("object"));
                v
            }
            Item::ConfigurationUpdate(update) => {
                let mut v = json!({"type":"configuration_update"});
                if let Some(effort) = crate::protocol::openai::reasoning::effort_label(update)? {
                    v["reasoning"] = json!({"effort": effort});
                }
                v
            }
            Item::Program(p) => {
                json!({
                    "type": "program",
                    "call_id": p.call_id.as_str(),
                    "code": p.code,
                    "fingerprint": p.fingerprint,
                })
            }
            Item::ProgramOutput(o) => {
                json!({
                    "type": "program_output",
                    "call_id": o.call_id.as_str(),
                    "result": o.result,
                    "status": status_label(o.status),
                })
            }
            Item::Reasoning(r) => crate::protocol::openai::reasoning::encode_item(r, response),
            Item::Compaction(value) => {
                json!({"type":"compaction","encrypted_content":value.as_str()})
            }
            Item::ProviderTool(_) => unreachable!("checked Provider carrier"),
            Item::ToolChange(_) => unreachable!("lowering rejects tool changes"),
        };
        if response {
            if let Some(actor) = fidelity.created_by(*id, item, resources) {
                v["created_by"] = json!(actor);
            }
            v["id"] = json!(
                fidelity
                    .response_item_id(*id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| crate::protocol::fidelity::generated_item_id(*id))
            );
            if let Some(s) = item.lifecycle() {
                v["status"] = json!(status_label(s));
            }
        } else {
            if let Some(wire) = fidelity.response_item_id(*id) {
                if v.get("type").is_none() {
                    v["type"] = json!("message");
                }
                v["id"] = json!(wire);
            } else if matches!(
                item,
                Item::Reasoning(_)
                    | Item::Program(_)
                    | Item::ProgramOutput(_)
                    | Item::Message(Message {
                        role: MessageRole::Assistant,
                        ..
                    })
            ) {
                // Typed snapshots require identity; fresh task items assign it. An input
                // instruction message has no `id` field, so none is minted for it.
                v["id"] = json!(crate::protocol::fidelity::generated_item_id(*id));
            }
            if item.lifecycle().is_some_and(|s| {
                s != ItemLifecycle::Completed
                    || matches!(
                        item,
                        Item::Message(Message {
                            role: MessageRole::Assistant,
                            ..
                        }) | Item::Instruction(_)
                    )
            }) {
                v["status"] = json!(status_label(item.lifecycle().expect("lifecycle")));
            }
        }
        out.push(v);
    }
    Ok(out)
}
