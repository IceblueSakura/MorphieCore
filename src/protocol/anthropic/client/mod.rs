//! Downstream Messages client boundary, distinct from native wire/IR intake.
mod request;
mod response;

pub use request::decode_client_request;
pub use response::encode_client_response;
pub(super) use response::redacted;
pub(crate) use response::{
    content_losses, encode_client_response_with, metadata_losses, project_snapshot, refusal_loss,
    report_losses, usage_loss, validate_tool_calls,
};
