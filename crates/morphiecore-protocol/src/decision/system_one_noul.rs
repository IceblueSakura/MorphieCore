//! Selected projections from TypeSafe plain-text Noul.
use super::*;
use crate::semantic::task::decision::Disposition;
use std::collections::BTreeSet;

pub struct Prepared {
    pub body: Value,
    pub expectation: Expectation,
}

pub struct Expectation {
    names: Vec<String>,
    upstream: Source,
}

enum Source {
    Gemini(gemini::Expectation),
    OpenAi(Request),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoulTarget {
    Gemini,
    OpenAi,
}

pub enum Delivery {
    Answered(Value),
    Blocked,
    Refused,
}

pub fn prepare(request: Request, model: &str, target: NoulTarget) -> Result<Prepared, CodecError> {
    let Evidence::StateText(state) = request.evidence() else {
        return Err(CodecError::invalid());
    };
    if state.as_str().is_empty() || !valid_model(model) {
        return Err(CodecError::invalid());
    }
    let mut unique = BTreeSet::new();
    let names = request
        .questions()
        .iter()
        .map(|q| {
            if !matches!(q.kind(), QuestionKind::Predicate { criteria: None })
                || q.instructions().is_none_or(str::is_empty)
            {
                return Err(CodecError::invalid());
            }
            let name = q.name().ok_or_else(CodecError::invalid)?;
            if !unique.insert(name) {
                return Err(CodecError::invalid());
            }
            Ok(name.to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (body, upstream) = match target {
        NoulTarget::Gemini => {
            let prepared = gemini::prepare(request, model)?;
            (prepared.body, Source::Gemini(prepared.expectation))
        }
        NoulTarget::OpenAi => {
            // decision.wrap-state-as-user.v1: preserve every byte of state,
            // while explicitly introducing a single user role.
            let request = Request::new(
                Evidence::HistoryUserMessages(vec![UserMessage::Text(state.clone())]),
                request.questions().to_vec(),
            )
            .map_err(|_| CodecError::invalid())?;
            let body = openai::encode_request(&request, model)?;
            (body, Source::OpenAi(request))
        }
    };
    Ok(Prepared {
        body,
        expectation: Expectation { names, upstream },
    })
}

impl Expectation {
    pub fn complete(self, bytes: &[u8], public_model: &str) -> Result<Delivery, CodecError> {
        if !valid_model(public_model) {
            return Err(CodecError::invalid());
        }
        let (answers, usage) = match self.upstream {
            Source::Gemini(expectation) => {
                let source = expectation.complete(bytes)?;
                let Disposition::Evaluated(answers) = source.disposition else {
                    return Ok(Delivery::Blocked);
                };
                (answers, source.usage.ok_or_else(CodecError::invalid)?)
            }
            Source::OpenAi(request) => {
                let root = decode_native_result(bytes)?;
                let (answers, usage) = openai::evaluate(&request, object(&root)?)?;
                if answers
                    .outcomes()
                    .iter()
                    .any(|answer| matches!(answer, Outcome::Refused))
                {
                    return Ok(Delivery::Refused);
                }
                (answers, usage)
            }
        };
        let input_tokens = usage.input_tokens.ok_or_else(CodecError::invalid)?;
        let output_tokens = usage.output_tokens.ok_or_else(CodecError::invalid)?;
        let answers = self
            .names
            .into_iter()
            .zip(answers.outcomes())
            .map(|(name, outcome)| {
                let Outcome::Answered(Answer::Predicate { probability, .. }) = outcome else {
                    return Err(CodecError::invalid());
                };
                // decision.omit-gemini-source-reports.v1: confidence is a
                // validated source fact, not P(true) or a System One Noul field.
                Ok((name, json!({"type":"noul","noul":probability.as_number()})))
            })
            .collect::<Result<Map<_, _>, _>>()?;
        // decision.omit-openai-usage-details.v1 and
        // decision.omit-gemini-source-reports.v1: no System One detail carrier.
        Ok(Delivery::Answered(json!({
            "model": public_model, "answers": answers,
            "usage": {"input_tokens": input_tokens,"output_tokens": output_tokens}
        })))
    }
}
