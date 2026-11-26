//! Selected TypeSafe → Gemini projection: plain-text Noul only.
use super::*;
use crate::semantic::task::decision::Disposition;
use std::collections::BTreeSet;

pub struct Prepared {
    pub body: Value,
    pub expectation: Expectation,
}

pub struct Expectation {
    names: Vec<String>,
    upstream: gemini::Expectation,
}

pub enum Delivery {
    Answered(Value),
    Blocked,
}

pub fn prepare(request: Request, model: &str) -> Result<Prepared, CodecError> {
    if !matches!(request.evidence(), Evidence::StateText(_)) {
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
    let source = gemini::prepare(request, model)?;
    Ok(Prepared {
        body: source.body,
        expectation: Expectation {
            names,
            upstream: source.expectation,
        },
    })
}

impl Expectation {
    pub fn complete(self, bytes: &[u8], public_model: &str) -> Result<Delivery, CodecError> {
        if !valid_model(public_model) {
            return Err(CodecError::invalid());
        }
        let source = self.upstream.complete(bytes)?;
        let Disposition::Evaluated(answers) = source.disposition else {
            return Ok(Delivery::Blocked);
        };
        let usage = source.usage.ok_or_else(CodecError::invalid)?;
        let input_tokens = usage.input_tokens.ok_or_else(CodecError::invalid)?;
        let output_tokens = usage.output_tokens.ok_or_else(CodecError::invalid)?;
        let answers = self
            .names
            .into_iter()
            .zip(answers.outcomes())
            .map(|(name, outcome)| {
                let Outcome::Answered(Answer::Predicate {
                    probability,
                    confidence: Some(Confidence::WinningMass(_)),
                }) = outcome
                else {
                    return Err(CodecError::invalid());
                };
                // decision.omit-gemini-source-reports.v1: confidence is a
                // validated source fact, not P(true) or a System One Noul field.
                Ok((name, json!({"type":"noul","noul":probability.as_number()})))
            })
            .collect::<Result<Map<_, _>, _>>()?;
        Ok(Delivery::Answered(json!({
            "model": public_model, "answers": answers,
            "usage": {"input_tokens": input_tokens,"output_tokens": output_tokens}
        })))
    }
}
