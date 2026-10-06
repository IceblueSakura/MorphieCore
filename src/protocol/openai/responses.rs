//! Responses task payload codec. HTTP request bindings are owned by `envelope`.
pub use super::static_response::{
    decode_responses as decode_response, encode_responses as encode_response,
};
use super::{CodecError, DecodedRequest, Profile, RequestRepresentation, common::*, settings};
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value, json};
pub fn decode_generation(v: &Value) -> Result<DecodedRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    let allowed: Vec<_> = settings::FIELDS.iter().copied().chain(["input"]).collect();
    fields(o, &allowed)?;
    let mut b = Items::default();
    match o.get("input") {
        Some(Value::String(s)) => {
            let id = b.id()?;
            let part = b.part(s)?;
            b.items.push((
                id,
                Item::Message(Message {
                    role: MessageRole::User,
                    parts: vec![part],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ));
        }
        Some(Value::Array(a)) => decode_items(&mut b, a, false, "completed")?,
        None if o.get("instructions").is_some_and(Value::is_string) => {}
        _ => return Err(CodecError::Invalid("input")),
    }
    let semantic = GenerationRequest::from_settings(b.items, settings::read(o, false)?)?;
    validate_program_history(&semantic)?;
    Ok(DecodedRequest {
        semantic,
        fidelity: b.fidelity,
    })
}
/// Responses replay requires each program's reported output. This wire constraint
/// must not prevent protocol-neutral IR from representing an outstanding program.
pub(crate) fn validate_program_history(request: &GenerationRequest) -> Result<(), CodecError> {
    let outputs: std::collections::BTreeSet<_> = request
        .items()
        .iter()
        .filter_map(|(_, item)| {
            if let Item::ProgramOutput(output) = item {
                Some(output.call_id.as_str())
            } else {
                None
            }
        })
        .collect();
    // Request validation already proves kind, uniqueness and preceding-call order.
    if request.items().iter().any(|(_, item)| {
        matches!(item, Item::Program(program) if !outputs.contains(program.call_id.as_str()))
    }) {
        return Err(CodecError::Invalid("program history"));
    }
    Ok(())
}
pub(super) fn read_phase(o: &Map<String, Value>) -> Result<Option<Phase>, CodecError> {
    match o.get("phase") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => s
            .parse::<Phase>()
            .map(Some)
            .map_err(|_| CodecError::Invalid("phase")),
        _ => Err(CodecError::Invalid("phase")),
    }
}
pub(super) fn status(
    o: &Map<String, Value>,
    default: ItemLifecycle,
) -> Result<ItemLifecycle, CodecError> {
    match o.get("status") {
        None => Ok(default),
        Some(Value::String(s)) => match s.as_str() {
            "completed" => Ok(ItemLifecycle::Completed),
            "incomplete" => Ok(ItemLifecycle::Incomplete),
            "in_progress" => Ok(ItemLifecycle::InProgress),
            _ => Err(CodecError::Invalid("item status")),
        },
        _ => Err(CodecError::Invalid("item status")),
    }
}
pub(super) fn status_label(s: ItemLifecycle) -> &'static str {
    match s {
        ItemLifecycle::Completed => "completed",
        ItemLifecycle::Incomplete => "incomplete",
        ItemLifecycle::InProgress => "in_progress",
    }
}
pub(super) fn call_context(o: &Map<String, Value>) -> Result<CallContext, CodecError> {
    let namespace = match o.get("namespace") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(text(s, "tool namespace", 128)?),
        _ => return Err(CodecError::Invalid("tool namespace")),
    };
    let async_call = match o.get("async") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        _ => return Err(CodecError::Invalid("async")),
    };
    let caller = match o.get("caller") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let caller = object(value)?;
            fields(caller, &["type", "caller_id"])?;
            match string(caller, "type")? {
                "direct" => {
                    if caller.contains_key("caller_id") {
                        return Err(CodecError::Invalid("caller"));
                    }
                    None
                }
                "program" => Some(CallOrigin::Program {
                    caller_id: text(string(caller, "caller_id")?, "caller id", 256)?,
                }),
                _ => return Err(CodecError::Unsupported("caller".into())),
            }
        }
    };
    Ok(CallContext {
        namespace,
        async_call,
        caller,
    })
}
pub(super) fn write_call_context(context: &CallContext, o: &mut Map<String, Value>) {
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
fn output(b: &mut Items, v: &Value) -> Result<ToolOutput, CodecError> {
    if let Some(s) = v.as_str() {
        if s.len() > MAX_TEXT_BYTES {
            return Err(CodecError::Limit);
        }
        return Ok(ToolOutput::Text(s.into()));
    }
    let a = v.as_array().ok_or(CodecError::Invalid("tool output"))?;
    if a.len() > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let mut p = vec![];
    for v in a {
        let o = object(v)?;
        let part = match string(o, "type")? {
            "input_text" => {
                fields(o, &["type", "text", "prompt_cache_breakpoint"])?;
                let id = b.part_id()?;
                record_input_form(b, id, o)?;
                (
                    id,
                    ToolResultPart::Text(
                        Text::allowing_empty(string(o, "text")?, "tool output", MAX_TEXT_BYTES)
                            .map_err(|_| CodecError::Limit)?,
                    ),
                )
            }
            // The pinned function/custom result union includes standard image
            // content. File IDs still require an issuer/resource lifecycle.
            // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_function_call_output_item_list_param.py
            "input_image" => (
                b.part_id()?,
                ToolResultPart::Resource(super::image::read(o, Profile::Responses)?),
            ),
            _ => return Err(CodecError::Unsupported("tool result media".into())),
        };
        p.push(part);
    }
    Ok(ToolOutput::Parts(p))
}
fn record_input_form(b: &mut Items, id: PartId, o: &Map<String, Value>) -> Result<(), CodecError> {
    b.fidelity.record_input_text(id)?;
    if let Some(v) = o.get("prompt_cache_breakpoint") {
        if v != &json!({"mode":"explicit"}) {
            return Err(CodecError::Invalid("prompt cache breakpoint"));
        }
        b.fidelity.record_cache_breakpoint(id)?;
    }
    Ok(())
}
fn input_part(id: PartId, text: &str, fidelity: &FidelityRecords) -> Value {
    let mut v = json!({"type":"input_text","text":text});
    if fidelity.cache_breakpoint(id) {
        v["prompt_cache_breakpoint"] = json!({"mode":"explicit"});
    }
    v
}
pub(super) fn configuration_effort(
    o: &Map<String, Value>,
    response: bool,
) -> Result<Option<ReasoningEffort>, CodecError> {
    match o.get("reasoning") {
        None => Ok(None),
        Some(Value::Null) if response => Ok(None),
        Some(Value::Null) => Err(CodecError::Invalid("configuration reasoning")),
        Some(value) => {
            let reasoning = object(value)?;
            fields(reasoning, &["effort"])?;
            match reasoning.get("effort") {
                None | Some(Value::Null) => Ok(None),
                Some(value) => Ok(Some(super::reasoning::effort(
                    value
                        .as_str()
                        .ok_or(CodecError::Invalid("configuration effort"))?,
                )?)),
            }
        }
    }
}
pub(super) fn decode_items(
    b: &mut Items,
    input: &[Value],
    response: bool,
    item_status: &str,
) -> Result<(), CodecError> {
    for value in input {
        let o = object(value)?;
        let id = b.id()?;
        let typ = o
            .get("type")
            .map(|v| v.as_str().ok_or(CodecError::Invalid("item type")))
            .transpose()?;
        let item = match typ {
            Some("function_call") => Item::ToolCall(tool_call(
                o,
                Profile::Responses,
                None,
                response.then_some(item_status),
                !response,
            )?),
            Some("custom_tool_call") => {
                fields(
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
                Item::CustomCall(CustomCall {
                    call_id: text(string(o, "call_id")?, "call id", 256)?,
                    name: text(string(o, "name")?, "custom name", 128)?,
                    input: raw_string(o, "input")?,
                    context: call_context(o)?,
                })
            }
            Some("function_call_output" | "custom_tool_call_output") if !response => {
                fields(
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
                let context = call_context(o)?;
                let call_id = string(o, "call_id")?;
                if let Some(name) = o.get("name").filter(|v| !v.is_null()) {
                    let expected = b.items.iter().find_map(|(_, i)| match i {
                        Item::ToolCall(c) if c.call_id.as_str() == call_id => Some(c.name.as_str()),
                        Item::CustomCall(c) if c.call_id.as_str() == call_id => {
                            Some(c.name.as_str())
                        }
                        _ => None,
                    });
                    if name.as_str() != expected {
                        return Err(CodecError::Invalid("tool result name"));
                    }
                }
                let r = ToolResult {
                    execution: None,
                    call_id: text(call_id, "call id", 256)?,
                    output: output(
                        b,
                        o.get("output").ok_or(CodecError::Invalid("tool output"))?,
                    )?,
                    status: if o.contains_key("status") {
                        Some(status(o, ItemLifecycle::Completed)?)
                    } else {
                        None
                    },
                    context,
                };
                if typ == Some("custom_tool_call_output") {
                    if r.status.is_some() {
                        return Err(CodecError::Unsupported("custom result status".into()));
                    }
                    Item::CustomResult(r)
                } else {
                    Item::ToolResult(r)
                }
            }
            Some("configuration_update") => {
                fields(o, &["type", "id", "reasoning"])?;
                if response {
                    text(string(o, "id")?, "configuration update id", 256)?;
                } else if o
                    .get("id")
                    .is_some_and(|v| !v.is_null() && v.as_str().is_none())
                {
                    return Err(CodecError::Invalid("configuration update id"));
                }
                Item::ConfigurationUpdate(ConfigurationUpdate {
                    effort: configuration_effort(o, response)?,
                })
            }
            Some("program") => {
                fields(o, &["type", "id", "call_id", "code", "fingerprint"])?;
                text(string(o, "id")?, "program id", 256)?;
                Item::Program(Program {
                    call_id: text(string(o, "call_id")?, "call id", 256)?,
                    code: raw_string(o, "code")?,
                    fingerprint: raw_string(o, "fingerprint")?,
                })
            }
            Some("program_output") => {
                fields(o, &["type", "id", "call_id", "result", "status"])?;
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
                        _ => return Err(CodecError::Invalid("program output status")),
                    },
                })
            }
            Some("reasoning") => {
                // Wire reasoning items carry required identity; it is never invented.
                text(string(o, "id")?, "reasoning id", 256)?;
                let r = super::reasoning::decode_item(
                    o,
                    &mut || b.part_id(),
                    !response || item_status != "completed",
                )?;
                b.fidelity.record_replay(id, &r, None)?;
                Item::Reasoning(r)
            }
            Some("message") | None => {
                fields(
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
                    return Err(CodecError::Invalid("phase"));
                }
                let phase = read_phase(o)?;
                let content = o.get("content").ok_or(CodecError::Invalid("content"))?;
                let values = if let Some(s) = content.as_str() {
                    if response {
                        return Err(CodecError::Invalid("output content"));
                    }
                    vec![if role == "assistant" {
                        // Shorthand has no annotation slot; the typed part still requires the array.
                        json!({"type":"output_text","text":s,"annotations":[]})
                    } else {
                        json!({"type":"input_text","text":s})
                    }]
                } else {
                    content
                        .as_array()
                        .ok_or(CodecError::Invalid("content parts"))?
                        .clone()
                };
                if values.len() > MAX_ITEMS {
                    return Err(CodecError::Limit);
                }
                if phase == Some(Phase::Commentary)
                    && values
                        .iter()
                        .any(|p| p.get("parsed").is_some_and(|v| !v.is_null()))
                {
                    // The pinned SDK never derives a parsed view from commentary text.
                    return Err(CodecError::Invalid("parsed"));
                }
                if role == "system" || role == "developer" {
                    // Omitted and explicit completed stay without a lifecycle field.
                    let instruction_status = status(o, ItemLifecycle::Completed)?;
                    let instruction_status = (instruction_status != ItemLifecycle::Completed)
                        .then_some(instruction_status);
                    let mut parts = vec![];
                    for v in &values {
                        let p = object(v)?;
                        fields(p, &["type", "text", "prompt_cache_breakpoint"])?;
                        if string(p, "type")? != "input_text" {
                            return Err(CodecError::Invalid("instruction part"));
                        }
                        let part_id = b.part_id()?;
                        record_input_form(b, part_id, p)?;
                        parts.push((
                            part_id,
                            Text::allowing_empty(string(p, "text")?, "instruction", MAX_TEXT_BYTES)
                                .map_err(|_| CodecError::Limit)?,
                        ));
                    }
                    Item::Instruction(Instruction {
                        authority: if role == "system" {
                            InstructionAuthority::System
                        } else {
                            InstructionAuthority::Developer
                        },
                        parts,
                        status: instruction_status,
                    })
                } else {
                    let role = match role {
                        "user" => MessageRole::User,
                        "assistant" => MessageRole::Assistant,
                        _ => return Err(CodecError::Unsupported("message role".into())),
                    };
                    let mut parts = vec![];
                    for v in &values {
                        let p = object(v)?;
                        let kind = string(p, "type")?;
                        let content = match kind {
                            "refusal" if role == MessageRole::Assistant => {
                                fields(p, &["type", "refusal"])?;
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
                                fields(p, &["type", "text", "prompt_cache_breakpoint"])?;
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
                                ContentPart::Resource(super::image::read(p, Profile::Responses)?)
                            }
                            "input_file" if !response && role == MessageRole::User => {
                                ContentPart::Resource(super::file::read(p)?)
                            }
                            "output_text" if role == MessageRole::Assistant => {
                                ContentPart::Text(super::text::read(p, !response)?)
                            }
                            _ => return Err(CodecError::Unsupported("content part".into())),
                        };
                        let part_id = b.part_id()?;
                        if kind == "input_text" {
                            record_input_form(b, part_id, p)?;
                        }
                        parts.push(Part {
                            id: part_id,
                            content,
                        });
                    }
                    let status = status(o, ItemLifecycle::Completed)?;
                    if response && item_status == "completed" && status != ItemLifecycle::Completed
                    {
                        return Err(CodecError::Invalid("completed item"));
                    }
                    Item::Message(Message {
                        role,
                        parts,
                        status,
                        phase,
                    })
                }
            }
            _ => return Err(CodecError::Unsupported("Responses item kind".into())),
        };
        // Function/custom IDs are optional/nullable; other required IDs have
        // already been validated by their owning branch or complete boundary.
        if !(matches!(typ, Some("function_call" | "custom_tool_call"))
            && o.get("id").is_some_and(Value::is_null))
        {
            b.record_id(id, o)?;
        }
        b.items.push((id, item));
    }
    Ok(())
}
pub fn encode_generation(target: &RequestRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    target.semantic.validate()?;
    check_item_carriers(target.semantic.items())?;
    if target.semantic.items().iter().any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result)
        if result.execution.is_some() || matches!(result.output, ToolOutput::Structured(_))
            || matches!(&result.output, ToolOutput::Parts(parts)
                if parts.iter().any(|(id, part)| matches!(part, ToolResultPart::Resource(_)) && target.fidelity.cache_breakpoint(*id))))) {
        return Err(CodecError::Unsupported("tool result semantics".into()));
    }
    let mut v = json!({"input":encode_items(target.semantic.items(), target.fidelity, false)});
    settings::write(
        target.semantic.settings(),
        v.as_object_mut().expect("object"),
    )?;
    bounded(&v)?;
    Ok(v)
}
fn output_wire(o: &ToolOutput, fidelity: &FidelityRecords) -> Value {
    match o {
        ToolOutput::Text(s) => json!(s),
        ToolOutput::Parts(p) => json!(
            p.iter()
                .map(|(id, part)| match part {
                    ToolResultPart::Text(text) => input_part(*id, text.as_str(), fidelity),
                    ToolResultPart::Resource(resource) =>
                        super::image::write(resource, Profile::Responses),
                })
                .collect::<Vec<_>>()
        ),
        ToolOutput::Structured(_) => {
            unreachable!("lowering rejects tool result semantics without a carrier")
        }
    }
}
pub(super) fn encode_items(
    items: &[(ItemId, Item)],
    fidelity: &FidelityRecords,
    response: bool,
) -> Vec<Value> {
    let mut out = vec![];
    for (id, item) in items {
        let mut v = match item {
            Item::Instruction(i) => {
                let mut v = json!({"role":match i.authority{InstructionAuthority::System=>"system",InstructionAuthority::Developer=>"developer"},"content":if let [(id,t)]=i.parts.as_slice() && !fidelity.cache_breakpoint(*id){json!(t.as_str())}else{json!(i.parts.iter().map(|(id,t)|input_part(*id,t.as_str(),fidelity)).collect::<Vec<_>>())}});
                if let Some(status) = i.status {
                    v["type"] = json!("message");
                    v["status"] = json!(status_label(status));
                }
                v
            }
            // Empty owners still carry identity, phase and lifecycle. Omitting
            // one here would also contradict its already emitted SSE item.
            Item::Message(m) => {
                let mut v = json!({"type":"message","role":if m.role==MessageRole::User{"user"}else{"assistant"},"content":m.parts.iter().map(|p|match &p.content{ContentPart::Text(t)=>if !response && (m.role==MessageRole::User || fidelity.input_text_form(p.id) && t.is_plain()){input_part(p.id,t.as_str(),fidelity)}else{super::text::write(t,"output_text")},ContentPart::Refusal(t)=>json!({"type":"refusal","refusal":t.as_str()}),ContentPart::Resource(resource)=>match resource.kind(){ResourceKind::Image=>super::image::write(resource,Profile::Responses),ResourceKind::File=>super::file::write(resource),ResourceKind::Audio=>unreachable!("lowering rejects audio resources without a Responses carrier")},ContentPart::Audio(_)|ContentPart::AudioReference(_)=>unreachable!("lowering rejects audio without a Responses carrier")}).collect::<Vec<_>>()});
                if let Some(p) = m.phase {
                    v["phase"] = json!(p.label());
                }
                v
            }
            Item::ToolCall(c) => {
                let mut v = json!({"type":"function_call","call_id":c.call_id.as_str(),"name":c.name.as_str(),"arguments":c.arguments.as_raw().expect("checked raw arguments")});
                write_call_context(&c.context, v.as_object_mut().expect("object"));
                v
            }
            Item::CustomCall(c) => {
                let mut v = json!({"type":"custom_tool_call","call_id":c.call_id.as_str(),"name":c.name.as_str(),"input":c.input});
                write_call_context(&c.context, v.as_object_mut().expect("object"));
                v
            }
            Item::ToolResult(r) | Item::CustomResult(r) => {
                let mut v = json!({"type":if matches!(item,Item::CustomResult(_)){"custom_tool_call_output"}else{"function_call_output"},"call_id":r.call_id.as_str(),"output":output_wire(&r.output, fidelity)});
                if let Some(s) = r.status {
                    v["status"] = json!(status_label(s));
                }
                write_call_context(&r.context, v.as_object_mut().expect("object"));
                v
            }
            Item::ConfigurationUpdate(update) => {
                let mut v = json!({"type":"configuration_update"});
                if let Some(effort) = update.effort {
                    v["reasoning"] = json!({"effort": super::reasoning::effort_label(effort)});
                }
                v
            }
            Item::Program(p) => {
                json!({"type":"program","call_id":p.call_id.as_str(),"code":p.code,"fingerprint":p.fingerprint})
            }
            Item::ProgramOutput(o) => {
                json!({"type":"program_output","call_id":o.call_id.as_str(),"result":o.result,"status":status_label(o.status)})
            }
            Item::Reasoning(r) => super::reasoning::encode_item(r, response),
        };
        if response {
            v["id"] = json!(
                fidelity
                    .response_item_id(*id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("item_{}", id.get()))
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
                Item::Reasoning(_) | Item::Program(_) | Item::ProgramOutput(_)
            ) || matches!(item, Item::Instruction(i) if i.status.is_some())
            {
                // Typed snapshots require identity; fresh task items assign it.
                v["id"] = json!(format!("item_{}", id.get()));
            }
            if item
                .lifecycle()
                .is_some_and(|s| s != ItemLifecycle::Completed)
            {
                v["status"] = json!(status_label(item.lifecycle().expect("lifecycle")));
            }
        }
        out.push(v);
    }
    out
}
