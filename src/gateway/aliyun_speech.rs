//! Native SSE intake; no URL fetch, partial publication or terminal repair.
use super::{
    ApiError,
    diagnostics::{Stage, Trace},
};
use crate::{
    protocol::aliyun_speech::Decoder, semantic::task::speech_synthesis::SpeechResult,
    transport::sse::SseDecoder,
};

pub(super) async fn receive(
    mut upstream: reqwest::Response,
    audio_limit: usize,
    wire_limit: usize,
    event_limit: usize,
    event_count: usize,
    trace: &mut Trace,
) -> Result<SpeechResult, ApiError> {
    trace.stage(Stage::ResponseHead);
    trace.head(upstream.status().as_u16(), upstream.headers());
    if upstream.status().as_u16() != 200 {
        return Err(ApiError::status(upstream.status().as_u16()));
    }
    let headers = upstream.headers();
    let mut types = headers.get_all("content-type").iter();
    let media: mime::Mime = types
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(ApiError::upstream)?;
    if types.next().is_some()
        || media.essence_str() != "text/event-stream"
        || media
            .params()
            .any(|(k, v)| k != mime::CHARSET || v != mime::UTF_8)
        || headers.contains_key("content-encoding")
    {
        return Err(ApiError::upstream());
    }
    let mut lengths = headers.get_all("content-length").iter();
    let length = lengths
        .next()
        .map(|v| {
            let raw = v.to_str().map_err(|_| ApiError::upstream())?;
            if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::upstream());
            }
            raw.parse::<u64>().map_err(|_| ApiError::upstream())
        })
        .transpose()?;
    if lengths.next().is_some() || length.is_some_and(|n| n == 0 || n > wire_limit as u64) {
        return Err(ApiError::upstream());
    }
    let mut framer = SseDecoder::new(event_limit);
    let mut decoder = Decoder::new(audio_limit, wire_limit, event_count);
    let mut received = 0usize;
    let mut events = 0usize;
    trace.stage(Stage::Intake);
    while let Some(chunk) = upstream.chunk().await.map_err(|e| {
        if e.is_timeout() {
            ApiError::timeout()
        } else {
            ApiError::upstream()
        }
    })? {
        received = received
            .checked_add(chunk.len())
            .ok_or_else(ApiError::upstream)?;
        if received > wire_limit {
            return Err(ApiError::upstream());
        }
        trace.received(chunk.len());
        let mut offset = 0;
        while offset < chunk.len() {
            let (event, used) = framer
                .push_until_event(&chunk[offset..])
                .map_err(|_| ApiError::upstream())?;
            offset += used;
            if let Some(event) = event {
                events += 1;
                if events > event_count
                    || !matches!(event.event(), None | Some("message" | "result"))
                {
                    return Err(ApiError::upstream());
                }
                if event.data().is_empty() {
                    return Err(ApiError::upstream());
                }
                let value = crate::protocol::openai::json::decode(event.data().as_bytes())
                    .map_err(|_| ApiError::upstream())?;
                decoder.push(&value).map_err(|_| ApiError::upstream())?;
            }
        }
    }
    trace.stage(Stage::Terminal);
    if length.is_some_and(|n| n != received as u64) {
        return Err(ApiError::upstream());
    }
    framer.finish_strict().map_err(|_| ApiError::upstream())?;
    decoder.finish().map_err(|_| ApiError::upstream())
}
