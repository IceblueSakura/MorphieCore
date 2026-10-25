//! Generation settings shared by requests and reported response echoes.
use crate::semantic::task::generation::*;
use crate::semantic::value::{Presence, Text};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Truncation {
    Auto,
    Disabled,
}
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct GenerationControls {
    pub max_output_tokens: Option<u64>,
    temperature_bits: Option<u64>,
    top_p_bits: Option<u64>,
    pub top_k: Option<u32>,
    pub stop_sequences: Vec<Text>,
    presence_penalty_bits: Option<u64>,
    frequency_penalty_bits: Option<u64>,
    /// Best-effort sampling reproducibility, not a determinism guarantee.
    pub seed: Option<i64>,
    pub top_logprobs: Option<u8>,
    pub logprobs: Presence<bool>,
    pub truncation: Option<Truncation>,
}
impl GenerationControls {
    pub fn without_sampling(mut self) -> Self {
        self.temperature_bits = None;
        self.top_p_bits = None;
        self
    }
    pub fn with_temperature(mut self, v: f64) -> Result<Self, GenerationError> {
        if !v.is_finite() {
            return Err(GenerationError::NonFiniteTemperature);
        }
        self.temperature_bits = Some(v.to_bits());
        Ok(self)
    }
    pub fn temperature(&self) -> Option<f64> {
        self.temperature_bits.map(f64::from_bits)
    }
    pub fn with_top_p(mut self, v: f64) -> Result<Self, GenerationError> {
        if !v.is_finite() || !(0.0..=1.0).contains(&v) {
            return Err(GenerationError::InvalidControl);
        }
        self.top_p_bits = Some(v.to_bits());
        Ok(self)
    }
    pub fn top_p(&self) -> Option<f64> {
        self.top_p_bits.map(f64::from_bits)
    }
    pub fn with_presence_penalty(mut self, value: f64) -> Result<Self, GenerationError> {
        if !value.is_finite() {
            return Err(GenerationError::InvalidControl);
        }
        self.presence_penalty_bits = Some(value.to_bits());
        Ok(self)
    }
    pub fn presence_penalty(&self) -> Option<f64> {
        self.presence_penalty_bits.map(f64::from_bits)
    }
    pub fn with_frequency_penalty(mut self, value: f64) -> Result<Self, GenerationError> {
        if !value.is_finite() {
            return Err(GenerationError::InvalidControl);
        }
        self.frequency_penalty_bits = Some(value.to_bits());
        Ok(self)
    }
    pub fn frequency_penalty(&self) -> Option<f64> {
        self.frequency_penalty_bits.map(f64::from_bits)
    }
    pub fn has_extended_controls(&self) -> bool {
        self.top_k.is_some()
            || !self.stop_sequences.is_empty()
            || self.presence_penalty().is_some()
            || self.frequency_penalty().is_some()
            || self.seed.is_some()
    }
}
/// One root instruction block. Its index is local to the current request's root,
/// not an ordered history identity or a replay owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstructionText {
    pub text: Text,
    pub cache: Option<PrefixCacheBoundary>,
}
/// Current-request application instructions; multipart text and cache boundaries
/// have one owner rather than a second copy in ordered history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootInstructions {
    pub parts: Vec<InstructionText>,
}
impl From<Text> for RootInstructions {
    fn from(text: Text) -> Self {
        Self {
            parts: vec![InstructionText { text, cache: None }],
        }
    }
}
impl RootInstructions {
    /// Exact string carrier, without joining parts or discarding cache intent.
    pub fn plain_text(&self) -> Option<&Text> {
        match self.parts.as_slice() {
            [InstructionText { text, cache: None }] => Some(text),
            _ => None,
        }
    }
    pub fn text_bytes(&self) -> usize {
        self.parts.iter().map(|p| p.text.as_str().len()).sum()
    }
    fn validate(&self) -> Result<usize, GenerationError> {
        if self.parts.is_empty() || self.parts.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut bytes = 0usize;
        for part in &self.parts {
            if part.text.as_str().len() > MAX_TEXT_BYTES {
                return Err(GenerationError::Limit);
            }
            bytes = bytes
                .checked_add(part.text.as_str().len() + 32)
                .filter(|n| *n <= MAX_TOTAL_BYTES)
                .ok_or(GenerationError::Limit)?;
        }
        Ok(bytes)
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationSettings {
    /// Request-wide plain-text instructions. They precede every ordered item,
    /// including ordered instruction items; history composition does not inherit them.
    pub instructions: Presence<RootInstructions>,
    pub controls: GenerationControls,
    /// Ordered predicted output text: an optimization hint, never an output constraint.
    pub prediction: Option<Vec<Text>>,
    /// Moving prefix boundary. Absence leaves the target's caching policy unchanged.
    pub automatic_prefix_cache: Option<PrefixCacheTtl>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub text: TextOptions,
    pub reasoning: ReasoningRequest,
    pub effort: Presence<EffortControl>,
    pub output_modalities: Presence<Vec<crate::semantic::task::generation::OutputModality>>,
    pub audio: Presence<crate::semantic::task::generation::AudioOutputOptions>,
    pub safety: Vec<SafetySetting>,
}
impl GenerationSettings {
    pub fn output(&self) -> &OutputConstraint {
        self.text.format.value().unwrap_or(&OutputConstraint::Text)
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.controls.max_output_tokens == Some(0)
            || self.controls.top_k == Some(0)
            || self.controls.top_logprobs.is_some_and(|n| n > 20)
            || self
                .controls
                .temperature()
                .is_some_and(|v| !(0.0..=2.0).contains(&v))
        {
            return Err(GenerationError::InvalidControl);
        }
        if self.controls.stop_sequences.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut stop_bytes = 0usize;
        for stop in &self.controls.stop_sequences {
            if stop.as_str().is_empty() || stop.as_str().len() > MAX_TEXT_BYTES {
                return Err(GenerationError::InvalidControl);
            }
            stop_bytes = stop_bytes
                .checked_add(stop.as_str().len())
                .ok_or(GenerationError::Limit)?;
            if stop_bytes > MAX_TOTAL_BYTES {
                return Err(GenerationError::Limit);
            }
        }
        if self
            .reasoning
            .budget_tokens
            .zip(self.controls.max_output_tokens)
            .is_some_and(|(budget, ceiling)| budget > ceiling)
        {
            return Err(GenerationError::InvalidControl);
        }
        // Absent/null containers cannot hide explicit values or null children.
        if self.text.presence.value().is_none()
            && (!self.text.format.is_absent() || !self.text.verbosity.is_absent())
        {
            return Err(GenerationError::InvalidControl);
        }
        self.reasoning.validate()?;
        if (self.reasoning.activation.value() == Some(&ReasoningActivation::Disabled)
            || self.reasoning.presence == ReasoningPresence::Null)
            && self
                .effort
                .value()
                .is_some_and(|e| e.scope == EffortScope::Reasoning)
        {
            return Err(GenerationError::InvalidControl);
        }
        if let Some(modalities) = self.output_modalities.value()
            && (modalities.is_empty()
                || modalities.len() > 2
                || (modalities.len() == 2 && modalities[0] == modalities[1]))
        {
            return Err(GenerationError::InvalidControl);
        }
        let audio = self
            .output_modalities
            .value()
            .is_some_and(|v| v.contains(&crate::semantic::task::generation::OutputModality::Audio));
        if audio != self.audio.value().is_some() {
            return Err(GenerationError::InvalidControl);
        }
        if let Some(options) = self.audio.value() {
            options.validate()?;
        }
        let instructions = self
            .instructions
            .value()
            .map_or(Ok(0), RootInstructions::validate)?;
        let mut prediction_bytes = 0usize;
        if let Some(parts) = &self.prediction {
            if parts.is_empty() || parts.len() > MAX_ITEMS {
                return Err(GenerationError::Limit);
            }
            for part in parts {
                if part.as_str().len() > MAX_TEXT_BYTES {
                    return Err(GenerationError::Limit);
                }
                prediction_bytes = prediction_bytes
                    .checked_add(part.as_str().len() + 32)
                    .filter(|n| *n <= MAX_TOTAL_BYTES)
                    .ok_or(GenerationError::Limit)?;
            }
        }
        let bytes = instructions
            + prediction_bytes
            + stop_bytes
            + crate::semantic::task::generation::safety::settings_bytes(&self.safety)?
            + usize::from(self.automatic_prefix_cache.is_some()) * 32
            + self
                .audio
                .value()
                .map_or(0, |v| v.voice.as_str().len() + v.format.label().len())
            + self
                .output_modalities
                .value()
                .map_or(0, |v| v.iter().map(|m| m.label().len()).sum::<usize>())
            + crate::semantic::task::generation::validate::tools(
                self.tools.as_deref().unwrap_or_default(),
                self.tool_choice.as_ref(),
            )?
            + crate::semantic::task::generation::validate::output(self.output())?;
        if bytes > MAX_TOTAL_BYTES {
            return Err(GenerationError::Limit);
        }
        Ok(bytes)
    }
}
