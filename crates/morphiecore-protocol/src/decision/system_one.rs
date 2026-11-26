//! TypeSafe System One keyed wire profile.
use super::*;

const PRECISION: distribution::Precision = distribution::Precision {
    probability_half_step: 0.005,
    score_half_step: 0.005,
};

fn decode_question(name: &str, question: &Value) -> Result<Question, CodecError> {
    let question = object(question)?;
    let criteria = question.get("criteria");
    let kind = match string(question.get("type"))? {
        "noul" if criteria.is_none_or(Value::is_null) => QuestionKind::Predicate { criteria: None },
        "choice" => {
            let entries = object(criteria.ok_or_else(CodecError::invalid)?)?;
            if !(1..=255).contains(&entries.len()) {
                return Err(CodecError::invalid());
            }
            QuestionKind::SingleChoice(
                entries
                    .iter()
                    .map(|(key, v)| {
                        Ok(ChoiceOption {
                            value: ChoiceValue::String(
                                Text::allowing_empty(key, "choice key", MAX_TEXT_BYTES)
                                    .map_err(|_| CodecError::Limit)?,
                            ),
                            criterion: optional_text(Some(v))?,
                        })
                    })
                    .collect::<Result<_, CodecError>>()?,
            )
        }
        "score" => {
            let entries = array(criteria.ok_or_else(CodecError::invalid)?)?;
            if !(1..=10).contains(&entries.len()) {
                return Err(CodecError::invalid());
            }
            QuestionKind::OrdinalScore(
                entries
                    .iter()
                    .map(|v| {
                        Ok(ScoreLevel {
                            label: None,
                            criterion: Some(text(Some(v))?),
                        })
                    })
                    .collect::<Result<_, CodecError>>()?,
            )
        }
        _ => return Err(CodecError::invalid()),
    };
    Question::new(
        Some(name.to_owned()),
        optional_text(question.get("instructions"))?,
        kind,
    )
    .map_err(|_| CodecError::invalid())
}

pub(super) fn decode_request(root: &Map<String, Value>) -> Result<Request, CodecError> {
    if root.get("assets").is_some_and(|value| !value.is_null()) {
        return Err(CodecError::invalid());
    }
    let state = match root.get("state").ok_or_else(CodecError::invalid)? {
        state if state.is_string() => Evidence::StateText(text(Some(state))?),
        state => Evidence::StateJson(
            StructuredEvidence::new(state.clone()).map_err(|_| CodecError::invalid())?,
        ),
    };
    let questions = object(root.get("questions").ok_or_else(CodecError::invalid)?)?;
    if questions.len() > crate::semantic::task::decision::MAX_QUESTIONS {
        return Err(CodecError::Limit);
    }
    let questions = questions
        .iter()
        .map(|(name, q)| decode_question(name, q))
        .collect::<Result<_, _>>()?;
    Request::new(state, questions).map_err(|_| CodecError::invalid())
}

fn choice_key(option: &ChoiceOption) -> Result<&str, CodecError> {
    let ChoiceValue::String(value) = &option.value else {
        return Err(CodecError::invalid());
    };
    Ok(value.as_str())
}

fn choice_index(options: &[ChoiceOption], key: &str) -> Result<usize, CodecError> {
    options
        .iter()
        .position(|option| choice_key(option).is_ok_and(|value| value == key))
        .ok_or_else(CodecError::invalid)
}

fn criterion(level: &ScoreLevel) -> Result<&str, CodecError> {
    if level.label.is_some() {
        return Err(CodecError::invalid());
    }
    level
        .criterion
        .as_ref()
        .map(Text::as_str)
        .ok_or_else(CodecError::invalid)
}

fn encode_question(question: &Question) -> Result<Value, CodecError> {
    let mut value = match question.kind() {
        QuestionKind::Predicate { criteria: None } => json!({"type":"noul"}),
        QuestionKind::SingleChoice(options) => {
            let criteria = options
                .iter()
                .map(|o| {
                    Ok((
                        choice_key(o)?.to_owned(),
                        json!(o.criterion.as_ref().map(Text::as_str)),
                    ))
                })
                .collect::<Result<Map<String, Value>, CodecError>>()?;
            json!({"type":"choice", "criteria":criteria})
        }
        QuestionKind::OrdinalScore(levels) if levels.len() <= 10 => {
            let criteria = levels
                .iter()
                .map(criterion)
                .collect::<Result<Vec<_>, _>>()?;
            json!({"type":"score", "criteria":criteria})
        }
        _ => return Err(CodecError::invalid()),
    };
    if let Some(instructions) = question.instructions() {
        value["instructions"] = json!(instructions);
    }
    Ok(value)
}

pub(super) fn encode_request(request: &Request, model: &str) -> Result<Value, CodecError> {
    let state = match request.evidence() {
        Evidence::StateText(state) => json!(state.as_str()),
        Evidence::StateJson(state) => state.value().clone(),
        _ => return Err(CodecError::invalid()),
    };
    let mut questions = Map::new();
    for question in request.questions() {
        let key = question.name().ok_or_else(CodecError::invalid)?;
        if questions
            .insert(key.to_owned(), encode_question(question)?)
            .is_some()
        {
            return Err(CodecError::invalid());
        }
    }
    Ok(json!({"model":model,"state":state,"questions":questions}))
}

fn decode_answer(question: &Question, value: &Value) -> Result<Outcome, CodecError> {
    let kind = value["type"].as_str().ok_or_else(CodecError::invalid)?;
    let answer = match (question.kind(), kind) {
        (QuestionKind::Predicate { criteria: None }, "noul") => Answer::Predicate {
            probability: probability(&value["noul"])?,
            confidence: None,
        },
        (QuestionKind::SingleChoice(options), "choice") => {
            let selected = choice_index(options, string(value.get("choice"))?)?;
            let probabilities = ordered_probabilities(
                options.len(),
                object(&value["probabilities"])?
                    .iter()
                    .map(|(key, p)| Ok((choice_index(options, key)?, probability(p)?))),
            )?;
            PRECISION.choice(&probabilities, selected)?;
            let confidence = probability(&value["confidence"])?;
            Answer::SingleChoice {
                selected,
                probabilities,
                confidence: if options.len() == 1 {
                    Confidence::Unspecified(confidence)
                } else {
                    Confidence::UniformAdjustedMaximum(confidence)
                },
            }
        }
        (QuestionKind::OrdinalScore(levels), "score") => {
            let score = value["score"]
                .as_number()
                .ok_or_else(CodecError::invalid)?
                .clone();
            let legend = object(&value["legend"])?;
            if legend.len() != levels.len() {
                return Err(CodecError::invalid());
            }
            let probabilities = ordered_probabilities(
                levels.len(),
                object(&value["probabilities"])?.iter().map(|(key, p)| {
                    let index = key.parse::<usize>().map_err(|_| CodecError::invalid())?;
                    let level = levels.get(index).ok_or_else(CodecError::invalid)?;
                    if index.to_string() != *key
                        || legend.get(key).and_then(Value::as_str) != Some(criterion(level)?)
                    {
                        return Err(CodecError::invalid());
                    }
                    Ok((index, probability(p)?))
                }),
            )?;
            PRECISION.score(&probabilities, &score)?;
            let confidence = probability(&value["confidence"])?;
            Answer::OrdinalScore {
                score,
                probabilities,
                confidence: if levels.len() == 1 {
                    Confidence::Unspecified(confidence)
                } else {
                    Confidence::CenteredOrdinalSpread(confidence)
                },
            }
        }
        _ => return Err(CodecError::invalid()),
    };
    Ok(Outcome::Answered(answer))
}

fn encode_answer(question: &Question, outcome: &Outcome) -> Result<Value, CodecError> {
    Ok(match (question.kind(), outcome) {
        (
            QuestionKind::Predicate { criteria: None },
            Outcome::Answered(Answer::Predicate {
                probability: p,
                confidence: None,
            }),
        ) => {
            json!({"type":"noul", "noul":p.as_number()})
        }
        (
            QuestionKind::SingleChoice(options),
            Outcome::Answered(Answer::SingleChoice {
                selected,
                probabilities,
                confidence,
            }),
        ) => {
            let probabilities = options
                .iter()
                .zip(probabilities)
                .map(|(o, p)| Ok((choice_key(o)?.to_owned(), json!(p.as_number()))))
                .collect::<Result<Map<String, Value>, CodecError>>()?;
            json!({"type":"choice", "choice":choice_key(&options[*selected])?,
                   "probabilities":probabilities, "confidence":confidence.reported().as_number()})
        }
        (
            QuestionKind::OrdinalScore(levels),
            Outcome::Answered(Answer::OrdinalScore {
                score,
                probabilities,
                confidence,
            }),
        ) => {
            let legend = levels
                .iter()
                .enumerate()
                .map(|(i, level)| Ok((i.to_string(), json!(criterion(level)?))))
                .collect::<Result<Map<String, Value>, CodecError>>()?;
            let probabilities = probabilities
                .iter()
                .enumerate()
                .map(|(i, p)| (i.to_string(), json!(p.as_number())))
                .collect::<Map<String, Value>>();
            json!({"type":"score", "score":score, "legend":legend,
                   "probabilities":probabilities, "confidence":confidence.reported().as_number()})
        }
        _ => return Err(CodecError::invalid()),
    })
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
        .map(|q| {
            let key = q.name().ok_or_else(CodecError::invalid)?;
            decode_answer(q, reports.get(key).ok_or_else(CodecError::invalid)?)
        })
        .collect::<Result<_, _>>()?;
    let answers = answers(request, outcomes)?;
    let usage = root.get("usage").ok_or_else(CodecError::invalid)?;
    let source = Usage {
        input_tokens: Some(count(&usage["input_tokens"])?),
        output_tokens: Some(count(&usage["output_tokens"])?),
        ..Usage::default()
    };
    let delivery = request
        .questions()
        .iter()
        .zip(answers.outcomes())
        .map(|(q, value)| {
            Ok((
                q.name().ok_or_else(CodecError::invalid)?.to_owned(),
                encode_answer(q, value)?,
            ))
        })
        .collect::<Result<Map<String, Value>, CodecError>>()?;
    Ok(json!({
        "model":public_model, "answers":delivery, "usage":{
            "input_tokens":source.input_tokens, "output_tokens":source.output_tokens,
        }
    }))
}
