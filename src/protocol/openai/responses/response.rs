//! Whole Responses snapshot decode and encode, including non-success terminals.
use crate::{
    protocol::{
        failure::{CarrierReason, ShapeReason},
        openai::{
            CodecError, DecodedResponse, GenerationApi, ResponseRepresentation, accounting,
            content::intake::Items,
            context,
            lowering::carriers::{
                check_item_carriers, check_message_carriers, check_response_carriers,
            },
            responses,
            shape::*,
            terminal::*,
        },
    },
    semantic::task::generation::*,
};
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
        GenerationApi::Responses,
        adaptation,
        DecodeDirection::Upstream,
    )?;
    let o = object(&v)?;
    crate::protocol::openai::responses::envelope::response_fields(o)?;
    if string(o, "object")? != "response" {
        return Err(CodecError::Invalid(ShapeReason::Object));
    }
    let outcome = match string(o, "status")? {
        "completed" => Outcome::Completed,
        "incomplete" => Outcome::Incomplete,
        "failed" => Outcome::Failed,
        "cancelled" => Outcome::Cancelled,
        _ => {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    };
    let output = o
        .get("output")
        .and_then(Value::as_array)
        .ok_or(CodecError::invalid())?;
    let mut b = Items::new(DecodeDirection::Upstream);
    let item_status = if outcome == Outcome::Completed {
        "completed"
    } else {
        "incomplete"
    };
    responses::items::decode_items(
        &mut b,
        output,
        responses::items::ItemSide::Output,
        item_status,
        false,
    )?;
    let usage = accounting::usage(
        o.get("usage"),
        GenerationApi::Responses,
        adaptation,
        &mut b.evidence,
        crate::protocol::conversion::generation::ConversionBoundary::ResponseDecode,
    )?;
    let semantic = GenerationResponse::from_resources(b.items, outcome, b.resources)?
        .with_usage(usage)?
        .with_details(decode_details(o)?)?;
    check_response_details(semantic.details())?;
    crate::protocol::openai::responses::envelope::record_vendor_shapes(
        GenerationApi::Responses,
        adaptation,
        o,
        &mut b.fidelity,
        Some(&semantic),
    )?;
    let metadata = context::metadata(
        o,
        GenerationApi::Responses,
        adaptation,
        &mut b.evidence,
        crate::protocol::conversion::generation::ConversionBoundary::ResponseDecode,
    )?;
    b.fidelity.capture_routing_extras(
        GenerationApi::Responses,
        adaptation,
        extras,
        &semantic,
        metadata.required_id()?,
    )?;
    Ok(DecodedResponse {
        evidence: b.evidence,
        semantic,
        fidelity: b.fidelity,
        metadata,
    })
}
pub fn encode(target: &ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    if target.profile != GenerationApi::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    if target.semantic.items().iter().any(|(_, item)| {
        matches!(item, Item::ToolResult(r) | Item::CustomResult(r) if r.status.is_none())
    }) {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if !target.metadata.context.system_fingerprint.is_absent() {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
        GenerationApi::Responses,
    )?;
    let output = responses::items::encode_items(
        target.semantic.items(),
        target.fidelity,
        true,
        target.semantic.resources(),
    )?;
    let m = &target.metadata;
    let created = m
        .created
        .as_ref()
        .ok_or(CodecError::Invalid(ShapeReason::Metadata))?;
    let usage = target
        .metadata
        .delivery
        .as_ref()
        .and_then(|delivery| delivery.usage)
        .or_else(|| target.semantic.usage())
        .map(|usage| {
            crate::protocol::openai::lowering::projection::responses_usage(
                usage,
                &target.adaptation.rules,
            )
            .map(|(usage, _)| {
                accounting::encode_usage(usage, GenerationApi::Responses, &target.adaptation.rules)
            })
            .map_err(|_| CodecError::Unsupported(CarrierReason::Usage))
        })
        .transpose()?;
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
