//! OpenAI target capabilities; independent of endpoint execution and credentials.
mod request;
use crate::lowering::{capabilities::SamplingConstraints, disposition::Dispositions};
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
    pub model_limits: crate::model_limits::ModelLimits,
    pub images: crate::image_constraints::ImageConstraints,
    pub files: crate::file_constraints::FileConstraints,
    pub cache: crate::cache::CacheTargetPolicy,
}
impl TargetContract {
    pub fn replay_origin(&self) -> Option<&crate::semantic::value::ReplayOrigin> {
        self.replay_origin
            .as_ref()
            .or(self.adaptation.scope.as_ref())
    }
    pub fn reasoning_origin(&self) -> Option<&crate::semantic::value::ReplayOrigin> {
        self.adaptation.reasoning_origin(self.replay_origin())
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
            model_limits: Default::default(),
            images: crate::image_constraints::ImageConstraints::all(),
            files: crate::file_constraints::FileConstraints::all(),
            cache: crate::cache::CacheTargetPolicy::all(),
        }
    }
}
