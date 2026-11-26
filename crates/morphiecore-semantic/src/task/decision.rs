//! Decision text evidence, ordered questions, and request-bound outcomes.
use super::ModalityTokenCount;
use crate::value::{Text, json_size};
use serde_json::{Number, Value};
use std::{collections::BTreeSet, fmt};

pub const MAX_QUESTIONS: usize = 1_000;
pub const MAX_TEXT_BYTES: usize = 1 << 20;
pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_LEVELS: usize = 255;
pub const MAX_TEXT_PARTS: usize = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("invalid Decision evidence or question set")]
    Request,
    #[error("Decision answers do not match the request")]
    Association,
    #[error("invalid Decision probability")]
    Probability,
}

/// State is role-less; OpenAI evidence retains its user-owned message boundaries.
#[derive(Clone, Eq, PartialEq)]
pub enum Evidence {
    StateText(Text),
    StateJson(StructuredEvidence),
    TextParts(Vec<Text>),
    HistoryUserText(Text),
    HistoryUserMessages(Vec<UserMessage>),
}

#[derive(Clone, Eq, PartialEq)]
pub enum UserMessage {
    Text(Text),
    Parts(Vec<UserPart>),
}

#[derive(Clone, Eq, PartialEq)]
pub enum UserPart {
    Text(Text),
    Json(StructuredEvidence),
}

/// Ordered business JSON, never interpreted as an instruction or media carrier.
#[derive(Clone, Eq, PartialEq)]
pub struct StructuredEvidence {
    value: Value,
    compact_bytes: usize,
}
impl StructuredEvidence {
    pub fn new(value: Value) -> Result<Self, Error> {
        const MAX_VALUES: usize = 10_000;
        const MAX_CONTAINERS: usize = 32;
        fn bounded(value: &Value, depth: usize, remaining: &mut usize) -> bool {
            let Some(left) = remaining.checked_sub(1) else {
                return false;
            };
            *remaining = left;
            match value {
                Value::Array(items) => {
                    depth < MAX_CONTAINERS && items.iter().all(|v| bounded(v, depth + 1, remaining))
                }
                Value::Object(items) => {
                    depth < MAX_CONTAINERS
                        && items.values().all(|v| bounded(v, depth + 1, remaining))
                }
                _ => true,
            }
        }
        if !matches!(value, Value::Array(_) | Value::Object(_)) {
            return Err(Error::Request);
        }
        let mut remaining = MAX_VALUES;
        if !bounded(&value, 0, &mut remaining) {
            return Err(Error::Request);
        }
        let compact_bytes = json_size(&value, MAX_TEXT_BYTES).map_err(|_| Error::Request)?;
        Ok(Self {
            value,
            compact_bytes,
        })
    }
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn compact_bytes(&self) -> usize {
        self.compact_bytes
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Question {
    name: Option<Text>,
    instructions: Option<Text>,
    kind: QuestionKind,
}
impl Question {
    pub fn new(
        name: Option<String>,
        instructions: Option<Text>,
        kind: QuestionKind,
    ) -> Result<Self, Error> {
        let name = name
            .map(|name| Text::allowing_empty(name, "question name", MAX_NAME_BYTES))
            .transpose()
            .map_err(|_| Error::Request)?;
        let within_bound = |text: &Text| text.as_str().len() <= MAX_TEXT_BYTES;
        if !instructions.as_ref().is_none_or(within_bound) {
            return Err(Error::Request);
        }
        match &kind {
            QuestionKind::Predicate { criteria } => {
                if instructions.is_none()
                    || criteria.as_ref().is_some_and(|criteria| {
                        !criteria.when_true.as_ref().is_none_or(within_bound)
                            || !criteria.when_false.as_ref().is_none_or(within_bound)
                    })
                {
                    return Err(Error::Request);
                }
            }
            QuestionKind::SingleChoice(options) => {
                let mut seen = BTreeSet::new();
                if !(1..=MAX_LEVELS).contains(&options.len()) || options.iter().any(|option| {
                    !seen.insert(&option.value)
                        || matches!(&option.value, ChoiceValue::String(value) if !within_bound(value))
                        || !option.criterion.as_ref().is_none_or(within_bound)
                }) {
                    return Err(Error::Request);
                }
            }
            QuestionKind::OrdinalScore(levels) => {
                if !(1..=MAX_LEVELS).contains(&levels.len())
                    || levels.iter().any(|level| {
                        (level.label.is_none() && level.criterion.is_none())
                            || !level.label.as_ref().is_none_or(within_bound)
                            || !level.criterion.as_ref().is_none_or(within_bound)
                    })
                {
                    return Err(Error::Request);
                }
            }
        }
        Ok(Self {
            name,
            instructions,
            kind,
        })
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_ref().map(Text::as_str)
    }
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_ref().map(Text::as_str)
    }
    pub fn kind(&self) -> &QuestionKind {
        &self.kind
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum QuestionKind {
    Predicate { criteria: Option<BinaryCriteria> },
    SingleChoice(Vec<ChoiceOption>),
    OrdinalScore(Vec<ScoreLevel>),
}

#[derive(Clone, Eq, PartialEq)]
pub struct BinaryCriteria {
    pub when_true: Option<Text>,
    pub when_false: Option<Text>,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChoiceValue {
    String(Text),
    Bool(bool),
}

#[derive(Clone, Eq, PartialEq)]
pub struct ChoiceOption {
    pub value: ChoiceValue,
    pub criterion: Option<Text>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ScoreLevel {
    pub label: Option<Text>,
    pub criterion: Option<Text>,
}

pub struct Request {
    evidence: Evidence,
    questions: Vec<Question>,
}
impl Request {
    pub fn new(evidence: Evidence, questions: Vec<Question>) -> Result<Self, Error> {
        let text_bound = |t: &Text| t.as_str().len() <= MAX_TEXT_BYTES;
        let evidence_ok = match &evidence {
            Evidence::StateText(text) | Evidence::HistoryUserText(text) => text_bound(text),
            Evidence::StateJson(_) => true,
            Evidence::TextParts(parts) => {
                !parts.is_empty()
                    && parts.len() <= MAX_TEXT_PARTS
                    && parts.iter().all(text_bound)
                    && parts.iter().any(|part| !part.as_str().is_empty())
            }
            Evidence::HistoryUserMessages(messages) => {
                messages.len() <= MAX_TEXT_PARTS
                    && messages.iter().all(|m| match m {
                        UserMessage::Text(text) => text_bound(text),
                        UserMessage::Parts(parts) => {
                            parts.len() <= MAX_TEXT_PARTS
                                && parts.iter().all(|part| match part {
                                    UserPart::Text(text) => text_bound(text),
                                    UserPart::Json(_) => true,
                                })
                        }
                    })
            }
        };
        if questions.is_empty() || questions.len() > MAX_QUESTIONS || !evidence_ok {
            return Err(Error::Request);
        }
        Ok(Self {
            evidence,
            questions,
        })
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn questions(&self) -> &[Question] {
        &self.questions
    }
}
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecisionRequest")
            .field("question_count", &self.questions.len())
            .finish_non_exhaustive()
    }
}

/// Exact decimal in [0, 1]. Check the decimal order before any floating conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Probability(Number);
impl Probability {
    pub fn new(value: Number) -> Result<Self, Error> {
        let spelling = value.as_str();
        if spelling.len() > 128 {
            return Err(Error::Probability);
        }
        let (mantissa, exponent) = match spelling.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (
                mantissa,
                i64::from(exponent.parse::<i32>().map_err(|_| Error::Probability)?),
            ),
            None => (spelling, 0),
        };
        let magnitude = mantissa.strip_prefix('-').unwrap_or(mantissa);
        let whole = magnitude
            .split_once('.')
            .map_or(magnitude, |(whole, _)| whole);
        let mut digits = magnitude.bytes().filter(|&byte| byte != b'.').peekable();
        let mut leading_zeroes = 0;
        while digits.next_if_eq(&b'0').is_some() {
            leading_zeroes += 1;
        }
        if let Some(first) = digits.next() {
            let order = whole.len() as i64 - leading_zeroes as i64 + exponent;
            if mantissa.starts_with('-')
                || order > 1
                || (order == 1 && (first != b'1' || digits.any(|digit| digit != b'0')))
            {
                return Err(Error::Probability);
            }
        }
        Ok(Self(value))
    }
    pub fn as_number(&self) -> &Number {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Answered(Answer),
    Refused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Answer {
    Predicate {
        probability: Probability,
        confidence: Option<Confidence>,
    },
    SingleChoice {
        selected: usize,
        probabilities: Vec<Probability>,
        confidence: Confidence,
    },
    OrdinalScore {
        score: Number,
        probabilities: Vec<Probability>,
        confidence: Confidence,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Confidence {
    Unspecified(Probability),
    WinningMass(Probability),
    UniformAdjustedMaximum(Probability),
    CenteredOrdinalSpread(Probability),
}
impl Confidence {
    pub fn reported(&self) -> &Probability {
        match self {
            Self::Unspecified(p)
            | Self::WinningMass(p)
            | Self::UniformAdjustedMaximum(p)
            | Self::CenteredOrdinalSpread(p) => p,
        }
    }
}

/// Answers in source question order, validated against the request cardinality.
#[derive(Debug)]
pub struct AnswerSet(Vec<Outcome>);
impl AnswerSet {
    pub fn for_request(request: &Request, answers: Vec<Outcome>) -> Result<Self, Error> {
        if answers.len() != request.questions.len()
            || request
                .questions
                .iter()
                .zip(&answers)
                .any(|(question, outcome)| match (&question.kind, outcome) {
                    (_, Outcome::Refused)
                    | (
                        QuestionKind::Predicate { .. },
                        Outcome::Answered(Answer::Predicate { .. }),
                    ) => false,
                    (
                        QuestionKind::SingleChoice(options),
                        Outcome::Answered(Answer::SingleChoice {
                            selected,
                            probabilities,
                            ..
                        }),
                    ) => *selected >= options.len() || probabilities.len() != options.len(),
                    (
                        QuestionKind::OrdinalScore(levels),
                        Outcome::Answered(Answer::OrdinalScore { probabilities, .. }),
                    ) => probabilities.len() != levels.len(),
                    _ => true,
                })
        {
            return Err(Error::Association);
        }
        Ok(Self(answers))
    }
    pub fn outcomes(&self) -> &[Outcome] {
        &self.0
    }
}

#[derive(Debug)]
pub struct Evaluation {
    pub disposition: Disposition,
    pub usage: Option<Usage>,
}

#[derive(Debug)]
pub enum Disposition {
    Evaluated(AnswerSet),
    Blocked(PromptBlock),
}

pub struct PromptBlock {
    pub reason: Text,
    pub message: Option<Text>,
}
impl fmt::Debug for PromptBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromptBlock").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub input_by_modality: Option<Vec<ModalityTokenCount>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_probability_does_not_round_out_of_range_decimals_into_bounds() {
        for (number, valid) in [
            ("0", true),
            ("-0.0", true),
            ("0.00001", true),
            ("1.0000000000000000000", true),
            ("1.0000000000000000001", false),
            ("-1e-999", false),
            ("100e-2", true),
            ("100.001e-2", false),
            ("0.01e2", true),
            ("0.011e2", false),
            ("2", false),
        ] {
            let value = Probability::new(number.parse().unwrap());
            assert_eq!(value.is_ok(), valid, "{number}");
        }
    }
}
