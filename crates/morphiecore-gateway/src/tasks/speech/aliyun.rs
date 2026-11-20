//! Native SSE intake; no URL fetch, partial publication or terminal repair.
use crate::{
    ApiError,
    diagnostics::{Stage, Trace},
    protocol::aliyun_speech::Decoder,
    semantic::task::speech_synthesis::SpeechResult,
    transport::sse::SseDecoder,
};

pub(super) async fn decode(
    mut receiver: super::Receiver,
    audio_limit: usize,
    wire_limit: usize,
    event_limit: usize,
    event_count: usize,
    trace: &mut Trace,
) -> Result<SpeechResult, ApiError> {
    receiver.require_media("text/event-stream")?;
    let mut framer = SseDecoder::new(event_limit);
    let mut decoder = Decoder::new(audio_limit, wire_limit, event_count);
    let mut events = 0usize;
    trace.stage(Stage::Intake);
    while let Some(chunk) = receiver.next(trace).await? {
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
                let value = crate::protocol::json::decode(event.data().as_bytes())
                    .map_err(|_| ApiError::upstream())?;
                decoder.push(&value).map_err(|_| ApiError::upstream())?;
            }
        }
    }
    trace.stage(Stage::Terminal);
    framer.finish_strict().map_err(|_| ApiError::upstream())?;
    decoder.finish().map_err(|_| ApiError::upstream())
}

#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    audio_limit: usize,
    wire_limit: usize,
    event_limit: usize,
    event_count: usize,
    trace: &mut Trace,
) -> Result<SpeechResult, ApiError> {
    let receiver = super::Receiver::new(upstream, wire_limit, trace)?;
    decode(
        receiver,
        audio_limit,
        wire_limit,
        event_limit,
        event_count,
        trace,
    )
    .await
}
