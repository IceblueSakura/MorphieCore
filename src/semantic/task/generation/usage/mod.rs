//! The final token report of one operation. Missing counts and undeclared
//! relationships stay unknown.
pub(in crate::semantic::task::generation) mod reports;
pub(in crate::semantic::task::generation) mod views;
use crate::semantic::task::generation::GenerationError;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputTokenRelation {
    IncludesReasoning,
    ExcludesReasoning,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputTokenRelation {
    IncludesCache,
    /// Input, cache reads and cache writes are reported disjoint categories.
    ExcludesCacheReadAndWrite,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TotalTokenRelation {
    InputAndOutput,
    InputOutputAndReasoning,
    InputCacheAndOutput,
    Unreported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub output_relation: OutputTokenRelation,
    pub input_relation: InputTokenRelation,
    pub total_relation: TotalTokenRelation,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub input_cache_write_tokens: Option<u64>,
    pub input_text_tokens: Option<u64>,
    pub input_image_tokens: Option<u64>,
    pub input_audio_tokens: Option<u64>,
    pub output_audio_tokens: Option<u64>,
    pub output_text_tokens: Option<u64>,
    pub accepted_prediction_tokens: Option<u64>,
    pub rejected_prediction_tokens: Option<u64>,
}
impl Usage {
    /// Explicit inclusive accounting, not a default for other sources.
    pub fn operation(input: u64, output: u64, total: u64) -> Self {
        Self {
            output_relation: OutputTokenRelation::IncludesReasoning,
            input_relation: InputTokenRelation::IncludesCache,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(total),
            reasoning_tokens: None,
            cached_input_tokens: None,
            input_cache_write_tokens: None,
            input_text_tokens: None,
            input_image_tokens: None,
            input_audio_tokens: None,
            output_audio_tokens: None,
            output_text_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }
    }
    pub fn validate(self) -> Result<(), GenerationError> {
        let subset = |detail: Option<u64>, total: Option<u64>| !matches!((detail,total),(Some(a),Some(b)) if a>b);
        if self.counters().iter().all(Option::is_none)
            || self.output_relation == OutputTokenRelation::IncludesReasoning
                && !subset(self.reasoning_tokens, self.output_tokens)
            || self.input_relation == InputTokenRelation::IncludesCache
                && (!subset(self.cached_input_tokens, self.input_tokens)
                    || !subset(self.input_cache_write_tokens, self.input_tokens))
            || !subset(self.input_text_tokens, self.input_tokens)
            || !subset(self.input_image_tokens, self.input_tokens)
            || !subset(self.input_audio_tokens, self.input_tokens)
            || !subset(self.output_audio_tokens, self.output_tokens)
            || !subset(self.output_text_tokens, self.output_tokens)
            || !subset(self.accepted_prediction_tokens, self.output_tokens)
            || !subset(self.rejected_prediction_tokens, self.output_tokens)
        {
            return Err(GenerationError::InvalidResponse);
        }
        if let (Some(a), Some(b)) = (
            self.accepted_prediction_tokens,
            self.rejected_prediction_tokens,
        ) {
            let sum = a.checked_add(b).ok_or(GenerationError::InvalidResponse)?;
            if self.output_tokens.is_some_and(|total| sum > total) {
                return Err(GenerationError::InvalidResponse);
            }
        }
        if self.total_relation == TotalTokenRelation::InputOutputAndReasoning
            && self.output_relation != OutputTokenRelation::ExcludesReasoning
            || self.total_relation == TotalTokenRelation::InputCacheAndOutput
                && (self.input_relation != InputTokenRelation::ExcludesCacheReadAndWrite
                    || self.output_relation != OutputTokenRelation::IncludesReasoning)
        {
            return Err(GenerationError::InvalidResponse);
        }
        let operands = match self.total_relation {
            TotalTokenRelation::InputAndOutput => Some(vec![self.input_tokens, self.output_tokens]),
            TotalTokenRelation::InputOutputAndReasoning => Some(vec![
                self.input_tokens,
                self.output_tokens,
                self.reasoning_tokens,
            ]),
            TotalTokenRelation::InputCacheAndOutput => Some(vec![
                self.input_tokens,
                self.cached_input_tokens,
                self.input_cache_write_tokens,
                self.output_tokens,
            ]),
            TotalTokenRelation::Unreported => None,
        };
        if let (Some(operands), Some(total)) = (operands, self.total_tokens) {
            // Known operands give a lower bound, not a replacement for missing reports.
            let known = operands
                .iter()
                .flatten()
                .try_fold(0u64, |sum, value| sum.checked_add(*value))
                .ok_or(GenerationError::InvalidResponse)?;
            if known > total || operands.iter().all(Option::is_some) && known != total {
                return Err(GenerationError::InvalidResponse);
            }
        }
        Ok(())
    }
    pub(crate) fn counters(self) -> [Option<u64>; 13] {
        [
            self.input_tokens,
            self.output_tokens,
            self.total_tokens,
            self.reasoning_tokens,
            self.cached_input_tokens,
            self.input_cache_write_tokens,
            self.input_text_tokens,
            self.input_image_tokens,
            self.input_audio_tokens,
            self.output_audio_tokens,
            self.output_text_tokens,
            self.accepted_prediction_tokens,
            self.rejected_prediction_tokens,
        ]
    }
}
