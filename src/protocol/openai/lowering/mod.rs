//! OpenAI representability: directional projection over shared immutable-IR constraints.
pub(in crate::protocol::openai) mod carriers;
mod checks;
pub(crate) use checks::chat_reasoning_carrier;
pub mod events;
pub(crate) mod message_groups;
pub(crate) mod native_response;
pub mod projection;
pub(crate) mod readable_reasoning;
mod request;
mod response;
pub(crate) mod strictness;

pub use crate::protocol::conversion::generation::RepresentationError;
pub use crate::protocol::openai::TargetContract;
pub use request::lower_request;
pub(crate) use request::render_request;
pub use response::lower_response;
pub(crate) use response::validate_response;
