//! OpenAI representability: directional projection over shared immutable-IR constraints.
pub(in crate::protocol::openai) mod carriers;
mod checks;
pub mod events;
mod message_groups;
pub(crate) mod native_response;
pub mod projection;
mod request;
mod response;

pub use crate::lowering::generation::{ReportedFactPolicy, RepresentationError};
pub use crate::protocol::openai::TargetCaps;
pub use request::{check, lower_request};
pub use response::{lower_response, require_reported_facts};
