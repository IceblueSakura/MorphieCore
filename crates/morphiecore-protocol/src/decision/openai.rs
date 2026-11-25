//! OpenAI Decisions ordered Predicate wire profile.
use super::*;

const MAX_QUESTIONS: usize = 400;

fn name(value: Option<&Value>) -> Result<Option<&str>, CodecError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(string(Some(value))?)),
    }
}

pub(super) fn decode_request(root: &Map<String, Value>) -> Result<Request, CodecError> {
    // Known service controls are not part of the initial profile.
    if [
        "prompt_cache_key",
        "prompt_cache_options",
        "safety_identifier",
        "store",
    ]
    .into_iter()
    .any(|field| root.get(field).is_some_and(|value| !value.is_null()))
    {
        return Err(CodecError::invalid());
    }
    let input = text(root.get("input"))?;
    let questions = root
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(CodecError::invalid)?;
    if !(1..=MAX_QUESTIONS).contains(&questions.len()) {
        return Err(CodecError::Limit);
    }
    let predicates = questions
        .iter()
        .map(|question| {
            let question = object(question)?;
            if string(question.get("type"))? != "predicate" {
                return Err(CodecError::invalid());
            }
            Predicate::new(
                name(question.get("name"))?.map(str::to_owned),
                string(question.get("instructions"))?.to_owned(),
            )
            .map_err(|_| CodecError::invalid())
        })
        .collect::<Result<_, _>>()?;
    Request::new(Evidence::HistoryUserText(input), predicates).map_err(|_| CodecError::invalid())
}

pub(super) fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    let Evidence::HistoryUserText(input) = request.evidence() else {
        return Err(CodecError::invalid());
    };
    if request.questions().len() > MAX_QUESTIONS {
        return Err(CodecError::Limit);
    }
    let questions: Vec<_> = request
        .questions()
        .iter()
        .map(|question| {
            let mut value = json!({"type":"predicate", "instructions":question.instructions()});
            if let Some(name) = question.name() {
                value["name"] = json!(name);
            }
            value
        })
        .collect();
    Ok(json!({"model":model,"input":input.as_str(),"questions":questions}))
}

pub(super) fn complete(
    request: &Request,
    root: &Map<String, Value>,
    public_model: &str,
) -> Result<Value, CodecError> {
    let reports = root
        .get("answers")
        .and_then(Value::as_array)
        .ok_or_else(CodecError::invalid)?;
    if request.questions().len() != reports.len() {
        return Err(CodecError::invalid());
    }
    let outcomes = reports
        .iter()
        .zip(request.questions())
        .map(|(native, question)| {
            let native = object(native)?;
            let reported_name = native.get("name").ok_or_else(CodecError::invalid)?;
            if name(Some(reported_name))? != question.name() {
                return Err(CodecError::invalid());
            }
            match string(native.get("type"))? {
                "predicate" => {
                    probability(native.get("probability").ok_or_else(CodecError::invalid)?)
                }
                "refusal" => Ok(Outcome::Refused),
                _ => Err(CodecError::invalid()),
            }
        })
        .collect::<Result<_, _>>()?;
    let answers = answers(request, outcomes)?;
    let usage = root.get("usage").ok_or_else(CodecError::invalid)?;
    let source = Usage {
        input_tokens: count(&usage["input_tokens"])?,
        output_tokens: count(&usage["output_tokens"])?,
        total_tokens: Some(count(&usage["total_tokens"])?),
        cache_read_tokens: Some(count(&usage["input_tokens_details"]["cached_tokens"])?),
        cache_write_tokens: Some(count(&usage["input_tokens_details"]["cache_write_tokens"])?),
        reasoning_tokens: Some(count(&usage["output_tokens_details"]["reasoning_tokens"])?),
    };
    let answers: Vec<_> = request
        .questions()
        .iter()
        .zip(answers.outcomes())
        .map(|(question, outcome)| match outcome {
            Outcome::Answered(probability) => json!({
                "type":"predicate", "name":question.name(), "probability":probability.as_number(),
            }),
            Outcome::Refused => json!({"type":"refusal", "name":question.name()}),
        })
        .collect();
    Ok(json!({
        "model":public_model, "answers":answers, "usage":{
            "input_tokens":source.input_tokens, "output_tokens":source.output_tokens,
            "total_tokens":source.total_tokens,
            "input_tokens_details":{"cached_tokens":source.cache_read_tokens, "cache_write_tokens":source.cache_write_tokens},
            "output_tokens_details":{"reasoning_tokens":source.reasoning_tokens}
        }
    }))
}
