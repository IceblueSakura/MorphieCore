//! OpenAI representability: directional projection over shared immutable-IR constraints.
mod checks;
pub mod events;
mod message_groups;
pub(crate) mod native_messages;
pub mod projection;
mod request;
mod response;

pub use crate::lowering::generation::{ReportedFactPolicy, RepresentationError};
pub use crate::protocol::openai::TargetCaps;
pub use request::{check, lower_request};
pub use response::{lower_response, require_reported_facts};
