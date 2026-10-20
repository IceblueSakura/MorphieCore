//! Native token reports carry no byte evidence or portable token IDs.
//! https://ai.google.dev/api/generate-content#LogprobsResult
use crate::{
    protocol::CodecError,
    semantic::task::generation::{Logprob, MAX_ITEMS, TopLogprob, validate_logprobs},
};
use serde_json::{Map, Number, Value};

fn invalid() -> CodecError {
    CodecError::Invalid("Gemini logprobs")
}
fn object(value: &Value) -> Result<&Map<String, Value>, CodecError> {
    value.as_object().ok_or_else(invalid)
}
fn list(value: Option<&Value>) -> Result<&[Value], CodecError> {
    match value {
        None => Ok(&[]),
        Some(Value::Array(values)) if values.len() <= MAX_ITEMS => Ok(values),
        _ => Err(invalid()),
    }
}
fn probability(value: Option<&Value>) -> Result<Number, CodecError> {
    // Proto3 omits zero-valued numeric fields.
    let number = match value {
        None => Number::from(0),
        Some(Value::Number(number)) => number.clone(),
        _ => return Err(invalid()),
    };
    if !number.as_f64().is_some_and(|n| n.is_finite() && n <= 0.0) {
        return Err(invalid());
    }
    Ok(number)
}
fn token(value: &Value) -> Result<Logprob, CodecError> {
    let object = object(value)?;
    if object
        .get("tokenId")
        .is_some_and(|id| id.as_u64().is_none_or(|id| id > i32::MAX as u64))
    {
        return Err(invalid());
    }
    Ok(Logprob {
        token: object
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?
            .into(),
        logprob: probability(object.get("logProbability"))?,
        bytes: None,
        top_logprobs: None,
    })
}
pub(super) fn read(value: &Value) -> Result<Vec<Logprob>, CodecError> {
    let object = object(value)?;
    if object.contains_key("logProbabilitySum") {
        probability(object.get("logProbabilitySum"))?;
    }
    let chosen = list(object.get("chosenCandidates"))?;
    let top = object
        .get("topCandidates")
        .map(|value| list(Some(value)))
        .transpose()?;
    if top.is_some_and(|top| top.len() != chosen.len()) {
        return Err(invalid());
    }
    let probabilities = chosen
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut chosen = token(value)?;
            if let Some(top) = top {
                let step = top[index].as_object().ok_or_else(invalid)?;
                let candidates = list(step.get("candidates"))?;
                if candidates.len() > 20 {
                    return Err(invalid());
                }
                let candidates = candidates
                    .iter()
                    .map(token)
                    .collect::<Result<Vec<_>, _>>()?;
                if candidates
                    .windows(2)
                    .any(|pair| pair[0].logprob.as_f64() < pair[1].logprob.as_f64())
                {
                    return Err(invalid());
                }
                chosen.top_logprobs = Some(
                    candidates
                        .into_iter()
                        .map(|candidate| TopLogprob {
                            token: Some(candidate.token),
                            logprob: Some(candidate.logprob),
                            bytes: None,
                        })
                        .collect(),
                );
            }
            Ok(chosen)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_logprobs(&probabilities)?;
    Ok(probabilities)
}
