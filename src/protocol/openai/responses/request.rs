//! Complete Responses task requests: settings and validated full-history input.
use super::items::{ItemSide, decode_items, encode_items};
use crate::protocol::openai::{
    CodecError, DecodedRequest, Profile, RequestRepresentation, settings, shape::*,
};
use crate::protocol::openai::{
    content::intake::Items,
    lowering::carriers::{check_item_carriers, check_message_carriers},
};
use crate::semantic::task::generation::*;
use serde_json::{Value, json};
pub fn decode_generation(v: &Value) -> Result<DecodedRequest, CodecError> {
    decode_generation_in(v, DecodeDirection::Request)
}
pub(crate) fn decode_generation_in(
    v: &Value,
    direction: DecodeDirection<'_>,
) -> Result<DecodedRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    let allowed: Vec<_> = settings::FIELDS.iter().copied().chain(["input"]).collect();
    direction.fields(o, &allowed)?;
    let mut b = Items::new(direction);
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
        None if o.get("instructions").is_some_and(Value::is_string) => {}
        _ => return Err(CodecError::Invalid("input")),
    }
    let mut groups = vec![];
    crate::protocol::attachments::restore(
        &mut b.items,
        &mut groups,
        &mut b.fidelity,
        std::mem::take(&mut b.attachments),
    )?;
    let semantic = GenerationRequest::from_resources(
        b.items,
        settings::read(o, false, direction)?,
        b.resources,
    )?
    .with_message_envelopes(groups)?;
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
pub fn encode_generation(target: &RequestRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    check_message_carriers(
        target.semantic.items(),
        target.semantic.message_envelopes(),
        Profile::Responses,
    )?;
    target.semantic.validate()?;
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::dialect::bailian::check_semantic(
            &target.semantic,
            Profile::Responses,
            "",
        )?;
    }
    check_item_carriers(target.semantic.items())?;
    crate::protocol::openai::content::citations::check_items(
        target.semantic.items(),
        target.semantic.resources(),
        Profile::Responses,
    )?;
    target
        .fidelity
        .check_wire_item_ids(target.semantic.items(), false)?;
    let input = encode_items(
        target.semantic.items(),
        target.fidelity,
        false,
        target.semantic.resources(),
    )?;
    let mut v = json!({ "input": input });
    if target.adaptation.rules.bailian_requests || target.adaptation.rules.thinking_controls {
        let mut standard = target.semantic.settings().clone();
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
            Profile::Responses,
        )?;
        if target.adaptation.rules.bailian_requests {
            crate::protocol::openai::dialect::bailian::write_tools(
                target.semantic.settings(),
                o,
                Profile::Responses,
            )?;
        }
    } else {
        settings::write(
            target.semantic.settings(),
            v.as_object_mut().expect("object"),
        )?;
    }
    bounded(&v)?;
    Ok(v)
}
