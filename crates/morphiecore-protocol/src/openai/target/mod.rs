//! OpenAI target capabilities; independent of endpoint execution and credentials.
mod request;
use crate::lowering::{
    capabilities::{ModelRequestPolicy, SamplingConstraints, SamplingValues},
    disposition::Dispositions,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContract {
    pub adaptation: crate::openai::dialect::adaptation::Adaptation,
    pub reasoning_history: Option<crate::reasoning::ReasoningHistory>,
    /// Permit the named projection of foreign readable reasoning into assistant text.
    pub reasoning_as_text: bool,
    /// Explicit origin for unbound callers; otherwise inherit the bound codec scope.
    pub replay_origin: Option<crate::semantic::value::ReplayOrigin>,
    /// Caller-supplied clock for checking a reported remote-resource expiry.
    pub resource_time: Option<u64>,
    pub dispositions: Dispositions,
    pub sampling: SamplingConstraints,
    pub model_policy: ModelRequestPolicy,
    pub model_limits: crate::model_limits::ModelLimits,
    pub images: crate::image_constraints::ImageConstraints,
    pub files: crate::file_constraints::FileConstraints,
    pub cache: crate::cache::CacheTargetPolicy,
}
impl TargetContract {
    /// Resolve native value restrictions after deployment selects the final
    /// upstream spelling. A catalog template can be retargeted by its operator.
    pub(in crate::openai) fn with_model_policy(
        mut self,
        model: &str,
        api: super::GenerationApi,
    ) -> Self {
        if self.adaptation.rules.hooks.bailian_requests {
            self.model_policy = super::dialect::bailian::request_policy(model, api);
            if self.sampling.values == SamplingValues::default() {
                self.sampling.values = super::dialect::bailian::sampling_values(model);
            }
        }
        self
    }

    pub fn replay_origin(&self) -> Option<&crate::semantic::value::ReplayOrigin> {
        self.replay_origin
            .as_ref()
            .or(self.adaptation.scope.as_ref())
    }
    pub fn reasoning_origin(&self) -> Option<&crate::semantic::value::ReplayOrigin> {
        self.adaptation.reasoning_origin(self.replay_origin())
    }
    pub(crate) fn has_numeric_budget(&self) -> bool {
        self.adaptation.rules.reasoning.thinking_controls
            // A resolved effort mapping may describe the canonical binding,
            // independently of the final upstream spelling used by the codec.
            && !self.adaptation.effort_mapping.is_some_and(|mapping| {
                super::dialect::bailian::rejects_budget(mapping.canonical_model)
            })
    }
    pub fn full() -> Self {
        Self {
            adaptation: Default::default(),
            reasoning_history: None,
            reasoning_as_text: false,
            replay_origin: None,
            resource_time: None,
            dispositions: Dispositions::full(),
            sampling: SamplingConstraints::default(),
            model_policy: ModelRequestPolicy::default(),
            model_limits: Default::default(),
            images: crate::image_constraints::ImageConstraints::all(),
            files: crate::file_constraints::FileConstraints::all(),
            cache: crate::cache::CacheTargetPolicy::all(),
        }
    }
}
