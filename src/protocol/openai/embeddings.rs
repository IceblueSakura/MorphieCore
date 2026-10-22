//! Text Embeddings with explicit float/LE-binary32 carriers and exact float reports.
//! https://developers.openai.com/api/reference/resources/embeddings/methods/create
pub use crate::adapter::embeddings::Encoding;
use crate::protocol::openai::images::{number, object, required, string};
use crate::{
    adapter::embeddings::{Profile, Request, Response},
    protocol::CodecError,
    semantic::{task::embedding::*, value::Presence},
};
use base64::{Engine, engine::general_purpose::STANDARD};
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
    let value = crate::protocol::json::decode(bytes)?;
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
        request.encoding = Presence::Value(match string(encoding)? {
            "float" => Encoding::Float,
            "base64" => Encoding::Base64,
            _ => return Err(invalid()),
        });
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
    if let Some(encoding) = request.encoding.value() {
        map.insert("encoding_format".into(), json!(encoding.name()));
    }
    if let Some(user) = request.identity.user.value() {
        map.insert("user".into(), json!(user));
    }
    Ok(value)
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, CodecError> {
    decode_response_with(bytes, Profile::Standard, Encoding::Float)
}
pub(crate) fn decode_response_with(
    bytes: &[u8],
    profile: Profile,
    encoding: Encoding,
) -> Result<Response, CodecError> {
    let value = crate::protocol::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(invalid)?;
    decode_map(map, profile, encoding)
}
fn decode_map(
    map: &Map<String, Value>,
    profile: Profile,
    encoding: Encoding,
) -> Result<Response, CodecError> {
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
        let item = item.as_object().ok_or_else(invalid)?;
        if string(required(item, "object")?)? != "embedding" {
            return Err(invalid());
        }
        let index = usize::try_from(number(required(item, "index")?)?).map_err(|_| invalid())?;
        let wire = required(item, "embedding")?;
        let vector = match encoding {
            Encoding::Float => {
                let values = wire.as_array().ok_or_else(invalid)?;
                if values.len() > MAX_DIMENSIONS {
                    return Err(CodecError::Limit);
                }
                let values = values
                    .iter()
                    .map(|v| v.as_number().cloned().ok_or_else(invalid))
                    .collect::<Result<_, _>>()?;
                EmbeddingVector::new(index, values).map_err(|_| invalid())?
            }
            Encoding::Base64 => {
                let encoded = string(wire)?;
                if encoded.len() > (MAX_DIMENSIONS * 4).div_ceil(3) * 4 {
                    return Err(CodecError::Limit);
                }
                let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
                let (words, remainder) = bytes.as_chunks::<4>();
                if !remainder.is_empty() {
                    return Err(invalid());
                }
                let bits = words.iter().map(|v| u32::from_le_bytes(*v)).collect();
                EmbeddingVector::from_binary32(index, bits).map_err(|_| invalid())?
            }
        };
        count = count
            .checked_add(vector.values().len())
            .ok_or(CodecError::Limit)?;
        if count > MAX_VECTOR_VALUES {
            return Err(CodecError::Limit);
        }
        vectors.push(vector);
    }
    let usage = required(map, "usage")?.as_object().ok_or_else(invalid)?;
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
    encode_response_as(response, Encoding::Float)
}
pub fn encode_response_as(response: &Response, encoding: Encoding) -> Result<Value, CodecError> {
    response.validate().map_err(|_| invalid())?;
    if response.id != Presence::Absent {
        return Err(invalid());
    }
    let usage = response.task.usage.value().ok_or_else(invalid)?;
    let data: Vec<_> = response
        .task
        .vectors()
        .iter()
        .map(|v| {
            let values = match (encoding, v.values()) {
                (Encoding::Float, EmbeddingValues::ExactJson(values)) => json!(values),
                (Encoding::Float, EmbeddingValues::Binary32(bits)) => Value::Array(
                    bits.iter()
                        .map(|bits| {
                            Value::Number(
                                serde_json::Number::from_f64(f64::from(f32::from_bits(*bits)))
                                    .expect("finite binary32"),
                            )
                        })
                        .collect(),
                ),
                (Encoding::Base64, EmbeddingValues::Binary32(bits)) => {
                    let bytes: Vec<_> = bits.iter().flat_map(|bits| bits.to_le_bytes()).collect();
                    Value::String(STANDARD.encode(bytes))
                }
                (Encoding::Base64, EmbeddingValues::ExactJson(_)) => return Err(invalid()),
            };
            Ok(json!({"object":"embedding","index":v.index(),"embedding":values}))
        })
        .collect::<Result<_, _>>()?;
    let value = json!({"object":"list","model":response.model,"data":data,
        "usage":{"prompt_tokens":usage.input_tokens(),"total_tokens":usage.total_tokens()}});
    crate::semantic::value::json_size(&value, crate::semantic::value::JsonLimits::ENVELOPE.bytes)
        .map_err(|_| CodecError::Limit)?;
    Ok(value)
}
