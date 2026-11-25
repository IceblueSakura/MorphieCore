//! OpenAI Decisions ordered wire profile.
use super::*;

const MAX_QUESTIONS: usize = 400;
const PRECISION: distribution::Precision = distribution::Precision {
    probability_half_step: 0.005,
    score_half_step: 0.005,
};

fn name(value: Option<&Value>) -> Result<Option<&str>, CodecError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(string(Some(value))?)),
    }
}

fn json_data(value: &StructuredEvidence) -> Result<&Value, CodecError> {
    if value.compact_bytes() > 256 << 10 {
        return Err(CodecError::Limit);
    }
    if value
        .value()
        .as_array()
        .is_some_and(|items| !items.iter().all(Value::is_object))
    {
        return Err(CodecError::invalid());
    }
    Ok(value.value())
}

fn decode_input(input: &Value) -> Result<Evidence, CodecError> {
    if input.is_string() {
        return Ok(Evidence::HistoryUserText(text(Some(input))?));
    }
    let messages = array(input)?
        .iter()
        .map(|item| {
            let message = object(item)?;
            if string(message.get("role"))? != "user"
                || message.get("type").is_some_and(|t| t != "message")
            {
                return Err(CodecError::invalid());
            }
            let content = message.get("content").ok_or_else(CodecError::invalid)?;
            if content.is_string() {
                return Ok(UserMessage::Text(text(Some(content))?));
            }
            let parts = array(content)?
                .iter()
                .map(|part| {
                    let part = object(part)?;
                    match string(part.get("type"))? {
                        "input_text"
                            if part
                                .get("prompt_cache_breakpoint")
                                .is_none_or(Value::is_null) =>
                        {
                            Ok(UserPart::Text(text(part.get("text"))?))
                        }
                        "input_json" => {
                            let value = part.get("data").ok_or_else(CodecError::invalid)?;
                            let data = StructuredEvidence::new(value.clone())
                                .map_err(|_| CodecError::invalid())?;
                            json_data(&data)?;
                            Ok(UserPart::Json(data))
                        }
                        _ => Err(CodecError::invalid()),
                    }
                })
                .collect::<Result<_, _>>()?;
            Ok(UserMessage::Parts(parts))
        })
        .collect::<Result<_, _>>()?;
    Ok(Evidence::HistoryUserMessages(messages))
}

fn encode_input(evidence: &Evidence) -> Result<Value, CodecError> {
    match evidence {
        Evidence::HistoryUserText(text) => Ok(json!(text.as_str())),
        Evidence::HistoryUserMessages(messages) => {
            let messages = messages
                .iter()
                .map(|message| {
                    let content = match message {
                        UserMessage::Text(text) => json!(text.as_str()),
                        UserMessage::Parts(parts) => Value::Array(
                            parts
                                .iter()
                                .map(|part| {
                                    Ok(match part {
                                        UserPart::Text(text) => {
                                            json!({"type":"input_text","text":text.as_str()})
                                        }
                                        UserPart::Json(data) => {
                                            json!({"type":"input_json","data":json_data(data)?})
                                        }
                                    })
                                })
                                .collect::<Result<_, CodecError>>()?,
                        ),
                    };
                    Ok(json!({"type":"message","role":"user","content":content}))
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Ok(Value::Array(messages))
        }
        _ => Err(CodecError::invalid()),
    }
}

fn choice_wire(value: &ChoiceValue) -> Value {
    match value {
        ChoiceValue::String(s) => json!(s.as_str()),
        ChoiceValue::Bool(b) => json!(b),
    }
}

fn choice_index(options: &[ChoiceOption], wire: &Value) -> Result<usize, CodecError> {
    options
        .iter()
        .position(|option| match &option.value {
            ChoiceValue::String(s) => wire.as_str() == Some(s.as_str()),
            ChoiceValue::Bool(b) => wire.as_bool() == Some(*b),
        })
        .ok_or_else(CodecError::invalid)
}

fn decode_question(question: &Value) -> Result<Question, CodecError> {
    let question = object(question)?;
    let kind = match string(question.get("type"))? {
        "predicate" => QuestionKind::Predicate,
        "choice" => {
            let options = array(question.get("choices").ok_or_else(CodecError::invalid)?)?;
            if !(2..=255).contains(&options.len()) {
                return Err(CodecError::invalid());
            }
            QuestionKind::SingleChoice(
                options
                    .iter()
                    .map(|option| {
                        let option = object(option)?;
                        let value = match option.get("value") {
                            Some(Value::Bool(b)) => ChoiceValue::Bool(*b),
                            v => ChoiceValue::String(text(v)?),
                        };
                        let criterion = option
                            .get("description")
                            .map(|v| text(Some(v)))
                            .transpose()?;
                        Ok(ChoiceOption { value, criterion })
                    })
                    .collect::<Result<_, CodecError>>()?,
            )
        }
        "score" => {
            let levels = array(question.get("levels").ok_or_else(CodecError::invalid)?)?;
            if !(2..=10).contains(&levels.len()) {
                return Err(CodecError::invalid());
            }
            QuestionKind::OrdinalScore(
                levels
                    .iter()
                    .map(|level| {
                        let level = object(level)?;
                        Ok(ScoreLevel {
                            label: Some(text(level.get("label"))?),
                            criterion: level
                                .get("description")
                                .map(|v| text(Some(v)))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_, CodecError>>()?,
            )
        }
        _ => return Err(CodecError::invalid()),
    };
    Question::new(
        name(question.get("name"))?.map(str::to_owned),
        Some(text(question.get("instructions"))?),
        kind,
    )
    .map_err(|_| CodecError::invalid())
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
    let input = decode_input(root.get("input").ok_or_else(CodecError::invalid)?)?;
    let questions = array(root.get("questions").ok_or_else(CodecError::invalid)?)?;
    if !(1..=MAX_QUESTIONS).contains(&questions.len()) {
        return Err(CodecError::Limit);
    }
    let questions = questions
        .iter()
        .map(decode_question)
        .collect::<Result<_, _>>()?;
    Request::new(input, questions).map_err(|_| CodecError::invalid())
}

fn describe(mut value: Value, description: Option<&Text>) -> Value {
    if let Some(description) = description {
        value["description"] = json!(description.as_str());
    }
    value
}

fn encode_question(question: &Question) -> Result<Value, CodecError> {
    let mut value = match question.kind() {
        QuestionKind::Predicate => json!({"type":"predicate"}),
        QuestionKind::SingleChoice(options) if (2..=255).contains(&options.len()) => {
            let choices: Vec<_> = options
                .iter()
                .map(|o| describe(json!({"value":choice_wire(&o.value)}), o.criterion.as_ref()))
                .collect();
            json!({"type":"choice", "choices":choices})
        }
        QuestionKind::OrdinalScore(levels) if (2..=10).contains(&levels.len()) => {
            let levels = levels
                .iter()
                .map(|l| {
                    let label = l.label.as_ref().ok_or_else(CodecError::invalid)?;
                    Ok(describe(
                        json!({"label":label.as_str()}),
                        l.criterion.as_ref(),
                    ))
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            json!({"type":"score", "levels":levels})
        }
        _ => return Err(CodecError::invalid()),
    };
    value["instructions"] = json!(question.instructions().ok_or_else(CodecError::invalid)?);
    if let Some(name) = question.name() {
        value["name"] = json!(name);
    }
    Ok(value)
}

pub(super) fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    let input = encode_input(request.evidence())?;
    if request.questions().len() > MAX_QUESTIONS {
        return Err(CodecError::Limit);
    }
    let questions = request
        .questions()
        .iter()
        .map(encode_question)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"model":model,"input":input,"questions":questions}))
}

fn decode_answer(question: &Question, value: &Value) -> Result<Outcome, CodecError> {
    let map = object(value)?;
    let reported_name = map.get("name").ok_or_else(CodecError::invalid)?;
    if name(Some(reported_name))? != question.name() {
        return Err(CodecError::invalid());
    }
    let parsed = match (question.kind(), string(map.get("type"))?) {
        (_, "refusal") => return Ok(Outcome::Refused),
        (QuestionKind::Predicate, "predicate") => {
            Answer::Predicate(probability(&value["probability"])?)
        }
        (QuestionKind::SingleChoice(options), "choice") => {
            let selected = choice_index(options, &value["choice"])?;
            let probabilities = ordered_probabilities(
                options.len(),
                array(&value["probabilities"])?.iter().map(|p| {
                    Ok((
                        choice_index(options, &p["value"])?,
                        probability(&p["probability"])?,
                    ))
                }),
            )?;
            PRECISION.choice(&probabilities, selected)?;
            Answer::SingleChoice {
                selected,
                probabilities,
                confidence: Confidence::Unspecified(probability(&value["confidence"])?),
            }
        }
        (QuestionKind::OrdinalScore(levels), "score") => {
            let score = value["score"]
                .as_number()
                .ok_or_else(CodecError::invalid)?
                .clone();
            let probabilities = ordered_probabilities(
                levels.len(),
                array(&value["probabilities"])?.iter().map(|p| {
                    let index =
                        usize::try_from(count(&p["value"])?).map_err(|_| CodecError::invalid())?;
                    let label = levels
                        .get(index)
                        .and_then(|l| l.label.as_ref())
                        .ok_or_else(CodecError::invalid)?;
                    if p["label"].as_str() != Some(label.as_str()) {
                        return Err(CodecError::invalid());
                    }
                    Ok((index, probability(&p["probability"])?))
                }),
            )?;
            PRECISION.score(&probabilities, &score)?;
            Answer::OrdinalScore {
                score,
                probabilities,
                confidence: Confidence::Unspecified(probability(&value["confidence"])?),
            }
        }
        _ => return Err(CodecError::invalid()),
    };
    Ok(Outcome::Answered(parsed))
}

fn encode_answer(question: &Question, outcome: &Outcome) -> Result<Value, CodecError> {
    let name = question.name();
    Ok(match (question.kind(), outcome) {
        (_, Outcome::Refused) => json!({"type":"refusal", "name":name}),
        (QuestionKind::Predicate, Outcome::Answered(Answer::Predicate(p))) => {
            json!({"type":"predicate", "name":name, "probability":p.as_number()})
        }
        (
            QuestionKind::SingleChoice(options),
            Outcome::Answered(Answer::SingleChoice {
                selected,
                probabilities,
                confidence: Confidence::Unspecified(confidence),
            }),
        ) => {
            let probabilities: Vec<_> = options.iter().zip(probabilities).map(|(option, p)| {
                json!({"value":choice_wire(&option.value), "probability":p.as_number()})
            }).collect();
            json!({"type":"choice", "name":name, "choice":choice_wire(&options[*selected].value),
                   "probabilities":probabilities, "confidence":confidence.as_number()})
        }
        (
            QuestionKind::OrdinalScore(levels),
            Outcome::Answered(Answer::OrdinalScore {
                score,
                probabilities,
                confidence: Confidence::Unspecified(confidence),
            }),
        ) => {
            let probabilities: Vec<_> = levels.iter().zip(probabilities).enumerate().map(|(index, (level, p))| {
                json!({"value":index, "label":level.label.as_ref().map(Text::as_str), "probability":p.as_number()})
            }).collect();
            json!({"type":"score", "name":name, "score":score, "probabilities":probabilities, "confidence":confidence.as_number()})
        }
        _ => return Err(CodecError::invalid()),
    })
}

pub(super) fn complete(
    request: &Request,
    root: &Map<String, Value>,
    public_model: &str,
) -> Result<Value, CodecError> {
    let reports = array(root.get("answers").ok_or_else(CodecError::invalid)?)?;
    if request.questions().len() != reports.len() {
        return Err(CodecError::invalid());
    }
    let outcomes = request
        .questions()
        .iter()
        .zip(reports)
        .map(|(q, value)| decode_answer(q, value))
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
    let answers = request
        .questions()
        .iter()
        .zip(answers.outcomes())
        .map(|(q, value)| encode_answer(q, value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "model":public_model, "answers":answers, "usage":{
            "input_tokens":source.input_tokens, "output_tokens":source.output_tokens,
            "total_tokens":source.total_tokens,
            "input_tokens_details":{"cached_tokens":source.cache_read_tokens, "cache_write_tokens":source.cache_write_tokens},
            "output_tokens_details":{"reasoning_tokens":source.reasoning_tokens}
        }
    }))
}
