//! One content unit in either direction: block → IR and IR → block.
use crate::protocol::failure::CarrierReason;
mod decode;
mod encode;

use crate::protocol::CodecError;

pub(in crate::protocol::anthropic::messages) use decode::{
    Intake, bind, function_tool, root_instructions, text, validate_cache,
};
pub(in crate::protocol::anthropic::messages) use encode::item_blocks;

/// An IR shape Messages has no carrier for; the caller names the owning rule.
pub(in crate::protocol::anthropic::messages) fn unsupported() -> CodecError {
    CodecError::Unsupported(CarrierReason::Unavailable)
}

/// Native boundaries own their TTL; a request-lifetime boundary cannot be guessed.
pub(in crate::protocol::anthropic::messages) fn cache_ttl(
    cache: Option<crate::semantic::task::generation::PrefixCacheBoundary>,
) -> Result<Option<crate::semantic::task::generation::PrefixCacheTtl>, CodecError> {
    use crate::semantic::task::generation::PrefixCacheBoundary;
    match cache {
        None => Ok(None),
        Some(PrefixCacheBoundary::Ttl(ttl)) => Ok(Some(ttl)),
        Some(PrefixCacheBoundary::RequestLifetime) => Err(unsupported()),
    }
}
