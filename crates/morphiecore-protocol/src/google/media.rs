//! Google vendor-wide media restrictions shared by every wire API.
use crate::CodecError;
use crate::failure::CarrierReason;

/// Google Files URLs select credential-owned state, not public media.
/// Reject the API host regardless of path spelling or version.
/// https://ai.google.dev/gemini-api/docs/files
pub fn check_public_media_url(raw: &str) -> Result<(), CodecError> {
    let url = url::Url::parse(raw).map_err(|_| CodecError::invalid())?;
    if url
        .host_str()
        .is_some_and(|host| host.trim_end_matches('.') == "generativelanguage.googleapis.com")
    {
        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
    }
    Ok(())
}
