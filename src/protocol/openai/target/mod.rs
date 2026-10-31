//! OpenAI target capabilities; independent of endpoint execution and credentials.
mod request;
use crate::{
    lowering::disposition::Dispositions, protocol::conversion::generation::ReportedFactPolicy,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContract {
    pub adaptation: crate::protocol::openai::dialect::adaptation::Adaptation,
    pub reasoning_history: Option<crate::protocol::reasoning::ReasoningHistory>,
    /// Existing output admission on unassessed bindings, not a history fact.
    pub chat_thinking_output: bool,
    /// Permit the named projection of foreign readable reasoning into assistant text.
    pub reasoning_as_text: bool,
    pub replay_origin: Option<crate::semantic::value::ReplayOrigin>,
    /// Caller-supplied clock for checking a reported remote-resource expiry.
    pub resource_time: Option<u64>,
    /// Delivery policy for reported facts; `Faithful` unless a consumer
    /// explicitly demands the strict complete form.
    pub reported_facts: ReportedFactPolicy,
    pub dispositions: Dispositions,
    pub model_limits: crate::protocol::model_limits::ModelLimits,
    pub images: crate::protocol::image_constraints::ImageConstraints,
    pub files: crate::protocol::file_constraints::FileConstraints,
    pub cache: crate::protocol::cache::CacheTargetPolicy,
}
impl TargetContract {
    pub fn full() -> Self {
        Self {
            adaptation: Default::default(),
            reasoning_history: None,
            chat_thinking_output: false,
            reasoning_as_text: false,
            replay_origin: None,
            resource_time: None,
            reported_facts: ReportedFactPolicy::Faithful,
            dispositions: Dispositions::full(),
            model_limits: Default::default(),
            images: crate::protocol::image_constraints::ImageConstraints::all(),
            files: crate::protocol::file_constraints::FileConstraints::all(),
            cache: crate::protocol::cache::CacheTargetPolicy::all(),
        }
    }
}
