//! Native Decision profiles. A prepared target owns return-path association.
use crate::{
    CodecError,
    semantic::{
        task::decision::{
            Answer, AnswerSet, ChoiceOption, ChoiceValue, Confidence, Evidence, MAX_TEXT_BYTES,
            Outcome, Probability, Question, QuestionKind, Request, ScoreLevel, Usage,
        },
        value::{JsonLimits, Text, parse_json},
    },
};
use serde_json::{Map, Number, Value, json};

mod distribution;
mod openai;
mod system_one;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Native {
    OpenAi,
    SystemOne,
}

#[derive(Debug)]
pub struct Ingress {
    pub model: String,
    pub task: Request,
}

pub struct Expectation {
    native: Native,
    request: Request,
}

pub struct Prepared {
    pub body: Value,
    pub expectation: Expectation,
}

impl Native {
    pub fn decode_request(self, bytes: &[u8]) -> Result<Ingress, CodecError> {
        let value = decode(bytes)?;
        let value = object(&value)?;
        let model = string(value.get("model"))?;
        if !valid_model(model) {
            return Err(CodecError::invalid());
        }
        let task = match self {
            Self::OpenAi => openai::decode_request(value)?,
            Self::SystemOne => system_one::decode_request(value)?,
        };
        Ok(Ingress {
            model: model.to_owned(),
            task,
        })
    }

    pub fn prepare(self, request: Request, model: &str) -> Result<Prepared, CodecError> {
        if !valid_model(model) {
            return Err(CodecError::invalid());
        }
        let body = match self {
            Self::OpenAi => openai::encode_request(&request, model)?,
            Self::SystemOne => system_one::encode_request(&request, model)?,
        };
        Ok(Prepared {
            body,
            expectation: Expectation {
                native: self,
                request,
            },
        })
    }
}

impl Expectation {
    /// Current profiles deliver only native answers; this never infers usage.
    pub fn complete(self, bytes: &[u8], public_model: &str) -> Result<Value, CodecError> {
        let root = decode(bytes)?;
        let root = object(&root)?;
        if root.get("error").is_some_and(|value| !value.is_null())
            || !valid_model(string(root.get("model"))?)
            || !valid_model(public_model)
        {
            return Err(CodecError::invalid());
        }
        match self.native {
            Native::OpenAi => openai::complete(&self.request, root, public_model),
            Native::SystemOne => system_one::complete(&self.request, root, public_model),
        }
    }
}

fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 256
        && !model.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn decode(bytes: &[u8]) -> Result<Value, CodecError> {
    const LIMITS: JsonLimits = JsonLimits {
        bytes: 4 << 20,
        depth: 32,
        nodes: 32_768,
    };
    parse_json(bytes, LIMITS).map_err(CodecError::from)
}

fn object(value: &Value) -> Result<&Map<String, Value>, CodecError> {
    value.as_object().ok_or_else(CodecError::invalid)
}

fn string(value: Option<&Value>) -> Result<&str, CodecError> {
    value
        .and_then(Value::as_str)
        .ok_or_else(CodecError::invalid)
}

fn text(value: Option<&Value>) -> Result<Text, CodecError> {
    Text::allowing_empty(string(value)?, "Decision evidence", MAX_TEXT_BYTES)
        .map_err(|_| CodecError::Limit)
}

fn optional_text(value: Option<&Value>) -> Result<Option<Text>, CodecError> {
    value
        .filter(|v| !v.is_null())
        .map(|v| text(Some(v)))
        .transpose()
}

fn array(value: &Value) -> Result<&[Value], CodecError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(CodecError::invalid)
}

fn probability(value: &Value) -> Result<Probability, CodecError> {
    let value = value.as_number().ok_or_else(CodecError::invalid)?.clone();
    Probability::new(value).map_err(|_| CodecError::invalid())
}

fn count(value: &Value) -> Result<u64, CodecError> {
    value.as_u64().ok_or_else(CodecError::invalid)
}

fn answers(request: &Request, outcomes: Vec<Outcome>) -> Result<AnswerSet, CodecError> {
    AnswerSet::for_request(request, outcomes).map_err(|_| CodecError::invalid())
}

/// Assemble the bijection in requested slot order, failing on any missing,
/// repeated or unknown option/level regardless of the native map/array order.
fn ordered_probabilities(
    n: usize,
    reports: impl IntoIterator<Item = Result<(usize, Probability), CodecError>>,
) -> Result<Vec<Probability>, CodecError> {
    let mut slots = vec![None; n];
    for report in reports {
        let (index, probability) = report?;
        if slots
            .get_mut(index)
            .ok_or_else(CodecError::invalid)?
            .replace(probability)
            .is_some()
        {
            return Err(CodecError::invalid());
        }
    }
    slots
        .into_iter()
        .collect::<Option<_>>()
        .ok_or_else(CodecError::invalid)
}

#[cfg(test)]
mod tests;
