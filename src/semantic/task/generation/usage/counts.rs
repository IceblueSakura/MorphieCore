//! Aggregate observations. Modality observations belong only to the scoped breakdown.
use super::{InputTokenRelation, OutputTokenRelation, TotalTokenRelation, Usage};
use crate::semantic::task::generation::GenerationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageCounts {
    pub output_relation: OutputTokenRelation,
    pub input_relation: InputTokenRelation,
    pub total_relation: TotalTokenRelation,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub input_cache_write_tokens: Option<u64>,
    pub accepted_prediction_tokens: Option<u64>,
    pub rejected_prediction_tokens: Option<u64>,
}
impl UsageCounts {
    /// Aggregate-only view, never a claim that absent modalities are zero.
    pub fn view(self) -> Usage {
        Usage {
            output_relation: self.output_relation,
            input_relation: self.input_relation,
            total_relation: self.total_relation,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            reasoning_tokens: self.reasoning_tokens,
            cached_input_tokens: self.cached_input_tokens,
            input_cache_write_tokens: self.input_cache_write_tokens,
            accepted_prediction_tokens: self.accepted_prediction_tokens,
            rejected_prediction_tokens: self.rejected_prediction_tokens,
            input_text_tokens: None,
            input_image_tokens: None,
            input_audio_tokens: None,
            output_text_tokens: None,
            output_audio_tokens: None,
        }
    }
    pub fn validate(self) -> Result<(), GenerationError> {
        self.view().validate()
    }
    pub(crate) fn counters(self) -> [Option<u64>; 13] {
        self.view().counters()
    }
}
impl Usage {
    /// Select aggregate values; callers retaining observations must also retain modalities.
    pub fn aggregate_counts(self) -> UsageCounts {
        UsageCounts {
            output_relation: self.output_relation,
            input_relation: self.input_relation,
            total_relation: self.total_relation,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            reasoning_tokens: self.reasoning_tokens,
            cached_input_tokens: self.cached_input_tokens,
            input_cache_write_tokens: self.input_cache_write_tokens,
            accepted_prediction_tokens: self.accepted_prediction_tokens,
            rejected_prediction_tokens: self.rejected_prediction_tokens,
        }
    }
}
