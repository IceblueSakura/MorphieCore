//! Protocol-neutral image request envelope for Gateway and typed consumers.
use crate::semantic::{
    context::{ClientIdentityHints, DeliveryIntent},
    task::image_generation::ImageGenerationRequest,
};
#[derive(Clone, Eq, PartialEq)]
pub struct Request {
    pub model: String,
    pub task: ImageGenerationRequest,
    pub delivery: DeliveryIntent,
    pub identity: ClientIdentityHints,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageRequest")
            .field("model", &self.model)
            .field("task", &self.task)
            .field("delivery", &self.delivery)
            .field("identity", &"[redacted]")
            .finish()
    }
}
pub mod projection;
impl Request {
    pub fn new(model: impl Into<String>, task: ImageGenerationRequest) -> Self {
        Self {
            model: model.into(),
            task,
            delivery: DeliveryIntent::default(),
            identity: ClientIdentityHints::default(),
        }
    }
}
