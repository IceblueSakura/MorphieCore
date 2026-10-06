//! One attempt's bounded upstream I/O and semantic projection, never candidate selection.
use super::{
    ApiError, BoundCandidate, BoundEntry, Limits,
    body::{Lane, send_frames},
    diagnostics::{Stage, Trace},
};
use crate::{
    adapter::Request,
    execution::{Attempt, ResponseDelivery},
    protocol::openai::sse::{Obfuscation, SseLimits},
};
use bytes::Bytes;
async fn produce(
    mut upstream: reqwest::Response,
    attempt: &mut Attempt,
    delivery: &mut ResponseDelivery,
    lane: &Lane<'_>,
    stream: bool,
    limit: usize,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let mut used_bytes = 0usize;
    while let Some(chunk) = upstream.chunk().await.map_err(|e| {
        ApiError::transport(if e.is_timeout() {
            crate::provider::ErrorClass::Timeout
        } else {
            crate::provider::ErrorClass::Upstream
        })
    })? {
        trace.stage(Stage::Intake);
        trace.received(chunk.len());
        used_bytes = used_bytes.saturating_add(chunk.len());
        if used_bytes > limit {
            return Err(ApiError::upstream());
        }
        let mut rest = chunk.as_ref();
        while !rest.is_empty() {
            let (used, events) = attempt.push(rest).map_err(|error| {
                trace.decode_error(&error);
                ApiError::upstream()
            })?;
            trace.events(&events);
            if used == 0 {
                return Err(ApiError::upstream());
            }
            rest = &rest[used..];
            if stream {
                trace.stage(Stage::Projection);
                let frames = delivery
                    .encode_events(attempt, &events)
                    .map_err(|_| ApiError::upstream())?;
                send_frames(lane, delivery, frames, trace).await?;
                trace.stage(Stage::Intake);
            }
        }
    }
    trace.stage(Stage::Terminal);
    attempt.finish().map_err(|error| {
        trace.decode_error(&error);
        ApiError::upstream()
    })?;
    trace.stage(Stage::Projection);
    let frames = if stream {
        delivery
            .finish_stream(attempt)
            .map_err(|_| ApiError::upstream())?
    } else {
        let bytes = delivery
            .encode_json(attempt)
            .map_err(|_| ApiError::upstream())?;
        if bytes.len() > limit {
            return Err(ApiError::upstream());
        }
        vec![Bytes::from(bytes)]
    };
    send_frames(lane, delivery, frames, trace).await?;
    delivery
        .complete(attempt)
        .map_err(|_| ApiError::upstream())?;
    Ok(())
}
pub(super) async fn produce_candidate(
    entry: &BoundEntry,
    candidate: &BoundCandidate,
    request: &Request,
    upstream: reqwest::Response,
    limits: &Limits,
    lane: &Lane<'_>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.stage(Stage::ResponseHead);
    let status = upstream.status().as_u16();
    trace.head(status, upstream.headers());
    if !(200..300).contains(&status) {
        return Err(ApiError::status(status));
    }
    let content_type = upstream
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let content_type = if !upstream.headers().contains_key("content-type")
        && candidate
            .endpoint
            .representation
            .adaptation
            .rules
            .responses_sse_without_content_type
    {
        "text/event-stream"
    } else {
        content_type
    };
    let media: mime::Mime = content_type.parse().map_err(|_| ApiError::upstream())?;
    let stream = request.delivery.streaming();
    let is_stream = media.type_() == mime::TEXT && media.subtype() == "event-stream";
    let is_json = media.type_() == mime::APPLICATION
        && (media.subtype() == mime::JSON || media.suffix() == Some(mime::JSON));
    let upstream_stream = stream
        || candidate
            .endpoint
            .representation
            .adaptation
            .rules
            .responses_forced_stream;
    if (upstream_stream && !is_stream) || (!upstream_stream && !is_json) {
        return Err(ApiError::upstream());
    }
    let limit = limits
        .response_bytes
        .min(candidate.endpoint.execution.response_body_limit);
    let sse = SseLimits {
        max_event_bytes: limits.event_bytes.min(limit),
        max_wire_bytes: limit,
        max_events: limits.max_events,
        max_obfuscation_bytes: (limit / 4).min(4 << 20),
    };
    let mut attempt = Attempt::new(candidate.endpoint.adapter(), limit, sse);
    attempt
        .begin(status, content_type)
        .map_err(|_| ApiError::upstream())?;
    let options = request
        .delivery
        .options
        .value()
        .cloned()
        .unwrap_or_default();
    let padding = if stream && options.obfuscation() {
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "entropy_unavailable",
            )
        })?;
        Obfuscation::Seeded(seed)
    } else {
        Obfuscation::Disabled
    };
    let mut delivery = ResponseDelivery::new(
        entry.client.clone(),
        entry.downstream.clone(),
        entry.public.id.as_str(),
        sse,
        options,
        padding,
    );
    let result = produce(
        upstream,
        &mut attempt,
        &mut delivery,
        lane,
        stream,
        limit,
        trace,
    )
    .await;
    if result.is_err() {
        attempt.cancel();
        delivery.cancel();
    }
    result
}
