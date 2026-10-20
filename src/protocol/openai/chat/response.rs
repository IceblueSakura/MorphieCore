//! Whole Chat completion decode and encode, including non-success terminals.
use crate::protocol::openai::{
    CodecError, DecodedResponse, Profile, ResponseRepresentation, accounting, chat, context,
    shape::*, terminal::*,
};
use crate::protocol::openai::{
    content::intake::Items,
    lowering::carriers::{check_item_carriers, check_message_carriers, check_response_carriers},
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
        Profile::Chat,
        adaptation,
        DecodeDirection::Upstream,
    )?;
    let o = object(&v)?;
    chat::items::reject_moderation(o)?;
    DecodeDirection::Upstream.fields(
        o,
        &[
            "id",
            "object",
            "created",
            "model",
            "choices",
            "usage",
            "system_fingerprint",
            "service_tier",
            "metadata",
        ],
    )?;
    if string(o, "object")? != "chat.completion" {
        return Err(CodecError::Invalid("response object"));
    }
    let choices = o
        .get("choices")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        .ok_or(CodecError::Unsupported("candidate count".into()))?;
    let c = object(&choices[0])?;
    DecodeDirection::Upstream.fields(c, &["index", "message", "finish_reason", "logprobs"])?;
    if c.get("index").and_then(Value::as_u64) != Some(0) {
        return Err(CodecError::Unsupported("candidate index".into()));
    }
    let outcome = match string(c, "finish_reason")? {
        "stop" | "tool_calls" => Outcome::Completed,
        "length" | "content_filter" => Outcome::Incomplete,
        _ => {
            return Err(CodecError::Unsupported("finish reason".into()));
        }
    };
    let message = object(c.get("message").ok_or(CodecError::Invalid("message"))?)?;
    let mut b = Items::new(DecodeDirection::Upstream);
    chat::items::decode_whole(
        &mut b,
        message,
        adaptation,
        chat::items::MessageSide::Output,
    )?;
    crate::protocol::openai::chat::logprobs::attach(&mut b.items, c.get("logprobs"))?;
    if matches!(outcome, Outcome::Incomplete) {
        for (_, item) in &mut b.items {
            match item {
                Item::ToolCall(call) => call.status = ItemLifecycle::Incomplete,
                Item::Message(m) => m.status = ItemLifecycle::Incomplete,
                // A complete encrypted detail already closed its owner. The
                // carrier can truncate later without making that token partial.
                Item::Reasoning(r)
                    if r.replay
                        .as_ref()
                        .and_then(ReplayValue::replay_token)
                        .is_none() =>
                {
                    r.status = ItemLifecycle::Incomplete;
                }
                _ => {}
            }
        }
    }
    let usage = accounting::usage(o.get("usage"), Profile::Chat, adaptation, &mut b.fidelity)?;
    let groups = chat::owners::envelopes(&b.message_owners)?;
    let semantic = GenerationResponse::from_resources(b.items, outcome, b.resources)?
        .with_usage(usage)?
        .with_message_envelopes(groups)?
        .with_details(if outcome == Outcome::Incomplete {
            TerminalDetails {
                error: None,
                completion: None,
                incomplete: Some(if c["finish_reason"] == "content_filter" {
                    IncompleteReason::ContentFilter
                } else {
                    IncompleteReason::MaxOutputTokens
                }),
            }
        } else {
            TerminalDetails {
                completion: (c["finish_reason"] == "stop")
                    .then_some(CompletionCause::NaturalOrStopSequence),
                ..Default::default()
            }
        })?;
    // Chat's finish label must agree with content even though response outcome
    // no longer encodes a call/no-call distinction.
    if chat_finish(&semantic)? != string(c, "finish_reason")? {
        return Err(CodecError::Invalid("finish reason conflicts with output"));
    }
    let metadata = context::metadata(o, Profile::Chat, adaptation, &mut b.fidelity)?;
    b.fidelity.capture_routing_extras(
        Profile::Chat,
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
    if target.profile != Profile::Chat {
        return Err(CodecError::ProfileMismatch);
    }
    if target
        .semantic
        .items()
        .iter()
        .any(|(_, item)| matches!(item, Item::ToolResult(_) | Item::CustomResult(_)))
    {
        return Err(CodecError::Unsupported("Chat result output".into()));
    }
    check_response_carriers(&target.semantic)?;
    crate::protocol::openai::content::citations::check_items(
        target.semantic.items(),
        target.semantic.resources(),
        target.profile,
    )?;
    check_item_carriers(target.semantic.items())?;
    target
        .fidelity
        .check_wire_item_ids(target.semantic.items(), true)?;
    let finish = chat_finish(&target.semantic)?;
    check_message_carriers(
        target.semantic.items(),
        target.semantic.message_envelopes(),
        Profile::Chat,
    )?;
    let mut messages = chat::items::encode_items_with(
        target.semantic.items(),
        &chat::owners::owners(target.semantic.items(), target.semantic.message_envelopes()),
        target.fidelity,
        target.adaptation.rules.structured_chat_reasoning,
        true,
        target.semantic.resources(),
    );
    if target.adaptation.rules.reasoning_alias {
        for message in &mut messages {
            crate::protocol::openai::dialect::adapter_shapes::encode_message(message);
        }
    }
    let message = messages
        .first_mut()
        .ok_or(CodecError::Invalid("empty Chat candidate"))?;
    if let Some(carrier) = &target.attachment {
        crate::protocol::attachments::check_chat_support(
            Profile::Chat,
            target.adaptation.rules.structured_chat_reasoning,
        )?;
        let details = message
            .as_object_mut()
            .ok_or(CodecError::Invalid("Chat message"))?
            .entry("reasoning_details")
            .or_insert_with(|| json!([]));
        details
            .as_array_mut()
            .ok_or(CodecError::Invalid("Chat reasoning details"))?
            .push(crate::protocol::attachments::chat_detail(carrier.as_str()));
    }
    let m = &target.metadata;
    let created = m
        .created
        .as_ref()
        .ok_or(CodecError::Invalid("created time"))?;
    let mut value = json!({"id":m.id,"object":"chat.completion","created":created,"model":m.model,
        "choices":[{"index":0,"message":message,"finish_reason":finish}],
        "usage":target.semantic.usage().map(|usage| accounting::encode_usage(usage, Profile::Chat, &target.adaptation.rules))});
    if let Some(probabilities) =
        crate::protocol::openai::chat::logprobs::choice(target.semantic.items())?
    {
        value["choices"][0]["logprobs"] = probabilities;
    }
    put_presence(
        value.as_object_mut().expect("object"),
        "metadata",
        &m.context.execution.metadata,
        |v| json!(v),
    );
    put_presence(
        value.as_object_mut().expect("object"),
        "system_fingerprint",
        &m.context.system_fingerprint,
        |v| json!(v),
    );
    crate::protocol::openai::service::write_class(
        value.as_object_mut().expect("object"),
        m.context.service_class(),
        Profile::Chat,
    )?;
    crate::protocol::openai::responses::envelope::write_response_extras(
        target.fidelity,
        target.profile,
        &target.adaptation,
        &target.semantic,
        target.metadata.required_id()?,
        value.as_object_mut().expect("object"),
    );
    bounded(&value)?;
    Ok(value)
}
