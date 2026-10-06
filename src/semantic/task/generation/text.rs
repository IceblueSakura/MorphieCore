//! Readable text and dependent annotations/probabilities have one immutable owner.
use super::{GenerationError, MAX_ITEMS, MAX_TEXT_BYTES};
use crate::semantic::value::{Presence, Text};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Annotation {
    UrlCitation {
        start_index: usize,
        end_index: usize,
        title: String,
        url: String,
    },
    FileCitation {
        file_id: String,
        filename: String,
        index: usize,
    },
    ContainerFileCitation {
        container_id: String,
        file_id: String,
        filename: String,
        start_index: usize,
        end_index: usize,
    },
    FilePath {
        file_id: String,
        index: usize,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopLogprob {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprob: Option<Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logprob {
    pub token: String,
    pub logprob: Number,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<TopLogprob>>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextContent {
    text: Text,
    annotations: Vec<Annotation>,
    logprobs: Presence<Vec<Logprob>>,
}
impl From<Text> for TextContent {
    fn from(text: Text) -> Self {
        Self {
            text,
            annotations: vec![],
            logprobs: Presence::Absent,
        }
    }
}
impl TextContent {
    pub fn new(
        text: Text,
        annotations: Vec<Annotation>,
        logprobs: Presence<Vec<Logprob>>,
    ) -> Result<Self, GenerationError> {
        let value = Self {
            text,
            annotations,
            logprobs,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }
    pub fn logprobs(&self) -> &Presence<Vec<Logprob>> {
        &self.logprobs
    }
    pub fn with_logprobs(
        mut self,
        logprobs: Presence<Vec<Logprob>>,
    ) -> Result<Self, GenerationError> {
        self.logprobs = logprobs;
        self.validate()?;
        Ok(self)
    }
    /// A text replacement cannot inherit offsets or model probabilities from the old text.
    pub fn replace_text(self, text: Text) -> Self {
        if self.text == text { self } else { text.into() }
    }
    pub fn is_plain(&self) -> bool {
        self.annotations.is_empty() && self.logprobs.is_absent()
    }
    pub fn bytes(&self) -> usize {
        if self.is_plain() {
            return self.text.as_str().len();
        }
        self.text
            .as_str()
            .len()
            .saturating_add(
                crate::semantic::value::json_size(&self.annotations, MAX_TEXT_BYTES)
                    .unwrap_or(usize::MAX),
            )
            .saturating_add(self.logprobs.value().map_or(0, |v| {
                crate::semantic::value::json_size(v, MAX_TEXT_BYTES).unwrap_or(usize::MAX)
            }))
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.text.as_str().len() > MAX_TEXT_BYTES
            || self.annotations.len() > MAX_ITEMS
            || self.logprobs.value().is_some_and(|v| v.len() > MAX_ITEMS)
            || self.bytes() > MAX_TEXT_BYTES
        {
            return Err(GenerationError::Limit);
        }
        let chars = self.text.as_str().chars().count();
        for a in &self.annotations {
            let range = match a {
                Annotation::UrlCitation {
                    start_index,
                    end_index,
                    ..
                }
                | Annotation::ContainerFileCitation {
                    start_index,
                    end_index,
                    ..
                } => Some((*start_index, *end_index)),
                _ => None,
            };
            if range.is_some_and(|(start, end)| start > end || end > chars) {
                return Err(GenerationError::InvalidResponse);
            }
        }
        if let Presence::Value(probs) = &self.logprobs {
            validate_logprobs(probs)?;
        }
        Ok(())
    }
}
/// Refusal probabilities belong to the refusal value, never to ordinary message text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefusalContent {
    text: Text,
    logprobs: Presence<Vec<Logprob>>,
}
impl From<Text> for RefusalContent {
    fn from(text: Text) -> Self {
        Self {
            text,
            logprobs: Presence::Absent,
        }
    }
}
impl RefusalContent {
    pub fn new(text: Text, logprobs: Presence<Vec<Logprob>>) -> Result<Self, GenerationError> {
        let value = Self { text, logprobs };
        value.validate()?;
        Ok(value)
    }
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }
    pub fn logprobs(&self) -> &Presence<Vec<Logprob>> {
        &self.logprobs
    }
    pub fn with_logprobs(
        mut self,
        logprobs: Presence<Vec<Logprob>>,
    ) -> Result<Self, GenerationError> {
        self.logprobs = logprobs;
        self.validate()?;
        Ok(self)
    }
    pub fn replace_text(self, text: Text) -> Self {
        if self.text == text { self } else { text.into() }
    }
    pub fn bytes(&self) -> usize {
        self.text
            .as_str()
            .len()
            .saturating_add(self.logprobs.value().map_or(0, |p| {
                crate::semantic::value::json_size(p, MAX_TEXT_BYTES).unwrap_or(usize::MAX)
            }))
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        if self.bytes() > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        if let Some(probs) = self.logprobs.value() {
            validate_logprobs(probs)?;
        }
        Ok(())
    }
}
/// A final snapshot may fill missing byte/top-token details, never replace observed probabilities.
pub fn compatible_logprobs(known: &[Logprob], final_value: &[Logprob]) -> bool {
    if known.is_empty() {
        return true;
    }
    known.len() == final_value.len()
        && known.iter().zip(final_value).all(|(a, b)| {
            let top = a.top_logprobs.as_deref().unwrap_or_default();
            let other = b.top_logprobs.as_deref().unwrap_or_default();
            a.token == b.token
                && a.logprob == b.logprob
                && a.bytes.as_ref().is_none_or(|v| Some(v) == b.bytes.as_ref())
                && (top.is_empty()
                    || top.len() == other.len()
                        && top.iter().zip(other).all(|(a, b)| {
                            a.token.as_ref().is_none_or(|v| Some(v) == b.token.as_ref())
                                && a.logprob
                                    .as_ref()
                                    .is_none_or(|v| Some(v) == b.logprob.as_ref())
                                && a.bytes.as_ref().is_none_or(|v| Some(v) == b.bytes.as_ref())
                        }))
        })
}
pub fn validate_logprobs(probs: &[Logprob]) -> Result<(), GenerationError> {
    fn probability(n: &Number) -> bool {
        n.as_f64().is_some_and(|v| v.is_finite() && v <= 0.0)
    }
    if probs.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    for p in probs {
        if p.token.len() > MAX_TEXT_BYTES
            || !probability(&p.logprob)
            || p.bytes.as_ref().is_some_and(|v| v.len() > MAX_TEXT_BYTES)
            || p.top_logprobs.as_ref().is_some_and(|v| v.len() > 20)
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let Some(top) = &p.top_logprobs {
            for t in top {
                if t.token.as_ref().is_some_and(|v| v.len() > MAX_TEXT_BYTES)
                    || t.logprob.as_ref().is_some_and(|n| !probability(n))
                    || t.bytes.as_ref().is_some_and(|v| v.len() > MAX_TEXT_BYTES)
                {
                    return Err(GenerationError::InvalidResponse);
                }
            }
        }
    }
    Ok(())
}
