//! OpenAI target capabilities; independent of endpoint execution and credentials.
use crate::{
    lowering::generation::ReportedFactPolicy,
    semantic::task::generation::GenerationSemanticContract,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCaps {
    pub adaptation: crate::protocol::openai::adaptation::Adaptation,
    pub replay_origin: Option<crate::semantic::value::ReplayOrigin>,
    /// Caller-supplied clock for checking a reported remote-resource expiry.
    pub resource_time: Option<u64>,
    /// Delivery policy for reported facts; `Faithful` unless a consumer
    /// explicitly demands the strict complete form.
    pub reported_facts: ReportedFactPolicy,
    pub semantics: GenerationSemanticContract,
    pub images: crate::protocol::image_constraints::ImageConstraints,
    pub files: crate::protocol::file_constraints::FileConstraints,
    pub cache: crate::protocol::cache::CacheProjection,
    /// Identity/safety acceptance is independent of optional cache hints.
    pub identity_hints: bool,
    /// Target admits metadata/service-tier/tool-budget context fields.
    pub standard_context: bool,
}
impl TargetCaps {
    pub fn full() -> Self {
        Self {
            adaptation: Default::default(),
            replay_origin: None,
            resource_time: None,
            reported_facts: ReportedFactPolicy::Faithful,
            semantics: GenerationSemanticContract::full(),
            images: crate::protocol::image_constraints::ImageConstraints::all(),
            files: crate::protocol::file_constraints::FileConstraints::all(),
            cache: crate::protocol::cache::CacheProjection::all(),
            identity_hints: true,
            standard_context: true,
        }
    }
}
