//! Gemini Developer API Decisions, upstream text profile only.
use super::*;
use crate::semantic::task::{
    InputModality, ModalityTokenCount,
    decision::{Disposition, Evaluation, PromptBlock},
};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

// The guide illustrates decimal rounding; tolerate its rounding envelope
// without replacing any observed probability, confidence, or score.
const HALF_STEP: f64 = 0.005;
const MAX_TEXT_PAYLOAD: usize = 4 << 20;
const PRECISION: distribution::Precision = distribution::Precision {
    probability_half_step: HALF_STEP,
    score_half_step: HALF_STEP,
};

pub struct Prepared {
    pub body: Value,
    pub expectation: Expectation,
}

pub struct Expectation {
    request: Request,
    questions: Vec<ExpectedQuestion>,
}

struct ExpectedQuestion {
    key: String,
    // Gemini evaluates UTF-8 sorted IDs, not the source order.
    choice_slots: BTreeMap<String, usize>,
}

fn charge<'a>(budget: &mut usize, text: &'a str) -> Result<&'a str, CodecError> {
    *budget = budget.checked_sub(text.len()).ok_or(CodecError::Limit)?;
    Ok(text)
}

pub fn prepare(request: Request, model: &str) -> Result<Prepared, CodecError> {
    if !valid_model(model) {
        return Err(CodecError::invalid());
    }
    // Bound cloning before the gateway checks the serialized (escaped) size.
    let mut budget = MAX_TEXT_PAYLOAD;
    let input = match request.evidence() {
        Evidence::StateText(text) if !text.as_str().is_empty() => {
            json!(charge(&mut budget, text.as_str())?)
        }
        Evidence::TextParts(parts) => Value::Array(
            parts
                .iter()
                .map(|part| Ok(json!({"type":"text","text":charge(&mut budget, part.as_str())?})))
                .collect::<Result<_, CodecError>>()?,
        ),
        _ => return Err(CodecError::invalid()),
    };
    let mut questions = Map::new();
    let mut expected = Vec::with_capacity(request.questions().len());
    for (i, question) in request.questions().iter().enumerate() {
        // Wire keys are invisible to Gemini's evaluator. Use request-local
        // slots so absent or repeated source names cannot collide.
        let key = i.to_string();
        let (body, choice_slots) = encode_question(question, &mut budget)?;
        questions.insert(key.clone(), body);
        expected.push(ExpectedQuestion { key, choice_slots });
    }
    Ok(Prepared {
        body: json!({"model":model,"input":input,"questions":questions}),
        expectation: Expectation {
            request,
            questions: expected,
        },
    })
}

fn encode_question(
    question: &Question,
    budget: &mut usize,
) -> Result<(Value, BTreeMap<String, usize>), CodecError> {
    let instructions = charge(
        budget,
        question.instructions().ok_or_else(CodecError::invalid)?,
    )?;
    let mut choice_slots = BTreeMap::new();
    let body = match question.kind() {
        QuestionKind::Predicate { criteria } => {
            let mut value = json!({"type":"noul", "instructions":instructions});
            if let Some(criteria) = criteria {
                let mut wire = Map::new();
                if let Some(text) = &criteria.when_true {
                    wire.insert(
                        "true_description".into(),
                        json!(charge(budget, text.as_str())?),
                    );
                }
                if let Some(text) = &criteria.when_false {
                    wire.insert(
                        "false_description".into(),
                        json!(charge(budget, text.as_str())?),
                    );
                }
                value["criteria"] = Value::Object(wire);
            }
            value
        }
        QuestionKind::SingleChoice(options) if options.len() >= 2 => {
            let mut criteria = Map::new();
            for (i, option) in options.iter().enumerate() {
                let ChoiceValue::String(key) = &option.value else {
                    return Err(CodecError::invalid());
                };
                let key = key.as_str();
                if key.is_empty() || key.trim() != key || key.chars().any(char::is_control) {
                    return Err(CodecError::invalid());
                }
                charge(budget, key)?;
                let description =
                    charge(budget, option.criterion.as_ref().map_or("", Text::as_str))?;
                choice_slots.insert(key.to_owned(), i);
                // Documented empty description: the option ID stands on its own.
                criteria.insert(key.to_owned(), json!(description));
            }
            json!({"type":"choice", "instructions":instructions, "criteria":criteria})
        }
        QuestionKind::OrdinalScore(levels) if levels.len() >= 2 => {
            let criteria = levels
                .iter()
                .map(|level| charge(budget, level_criterion(level)?))
                .collect::<Result<Vec<_>, _>>()?;
            json!({"type":"score", "instructions":instructions, "criteria":criteria})
        }
        _ => return Err(CodecError::invalid()),
    };
    Ok((body, choice_slots))
}

fn level_criterion(level: &ScoreLevel) -> Result<&str, CodecError> {
    match (&level.label, &level.criterion) {
        (None, Some(text)) if !text.as_str().is_empty() => Ok(text.as_str()),
        _ => Err(CodecError::invalid()),
    }
}

impl Expectation {
    pub fn complete(self, bytes: &[u8]) -> Result<Evaluation, CodecError> {
        let value = decode(bytes)?;
        let root = object(&value)?;
        if root.get("error").is_some_and(|error| !error.is_null()) {
            return Err(CodecError::invalid());
        }
        let block = root
            .get("prompt_feedback")
            .filter(|v| !v.is_null())
            .map(decode_block)
            .transpose()?
            .flatten();
        let reports = root.get("answers").map(object).transpose()?;
        let usage = root
            .get("usage")
            .filter(|v| !v.is_null())
            .map(decode_usage)
            .transpose()?;
        let disposition = if let Some(block) = block {
            if reports.is_some_and(|reports| !reports.is_empty()) {
                return Err(CodecError::invalid());
            }
            Disposition::Blocked(block)
        } else {
            let reports = reports
                .filter(|reports| reports.len() == self.questions.len())
                .ok_or_else(CodecError::invalid)?;
            let outcomes = self
                .request
                .questions()
                .iter()
                .zip(&self.questions)
                .map(|(question, expected)| {
                    let answer = reports.get(&expected.key).ok_or_else(CodecError::invalid)?;
                    decode_answer(question, expected, answer)
                })
                .collect::<Result<_, _>>()?;
            Disposition::Evaluated(answers(&self.request, outcomes)?)
        };
        Ok(Evaluation { disposition, usage })
    }
}

fn decode_block(value: &Value) -> Result<Option<PromptBlock>, CodecError> {
    let value = object(value)?;
    let message = optional_text(value.get("block_reason_message"))?;
    let Some(reason) = value.get("block_reason").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let reason = string(Some(reason))?;
    if reason.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(CodecError::invalid());
    }
    let reason =
        Text::new(reason, "Decision block reason", 128).map_err(|_| CodecError::invalid())?;
    Ok(Some(PromptBlock { reason, message }))
}

fn decode_answer(
    question: &Question,
    expected: &ExpectedQuestion,
    value: &Value,
) -> Result<Outcome, CodecError> {
    let map = object(value)?;
    let confidence = probability(&value["confidence"])?;
    let answer = match (question.kind(), string(map.get("type"))?) {
        (QuestionKind::Predicate { .. }, "noul") => {
            let probability = probability(&value["noul"])?;
            let p = mass(&probability)?;
            if !(0.01..=0.99).contains(&p) {
                return Err(CodecError::invalid());
            }
            winning_mass(&confidence, p.max(1. - p))?;
            Answer::Predicate {
                probability,
                confidence: Some(Confidence::WinningMass(confidence)),
            }
        }
        (QuestionKind::SingleChoice(options), "choice") => {
            let selected = string(map.get("choice"))?;
            let reports = object(&value["probabilities"])?;
            if reports.len() != options.len() {
                return Err(CodecError::invalid());
            }
            // Read in target order, validate target winner, then restore slots.
            let ordered = expected
                .choice_slots
                .keys()
                .map(|key| probability(reports.get(key).ok_or_else(CodecError::invalid)?))
                .collect::<Result<Vec<_>, _>>()?;
            let target_slot = expected
                .choice_slots
                .keys()
                .position(|key| key == selected)
                .ok_or_else(CodecError::invalid)?;
            PRECISION.choice(&ordered, target_slot)?;
            let winner = mass(&ordered[target_slot])?;
            for p in &ordered[..target_slot] {
                if mass(p)? == winner {
                    return Err(CodecError::invalid());
                }
            }
            winning_mass(&confidence, winner)?;
            let probabilities = ordered_probabilities(
                options.len(),
                expected.choice_slots.values().copied().zip(ordered).map(Ok),
            )?;
            Answer::SingleChoice {
                selected: expected.choice_slots[selected],
                probabilities,
                confidence: Confidence::WinningMass(confidence),
            }
        }
        (QuestionKind::OrdinalScore(levels), "score") => {
            let legend = object(&value["legend"])?;
            let reports = object(&value["probabilities"])?;
            if legend.len() != levels.len() || reports.len() != levels.len() {
                return Err(CodecError::invalid());
            }
            let probabilities = levels
                .iter()
                .enumerate()
                .map(|(i, level)| {
                    let key = i.to_string();
                    if string(legend.get(&key))? != level_criterion(level)? {
                        return Err(CodecError::invalid());
                    }
                    probability(reports.get(&key).ok_or_else(CodecError::invalid)?)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let score = value["score"]
                .as_number()
                .ok_or_else(CodecError::invalid)?
                .clone();
            PRECISION.score(&probabilities, &score)?;
            let maximum = probabilities
                .iter()
                .try_fold(0_f64, |top, p| mass(p).map(|mass| top.max(mass)))?;
            winning_mass(&confidence, maximum)?;
            Answer::OrdinalScore {
                score,
                probabilities,
                confidence: Confidence::WinningMass(confidence),
            }
        }
        _ => return Err(CodecError::invalid()),
    };
    Ok(Outcome::Answered(answer))
}

fn mass(p: &Probability) -> Result<f64, CodecError> {
    p.as_number().as_f64().ok_or_else(CodecError::invalid)
}

fn winning_mass(confidence: &Probability, expected: f64) -> Result<(), CodecError> {
    // Both sides of the documented identity may be rounded.
    if (mass(confidence)? - expected).abs() > 2. * HALF_STEP + 1e-12 {
        return Err(CodecError::invalid());
    }
    Ok(())
}

fn decode_usage(value: &Value) -> Result<Usage, CodecError> {
    let value = object(value)?;
    let optional_count = |key| {
        value
            .get(key)
            .filter(|v| !v.is_null())
            .map(count)
            .transpose()
    };
    let output_tokens = optional_count("total_output_tokens")?;
    if output_tokens.is_some_and(|n| n != 0) {
        return Err(CodecError::invalid());
    }
    let input_by_modality = value
        .get("input_tokens_by_modality")
        .filter(|v| !v.is_null())
        .map(|items| {
            array(items)?
                .iter()
                .map(|row| {
                    let modality = match string(row.get("modality"))? {
                        "text" => InputModality::Text,
                        "image" => InputModality::Image,
                        "audio" => InputModality::Audio,
                        "video" => InputModality::Video,
                        "document" => InputModality::Document,
                        _ => return Err(CodecError::invalid()),
                    };
                    Ok(ModalityTokenCount {
                        modality,
                        tokens: count(&row["tokens"])?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Usage {
        input_tokens: optional_count("total_input_tokens")?,
        output_tokens,
        total_tokens: optional_count("total_tokens")?,
        cache_read_tokens: optional_count("total_cached_tokens")?,
        reasoning_tokens: optional_count("total_thought_tokens")?,
        input_by_modality,
        ..Usage::default()
    })
}
