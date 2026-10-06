//! Complete stateless Responses envelopes. Execution hints never become task instructions.
use super::{
    CodecError, DecodedRequest, Profile, RequestRepresentation, ResponseMetadata, common::*,
    settings,
};
use crate::semantic::{
    context::{CacheHints, ExecutionHints, ResponseContext},
    task::generation::*,
    value::Presence,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value, json};
pub(super) const CACHE_FIELDS: &[&str] = &[
    "prompt_cache_key",
    "prompt_cache_retention",
    "prompt_cache_options",
    "safety_identifier",
    "user",
];
impl CacheHints {
    pub(super) fn write(&self, o: &mut Map<String, Value>) -> Result<(), CodecError> {
        self.validate()?;
        let Value::Object(v) =
            serde_json::to_value(self).map_err(|_| CodecError::Invalid("cache hints"))?
        else {
            unreachable!()
        };
        o.extend(v);
        Ok(())
    }
}
pub(super) const EXEC_FIELDS: &[&str] = &[
    "metadata",
    "service_tier",
    "max_tool_calls",
    "store",
    "background",
    "previous_response_id",
    "conversation",
    "prompt",
    "moderation",
    "context_management",
];
impl ExecutionHints {
    pub(super) fn read(o: &Map<String, Value>) -> Result<Self, CodecError> {
        let fields: Map<_, _> = o
            .iter()
            .filter(|(k, _)| {
                EXEC_FIELDS.contains(&k.as_str()) || CACHE_FIELDS.contains(&k.as_str())
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let hints: Self = serde_json::from_value(Value::Object(fields))
            .map_err(|_| CodecError::Invalid("execution hints"))?;
        hints.validate()?;
        Ok(hints)
    }
    pub(super) fn write(&self, o: &mut Map<String, Value>) -> Result<(), CodecError> {
        self.validate()?;
        let Value::Object(v) =
            serde_json::to_value(self).map_err(|_| CodecError::Invalid("execution hints"))?
        else {
            unreachable!()
        };
        o.extend(v);
        Ok(())
    }
}
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
    /// Provider-scoped body sections (`client_metadata`), never task semantics.
    pub extensions: crate::protocol::extensions::CustomSections,
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
        Ok(self.execution.validate()?)
    }
}
/// `model` is a public binding label, not an upstream address. The caller resolves it outside IR.
/// Decode a bounded raw JSON request, rejecting duplicate keys before constructing IR.
/// Callers must separately bound body collection; this function receives an existing slice.
pub fn decode_request_bytes(bytes: &[u8]) -> Result<DecodedResponsesRequest, CodecError> {
    decode_request(&super::json::decode(bytes)?)
}
/// Decode a pre-parsed value; original duplicate keys and raw byte size cannot be checked here.
pub fn decode_request(v: &Value) -> Result<DecodedResponsesRequest, CodecError> {
    bounded(v)?;
    let o = object(v)?;
    let allowed: Vec<_> = settings::FIELDS
        .iter()
        .chain(EXEC_FIELDS)
        .chain(CACHE_FIELDS)
        .copied()
        .chain(["input", "model", "stream", "stream_options"])
        .chain(crate::protocol::extensions::SECTION_NAMES.iter().copied())
        .collect();
    fields(o, &allowed)?;
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
        execution: ExecutionHints::read(o)?,
        extensions: crate::protocol::extensions::CustomSections::read(o)?,
    };
    context.validate()?;
    let task: Map<_, _> = o
        .iter()
        .filter(|(k, _)| k.as_str() == "input" || settings::FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(DecodedResponsesRequest {
        task: super::responses::decode_generation(&Value::Object(task))?,
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
    let mut v = super::responses::encode_generation(target)?;
    let o = v.as_object_mut().expect("object");
    context.execution.write(o)?;
    context.extensions.write(o)?;
    o.insert("store".into(), json!(false));
    o.insert("model".into(), json!(context.model));
    put_presence(o, "stream", &context.delivery.stream, |s| json!(s));
    put_presence(o, "stream_options", &context.delivery.options, |s| json!(s));
    bounded(&v)?;
    Ok(v)
}
impl ResponseContext {
    pub(super) fn read(
        o: &Map<String, Value>,
    ) -> Result<(Self, crate::protocol::fidelity::FidelityRecords), CodecError> {
        let mut controls = o.clone();
        let mut fidelity = crate::protocol::fidelity::FidelityRecords::default();
        let instruction_messages = if let Some(Value::Array(values)) = o.get("instructions") {
            controls.remove("instructions");
            let mut items = Items::default();
            super::responses::decode_items(&mut items, values, false, "completed")?;
            fidelity = items.fidelity;
            Some(items.items)
        } else {
            None
        };
        let s = if settings::FIELDS.iter().any(|k| o.contains_key(*k)) {
            Some(settings::read(&controls, true)?)
        } else {
            None
        };
        let value = Self {
            settings: s,
            instruction_messages,
            execution: ExecutionHints::read(o)?,
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
    pub(super) fn write(
        &self,
        fidelity: &crate::protocol::fidelity::FidelityRecords,
        o: &mut Map<String, Value>,
        completed: bool,
    ) -> Result<(), CodecError> {
        self.validate()?;
        if let Some(s) = &self.settings {
            settings::write(s, o)?;
        }
        self.execution.write(o)?;
        if let Some(messages) = &self.instruction_messages {
            o.insert(
                "instructions".into(),
                json!(super::responses::encode_items(messages, fidelity, false)),
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
pub fn decode_response_bytes(bytes: &[u8]) -> Result<super::DecodedResponse, CodecError> {
    decode_response(&super::json::decode(bytes)?)
}
/// Full envelope validation, unlike the lower-level task snapshot codec.
/// A pre-parsed value cannot prove original JSON syntax, duplicate-key or raw-byte validity.
pub fn decode_response(v: &Value) -> Result<super::DecodedResponse, CodecError> {
    bounded(v)?;
    validate_response_snapshot(v)?;
    super::responses::decode_response(v)
}
pub fn encode_response(target: &super::ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    target.metadata.context.validate()?;
    let v = super::responses::encode_response(target)?;
    validate_response_snapshot_with(&v, &target.adaptation)?;
    Ok(v)
}
/// Vendor adaptation (ADR 0008): validate-then-drop derived views, then record
/// classified response extras into fidelity for same-origin re-encode.
pub(crate) fn record_vendor_shapes(
    profile: Profile,
    adaptation: &crate::protocol::adaptation::Adaptation,
    o: &Map<String, Value>,
    fidelity: &mut crate::protocol::fidelity::FidelityRecords,
    semantic: Option<&GenerationResponse>,
) -> Result<(), CodecError> {
    adaptation.validate_response(profile, o)?;
    if let Some(view) = o.get("output_text").filter(|v| !v.is_null()) {
        let view = view.as_str().ok_or(CodecError::Invalid("output_text"))?;
        // The equality rule applies where the snapshot claims finality: a
        // non-terminal snapshot may report a partial view while items stream.
        let terminal = o
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|s| matches!(s, "completed" | "incomplete" | "failed" | "cancelled"));
        if terminal {
            let mut authoritative = String::new();
            if let Some(items) = o.get("output").and_then(Value::as_array) {
                for item in items {
                    if item.get("type").and_then(Value::as_str) != Some("message") {
                        continue;
                    }
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for part in parts {
                            if part.get("type").and_then(Value::as_str) == Some("output_text")
                                && let Some(text) = part.get("text").and_then(Value::as_str)
                            {
                                authoritative.push_str(text);
                            }
                        }
                    }
                }
            }
            if view != authoritative {
                return Err(CodecError::Invalid("output_text"));
            }
        }
    }
    fidelity.capture_response_extras(profile, adaptation, o, semantic)
}
/// Same-origin re-encode of classified vendor extras; never standard semantics.
pub(crate) fn write_response_extras(
    fidelity: &crate::protocol::fidelity::FidelityRecords,
    profile: Profile,
    adaptation: &crate::protocol::adaptation::Adaptation,
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
        super::adapter_shapes::write_extras(o, extras);
    }
}
/// Structural snapshot validation (ADR 0008): reported facts are presence-preserving
/// and typed by the task codec; this layer keeps the structural requirements.
pub(crate) fn validate_response_snapshot(v: &Value) -> Result<(), CodecError> {
    validate_response_snapshot_with(v, &Default::default())
}
pub(crate) fn validate_response_snapshot_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
) -> Result<(), CodecError> {
    // Complete-envelope validation sees the same explicit wire mapping as the
    // task codec. Required identity, items and state rejection remain intact.
    let (v, _) = super::adapter_shapes::decode(v, Profile::Responses, adaptation)?;
    let o = object(&v)?;
    ResponseContext::read(o)?.0.validate()?;
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
pub(super) fn validate_stream_payload(v: &Value) -> Result<(), CodecError> {
    validate_stream_payload_with(v, &Default::default())
}
pub(super) fn validate_stream_payload_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
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
        validate_response_snapshot_with(response, adaptation)?;
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
pub(super) fn validate_item_snapshot(v: &Value) -> Result<(), CodecError> {
    let o = object(v)?;
    if string(o, "type")? == "message" {
        // Full wire snapshots report identity/lifecycle; only task codecs allow defaults.
        text(string(o, "id")?, "wire item id", 256)?;
        if string(o, "role")? != "assistant" || !o.contains_key("status") {
            return Err(CodecError::Invalid("output message headers"));
        }
        super::responses::status(o, ItemLifecycle::Completed)?;
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
pub(super) fn validate_part_snapshot(v: &Value) -> Result<(), CodecError> {
    if v.get("type").and_then(Value::as_str) == Some("output_text") {
        let annotations = v.get("annotations").filter(|a| !a.is_null());
        if !annotations.is_some_and(Value::is_array) {
            return Err(CodecError::Invalid("annotations"));
        }
    }
    if v.get("type").and_then(Value::as_str) == Some("output_text")
        && let Some(probs) = v.get("logprobs").filter(|v| !v.is_null())
    {
        for p in super::text::read_logprobs(probs)? {
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
pub(super) fn timestamp(v: &Value) -> Result<Number, CodecError> {
    match v {
        Value::Number(n) if crate::semantic::value::valid_timestamp(n) => Ok(n.clone()),
        _ => Err(CodecError::Invalid("timestamp")),
    }
}
pub(super) fn response_fields(o: &Map<String, Value>) -> Result<(), CodecError> {
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
    fields(o, &allowed)
}
pub(super) fn write_metadata(
    metadata: &ResponseMetadata,
    o: &mut Map<String, Value>,
    completed: bool,
) -> Result<(), CodecError> {
    metadata
        .context
        .write(&metadata.instruction_fidelity, o, completed)
}
