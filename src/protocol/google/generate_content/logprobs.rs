//! Native token reports carry no byte evidence or portable token IDs.
//! https://ai.google.dev/api/generate-content#LogprobsResult
use crate::protocol::failure::{CarrierReason, ShapeReason};
use crate::{
    protocol::CodecError,
    semantic::task::generation::{Logprob, MAX_ITEMS, TopLogprob, validate_logprobs},
};
use serde_json::{Map, Number, Value};

fn invalid() -> CodecError {
    CodecError::Invalid(ShapeReason::Probabilities)
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
fn probability(value: &Value) -> Result<Number, CodecError> {
    // log_probability is optional: omission is not evidence of probability one.
    let number = match value {
        Value::Number(number) => number.clone(),
        _ => return Err(invalid()),
    };
    if !number.as_f64().is_some_and(|n| n.is_finite() && n <= 0.0) {
        return Err(invalid());
    }
    Ok(number)
}
fn token(value: &Value) -> Result<TopLogprob, CodecError> {
    let object = object(value)?;
    if let Some(id) = object.get("tokenId") {
        if id.as_i64().is_none_or(|id| i32::try_from(id).is_err()) {
            return Err(invalid());
        }
        return Err(CodecError::Unsupported(CarrierReason::TokenId));
    }
    Ok(TopLogprob {
        token: object
            .get("token")
            .map(|value| value.as_str().map(str::to_owned).ok_or_else(invalid))
            .transpose()?,
        logprob: object.get("logProbability").map(probability).transpose()?,
        bytes: None,
    })
}
pub(super) fn read(value: &Value) -> Result<Vec<Logprob>, CodecError> {
    let object = object(value)?;
    if let Some(value) = object.get("logProbabilitySum") {
        probability(value)?;
        return Err(CodecError::Unsupported(CarrierReason::SumLogprob));
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
            let reported = token(value)?;
            // Chosen tokens require an aligned string and actual probability.
            // Alternatives can preserve either fact as unreported.
            let mut chosen = Logprob {
                token: reported.token.ok_or_else(invalid)?,
                logprob: reported.logprob.ok_or_else(invalid)?,
                bytes: None,
                top_logprobs: None,
            };
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
                let mut previous = None;
                for candidate in &candidates {
                    if let Some(number) = candidate.logprob.as_ref().and_then(Number::as_f64) {
                        if previous.is_some_and(|previous| previous < number) {
                            return Err(invalid());
                        }
                        previous = Some(number);
                    }
                }
                chosen.top_logprobs = Some(candidates);
            }
            Ok(chosen)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_logprobs(&probabilities)?;
    Ok(probabilities)
}
