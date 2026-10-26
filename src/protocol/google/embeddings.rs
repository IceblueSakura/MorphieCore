//! Gemini Developer API synchronous batch embeddings, not Vertex or asynchronous jobs.
//! https://ai.google.dev/api/embeddings
use crate::protocol::embedding::{number, optional_count, required, string};
use crate::{
    adapter::embeddings::{valid_model, *},
    protocol::CodecError,
    semantic::task::embedding::*,
};
use serde_json::{Value, json};

pub fn encode_request(request: &TargetRequest, model: &str) -> Result<Value, CodecError> {
    if !valid_model(model) || request.encoding != Encoding::Float || request.user.is_some() {
        return Err(CodecError::invalid());
    }
    let model = if model.starts_with("models/") {
        model.to_owned()
    } else {
        format!("models/{model}")
    };
    let requests = request
        .task
        .samples()
        .iter()
        .map(|sample| {
            let EmbeddingInput::TextParts(parts) = sample.input() else {
                return Err(CodecError::invalid());
            };
            let mut config = json!({"autoTruncate":false});
            if let Some(d) = sample.dimensions() {
                config["outputDimensionality"] = json!(d);
            }
            Ok(json!({
                "model":model,
                "content":{"parts":parts.iter().map(|text|json!({"text":text})).collect::<Vec<_>>()},
                "embedContentConfig":config
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"requests":requests}))
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, CodecError> {
    let value = crate::protocol::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    let data = required(map, "embeddings")?
        .as_array()
        .ok_or_else(CodecError::invalid)?;
    if !(1..=MAX_INPUTS).contains(&data.len()) {
        return Err(CodecError::Limit);
    }
    let mut count = 0usize;
    let mut vectors = vec![];
    for (index, item) in data.iter().enumerate() {
        let item = item.as_object().ok_or_else(CodecError::invalid)?;
        if let Some(shape) = item.get("shape") {
            let shape = shape.as_array().ok_or_else(CodecError::invalid)?;
            for dimension in shape {
                number(dimension)?;
            }
            if !shape.is_empty() {
                return Err(CodecError::invalid());
            }
        }
        // This native contract guarantees request order; it does not report an index.
        let values = required(item, "values")?
            .as_array()
            .ok_or_else(CodecError::invalid)?;
        if values.len() > MAX_DIMENSIONS {
            return Err(CodecError::Limit);
        }
        let vector = EmbeddingVector::new(
            index,
            values
                .iter()
                .map(|v| v.as_number().cloned().ok_or_else(CodecError::invalid))
                .collect::<Result<_, _>>()?,
        )
        .map_err(|_| CodecError::invalid())?;
        count = count
            .checked_add(vector.values().len())
            .ok_or(CodecError::Limit)?;
        if count > MAX_VECTOR_VALUES {
            return Err(CodecError::Limit);
        }
        vectors.push(vector);
    }
    let usage = map
        .get("usageMetadata")
        .map(|v| {
            let u = v.as_object().ok_or_else(CodecError::invalid)?;
            let input = optional_count(u, "promptTokenCount")?;
            let details = u
                .get("promptTokenDetails")
                .map(|v| {
                    let details = v.as_array().ok_or_else(CodecError::invalid)?;
                    if details.len() > 16 {
                        return Err(CodecError::Limit);
                    }
                    details
                        .iter()
                        .map(|v| {
                            let v = v.as_object().ok_or_else(CodecError::invalid)?;
                            let modality = match string(required(v, "modality")?)? {
                                "MODALITY_UNSPECIFIED" => InputModality::Unspecified,
                                "TEXT" => InputModality::Text,
                                "IMAGE" => InputModality::Image,
                                "AUDIO" => InputModality::Audio,
                                "VIDEO" => InputModality::Video,
                                "DOCUMENT" => InputModality::Document,
                                _ => return Err(CodecError::invalid()),
                            };
                            Ok(ModalityTokenCount {
                                modality,
                                tokens: number(required(v, "tokenCount")?)?,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            EmbeddingUsage::reported(input, None, AccountingBasis::InputOnly, details)
                .map_err(|_| CodecError::invalid())
        })
        .transpose()?;
    let result = Response {
        model: None,
        id: None,
        task: EmbeddingResponse::new(vectors, usage).map_err(|_| CodecError::invalid())?,
    };
    result.validate().map_err(|_| CodecError::invalid())?;
    Ok(result)
}
