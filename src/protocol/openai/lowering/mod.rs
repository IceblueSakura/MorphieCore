//! OpenAI representability: directional projection over shared immutable-IR constraints.
pub(in crate::protocol::openai) mod carriers;
mod checks;
pub mod events;
mod message_groups;
pub(crate) mod native_response;
pub mod projection;
pub(crate) mod readable_reasoning;
mod request;
mod response;
pub(crate) mod strictness;

pub use crate::protocol::conversion::generation::{ReportedFactPolicy, RepresentationError};
pub use crate::protocol::openai::TargetContract;
pub use request::{check, lower_request};
pub use response::{lower_response, require_reported_facts};
