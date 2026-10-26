//! Complete Responses task requests: settings and validated full-history input.
use super::items::{ItemSide, decode_items, encode_items};
use crate::{
    protocol::openai::{
        CodecError, DecodedRequest, GenerationApi, RequestRepresentation,
        content::intake::Items,
        lowering::carriers::{check_item_carriers, check_message_carriers},
        settings,
        shape::*,
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};
pub fn decode_generation(v: &Value) -> Result<DecodedRequest, CodecError> {
    decode_generation_in(v, DecodeDirection::Request)
}
pub(crate) fn decode_generation_in(
    v: &Value,
    direction: DecodeDirection<'_>,
) -> Result<DecodedRequest, CodecError> {
    decode_generation_with_history(v, direction, None)
}
pub(super) fn decode_generation_with_history(
    v: &Value,
    direction: DecodeDirection<'_>,
    history: Option<(&DecodedRequest, &crate::protocol::DecodedResponse)>,
) -> Result<DecodedRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    let allowed: Vec<_> = settings::FIELDS.iter().copied().chain(["input"]).collect();
    direction.fields(o, &allowed)?;
    let (mut b, mut groups) = match history {
        Some((input, output)) => super::history::seed(input, output, direction)?,
        None => (Items::new(direction), vec![]),
    };
    match o.get("input") {
        Some(Value::String(s)) => {
            let id = b.id()?;
            let part = b.part(s)?;
            b.items.push((
                id,
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::User,
                    parts: vec![part],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ));
        }
        Some(Value::Array(a)) => decode_items(&mut b, a, ItemSide::History, "completed")?,
        None if history.is_some() || o.get("instructions").is_some_and(Value::is_string) => {}
        _ => {
            return Err(CodecError::invalid());
        }
    }
    if let Some((input, output)) = history {
        for (index, group) in groups.iter_mut().enumerate() {
            if group.members().is_empty() && group.before().is_none() {
                let boundary = input.semantic.items().len()
                    + if index < input.semantic.message_envelopes().len() {
                        0
                    } else {
                        output.semantic.items().len()
                    };
                *group = MessageEnvelope::empty(
                    group.id(),
                    group.role(),
                    b.items.get(boundary).map(|(id, _)| *id),
                );
            }
        }
    }
    let semantic = GenerationRequest::from_resources(
        b.items,
        settings::read(o, false, direction)?,
        b.resources,
    )?
    .with_message_envelopes(groups)?
    .with_cached_context(history.and_then(|(input, _)| input.semantic.cached_context().cloned()))?;
    validate_program_history(&semantic)?;
    Ok(DecodedRequest {
        evidence: b.evidence,
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
        return Err(CodecError::invalid());
    }
    Ok(())
}
pub fn encode_generation(target: &RequestRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != GenerationApi::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    check_message_carriers(
        target.semantic.items(),
        target.semantic.message_envelopes(),
        GenerationApi::Responses,
    )?;
    target.semantic.validate()?;
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::dialect::bailian::check_semantic(
            &target.semantic,
            GenerationApi::Responses,
            "",
        )?;
    }
    check_item_carriers(target.semantic.items())?;
    crate::protocol::openai::content::citations::check_items(
        target.semantic.items(),
        target.semantic.resources(),
        GenerationApi::Responses,
    )?;
    target
        .fidelity
        .check_wire_item_ids(target.semantic.items(), false)?;
    let mut input = encode_items(
        target.semantic.items(),
        target.fidelity,
        false,
        target.semantic.resources(),
    )?;
    let mut encoded_settings = target.semantic.settings().clone();
    if let Some(root) = encoded_settings.instructions.value()
        && root.plain_text().is_none()
    {
        input.insert(
            0,
            json!({
                "role":"developer",
                "content":settings::instruction_content(root, GenerationApi::Responses)?,
            }),
        );
        encoded_settings.instructions = Presence::Absent;
    }
    for item in &mut input {
        if target.adaptation.rules.responses_typed_messages
            && matches!(
                item.get("role").and_then(Value::as_str),
                Some("system" | "developer" | "user" | "assistant")
            )
        {
            item["type"] = json!("message");
        }
        if target.adaptation.rules.system_instruction_messages
            && item.get("role").and_then(Value::as_str) == Some("developer")
        {
            item["role"] = json!("system");
        }
    }
    let mut v = json!({ "input": input });
    if target.adaptation.rules.bailian_requests || target.adaptation.rules.thinking_controls {
        let mut standard = encoded_settings.clone();
        standard.reasoning = ReasoningRequest::absent();
        standard.effort = crate::semantic::value::Presence::Absent;
        if target.adaptation.rules.bailian_requests {
            standard.tools = None;
            standard.tool_choice = None;
            standard.parallel_tool_calls = None;
        }
        let o = v.as_object_mut().expect("object");
        settings::write(&standard, o)?;
        crate::protocol::openai::dialect::bailian::write_reasoning(
            target.semantic.settings(),
            o,
            GenerationApi::Responses,
        )?;
        if target.adaptation.rules.bailian_requests {
            crate::protocol::openai::dialect::bailian::write_tools(
                target.semantic.settings(),
                o,
                GenerationApi::Responses,
            )?;
        }
    } else {
        settings::write(&encoded_settings, v.as_object_mut().expect("object"))?;
    }
    bounded(&v)?;
    Ok(v)
}
