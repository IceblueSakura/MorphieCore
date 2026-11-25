//! TypeSafe System One keyed Noul wire profile.
use super::*;

pub(super) fn decode_request(root: &Map<String, Value>) -> Result<Request, CodecError> {
    if root.get("assets").is_some_and(|value| !value.is_null()) {
        return Err(CodecError::invalid());
    }
    let state = text(root.get("state"))?;
    let questions = object(root.get("questions").ok_or_else(CodecError::invalid)?)?;
    if questions.len() > crate::semantic::task::decision::MAX_QUESTIONS {
        return Err(CodecError::Limit);
    }
    let predicates = questions
        .iter()
        .map(|(name, question)| {
            let question = object(question)?;
            if string(question.get("type"))? != "noul"
                || question
                    .get("criteria")
                    .is_some_and(|value| !value.is_null())
            {
                return Err(CodecError::invalid());
            }
            Predicate::new(
                Some(name.clone()),
                string(question.get("instructions"))?.to_owned(),
            )
            .map_err(|_| CodecError::invalid())
        })
        .collect::<Result<_, _>>()?;
    Request::new(Evidence::StateText(state), predicates).map_err(|_| CodecError::invalid())
}

pub(super) fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    let Evidence::StateText(state) = request.evidence() else {
        return Err(CodecError::invalid());
    };
    let mut questions = Map::new();
    for predicate in request.questions() {
        let name = predicate.name().ok_or_else(CodecError::invalid)?;
        if questions
            .insert(
                name.to_owned(),
                json!({"type":"noul", "instructions":predicate.instructions()}),
            )
            .is_some()
        {
            return Err(CodecError::invalid());
        }
    }
    Ok(json!({"model":model,"state":state.as_str(),"questions":questions}))
}

pub(super) fn complete(
    request: &Request,
    root: &Map<String, Value>,
    public_model: &str,
) -> Result<Value, CodecError> {
    let reports = object(root.get("answers").ok_or_else(CodecError::invalid)?)?;
    if request.questions().len() != reports.len() {
        return Err(CodecError::invalid());
    }
    let outcomes = request
        .questions()
        .iter()
        .map(|question| {
            let key = question.name().ok_or_else(CodecError::invalid)?;
            let native = reports.get(key).ok_or_else(CodecError::invalid)?;
            if native["type"] != "noul" {
                return Err(CodecError::invalid());
            }
            probability(&native["noul"])
        })
        .collect::<Result<_, _>>()?;
    let answers = answers(request, outcomes)?;
    let usage = root.get("usage").ok_or_else(CodecError::invalid)?;
    let source = Usage {
        input_tokens: count(&usage["input_tokens"])?,
        output_tokens: count(&usage["output_tokens"])?,
        total_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
    };
    let delivery = request
        .questions()
        .iter()
        .zip(answers.outcomes())
        .map(|(question, outcome)| {
            let Outcome::Answered(probability) = outcome else {
                return Err(CodecError::invalid());
            };
            let key = question.name().ok_or_else(CodecError::invalid)?;
            Ok((
                key.to_owned(),
                json!({"type":"noul", "noul":probability.as_number()}),
            ))
        })
        .collect::<Result<Map<String, Value>, _>>()?;
    Ok(json!({
        "model":public_model, "answers":delivery, "usage":{
            "input_tokens":source.input_tokens, "output_tokens":source.output_tokens,
        }
    }))
}
