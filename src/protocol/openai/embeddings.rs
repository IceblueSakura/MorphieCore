//! Selected standard text/float Embeddings profile; exact numeric reports remain authoritative.
//! https://developers.openai.com/api/reference/resources/embeddings/methods/create
use super::images::{number, object, required, string};
pub use crate::adapter::embeddings::Encoding;
use crate::{
    adapter::embeddings::{Profile, Request, Response},
    protocol::CodecError,
    semantic::{task::embedding::*, value::Presence},
};
use serde_json::{Map, Value, json};

fn invalid() -> CodecError {
    CodecError::Invalid("embedding")
}
fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_control() || c.is_whitespace())
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let value = super::json::decode(bytes)?;
    let map = object(
        &value,
        &["model", "input", "dimensions", "encoding_format", "user"],
    )?;
    let model = string(required(map, "model")?)?;
    if !valid_model(model) {
        return Err(invalid());
    }
    let inputs = match required(map, "input")? {
        Value::String(text) => vec![text.clone()],
        Value::Array(values) if values.len() <= MAX_INPUTS => values
            .iter()
            .map(|v| string(v).map(str::to_owned))
            .collect::<Result<_, _>>()?,
        _ => return Err(invalid()),
    };
    let mut request = Request::new(model, EmbeddingRequest::new(inputs).map_err(|_| invalid())?);
    if let Some(d) = map.get("dimensions") {
        request.task.dimensions =
            Presence::Value(usize::try_from(number(d)?).map_err(|_| invalid())?);
    }
    if let Some(encoding) = map.get("encoding_format") {
        if string(encoding)? != "float" {
            return Err(invalid());
        }
        request.encoding = Presence::Value(Encoding::Float);
    }
    if let Some(user) = map.get("user") {
        let user = string(user)?;
        if user.is_empty() || user.len() > 256 {
            return Err(invalid());
        }
        request.identity.user = Presence::Value(user.into());
    }
    request.task.validate().map_err(|_| invalid())?;
    Ok(request)
}
pub fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    request.task.validate().map_err(|_| invalid())?;
    if !valid_model(model)
        || !valid_model(&request.model)
        || request.encoding == Presence::Null
        || request.identity.user == Presence::Null
        || request.identity.safety_identifier != Presence::Absent
        || request
            .identity
            .user
            .value()
            .is_some_and(|u| u.is_empty() || u.len() > 256)
    {
        return Err(invalid());
    }
    let mut value = json!({"model":model,"input":request.task.inputs()});
    let map = value.as_object_mut().expect("object");
    if let Some(d) = request.task.dimensions.value() {
        map.insert("dimensions".into(), json!(d));
    }
    if request.encoding.value().is_some() {
        map.insert("encoding_format".into(), json!("float"));
    }
    if let Some(user) = request.identity.user.value() {
        map.insert("user".into(), json!(user));
    }
    Ok(value)
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, CodecError> {
    decode_response_with(bytes, Profile::Standard)
}
pub(crate) fn decode_response_with(bytes: &[u8], profile: Profile) -> Result<Response, CodecError> {
    let value = super::json::decode(bytes)?;
    let fields: &[&str] = if profile == Profile::Standard {
        &["object", "model", "data", "usage"]
    } else {
        &["object", "model", "data", "usage", "id"]
    };
    let map = object(&value, fields)?;
    decode_map(map, profile)
}
fn decode_map(map: &Map<String, Value>, profile: Profile) -> Result<Response, CodecError> {
    if string(required(map, "object")?)? != "list" {
        return Err(invalid());
    }
    let model = string(required(map, "model")?)?;
    if !valid_model(model) {
        return Err(invalid());
    }
    let data = required(map, "data")?.as_array().ok_or_else(invalid)?;
    if !(1..=MAX_INPUTS).contains(&data.len()) {
        return Err(invalid());
    }
    let mut vectors = Vec::with_capacity(data.len());
    let mut count = 0usize;
    for item in data {
        let item = object(item, &["object", "index", "embedding"])?;
        if string(required(item, "object")?)? != "embedding" {
            return Err(invalid());
        }
        let index = usize::try_from(number(required(item, "index")?)?).map_err(|_| invalid())?;
        let values = required(item, "embedding")?
            .as_array()
            .ok_or_else(invalid)?;
        count = count.checked_add(values.len()).ok_or(CodecError::Limit)?;
        if count > MAX_VECTOR_VALUES {
            return Err(CodecError::Limit);
        }
        let values = values
            .iter()
            .map(|v| v.as_number().cloned().ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        vectors.push(EmbeddingVector::new(index, values).map_err(|_| invalid())?);
    }
    let usage = object(required(map, "usage")?, &["prompt_tokens", "total_tokens"])?;
    let total = number(required(usage, "total_tokens")?)?;
    let usage = match usage.get("prompt_tokens") {
        Some(value) => EmbeddingUsage::new(number(value)?, total).map_err(|_| invalid())?,
        // This named carrier reports tokens produced by input tokenization.
        // https://help.aliyun.com/en/model-studio/text-embedding-synchronous-api
        None if profile == Profile::DashScope => EmbeddingUsage::from_input_only_total(total),
        None => return Err(invalid()),
    };
    let response = Response {
        model: model.into(),
        task: EmbeddingResponse::new(vectors, usage).map_err(|_| invalid())?,
        id: match map.get("id") {
            Some(value) => Presence::Value(string(value)?.into()),
            None => Presence::Absent,
        },
    };
    response.validate().map_err(|_| invalid())?;
    Ok(response)
}
pub fn encode_response(response: &Response) -> Result<Value, CodecError> {
    response.validate().map_err(|_| invalid())?;
    if response.id != Presence::Absent {
        return Err(invalid());
    }
    let usage = response.task.usage.value().ok_or_else(invalid)?;
    let data: Vec<_> = response
        .task
        .vectors()
        .iter()
        .map(|v| json!({"object":"embedding","index":v.index(),"embedding":v.values()}))
        .collect();
    let value = json!({"object":"list","model":response.model,"data":data,
        "usage":{"prompt_tokens":usage.input_tokens(),"total_tokens":usage.total_tokens()}});
    crate::semantic::value::json_size(&value, crate::semantic::value::JsonLimits::ENVELOPE.bytes)
        .map_err(|_| CodecError::Limit)?;
    Ok(value)
}
