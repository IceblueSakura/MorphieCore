//! Complete single-candidate Chat envelopes; task mapping stays in `chat`.
use crate::protocol::failure::CarrierReason;
use crate::protocol::openai::{
    CodecError, DecodedRequest, DecodedResponse, RequestRepresentation, ResponseRepresentation,
    chat, shape::*, validate_identity,
};
use crate::semantic::{
    context::{CacheHints, ClientIdentityHints, ExecutionHints, ServicePolicy, StreamOptions},
    value::Presence,
};
use serde_json::{Map, Value, json};

impl StreamOptions {
    fn read(v: &Value, direction: DecodeDirection<'_>) -> Result<Self, CodecError> {
        let o = object(v)?;
        direction.fields(o, &["include_usage", "include_obfuscation"])?;
        let boolean = |v: &Value| v.as_bool().ok_or(CodecError::invalid());
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
    pub service_policy: Presence<ServicePolicy>,
}
impl RequestContext {
    pub fn streaming(&self) -> bool {
        self.stream == Presence::Value(true)
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        text(&self.model, "model", 256)?;
        if self.n.value().is_some_and(|n| *n != 1) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        if let Some(options) = self.stream_options.value() {
            if !self.streaming() {
                return Err(CodecError::invalid());
            }
            options.validate()?;
        }
        if let Some(policy) = self.service_policy.value() {
            super::super::service::policy_wire(*policy, super::super::Profile::Chat)?;
        }
        validate_context(&ExecutionHints {
            cache: self.cache.clone(),
            identity: self.identity.clone(),
            metadata: self.metadata.clone(),
            ..Default::default()
        })
    }
}
/// Chat has no carrier for Responses-only cache operations.
pub(crate) fn validate_context(context: &ExecutionHints) -> Result<(), CodecError> {
    context.validate()?;
    validate_identity(&context.identity)?;
    if context
        .cache
        .prompt_cache_options
        .value()
        .is_some_and(|options| {
            !options.comparison_response_id.is_absent() || !options.prewarm.is_absent()
        })
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
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
    decode_request(&crate::protocol::json::decode(bytes)?)
}
/// Pre-parsed values cannot prove original raw-byte or duplicate-key validity.
pub fn decode_request(v: &Value) -> Result<DecodedChatRequest, CodecError> {
    decode_request_with(v, &Default::default(), DecodeDirection::Request)
}
pub(crate) fn decode_request_with(
    v: &Value,
    adaptation: &crate::protocol::openai::dialect::adaptation::Adaptation,
    direction: DecodeDirection<'_>,
) -> Result<DecodedChatRequest, CodecError> {
    bounded(v)?;
    let mut normalized;
    // Deprecated `max_tokens` is still an official output cap; the rule only selects
    // the upstream spelling.
    let v = if v.get("max_tokens").is_some() {
        normalized = v.clone();
        let o = normalized.as_object_mut().ok_or(CodecError::invalid())?;
        if o.contains_key("max_completion_tokens") {
            return Err(CodecError::invalid());
        }
        let limit = o.shift_remove("max_tokens").expect("checked field");
        o.insert("max_completion_tokens".into(), limit);
        &normalized
    } else {
        v
    };
    let o = object(v)?;
    // Known security intent is not an unregistered extension.
    if o.contains_key("moderation") {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    let allowed: Vec<_> = chat::request::FIELDS
        .iter()
        .copied()
        .chain(
            crate::protocol::openai::context::CACHE_FIELDS
                .iter()
                .copied(),
        )
        .chain([
            "model",
            "n",
            "stream",
            "stream_options",
            "metadata",
            "service_tier",
            "store",
        ])
        .collect();
    direction.fields(o, &allowed)?;
    let hints = ExecutionHints::read(o, crate::protocol::openai::Profile::Chat, direction)?;
    let context = RequestContext {
        model: string(o, "model")?.into(),
        n: read_presence(o, "n", |v| v.as_u64().ok_or(CodecError::invalid()))?,
        stream: read_presence(o, "stream", |v| v.as_bool().ok_or(CodecError::invalid()))?,
        stream_options: read_presence(o, "stream_options", |v| StreamOptions::read(v, direction))?,
        cache: hints.cache,
        identity: hints.identity,
        metadata: hints.metadata,
        service_policy: super::super::service::read_policy(o, super::super::Profile::Chat)?,
    };
    context.validate()?;
    let task: Map<_, _> = o
        .iter()
        .filter(|(k, _)| chat::request::FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(DecodedChatRequest {
        task: chat::request::decode_generation_with(&Value::Object(task), adaptation, direction)?,
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
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    if target.adaptation.rules.bailian_requests {
        crate::protocol::openai::dialect::bailian::check_semantic(
            target.semantic(),
            super::super::Profile::Chat,
            &context.model,
        )?;
        crate::protocol::openai::dialect::bailian::check_native_effort(
            target.semantic().settings(),
            &context.model,
            super::super::Profile::Chat,
        )?;
        if !context.metadata.is_absent() || !context.service_policy.is_absent() {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
    }
    let mut v = chat::encode_generation(target)?;
    let o = v.as_object_mut().expect("object");
    o.insert("model".into(), json!(context.model));
    put_presence(o, "n", &context.n, |v| json!(v));
    put_presence(o, "stream", &context.stream, |v| json!(v));
    if target.adaptation.rules.bailian_requests {
        if let Some(options) = context.stream_options.value()
            && !options.include_usage.is_absent()
        {
            let mut native = Map::new();
            put_presence(&mut native, "include_usage", &options.include_usage, |v| {
                json!(v)
            });
            o.insert("stream_options".into(), Value::Object(native));
        }
    } else {
        put_presence(
            o,
            "stream_options",
            &context.stream_options,
            StreamOptions::write,
        );
    }
    context.cache.write(o)?;
    let Value::Object(identity) =
        serde_json::to_value(&context.identity).map_err(|_| CodecError::invalid())?
    else {
        unreachable!()
    };
    o.extend(identity);
    put_presence(o, "metadata", &context.metadata, |v| json!(v));
    super::super::service::write_policy(o, &context.service_policy, super::super::Profile::Chat)?;
    bounded(&v)?;
    Ok(v)
}
pub fn decode_response_bytes(bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
    decode_response(&crate::protocol::json::decode(bytes)?)
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
        .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?;
    if choices[0].pointer("/message/role").and_then(Value::as_str) != Some("assistant") {
        return Err(CodecError::invalid());
    }
    Ok(())
}
pub(in crate::protocol::openai) fn headers(v: &Value, kind: &str) -> Result<(), CodecError> {
    let o = object(v)?;
    // Field admission belongs to the selected static/event codec, after its
    // named adapter mappings. Structural identity remains strict here.
    text(string(o, "id")?, "response id", 256)?;
    text(string(o, "model")?, "response model", 256)?;
    if string(o, "object")? != kind || o.get("created").and_then(Value::as_u64).is_none() {
        return Err(CodecError::invalid());
    }
    Ok(())
}
