//! Bounded TypeSafe SDK request compression; never forward the encoding upstream.
use crate::{ApiError, error::ErrorKind};
use axum::http::{HeaderMap, StatusCode, header::CONTENT_ENCODING};
use std::io::Read;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

pub(super) fn consume_encoding(headers: &mut HeaderMap) -> Result<bool, ApiError> {
    let mut values = headers.get_all(CONTENT_ENCODING).iter();
    let Some(encoding) = values.next() else {
        return Ok(false);
    };
    if values.next().is_some() || !encoding.as_bytes().eq_ignore_ascii_case(b"zstd") {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ErrorKind::UnsupportedMediaType,
        ));
    }
    headers.remove(CONTENT_ENCODING);
    Ok(true)
}

pub(super) async fn decode(
    compressed: Vec<u8>,
    expanded_limit: usize,
    shutdown: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<u8>, ApiError> {
    let cancel = shutdown.child_token();
    let _cancel_on_drop = cancel.clone().drop_guard();
    let decoded =
        tokio::task::spawn_blocking(move || inflate(&compressed, expanded_limit, &cancel));
    tokio::select! {
        biased;
        _ = shutdown.cancelled() => Err(ApiError::shutdown()),
        _ = sleep_until(deadline) => Err(ApiError::new(StatusCode::REQUEST_TIMEOUT, ErrorKind::RequestTimeout)),
        result = decoded => result.map_err(|_| ApiError::upstream())?,
    }
}

fn inflate(input: &[u8], limit: usize, cancel: &CancellationToken) -> Result<Vec<u8>, ApiError> {
    let invalid = |_| ApiError::invalid();
    let mut decoder = zstd::stream::read::Decoder::with_buffer(input)
        .map_err(invalid)?
        .single_frame();
    // Bound decoder memory independently of compressed and expanded body lengths.
    decoder.window_log_max(22).map_err(invalid)?;
    let mut result = Vec::new();
    let mut chunk = [0; 16 << 10];
    loop {
        if cancel.is_cancelled() {
            return Err(ApiError::shutdown());
        }
        let n = decoder.read(&mut chunk).map_err(invalid)?;
        if n == 0 {
            if !decoder.finish().is_empty() {
                return Err(ApiError::invalid());
            }
            return Ok(result);
        }
        if n > limit.saturating_sub(result.len()) {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorKind::RequestTooLarge,
            ));
        }
        result.extend_from_slice(&chunk[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_safe_zstd_expansion_is_bounded_and_truncation_is_not_eof() {
        let data = vec![b'a'; 1 << 15];
        let mut compressed = zstd::stream::encode_all(data.as_slice(), 0).unwrap();
        let cancel = CancellationToken::new();
        assert!(compressed.len() < 1024);
        assert_eq!(inflate(&compressed, data.len(), &cancel).unwrap(), data);
        assert_eq!(
            inflate(&compressed, data.len() - 1, &cancel)
                .unwrap_err()
                .status,
            StatusCode::PAYLOAD_TOO_LARGE,
        );
        assert_eq!(
            inflate(
                &[compressed.as_slice(), compressed.as_slice()].concat(),
                data.len() * 2,
                &cancel
            )
            .unwrap_err()
            .kind,
            ErrorKind::InvalidRequest,
        );
        compressed.pop();
        assert_eq!(
            inflate(&compressed, data.len(), &cancel).unwrap_err().kind,
            ErrorKind::InvalidRequest,
        );
    }
}
