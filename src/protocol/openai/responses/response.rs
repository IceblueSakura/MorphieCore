//! Whole Responses snapshot decode and encode, including non-success terminals.
use crate::protocol::openai::{
    CodecError, DecodedResponse, Profile, ResponseRepresentation, accounting, common::*, context,
    responses, terminal::*,
};
use crate::semantic::task::generation::*;
use serde_json::{Value, json};

pub fn decode(v: &Value) -> Result<DecodedResponse, CodecError> {
    decode_with(v, &Default::default())
}
pub(crate) fn decode_with(
    v: &Value,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
) -> Result<DecodedResponse, CodecError> {
    let (v, extras) = crate::protocol::openai::dialect::adapter_shapes::decode(
        v,
        Profile::Responses,
        adaptation,
        DecodeDirection::Upstream,
    )?;
    let o = object(&v)?;
    crate::protocol::openai::responses::envelope::response_fields(o)?;
    if string(o, "object")? != "response" {
        return Err(CodecError::Invalid("response object"));
    }
    let outcome = match string(o, "status")? {
        "completed" => Outcome::Completed,
        "incomplete" => Outcome::Incomplete,
        "failed" => Outcome::Failed,
        "cancelled" => Outcome::Cancelled,
        _ => return Err(CodecError::Unsupported("response status".into())),
    };
    let output = o
        .get("output")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("output"))?;
    let mut b = Items::new(DecodeDirection::Upstream);
    let item_status = if outcome == Outcome::Completed {
        "completed"
    } else {
        "incomplete"
    };
    responses::items::decode_items(&mut b, output, true, item_status)?;
    for (_, item) in &mut b.items {
        if let Item::ConfigurationUpdate(update) = item {
            let native = update.effort;
            crate::protocol::openai::reasoning::admit_configuration_mapping(
                update,
                adaptation.effort_mapping,
                true,
            )?;
            if let (Some(mapping), Some(input), Some(output)) =
                (adaptation.effort_mapping, native, update.effort)
                && input != output
            {
                b.fidelity.record_effort_mapping(
                    mapping.rule,
                    input.level.into(),
                    output.level.into(),
                );
            }
        }
    }
    let usage = accounting::usage(
        o.get("usage"),
        Profile::Responses,
        adaptation,
        &mut b.fidelity,
    )?;
    let semantic = GenerationResponse::from_resources(b.items, outcome, b.resources)?
        .with_usage(usage)?
        .with_details(decode_details(o)?)?;
    crate::protocol::openai::responses::envelope::record_vendor_shapes(
        Profile::Responses,
        adaptation,
        o,
        &mut b.fidelity,
        Some(&semantic),
    )?;
    let metadata = context::metadata(o, Profile::Responses, adaptation, &mut b.fidelity)?;
    b.fidelity.capture_routing_extras(
        Profile::Responses,
        adaptation,
        extras,
        &semantic,
        metadata.required_id()?,
    )?;
    Ok(DecodedResponse {
        semantic,
        fidelity: b.fidelity,
        metadata,
    })
}
pub fn encode(target: &ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    if target.semantic.items().iter().any(|(_, item)| {
        matches!(item, Item::ToolResult(r) | Item::CustomResult(r) if r.status.is_none())
    }) {
        return Err(CodecError::Unsupported("result output lifecycle".into()));
    }
    if !target.metadata.context.system_fingerprint.is_absent() {
        return Err(CodecError::Unsupported("unprojected fingerprint".into()));
    }
    check_response_carriers(&target.semantic)?;
    crate::protocol::openai::content::citations::check_items(
        target.semantic.items(),
        target.semantic.resources(),
        target.profile,
    )?;
    check_item_carriers(target.semantic.items())?;
    let status = match target.semantic.outcome() {
        Outcome::Completed => "completed",
        Outcome::Incomplete => "incomplete",
        Outcome::Failed => "failed",
        Outcome::Cancelled => "cancelled",
    };
    target
        .fidelity
        .check_wire_item_ids(target.semantic.items(), true)?;
    check_message_carriers(
        target.semantic.items(),
        target.semantic.message_envelopes(),
        Profile::Responses,
    )?;
    let mut output = responses::items::encode_items(
        target.semantic.items(),
        target.fidelity,
        true,
        target.semantic.resources(),
    )?;
    if let Some(carrier) = &target.attachment {
        let item = crate::protocol::attachments::response_item(carrier.as_str());
        if output.iter().any(|old| old.get("id") == item.get("id")) {
            return Err(CodecError::Invalid("attachment identity"));
        }
        output.push(item);
    }
    let m = &target.metadata;
    let created = m
        .created
        .as_ref()
        .ok_or(CodecError::Invalid("created time"))?;
    let usage = target
        .semantic
        .usage()
        .map(|usage| accounting::encode_usage(usage, Profile::Responses, &target.adaptation.rules));
    let mut value = json!({
        "id": m.id,
        "object": "response",
        "created_at": created,
        "model": m.model,
        "status": status,
        "output": output,
        "usage": usage,
    });
    crate::protocol::openai::responses::envelope::write_metadata(
        m,
        value.as_object_mut().expect("object"),
        status == "completed",
    )?;
    crate::protocol::openai::responses::envelope::write_response_extras(
        target.fidelity,
        target.profile,
        &target.adaptation,
        &target.semantic,
        target.metadata.required_id()?,
        value.as_object_mut().expect("object"),
    );
    if target.semantic.details().error.is_some() {
        value["error"] = encode_error(target.semantic.details().error.as_ref());
    }
    if target.semantic.details().incomplete.is_some() {
        value["incomplete_details"] =
            encode_incomplete(target.semantic.details().incomplete.as_ref())?;
    }
    bounded(&value)?;
    Ok(value)
}
