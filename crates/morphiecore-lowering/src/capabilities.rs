//! Protocol-neutral constraints declared by trusted Generation targets.

use crate::disposition::Dispositions;
use morphiecore_semantic::task::generation::{Feature, GenerationRequest, ReasoningActivation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SamplingConstraints {
    #[default]
    Unrestricted,
    /// Sampling and probability controls require explicitly disabled reasoning.
    RequiresReasoningOff,
}

impl SamplingConstraints {
    pub fn apply(self, source: &GenerationRequest, table: &mut Dispositions) {
        if self == Self::RequiresReasoningOff
            && source.resolved_configuration().active.activation
                != Some(ReasoningActivation::Disabled)
        {
            for feature in [Feature::Temperature, Feature::TopP, Feature::Logprobs] {
                table.set_native(feature, false);
            }
        }
    }
}
