//! Protocol-neutral capability, disposition, reasoning and conversion boundaries.

pub mod capabilities;
pub mod conversion;
pub mod disposition;
pub mod error;
pub mod failure;
pub mod model_limits;
pub mod reasoning;
pub mod request;
pub mod tool_set;

pub use conversion::{ConversionCause, ConversionFailure};
pub use error::CodecError;
