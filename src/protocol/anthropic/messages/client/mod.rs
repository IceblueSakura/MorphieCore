//! Downstream Messages client boundary, distinct from native wire/IR intake.
mod projection;
mod request;
mod response;
pub mod sse;
pub(crate) use sse::schedule::Schedule;

pub(crate) use projection::{
    GoogleTerminalPolicy, completed_without_stops, metadata_losses, project_metadata,
    report_losses, usage_loss, validate_tool_calls,
};
pub use request::decode_client_request;
pub use response::encode_client_response;
