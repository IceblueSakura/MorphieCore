//! One attempt's bounded upstream I/O and semantic projection, never candidate selection.
use crate::gateway::{
    ApiError, BoundCandidate, BoundEntry, Limits,
    body::{Lane, send_frames},
    diagnostics::{Stage, Trace},
};
use crate::{
    adapter::Request,
    execution::{Attempt, ResponseDelivery},
    protocol::{openai::stream::Obfuscation, stream::SseLimits},
};
use bytes::Bytes;
/// While another candidate could still answer, lifecycle-only frames wait for the
/// first content so a pre-content upstream failure can fail over unseen.
const HOLD_BYTES: usize = 64 << 10;
const HOLD_TIME: std::time::Duration = std::time::Duration::from_secs(20);
fn lifecycle_only(event: &crate::semantic::task::generation::StreamEvent) -> bool {
    use crate::semantic::task::generation::{ItemKind, StreamEvent};
    match event {
        StreamEvent::Started
        | StreamEvent::Queued
        | StreamEvent::PartStarted { .. }
        | StreamEvent::MessageEnvelope(_) => true,
        // A started tool call already names its tool: that is content.
        StreamEvent::ItemStarted { kind, .. } => {
            matches!(kind, ItemKind::Message { .. } | ItemKind::Reasoning)
        }
        _ => false,
    }
}
#[allow(clippy::too_many_arguments)]
async fn produce(
    mut upstream: reqwest::Response,
    attempt: &mut Attempt,
    delivery: &mut ResponseDelivery,
    lane: &Lane<'_>,
    stream: bool,
    hold: bool,
    limit: usize,
    history: Option<&super::history::Pending>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let json = attempt.static_body().is_some();
    let mut capture = trace.capture.as_ref().and_then(|capture| {
        capture.body(
            crate::gateway::capture::Boundary::UpstreamResponse,
            Some(trace.capture_attempt),
            if json {
                "application/json"
            } else {
                "text/event-stream"
            },
        )
    });
    let started = std::time::Instant::now();
    let mut held: Option<Vec<Bytes>> = (stream && hold).then(Vec::new);
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
        if !json && let Some(capture) = &mut capture {
            capture.feed(&chunk);
        }
        used_bytes = used_bytes.saturating_add(chunk.len());
        if used_bytes > limit {
            return Err(ApiError::upstream());
        }
        let mut rest = chunk.as_ref();
        while !rest.is_empty() {
            let (used, events) = match attempt.push(rest) {
                Ok(pushed) => pushed,
                Err(error) => {
                    trace.decode_error(&error);
                    // An invalid or truncated upstream reply may fail over while unpublished.
                    let Some(status) = error.stream_failure() else {
                        return Err(ApiError::transport(crate::provider::ErrorClass::Upstream));
                    };
                    let failure = ApiError::failure(status, b"", None);
                    // After headers the class still reaches the client, as a failure frame.
                    if stream
                        && lane
                            .state
                            .published
                            .load(std::sync::atomic::Ordering::Acquire)
                    {
                        let body = if delivery.is_messages() {
                            failure.messages_error().1
                        } else {
                            failure.body()
                        };
                        let frames = delivery.fail_stream(&body).map_err(|_| failure)?;
                        for frame in frames {
                            crate::gateway::body::send_frame(lane, frame, trace).await?;
                        }
                    }
                    return Err(failure);
                }
            };
            trace.events(&events);
            trace.ignored_events(attempt.ignored_events());
            trace.reported_settings(
                attempt
                    .metadata()
                    .and_then(|metadata| metadata.context.settings.as_ref()),
                attempt.fidelity(),
            );
            if used == 0 {
                return Err(ApiError::upstream());
            }
            rest = &rest[used..];
            if stream {
                trace.stage(Stage::Projection);
                let frames = delivery.encode_events(attempt, &events).map_err(|error| {
                    trace.projection_error(&error);
                    ApiError::upstream()
                })?;
                trace.delivery_losses(delivery.losses());
                if let Some(buffer) = held.as_mut()
                    && events.iter().all(lifecycle_only)
                    && buffer.iter().chain(&frames).map(Bytes::len).sum::<usize>() <= HOLD_BYTES
                    && started.elapsed() < HOLD_TIME
                {
                    buffer.extend(frames);
                } else {
                    if let Some(buffer) = held.take() {
                        send_frames(lane, delivery, buffer, trace).await?;
                    }
                    send_frames(lane, delivery, frames, trace).await?;
                }
                trace.stage(Stage::Intake);
            }
        }
    }
    if let Some(capture) = &mut capture {
        if json && let Some(bytes) = attempt.static_body() {
            capture.feed(bytes);
        }
        capture.finish();
    }
    trace.stage(Stage::Terminal);
    attempt.finish().map_err(|error| {
        trace.decode_error(&error);
        ApiError::transport(crate::provider::ErrorClass::Upstream)
    })?;
    trace.reported_settings(
        attempt
            .metadata()
            .and_then(|metadata| metadata.context.settings.as_ref()),
        attempt.fidelity(),
    );
    trace.stage(Stage::Projection);
    let (frames, json_snapshot) = if stream {
        let frames = delivery.finish_stream(attempt).map_err(|error| {
            trace.projection_error(&error);
            ApiError::upstream()
        })?;
        (frames, None)
    } else {
        if let Some(usage) = attempt.response().ok().and_then(|r| r.semantic.usage()) {
            trace.reported_usage(usage);
        }
        let (response, bytes) = delivery.encode_json_snapshot(attempt).map_err(|error| {
            trace.projection_error(&error);
            ApiError::upstream()
        })?;
        if bytes.len() > limit {
            return Err(ApiError::upstream());
        }
        let body = Bytes::from(bytes);
        let snapshot = history
            .filter(|h| h.input.is_some())
            .map(|_| (response, body.clone()));
        (vec![body], snapshot)
    };
    trace.delivery_losses(delivery.losses());
    if let Some(buffer) = held.take() {
        send_frames(lane, delivery, buffer, trace).await?;
    }
    let reservation = if let Some(history) = history.filter(|h| h.input.is_some()) {
        let (response, body) = match json_snapshot {
            Some(snapshot) => snapshot,
            None => {
                let (response, body) = delivery
                    .snapshot(attempt)
                    .map_err(|_| ApiError::upstream())?;
                (response, Bytes::from(body))
            }
        };
        if body.len() > limit {
            return Err(ApiError::upstream());
        }
        Some(history.stage(response, body)?)
    } else {
        None
    };
    crate::gateway::body::send_final_frames(lane, delivery, frames, reservation, trace).await?;
    delivery
        .complete(attempt)
        .map_err(|_| ApiError::upstream())?;
    Ok(())
}
/// Error bodies are read for classification only, within the attempt deadline.
async fn error_body(mut upstream: reqwest::Response, trace: &Trace) -> Vec<u8> {
    const LIMIT: usize = 16 << 10;
    let content_type = upstream
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let json = content_type.starts_with("application/json");
    let mut capture = trace.capture.as_ref().and_then(|capture| {
        capture.body(
            crate::gateway::capture::Boundary::UpstreamResponse,
            Some(trace.capture_attempt),
            content_type,
        )
    });
    let mut body = vec![];
    loop {
        let chunk = match upstream.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(_) => {
                if json && let Some(capture) = &mut capture {
                    capture.feed(&body);
                }
                return body;
            }
        };
        if body.len() + chunk.len() > LIMIT {
            return vec![];
        }
        if !json && let Some(capture) = &mut capture {
            capture.feed(&chunk);
        }
        body.extend_from_slice(&chunk);
    }
    if let Some(capture) = &mut capture {
        if json {
            capture.feed(&body);
        }
        capture.finish();
    }
    body
}
#[allow(clippy::too_many_arguments)]
pub(in crate::gateway) async fn produce_candidate(
    entry: &BoundEntry,
    candidate: &BoundCandidate,
    request: &Request,
    upstream: reqwest::Response,
    limits: &Limits,
    lane: &Lane<'_>,
    hold: bool,
    history: Option<&super::history::Pending>,
    trace: &mut Trace,
) -> Result<Option<Request>, ApiError> {
    trace.stage(Stage::ResponseHead);
    let status = upstream.status().as_u16();
    trace.head(status, upstream.headers());
    if let Some(capture) = &trace.capture {
        capture.head(
            crate::gateway::capture::Boundary::UpstreamResponse,
            Some(trace.capture_attempt),
            upstream.headers(),
            None,
        );
    }
    if !(200..300).contains(&status) {
        if !(400..600).contains(&status) {
            return Err(ApiError::status(status));
        }
        let retry_after = crate::gateway::error::retry_after(upstream.headers());
        return Err(ApiError::failure(
            status,
            &error_body(upstream, trace).await,
            retry_after,
        ));
    }
    // A 2xx that is not an API reply (an HTML page, a missing or foreign type)
    // fails over like a 5xx.
    let not_api = || ApiError::transport(crate::provider::ErrorClass::Upstream);
    let mut content_types = upstream.headers().get_all("content-type").iter();
    let content_type = match (content_types.next(), content_types.next()) {
        // Only true absence may use the trusted SSE-only contract.
        (None, None)
            if candidate
                .endpoint
                .representation
                .openai()
                .is_some_and(|caps| caps.adaptation.rules.responses_sse_without_content_type) =>
        {
            "text/event-stream"
        }
        (Some(value), None) => value.to_str().map_err(|_| not_api())?,
        _ => return Err(not_api()),
    };
    let media: mime::Mime = content_type.parse().map_err(|_| not_api())?;
    let stream = request.delivery.streaming();
    let is_stream = media.type_() == mime::TEXT && media.subtype() == "event-stream";
    let is_json = media.type_() == mime::APPLICATION
        && (media.subtype() == mime::JSON || media.suffix() == Some(mime::JSON));
    let upstream_stream = stream || candidate.endpoint.representation.forced_stream();
    if (upstream_stream && !is_stream) || (!upstream_stream && !is_json) {
        return Err(not_api());
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
    let adapter = candidate
        .endpoint
        .adapter()
        .map_err(|_| ApiError::upstream())?;
    let mut attempt = Attempt::new(adapter, limit, sse);
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
        entry.public.id.as_str(),
        sse,
        options,
        padding,
    );
    if let Some(history) = history {
        delivery = delivery.with_storage(&history.id, history.options.metadata());
    }
    let result = produce(
        upstream,
        &mut attempt,
        &mut delivery,
        lane,
        stream,
        hold,
        limit,
        history,
        trace,
    )
    .await;
    if result.is_err() {
        attempt.cancel();
        delivery.cancel();
    }
    result?;
    Ok(request
        .inferred_affinity
        .is_some()
        .then(|| delivery.cache_successor(&attempt, request))
        .flatten())
}
