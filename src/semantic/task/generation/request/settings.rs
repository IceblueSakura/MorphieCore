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
    pub top_logprobs: Option<u8>,
    pub logprobs: Presence<bool>,
    pub truncation: Option<Truncation>,
}
impl GenerationControls {
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
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenerationSettings {
    pub instructions: Presence<Text>,
    pub controls: GenerationControls,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<ToolChoice>,
    pub parallel_tool_calls: Option<bool>,
    pub text: TextOptions,
    pub reasoning: ReasoningRequest,
    pub output_modalities: Presence<Vec<crate::semantic::task::generation::OutputModality>>,
    pub audio: Presence<crate::semantic::task::generation::AudioOutputOptions>,
}
impl GenerationSettings {
    pub fn output(&self) -> &OutputConstraint {
        self.text.format.value().unwrap_or(&OutputConstraint::Text)
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.controls.max_output_tokens == Some(0)
            || self.controls.top_logprobs.is_some_and(|n| n > 20)
            || self
                .controls
                .temperature()
                .is_some_and(|v| !(0.0..=2.0).contains(&v))
        {
            return Err(GenerationError::InvalidControl);
        }
        // An absent container cannot hide explicit values or null children.
        if !self.text.presence
            && (!self.text.format.is_absent() || !self.text.verbosity.is_absent())
        {
            return Err(GenerationError::InvalidControl);
        }
        self.reasoning.validate()?;
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
        let instructions = self.instructions.value().map_or(0, |t| t.as_str().len());
        if instructions > MAX_TEXT_BYTES {
            return Err(GenerationError::Limit);
        }
        let bytes = instructions
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
