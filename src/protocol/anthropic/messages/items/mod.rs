//! One content unit in either direction: block → IR and IR → block.
mod decode;
mod encode;

use crate::protocol::CodecError;

pub(in crate::protocol::anthropic::messages) use decode::{
    Intake, bind, function_tool, text, validate_cache,
};
pub(in crate::protocol::anthropic::messages) use encode::item_blocks;

/// An IR shape Messages has no carrier for; the caller names the owning rule.
pub(in crate::protocol::anthropic::messages) fn unsupported() -> CodecError {
    CodecError::Unsupported("native semantic carrier".into())
}
