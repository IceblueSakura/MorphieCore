//! OpenAI representability: directional projection over shared immutable-IR constraints.
pub(in crate::openai) mod carriers;
mod checks;
pub use checks::chat_reasoning_carrier;
pub mod events;
pub mod message_groups;
pub mod native_response;
pub mod projection;
pub mod readable_reasoning;
mod request;
mod response;
pub mod strictness;

pub use crate::conversion::generation::RepresentationError;
pub use crate::openai::TargetContract;
pub use request::lower_request;
pub use request::render_request;
pub use response::lower_response;
pub use response::validate_response;
