//! Complete single-candidate Chat envelopes; task mapping stays in `chat`.
use super::{
    CodecError, DecodedRequest, DecodedResponse, RequestRepresentation, ResponseRepresentation,
    chat, common::*,
};
use crate::semantic::{
    context::{CacheHints, ClientIdentityHints, ExecutionHints, ServiceTier, StreamOptions},
    value::Presence,
};
use serde_json::{Map, Value, json};

impl StreamOptions {
    fn read(v: &Value) -> Result<Self, CodecError> {
        let o = object(v)?;
        fields(o, &["include_usage", "include_obfuscation"])?;
        let boolean = |v: &Value| v.as_bool().ok_or(CodecError::Invalid("Chat stream option"));
        let s = Self {
            include_usage: read_presence(o, "include_usage", boolean)?,
            include_obfuscation: read_presence(o, "include_obfuscation", boolean)?,
        };
        s.validate()?;
        Ok(s)
    }
    fn write(&self) -> Value {
        let mut o = Map::new();
        put_presence(&mut o, "include_usage", &self.include_usage, |v| json!(v));
        put_presence(
            &mut o,
            "include_obfuscation",
            &self.include_obfuscation,
            |v| json!(v),
        );
        Value::Object(o)
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestContext {
    pub model: String,
    pub n: Presence<u64>,
    pub stream: Presence<bool>,
    pub stream_options: Presence<StreamOptions>,
    pub cache: CacheHints,
    pub identity: ClientIdentityHints,
    pub metadata: Presence<std::collections::BTreeMap<String, String>>,
    pub service_tier: Presence<ServiceTier>,
}
impl RequestContext {
    pub fn streaming(&self) -> bool {
        self.stream == Presence::Value(true)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        text(&self.model, "model", 256)?;
        if self.n.value().is_some_and(|n| *n != 1) {
            return Err(CodecError::Unsupported("candidate count".into()));
        }
        if let Some(options) = self.stream_options.value() {
            if !self.streaming() {
                return Err(CodecError::Invalid("stream_options without streaming"));
            }
            options.validate()?;
        }
        validate_context(&ExecutionHints {
            cache: self.cache.clone(),
            identity: self.identity.clone(),
            metadata: self.metadata.clone(),
            service_tier: self.service_tier.clone(),
            ..Default::default()
        })
    }
}
/// The pinned Chat schema is narrower than the shared Responses tier vocabulary.
/// https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/completion_create_params.py
pub(crate) fn validate_context(context: &ExecutionHints) -> Result<(), CodecError> {
    context.validate()?;
    if context.service_tier == Presence::Value(ServiceTier::Ultrafast) {
        return Err(CodecError::Unsupported("Chat service tier".into()));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedChatRequest {
    pub task: DecodedRequest,
    pub context: RequestContext,
}
/// Strict raw JSON admission; body collection must be bounded separately by the caller.
pub fn decode_request_bytes(bytes: &[u8]) -> Result<DecodedChatRequest, CodecError> {
    decode_request(&super::json::decode(bytes)?)
}
/// Pre-parsed values cannot prove original raw-byte or duplicate-key validity.
pub fn decode_request(v: &Value) -> Result<DecodedChatRequest, CodecError> {
    decode_request_with(v, &Default::default())
}
pub(crate) fn decode_request_with(
    v: &Value,
    adaptation: &crate::protocol::adaptation::Adaptation,
) -> Result<DecodedChatRequest, CodecError> {
    bounded(v)?;
    let mut normalized;
    let v = if adaptation.rules.legacy_max_tokens && v.get("max_tokens").is_some() {
        normalized = v.clone();
        let o = normalized
            .as_object_mut()
            .ok_or(CodecError::Invalid("request"))?;
        if o.contains_key("max_completion_tokens") {
            return Err(CodecError::Invalid("conflicting token limits"));
        }
        let limit = o.shift_remove("max_tokens").expect("checked field");
        o.insert("max_completion_tokens".into(), limit);
        &normalized
    } else {
        v
    };
    let o = object(v)?;
    let allowed: Vec<_> = chat::FIELDS
        .iter()
        .copied()
        .chain(super::envelope::CACHE_FIELDS.iter().copied())
        .chain([
            "model",
            "n",
            "stream",
            "stream_options",
            "metadata",
            "service_tier",
        ])
        .collect();
    fields(o, &allowed)?;
    let hints = ExecutionHints::read(o)?;
    let context = RequestContext {
        model: string(o, "model")?.into(),
        n: read_presence(o, "n", |v| v.as_u64().ok_or(CodecError::Invalid("n")))?,
        stream: read_presence(o, "stream", |v| {
            v.as_bool().ok_or(CodecError::Invalid("stream"))
        })?,
        stream_options: read_presence(o, "stream_options", StreamOptions::read)?,
        cache: hints.cache,
        identity: hints.identity,
        metadata: hints.metadata,
        service_tier: hints.service_tier,
    };
    context.validate()?;
    let task: Map<_, _> = o
        .iter()
        .filter(|(k, _)| chat::FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(DecodedChatRequest {
        task: chat::decode_generation_with(&Value::Object(task), adaptation)?,
        context,
    })
}
pub fn encode_request(
    target: &RequestRepresentation<'_>,
    context: &RequestContext,
) -> Result<Value, CodecError> {
    context.validate()?;
    if context.streaming()
        && target
            .semantic
            .settings()
            .audio
            .value()
            .is_some_and(|a| a.format != crate::semantic::task::generation::AudioFormat::Pcm16)
    {
        return Err(CodecError::Unsupported(
            "streaming audio requires pcm16".into(),
        ));
    }
    let mut v = chat::encode_generation(target)?;
    let o = v.as_object_mut().expect("object");
    o.insert("model".into(), json!(context.model));
    put_presence(o, "n", &context.n, |v| json!(v));
    put_presence(o, "stream", &context.stream, |v| json!(v));
    put_presence(
        o,
        "stream_options",
        &context.stream_options,
        StreamOptions::write,
    );
    context.cache.write(o)?;
    let Value::Object(identity) = serde_json::to_value(&context.identity)
        .map_err(|_| CodecError::Invalid("identity hints"))?
    else {
        unreachable!()
    };
    o.extend(identity);
    put_presence(o, "metadata", &context.metadata, |v| json!(v));
    put_presence(o, "service_tier", &context.service_tier, |v| json!(v));
    bounded(&v)?;
    Ok(v)
}
pub fn decode_response_bytes(bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
    decode_response(&super::json::decode(bytes)?)
}
/// Validate a complete single-candidate response, not a permissive message-history abbreviation.
pub fn decode_response(v: &Value) -> Result<DecodedResponse, CodecError> {
    bounded(v)?;
    validate_response(v)?;
    chat::decode_response(v)
}
pub fn encode_response(target: &ResponseRepresentation<'_>) -> Result<Value, CodecError> {
    let v = chat::encode_response(target)?;
    validate_response(&v)?;
    Ok(v)
}
pub(crate) fn validate_response(v: &Value) -> Result<(), CodecError> {
    headers(v, "chat.completion")?;
    let choices = v
        .get("choices")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        .ok_or(CodecError::Unsupported("candidate count".into()))?;
    if choices[0].pointer("/message/role").and_then(Value::as_str) != Some("assistant") {
        return Err(CodecError::Invalid("Chat output role"));
    }
    Ok(())
}
pub(super) fn headers(v: &Value, kind: &str) -> Result<(), CodecError> {
    let o = object(v)?;
    // Field admission belongs to the selected static/event codec, after its
    // named adapter mappings. Structural identity remains strict here.
    text(string(o, "id")?, "response id", 256)?;
    text(string(o, "model")?, "response model", 256)?;
    if string(o, "object")? != kind || o.get("created").and_then(Value::as_u64).is_none() {
        return Err(CodecError::Invalid("Chat response headers"));
    }
    Ok(())
}
