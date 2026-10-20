//! Complete stateless Responses envelopes. Execution hints never become task instructions.
use crate::protocol::openai::{
    CodecError, DecodedRequest, Profile, RequestRepresentation, ResponseMetadata,
    common::*,
    context::{CACHE_FIELDS, EXEC_FIELDS, check_access_programs},
    settings,
};
use crate::semantic::{
    context::{ExecutionHints, ResponseContext, ServicePolicy},
    task::generation::*,
    value::Presence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value, json};
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamOptions {
    #[serde(default, skip_serializing_if = "Presence::is_absent")]
    pub include_obfuscation: Presence<bool>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Delivery {
    pub stream: Presence<bool>,
    pub options: Presence<StreamOptions>,
}
impl Delivery {
    pub fn streaming(&self) -> bool {
        self.stream == Presence::Value(true)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        if self.options.value().is_some() && !self.streaming() {
            return Err(CodecError::Invalid("stream_options without streaming"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    pub model: String,
    pub delivery: Delivery,
    pub execution: ExecutionHints,
    pub service_policy: Presence<ServicePolicy>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedResponsesRequest {
    pub task: DecodedRequest,
    pub context: RequestContext,
}
impl RequestContext {
    pub fn validate(&self) -> Result<(), CodecError> {
        text(&self.model, "model", 256)?;
        self.delivery.validate()?;
        if let Some(policy) = self.service_policy.value() {
            super::super::service::policy_wire(*policy, Profile::Responses)?;
        }
        Ok(self.execution.validate()?)
    }
}
/// `model` is a public binding label, not an upstream address. The caller resolves it outside IR.
/// Decode a bounded raw JSON request, rejecting duplicate keys before constructing IR.
/// Callers must separately bound body collection; this function receives an existing slice.
pub fn decode_request_bytes(bytes: &[u8]) -> Result<DecodedResponsesRequest, CodecError> {
    decode_request(&crate::protocol::json::decode(bytes)?)
}
/// Decode a pre-parsed value; original duplicate keys and raw byte size cannot be checked here.
pub fn decode_request(v: &Value) -> Result<DecodedResponsesRequest, CodecError> {
    decode_request_in(v, DecodeDirection::Request)
}
pub(crate) fn decode_request_in(
    v: &Value,
    direction: DecodeDirection<'_>,
) -> Result<DecodedResponsesRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    let allowed: Vec<_> = settings::FIELDS
        .iter()
        .chain(EXEC_FIELDS)
        .chain(CACHE_FIELDS)
        .copied()
        .chain(["input", "model", "stream", "stream_options"])
        .collect();
    direction.fields(o, &allowed)?;
    let context = RequestContext {
        model: string(o, "model")?.into(),
        delivery: Delivery {
            stream: read_presence(o, "stream", |v| {
                v.as_bool().ok_or(CodecError::Invalid("stream"))
            })?,
            options: read_presence(o, "stream_options", |v| {
                serde_json::from_value(v.clone()).map_err(|_| CodecError::Invalid("stream_options"))
            })?,
        },
        execution: ExecutionHints::read(o, Profile::Responses, direction)?,
        service_policy: super::super::service::read_policy(o, Profile::Responses)?,
    };
    context.validate()?;
    let task: Map<_, _> = o
        .iter()
        .filter(|(k, _)| k.as_str() == "input" || settings::FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(DecodedResponsesRequest {
        task: crate::protocol::openai::responses::request::decode_generation_in(
            &Value::Object(task),
            direction,
        )?,
        context,
    })
}
/// Stateless policy is explicit: omission at ingress never enables provider storage by default.
pub fn encode_request(
    target: &RequestRepresentation<'_>,
    context: &RequestContext,
) -> Result<Value, CodecError> {
    context.validate()?;
    if target.profile != Profile::Responses {
        return Err(CodecError::ProfileMismatch);
    }
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::bailian::check_semantic(
            target.semantic(),
            Profile::Responses,
            &context.model,
        )?;
        crate::protocol::openai::bailian::check_native_effort(
            target.semantic().settings(),
            &context.model,
            Profile::Responses,
        )?;
        crate::protocol::openai::bailian::check_context(
            &context.execution,
            &context.service_policy,
            Profile::Responses,
        )?;
    }
    let mut v = crate::protocol::openai::responses::encode_generation(target)?;
    let o = v.as_object_mut().expect("object");
    context.execution.write(o)?;
    super::super::service::write_policy(o, &context.service_policy, Profile::Responses)?;
    o.insert("store".into(), json!(false));
    o.insert("model".into(), json!(context.model));
    put_presence(o, "stream", &context.delivery.stream, |s| json!(s));
    // Client delivery preferences are implemented by ResponseDelivery.
    // Bailian does not advertise a matching upstream option.
    if !target.adaptation.rules.bailian_requests {
        put_presence(o, "stream_options", &context.delivery.options, |s| json!(s));
    }
    bounded(&v)?;
    Ok(v)
}
impl ResponseContext {
    pub(in crate::protocol::openai) fn read(
        o: &Map<String, Value>,
        direction: DecodeDirection,
    ) -> Result<(Self, crate::protocol::fidelity::FidelityRecords), CodecError> {
        check_access_programs(o)?;
        let mut controls = o.clone();
        // Storage facts belong to the API envelope, never upstream execution intent.
        let storage = super::managed::read_storage(o)?;
        if matches!(direction, DecodeDirection::Upstream)
            && storage.as_ref().is_some_and(|storage| {
                storage.store == Presence::Value(true)
                    || storage.previous_response_id.value().is_some()
            })
        {
            return Err(CodecError::Unsupported("upstream response state".into()));
        }
        let execution = o
            .iter()
            .filter(|(key, _)| {
                (EXEC_FIELDS.contains(&key.as_str()) || CACHE_FIELDS.contains(&key.as_str()))
                    && key.as_str() != "store"
                    && key.as_str() != "previous_response_id"
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let mut fidelity = crate::protocol::fidelity::FidelityRecords::default();
        let instruction_messages = if let Some(Value::Array(values)) = o.get("instructions") {
            controls.remove("instructions");
            let mut items = Items::new(direction);
            crate::protocol::openai::responses::items::decode_items(
                &mut items,
                values,
                crate::protocol::openai::responses::items::ItemSide::History,
                "completed",
            )?;
            fidelity = items.fidelity;
            Some(items.items)
        } else {
            None
        };
        let s = if settings::FIELDS.iter().any(|k| o.contains_key(*k)) {
            Some(settings::read(&controls, true, direction)?)
        } else {
            None
        };
        let value = Self {
            reports: crate::semantic::context::ResponseReports::phased(
                match o.get("status").and_then(Value::as_str) {
                    Some("queued" | "in_progress") => {
                        crate::semantic::context::ReportPhase::Opening
                    }
                    _ => crate::semantic::context::ReportPhase::Terminal,
                },
                crate::semantic::context::ResponseReport {
                    settings: s,
                    service_class: super::super::service::read_class(o, Profile::Responses)?,
                },
            ),
            instruction_messages,
            execution: ExecutionHints::read(&execution, Profile::Responses, direction)?,
            completed_at: read_presence(o, "completed_at", timestamp)?,
            cache_diagnostics: read_presence(o, "prompt_cache_diagnostics", |v| {
                serde_json::from_value(v.clone())
                    .map_err(|_| CodecError::Invalid("cache diagnostics"))
            })?,
            // Responses wire has no `system_fingerprint` position; the field is
            // Chat-only and rejected on Responses bodies by `response_fields`.
            system_fingerprint: crate::semantic::value::Presence::Absent,
        };
        value.validate()?;
        Ok((value, fidelity))
    }
    pub(in crate::protocol::openai) fn write(
        &self,
        fidelity: &crate::protocol::fidelity::FidelityRecords,
        o: &mut Map<String, Value>,
        completed: bool,
    ) -> Result<(), CodecError> {
        self.validate()?;
        if let Some(s) = self.settings() {
            settings::write(s, o)?;
        }
        self.execution.write(o)?;
        super::super::service::write_class(o, self.service_class(), Profile::Responses)?;
        if let Some(messages) = &self.instruction_messages {
            o.insert(
                "instructions".into(),
                json!(crate::protocol::openai::responses::items::encode_items(
                    messages,
                    fidelity,
                    false,
                    &ResourceTable::default()
                )?),
            );
        }
        if completed {
            put_presence(o, "completed_at", &self.completed_at, |v| {
                Value::Number(v.clone())
            });
        }
        put_presence(
            o,
            "prompt_cache_diagnostics",
            &self.cache_diagnostics,
            |v| json!(v),
        );
        Ok(())
    }
}
/// Decode a bounded raw JSON response with strict syntax and complete envelope validation.
pub fn decode_response_bytes(
    bytes: &[u8],
) -> Result<crate::protocol::openai::DecodedResponse, CodecError> {
    decode_response(&crate::protocol::json::decode(bytes)?)
}
/// Full envelope validation, unlike the lower-level task snapshot codec.
/// A pre-parsed value cannot prove original JSON syntax, duplicate-key or raw-byte validity.
pub fn decode_response(v: &Value) -> Result<crate::protocol::openai::DecodedResponse, CodecError> {
    bounded(v)?;
    validate_response_snapshot(v)?;
    crate::protocol::openai::responses::decode_response(v)
}
pub fn encode_response(
    target: &crate::protocol::openai::ResponseRepresentation<'_>,
) -> Result<Value, CodecError> {
    target.metadata.context.validate()?;
    let v = crate::protocol::openai::responses::encode_response(target)?;
    validate_response_snapshot_with(&v, &target.adaptation, DecodeDirection::Encoded)?;
    Ok(v)
}
/// Record classified response extras into fidelity for same-origin re-encode.
/// The SDK-derived `output_text` view is ignored, never validated or restored.
pub(crate) fn record_vendor_shapes(
    profile: Profile,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    o: &Map<String, Value>,
    fidelity: &mut crate::protocol::fidelity::FidelityRecords,
    semantic: Option<&GenerationResponse>,
) -> Result<(), CodecError> {
    fidelity.capture_response_extras(profile, adaptation, o, semantic)
}
/// Same-origin re-encode of classified vendor extras; never standard semantics.
pub(crate) fn write_response_extras(
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    profile: Profile,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    semantic: &GenerationResponse,
    response_id: &str,
    o: &mut Map<String, Value>,
) {
    if let Some(extras) =
        fidelity.projected_response_extras(profile, adaptation, semantic, response_id)
    {
        o.extend(
            extras
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
    }
    if let Some(extras) =
        fidelity.projected_routing_extras(profile, adaptation, semantic, response_id)
    {
        crate::protocol::openai::dialect::adapter_shapes::write_extras(o, extras);
    }
}
/// Structural snapshot validation (ADR 0008): reported facts are presence-preserving
/// and typed by the task codec; this layer keeps the structural requirements.
pub(crate) fn validate_response_snapshot(v: &Value) -> Result<(), CodecError> {
    validate_response_snapshot_with(v, &Default::default(), DecodeDirection::Upstream)
}
pub(crate) fn validate_response_snapshot_with(
    v: &Value,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    // Complete-envelope validation sees the same explicit wire mapping as the
    // task codec. Required identity, items and state rejection remain intact.
    let (v, _) = crate::protocol::openai::dialect::adapter_shapes::decode(
        v,
        Profile::Responses,
        adaptation,
        direction,
    )?;
    let o = object(&v)?;
    ResponseContext::read(o, direction)?.0.validate()?;
    // A complete Response snapshot requires the output array; an absent value is
    // never the explicit empty array and no lower layer may backfill it.
    for item in o
        .get("output")
        .and_then(Value::as_array)
        .ok_or(CodecError::Invalid("output"))?
    {
        validate_item_snapshot(item)?;
    }
    Ok(())
}
pub(in crate::protocol::openai) fn validate_stream_payload(v: &Value) -> Result<(), CodecError> {
    validate_stream_payload_with(v, &Default::default(), DecodeDirection::Encoded)
}
pub(in crate::protocol::openai) fn validate_stream_payload_with(
    v: &Value,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    // Every complete Responses SSE event carries the required integer sequence
    // number; event ordering never substitutes for its presence.
    if object(v)?
        .get("sequence_number")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err(CodecError::Invalid("sequence"));
    }
    if let Some(response) = v.get("response") {
        validate_response_snapshot_with(response, adaptation, direction)?;
    }
    if let Some(item) = v.get("item") {
        validate_item_snapshot(item)?;
    }
    if let Some(part) = v.get("part") {
        validate_part_snapshot(part)?;
    }
    if v.get("type").and_then(Value::as_str) == Some("response.output_text.annotation.added")
        && v.pointer("/annotation/type").and_then(Value::as_str) == Some("file_path")
    {
        return Err(CodecError::Unsupported("file_path annotation event".into()));
    }
    Ok(())
}
pub(in crate::protocol::openai) fn validate_item_snapshot(v: &Value) -> Result<(), CodecError> {
    let o = object(v)?;
    if string(o, "type")? == "message" {
        // Full wire snapshots report identity/lifecycle; only task codecs allow defaults.
        text(string(o, "id")?, "wire item id", 256)?;
        if string(o, "role")? != "assistant" || !o.contains_key("status") {
            return Err(CodecError::Invalid("output message headers"));
        }
        crate::protocol::openai::responses::items::status(o, ItemLifecycle::Completed)?;
        let parts = o
            .get("content")
            .and_then(Value::as_array)
            .ok_or(CodecError::Invalid("output content"))?;
        for p in parts {
            validate_part_snapshot(p)?;
        }
    } else if matches!(string(o, "type")?, "reasoning" | "configuration_update") {
        // These snapshots require identity; task codecs still assign fresh identity.
        text(string(o, "id")?, "wire item id", 256)?;
    }
    Ok(())
}
pub(in crate::protocol::openai) fn validate_part_snapshot(v: &Value) -> Result<(), CodecError> {
    if v.get("type").and_then(Value::as_str) == Some("output_text") {
        let annotations = v.get("annotations").filter(|a| !a.is_null());
        if !annotations.is_some_and(Value::is_array) {
            return Err(CodecError::Invalid("annotations"));
        }
    }
    if v.get("type").and_then(Value::as_str) == Some("output_text")
        && let Some(probs) = v.get("logprobs").filter(|v| !v.is_null())
    {
        for p in crate::protocol::openai::content::text::read_logprobs(probs)? {
            if p.bytes.is_none()
                || p.top_logprobs.as_ref().is_none_or(|v| {
                    v.iter()
                        .any(|p| p.token.is_none() || p.logprob.is_none() || p.bytes.is_none())
                })
            {
                return Err(CodecError::Invalid("incomplete static logprobs"));
            }
        }
    }
    Ok(())
}
pub(in crate::protocol::openai) fn timestamp(v: &Value) -> Result<Number, CodecError> {
    match v {
        Value::Number(n) if crate::semantic::value::valid_timestamp(n) => Ok(n.clone()),
        _ => Err(CodecError::Invalid("timestamp")),
    }
}
pub(in crate::protocol::openai) fn response_fields(
    o: &Map<String, Value>,
) -> Result<(), CodecError> {
    let allowed: Vec<_> = settings::FIELDS
        .iter()
        .chain(EXEC_FIELDS)
        .chain(CACHE_FIELDS)
        .copied()
        .chain([
            "id",
            "object",
            "created_at",
            "completed_at",
            "model",
            "status",
            "output",
            "usage",
            "error",
            "incomplete_details",
            "prompt_cache_diagnostics",
            // Derived view, validated then dropped by the admitting adapter.
            "output_text",
        ])
        .chain(
            crate::protocol::fidelity::RESPONSE_EXTRA_FIELDS
                .iter()
                .copied(),
        )
        .collect();
    DecodeDirection::Upstream.fields(o, &allowed)
}
pub(in crate::protocol::openai) fn write_metadata(
    metadata: &ResponseMetadata,
    o: &mut Map<String, Value>,
    completed: bool,
) -> Result<(), CodecError> {
    metadata
        .context
        .write(&metadata.instruction_fidelity, o, completed)?;
    if let Some(storage) = &metadata.storage {
        storage.validate()?;
        put_presence(o, "store", &storage.store, |value| json!(value));
        put_presence(
            o,
            "previous_response_id",
            &storage.previous_response_id,
            |value| json!(value),
        );
    }
    Ok(())
}
