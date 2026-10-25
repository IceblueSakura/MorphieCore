//! Complete Chat task requests: settings, tools and ordered message history.
use super::{
    items::{MessageSide, decode_whole, encode_items_with, request_text_parts},
    owners,
};
use crate::protocol::openai::{
    CodecError, DecodedRequest, Profile, RequestRepresentation, function_tools, shape::*,
};
use crate::protocol::openai::{
    content::intake::Items,
    lowering::carriers::{check_item_carriers, check_message_carriers},
    settings::{controls, write_control_values},
};
use crate::semantic::task::generation::*;
use crate::semantic::value::Presence;
use serde_json::{Map, Value, json};
pub(in crate::protocol::openai) const FIELDS: &[&str] = &[
    "messages",
    "temperature",
    "top_p",
    "max_completion_tokens",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "reasoning_effort",
    "preserve_thinking",
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
    if let Some(_key) = ["logit_bias"].into_iter().find(|key| o.contains_key(*key)) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::UnknownField,
        ));
    }
    if let Some(_key) = UNSUPPORTED.iter().find(|key| {
        o.get(**key)
            .is_some_and(|v| !v.is_null() && v != &json!([]) && v != &json!(""))
    }) {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::UnknownField,
        ));
    }
    let messages = o
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ))?;
    let mut b = Items::new(direction);
    let prediction = if let Some(prediction) = o.get("prediction").filter(|v| !v.is_null()) {
        let prediction = object(prediction)?;
        direction.fields(prediction, &["type", "content"])?;
        if string(prediction, "type")? != "content" {
            return Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ));
        }
        let content = prediction.get("content").ok_or(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ))?;
        Some(
            request_text_parts(content, direction)?
                .into_iter()
                .map(|text| {
                    crate::semantic::value::Text::allowing_empty(
                        text,
                        "prediction text",
                        MAX_TEXT_BYTES,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| CodecError::Limit)?,
        )
    } else {
        None
    };
    for message in messages {
        decode_whole(&mut b, object(message)?, adaptation, MessageSide::History)?;
    }
    let mut controls = controls(o, "max_completion_tokens")?;
    controls.stop_sequences = crate::protocol::generation_controls::stops(o.get("stop"), true, 4)?;
    controls.seed = o
        .get("seed")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_i64().ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ))
        })
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
            let value =
                value
                    .as_f64()
                    .filter(|v| (-2.0..=2.0).contains(v))
                    .ok_or(CodecError::Invalid(
                        crate::protocol::failure::ShapeReason::InvalidValue,
                    ))?;
            controls = apply(controls, value)?;
        }
    }
    if let Some(v) = o.get("top_p").filter(|v| !v.is_null()) {
        controls = controls.with_top_p(v.as_f64().ok_or(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ))?)?;
    }
    controls.logprobs = read_presence(o, "logprobs", |v| {
        v.as_bool().ok_or(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::Probabilities,
        ))
    })?;
    controls.top_logprobs = o
        .get("top_logprobs")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_u64()
                .filter(|n| *n <= 20)
                .map(|n| n as u8)
                .ok_or(CodecError::Invalid(
                    crate::protocol::failure::ShapeReason::InvalidValue,
                ))
        })
        .transpose()?;
    if controls.top_logprobs.is_some() && controls.logprobs != Presence::Value(true) {
        return Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ));
    }
    if o.get("response_format").is_some_and(Value::is_null) {
        return Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue,
        ));
    }
    let format = read_presence(o, "response_format", |v| read_response_format(v, direction))?;
    let verbosity = read_presence(o, "verbosity", crate::protocol::openai::settings::verbosity)?;
    let mut settings = GenerationSettings {
        controls,
        prediction,
        text: TextOptions {
            presence: if format.is_absent() && verbosity.is_absent() {
                Presence::Absent
            } else {
                Presence::Value(())
            },
            format,
            verbosity,
        },
        ..Default::default()
    };
    crate::protocol::openai::reasoning::chat_request(o, &mut settings)?;
    if let Some(value) = o.get("preserve_thinking") {
        if adaptation.rules.chat_preserve_thinking {
            settings.reasoning.presence = ReasoningPresence::Present;
            settings.reasoning.read_history = Presence::Value(value.as_bool().ok_or(
                CodecError::Invalid(crate::protocol::failure::ShapeReason::InvalidValue),
            )?);
        } else {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Reasoning,
            ));
        }
    }
    crate::protocol::openai::chat::audio::settings(o, &mut settings, direction)?;
    let groups = owners::envelopes(&b.message_owners)?;
    let r =
        GenerationRequest::from_resources_and_envelopes(b.items, settings, b.resources, groups)?;
    Ok(DecodedRequest {
        evidence: b.evidence,
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
            let body = object(o.get("json_schema").ok_or(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::Json,
            ))?)?;
            direction.fields(body, &["name", "description", "schema", "strict"])?;
            crate::protocol::openai::settings::read_schema_body(body, false)?
        }
        _ => {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
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
    let unrepresentable_result = |item: &Item| match item {
        Item::ToolResult(result) | Item::CustomResult(result) => {
            result.execution.is_some() || result.is_error.is_some() || !result.output.is_text_only()
        }
        _ => false,
    };
    if target
        .semantic
        .items()
        .iter()
        .any(|(_, item)| unrepresentable_result(item))
    {
        return Err(CodecError::Unsupported(
            crate::protocol::failure::CarrierReason::Tools,
        ));
    }
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::dialect::bailian::check_semantic(
            &target.semantic,
            Profile::Chat,
            "",
        )?;
    }
    let mut messages = encode_items_with(
        target.semantic.items(),
        &owners::owners(target.semantic.items(), target.semantic.message_envelopes()),
        target.fidelity,
        target.adaptation.rules.structured_chat_reasoning,
        false,
        target.semantic.resources(),
    );
    let role = if target.adaptation.rules.system_instruction_messages {
        for message in &mut messages {
            if message.get("role").and_then(Value::as_str) == Some("developer") {
                message["role"] = json!("system");
            }
        }
        "system"
    } else {
        "developer"
    };
    if let Some(root) = target.semantic.instructions().value() {
        messages.insert(0, json!({
            "role":role,
            "content":crate::protocol::openai::settings::instruction_content(root, Profile::Chat)?,
        }));
    }
    for (index, message) in messages.iter().enumerate() {
        if !matches!(
            message.get("role").and_then(Value::as_str),
            Some("developer" | "system")
        ) {
            continue;
        }
        let rules = &target.adaptation.rules;
        if rules.chat_string_instructions && !message["content"].is_string()
            || rules.chat_initial_instruction_only
                && (index != 0
                    || message.as_object().is_some_and(|fields| {
                        fields
                            .keys()
                            .any(|key| !matches!(key.as_str(), "role" | "content"))
                    }))
        {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
    }
    let mut v = json!({"messages": messages});
    let o = v.as_object_mut().expect("object literal");
    if let Some(parts) = &target.semantic.settings().prediction {
        let content = if parts.len() == 1 {
            json!(parts[0].as_str())
        } else {
            json!(
                parts
                    .iter()
                    .map(|part| json!({"type":"text","text":part.as_str()}))
                    .collect::<Vec<_>>()
            )
        };
        o.insert(
            "prediction".into(),
            json!({"type":"content","content":content}),
        );
    }
    write_control_values(
        target.semantic.controls(),
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
    if let Some(k) = target.semantic.controls().top_k {
        if !target.adaptation.rules.thinking_controls {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Unavailable,
            ));
        }
        o.insert("top_k".into(), json!(k));
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
            return Err(CodecError::Invalid(
                crate::protocol::failure::ShapeReason::InvalidValue,
            ));
        }
        crate::semantic::value::Presence::Value(format) => {
            o.insert("response_format".into(), write_response_format(format)?);
        }
    }
    crate::protocol::openai::chat::audio::write_settings(target.semantic.settings(), o);
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::dialect::bailian::write_tools(
            target.semantic.settings(),
            o,
            Profile::Chat,
        )?;
    } else {
        function_tools::encode(&target.semantic, Profile::Chat, o)?;
    }
    let mut settings = target.semantic.settings().clone();
    if !settings.reasoning.read_history.is_absent() {
        if !target.adaptation.rules.chat_preserve_thinking {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Reasoning,
            ));
        }
        put_presence(
            o,
            "preserve_thinking",
            &settings.reasoning.read_history,
            |v| json!(v),
        );
        settings.reasoning.read_history = Presence::Absent;
    }
    if target.adaptation.rules.thinking_controls {
        crate::protocol::openai::dialect::bailian::write_reasoning(&settings, o, Profile::Chat)?;
    } else {
        crate::protocol::openai::reasoning::write_request(&settings, o, Profile::Chat)?;
    }
    bounded(&v)?;
    Ok(v)
}
