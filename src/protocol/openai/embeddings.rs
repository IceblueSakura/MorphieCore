//! OpenAI Embeddings: scoped token inputs and independent numeric/delivery representations.
//! https://developers.openai.com/api/reference/resources/embeddings/methods/create
use crate::protocol::CodecError;
pub use crate::protocol::embedding::Encoding;
use crate::{
    protocol::embedding::{
        number, optional_count, optional_string, required, string, valid_model, *,
    },
    semantic::task::embedding::*,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dialect {
    Standard,
    DashScope,
    OpenRouter,
}
enum Inputs {
    Text(Vec<String>),
    Tokens(Vec<Vec<u32>>),
}
/// Bounded unbound token carrier; only trusted parsing context may supply its tokenizer.
pub struct ParsedRequest {
    model: String,
    input: Inputs,
    dimensions: Option<usize>,
    encoding: Option<Encoding>,
    user: Option<String>,
}
impl ParsedRequest {
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn into_request(self, tokenizer: Option<TokenizerScope>) -> Result<Request, CodecError> {
        let inputs = match self.input {
            Inputs::Text(texts) => texts
                .into_iter()
                .map(|t| EmbeddingInput::TextParts(vec![t]))
                .collect(),
            Inputs::Tokens(sequences) => {
                let tokenizer = tokenizer.ok_or_else(CodecError::invalid)?;
                sequences
                    .into_iter()
                    .map(|ids| EmbeddingInput::TokenIds { tokenizer, ids })
                    .collect::<Vec<_>>()
            }
        };
        let samples = inputs
            .into_iter()
            .map(|input| {
                EmbeddingSample::new(input, self.dimensions).map_err(|_| CodecError::invalid())
            })
            .collect::<Result<_, _>>()?;
        let mut request = Request::new(
            self.model,
            EmbeddingRequest::new(samples).map_err(|_| CodecError::invalid())?,
        );
        request.encoding = self.encoding;
        request.identity.user = self.user;
        request.validate().map_err(|_| CodecError::invalid())?;
        Ok(request)
    }
}
fn tokens(values: &[Value]) -> Result<Vec<u32>, CodecError> {
    if values.is_empty() || values.len() > MAX_TOKEN_IDS {
        return Err(CodecError::invalid());
    }
    values
        .iter()
        .map(|v| u32::try_from(number(v)?).map_err(|_| CodecError::invalid()))
        .collect()
}
pub fn parse_request(bytes: &[u8]) -> Result<ParsedRequest, CodecError> {
    let value = crate::protocol::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    if map.keys().any(|key| {
        !["model", "input", "dimensions", "encoding_format", "user"].contains(&key.as_str())
    }) {
        return Err(CodecError::invalid());
    }
    let model = string(required(map, "model")?)?;
    if !valid_model(model) {
        return Err(CodecError::invalid());
    }
    let input = match required(map, "input")? {
        Value::String(s) => Inputs::Text(vec![s.clone()]),
        Value::Array(v) if !v.is_empty() => match &v[0] {
            Value::String(_) if v.len() <= MAX_INPUTS => Inputs::Text(
                v.iter()
                    .map(|v| string(v).map(str::to_owned))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Number(_) => Inputs::Tokens(vec![tokens(v)?]),
            Value::Array(_) if v.len() <= MAX_INPUTS => Inputs::Tokens(
                v.iter()
                    .map(|v| tokens(v.as_array().ok_or_else(CodecError::invalid)?))
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(CodecError::invalid()),
        },
        _ => return Err(CodecError::invalid()),
    };
    let size = match &input {
        Inputs::Text(v) => v.iter().try_fold(0usize, |n, s| n.checked_add(s.len())),
        Inputs::Tokens(v) => v.iter().try_fold(0usize, |n, s| n.checked_add(s.len())),
    }
    .ok_or(CodecError::Limit)?;
    if size
        > match input {
            Inputs::Text(_) => MAX_INPUT_BYTES,
            Inputs::Tokens(_) => MAX_TOKEN_IDS,
        }
    {
        return Err(CodecError::Limit);
    }
    let dimensions = map
        .get("dimensions")
        .map(|d| usize::try_from(number(d)?).map_err(|_| CodecError::invalid()))
        .transpose()?;
    if dimensions.is_some_and(|d| !(1..=MAX_DIMENSIONS).contains(&d)) {
        return Err(CodecError::invalid());
    }
    let encoding = map
        .get("encoding_format")
        .map(|v| match string(v)? {
            "float" => Ok(Encoding::Float),
            "base64" => Ok(Encoding::Base64),
            _ => Err(CodecError::invalid()),
        })
        .transpose()?;
    let user = map
        .get("user")
        .map(|v| string(v).map(str::to_owned))
        .transpose()?;
    Ok(ParsedRequest {
        model: model.into(),
        input,
        dimensions,
        encoding,
        user,
    })
}
/// Text-only convenience; callers admitting tokens must use explicit trusted scope.
pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    parse_request(bytes)?.into_request(None)
}
pub fn encode_request(request: &TargetRequest, model: &str) -> Result<Value, CodecError> {
    if !valid_model(model)
        || request
            .user
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256)
    {
        return Err(CodecError::invalid());
    }
    let samples = request.task.samples();
    let dimension = samples[0].dimensions();
    let first_text = matches!(samples[0].input(), EmbeddingInput::TextParts(_));
    let input=samples.iter().map(|s| {
        if s.dimensions()!=dimension {return Err(CodecError::invalid());}
        match s.input() {
            EmbeddingInput::TextParts(parts) if first_text && parts.len()==1=>Ok(json!(parts[0])),
            EmbeddingInput::TokenIds{ids,tokenizer} if !first_text=>{
                if !matches!(samples[0].input(),EmbeddingInput::TokenIds{tokenizer:first,..} if first==tokenizer) {return Err(CodecError::invalid());}
                Ok(json!(ids))
            },
            _=>Err(CodecError::invalid()),
        }
    }).collect::<Result<Vec<_>,_>>()?;
    let mut value = json!({"model":model,"input":input,"encoding_format":request.encoding.name()});
    if let Some(d) = dimension {
        value["dimensions"] = json!(d);
    }
    if let Some(user) = &request.user {
        value["user"] = json!(user);
    }
    Ok(value)
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, CodecError> {
    decode_response_with(bytes, Dialect::Standard, Encoding::Float)
}
pub(crate) fn decode_response_with(
    bytes: &[u8],
    profile: Dialect,
    encoding: Encoding,
) -> Result<Response, CodecError> {
    let value = crate::protocol::json::decode(bytes)?;
    let map = value.as_object().ok_or_else(CodecError::invalid)?;
    if string(required(map, "object")?)? != "list" {
        return Err(CodecError::invalid());
    }
    let model = optional_string(map, "model")?;
    let strict = profile == Dialect::Standard;
    if strict && model.is_none() {
        return Err(CodecError::invalid());
    }
    let data = required(map, "data")?
        .as_array()
        .ok_or_else(CodecError::invalid)?;
    if !(1..=MAX_INPUTS).contains(&data.len()) {
        return Err(CodecError::Limit);
    }
    let mut vectors = vec![];
    let mut count = 0usize;
    for item in data {
        let item = item.as_object().ok_or_else(CodecError::invalid)?;
        if string(required(item, "object")?)? != "embedding" {
            return Err(CodecError::invalid());
        }
        let index = usize::try_from(number(required(item, "index")?)?)
            .map_err(|_| CodecError::invalid())?;
        let wire = required(item, "embedding")?;
        let actual = if wire.is_array() {
            Encoding::Float
        } else {
            Encoding::Base64
        };
        if strict && actual != encoding {
            return Err(CodecError::invalid());
        }
        let vector = decode_vector(index, wire, actual)?;
        count = count
            .checked_add(vector.values().len())
            .ok_or(CodecError::Limit)?;
        if count > MAX_VECTOR_VALUES {
            return Err(CodecError::Limit);
        }
        vectors.push(vector);
    }
    let usage = map
        .get("usage")
        .map(|v| {
            let u = v.as_object().ok_or_else(CodecError::invalid)?;
            let input = optional_count(u, "prompt_tokens")?;
            let total = optional_count(u, "total_tokens")?;
            if strict && (input.is_none() || total.is_none()) {
                return Err(CodecError::invalid());
            }
            EmbeddingUsage::reported(
                input,
                total,
                if profile == Dialect::DashScope {
                    AccountingBasis::InputOnly
                } else {
                    AccountingBasis::Unspecified
                },
                None,
            )
            .map_err(|_| CodecError::invalid())
        })
        .transpose()?;
    if strict && usage.is_none() {
        return Err(CodecError::invalid());
    }
    if !uniform_width(&vectors) {
        return Err(CodecError::invalid());
    }
    let response = Response {
        model,
        task: EmbeddingResponse::new(vectors, usage).map_err(|_| CodecError::invalid())?,
        id: optional_string(map, "id")?,
    };
    response.validate().map_err(|_| CodecError::invalid())?;
    Ok(response)
}
fn decode_vector(
    index: usize,
    wire: &Value,
    encoding: Encoding,
) -> Result<EmbeddingVector, CodecError> {
    match encoding {
        Encoding::Float => {
            let values = wire.as_array().ok_or_else(CodecError::invalid)?;
            if values.len() > MAX_DIMENSIONS {
                return Err(CodecError::Limit);
            }
            EmbeddingVector::new(
                index,
                values
                    .iter()
                    .map(|v| v.as_number().cloned().ok_or_else(CodecError::invalid))
                    .collect::<Result<_, _>>()?,
            )
            .map_err(|_| CodecError::invalid())
        }
        Encoding::Base64 => {
            let s = string(wire)?;
            if s.len() > (MAX_DIMENSIONS * 4).div_ceil(3) * 4 {
                return Err(CodecError::Limit);
            }
            let bytes = STANDARD.decode(s).map_err(|_| CodecError::invalid())?;
            let (words, rest) = bytes.as_chunks::<4>();
            if !rest.is_empty() {
                return Err(CodecError::invalid());
            }
            EmbeddingVector::from_binary32(
                index,
                words.iter().map(|w| u32::from_le_bytes(*w)).collect(),
            )
            .map_err(|_| CodecError::invalid())
        }
    }
}
/// Strict native OpenAI response serialization, distinct from compatible delivery.
pub fn encode_response(response: &Response) -> Result<Value, CodecError> {
    response.validate().map_err(|_| CodecError::invalid())?;
    if response.id.is_some() || response.task.usage().is_some_and(|u| u.details().is_some()) {
        return Err(CodecError::invalid());
    }
    let usage = response.task.usage().ok_or_else(CodecError::invalid)?;
    let delivery = Delivery {
        model: response.model.clone().ok_or_else(CodecError::invalid)?,
        vectors: response.vectors().to_vec(),
        usage: DeliveryUsage {
            input_tokens: usage
                .reported_input_tokens()
                .ok_or_else(CodecError::invalid)?,
            total_tokens: usage
                .reported_total_tokens()
                .ok_or_else(CodecError::invalid)?,
        },
    };
    encode_response_as(&delivery, Encoding::Float)
}
pub fn encode_response_as(response: &Delivery, encoding: Encoding) -> Result<Value, CodecError> {
    if !uniform_width(response.vectors()) {
        return Err(CodecError::invalid());
    }
    let data = response
        .vectors()
        .iter()
        .map(|v| {
            let values = match (encoding, v.values()) {
                (Encoding::Float, EmbeddingValues::ExactJson(values)) => json!(values),
                (Encoding::Float, EmbeddingValues::Binary32(bits)) => Value::Array(
                    bits.iter()
                        .map(|b| {
                            Value::Number(
                                serde_json::Number::from_f64(f64::from(f32::from_bits(*b)))
                                    .expect("finite binary32"),
                            )
                        })
                        .collect(),
                ),
                (Encoding::Base64, EmbeddingValues::Binary32(bits)) => Value::String(
                    STANDARD.encode(
                        bits.iter()
                            .flat_map(|b| b.to_le_bytes())
                            .collect::<Vec<_>>(),
                    ),
                ),
                (Encoding::Base64, EmbeddingValues::ExactJson(_)) => {
                    return Err(CodecError::invalid());
                }
            };
            Ok(json!({"object":"embedding","index":v.index(),"embedding":values}))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let u = response.usage();
    let value = json!({"object":"list","model":response.model(),"data":data,
        "usage":{"prompt_tokens":u.input_tokens,"total_tokens":u.total_tokens}});
    crate::semantic::value::json_size(&value, crate::semantic::value::JsonLimits::ENVELOPE.bytes)
        .map_err(|_| CodecError::Limit)?;
    Ok(value)
}
fn uniform_width(vectors: &[EmbeddingVector]) -> bool {
    vectors
        .windows(2)
        .all(|pair| pair[0].values().len() == pair[1].values().len())
}
