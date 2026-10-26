//! One attempt's bounded upstream I/O and semantic projection, never candidate selection.
use crate::gateway::{
    ApiError, BoundCandidate, BoundEntry, Limits,
    body::{Lane, send_frames},
    diagnostics::{Stage, Trace},
};
use crate::lowering::generation::ConversionBoundary;
use crate::protocol::failure::{Observation, SourceFailureEvidence};
use crate::{
    adapter::Request,
    execution::{Attempt, ResponseDelivery},
    protocol::{openai::obfuscation::Obfuscation, stream::SseLimits},
};
use bytes::Bytes;
/// While another candidate could still answer, lifecycle-only frames wait for the
/// first content so a pre-content upstream failure can fail over unseen.
const HOLD_BYTES: usize = 64 << 10;
const HOLD_TIME: std::time::Duration = std::time::Duration::from_secs(20);
fn lifecycle_only(event: &crate::semantic::task::generation::StreamEvent) -> bool {
    use crate::semantic::task::generation::{ItemKind, StreamEvent, UsageStage};
    match event {
        StreamEvent::Started
        | StreamEvent::Queued
        | StreamEvent::PartStarted { .. }
        | StreamEvent::MessageEnvelope(_) => true,
        // Opening accounting is not generated content and must not end the existing hold.
        StreamEvent::UsageReport(report) if report.stage == UsageStage::Checkpoint => true,
        // A started tool call already names its tool: that is content.
        StreamEvent::ItemStarted { kind, .. } => {
            matches!(kind, ItemKind::Message { .. } | ItemKind::Reasoning)
        }
        _ => false,
    }
}
/// Record a failed delivery projection at `boundary`; the client sees an upstream error.
fn projection_failed(
    trace: &mut Trace,
    boundary: ConversionBoundary,
) -> impl FnOnce(crate::execution::AttemptError) -> ApiError + '_ {
    move |error| {
        trace.projection_error(boundary, &error);
        ApiError::upstream()
    }
}
/// Publish lifecycle frames held while another candidate could still answer.
async fn release_held(
    held: &mut Option<Vec<crate::execution::report::EncodedBatch>>,
    lane: &Lane<'_>,
    delivery: &mut ResponseDelivery,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    for batch in held.take().into_iter().flatten() {
        send_frames(lane, delivery, batch, trace).await?;
    }
    Ok(())
}
async fn flush_pending(
    lane: &Lane<'_>,
    delivery: &mut ResponseDelivery,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let frames = delivery
        .flush_pending()
        .map_err(projection_failed(trace, ConversionBoundary::EventEncode))?;

    send_frames(lane, delivery, frames, trace).await
}
/// A classified upstream failure: it may fail over while unpublished; after
/// publication it still reaches the client as the protocol's failure frame.
async fn fail_classified(
    lane: &Lane<'_>,
    delivery: &mut ResponseDelivery,
    stream: bool,
    failure: ApiError,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    *lane.state.failure_head.lock().expect("upstream failure") = Some(failure);
    let published = lane
        .state
        .published
        .load(std::sync::atomic::Ordering::Acquire);
    if stream && published {
        flush_pending(lane, delivery, trace).await?;
        let body = if delivery.is_messages() {
            failure.messages_error().1
        } else {
            failure.body()
        };
        let frames = delivery.fail_stream(&body).map_err(|error| {
            trace.projection_error(ConversionBoundary::EventEncode, &error);
            failure
        })?;
        crate::gateway::body::send_error_frames(lane, frames, trace).await?;
    }
    Err(failure)
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
    let started = tokio::time::Instant::now();
    let mut held: Option<Vec<crate::execution::report::EncodedBatch>> =
        (stream && hold).then(Vec::new);
    let mut used_bytes = 0usize;
    loop {
        let deadline = delivery
            .flush_deadline()
            .map(tokio::time::Instant::from_std);
        lane.state
            .waiting_upstream
            .store(true, std::sync::atomic::Ordering::Release);
        let chunk = tokio::select! {
            biased;
            _ = tokio::time::sleep_until(started + HOLD_TIME), if held.is_some() => {
                release_held(&mut held, lane, delivery, trace).await?;
                continue;
            }
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                flush_pending(lane, delivery, trace).await?;
                continue;
            }
            chunk = upstream.chunk() => chunk.map_err(|e| {
                let class=if e.is_timeout(){crate::provider::ErrorClass::Timeout}else{crate::provider::ErrorClass::Upstream};
                trace.report.failure(None,&crate::execution::AttemptError::Transport(class));
                ApiError::transport(class)
            })?,
        };
        lane.state
            .waiting_upstream
            .store(false, std::sync::atomic::Ordering::Release);
        let Some(chunk) = chunk else { break };
        trace.stage(Stage::Intake);
        trace.received(chunk.len());
        if !json && let Some(capture) = &mut capture {
            capture.feed(&chunk);
        }
        used_bytes = used_bytes.saturating_add(chunk.len());
        if used_bytes > limit {
            trace.report.failure(
                None,
                &crate::execution::AttemptError::Resource(
                    crate::protocol::failure::Resource::Bytes,
                ),
            );
            return Err(ApiError::upstream());
        }
        let mut rest = chunk.as_ref();
        while !rest.is_empty() {
            if delivery
                .flush_deadline()
                .is_some_and(|deadline| deadline <= std::time::Instant::now())
            {
                flush_pending(lane, delivery, trace).await?;
            }
            let (used, events) = match attempt.push(rest) {
                Ok(pushed) => pushed,
                Err(error) => {
                    trace.decode_error(&error);
                    // An invalid or truncated upstream reply may fail over while unpublished.
                    let Some(failure) = error.source_failure().and_then(ApiError::source_failure)
                    else {
                        return Err(ApiError::transport(crate::provider::ErrorClass::Upstream));
                    };
                    return fail_classified(lane, delivery, stream, failure, trace).await;
                }
            };
            let evidence = attempt.take_evidence();
            trace.events(&events, &evidence);
            if let Some(report) = attempt.pending_source_usage_report() {
                trace.report.source_usage(report, &evidence);
            }
            trace
                .report
                .accept(crate::execution::report::View::SourceIntake, evidence);
            trace.ignored_events(attempt.ignored_events());
            trace.reported_settings(
                attempt
                    .metadata()
                    .and_then(|metadata| metadata.context.settings()),
            );
            if used == 0 {
                return Err(ApiError::upstream());
            }
            rest = &rest[used..];
            if stream {
                trace.stage(Stage::Projection);
                if delivery.must_flush_before(attempt, &events) {
                    flush_pending(lane, delivery, trace).await?;
                }
                let batches = delivery.queue_events(attempt, &events, std::time::Instant::now());

                let batches =
                    batches.map_err(projection_failed(trace, ConversionBoundary::EventProject))?;
                // Handoff each drained run before encoding a following barrier:
                // a later target error must not erase validated pending content.
                for batch in batches {
                    let frames = delivery.encode_prepared(batch);

                    let frames = frames
                        .map_err(projection_failed(trace, ConversionBoundary::EventEncode))?;
                    if frames.is_empty() {
                        continue;
                    }
                    if let Some(buffer) = held.as_mut()
                        && events.iter().all(lifecycle_only)
                        && buffer
                            .iter()
                            .map(crate::execution::report::EncodedBatch::byte_len)
                            .sum::<usize>()
                            + frames.byte_len()
                            <= HOLD_BYTES
                        && started.elapsed() < HOLD_TIME
                    {
                        buffer.push(frames);
                    } else {
                        release_held(&mut held, lane, delivery, trace).await?;
                        send_frames(lane, delivery, frames, trace).await?;
                    }
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
    // Chat and Messages have no failed-response carrier; an upstream-reported
    // failure reaches them like an in-band stream error.
    if !delivery.carries_failed_responses()
        && let Some(failure) = attempt
            .reported_failure()
            .and_then(ApiError::source_failure)
    {
        return fail_classified(lane, delivery, stream, failure, trace).await;
    }
    let evidence = attempt.take_evidence();
    // Stream reports were observed with their source-unit provenance. Re-reading
    // the materialized response after draining those deltas would lose that basis.
    if json && let Ok(response) = attempt.response() {
        for report in response.semantic.usage_reports().reports() {
            trace.report.source_usage(report, &evidence);
        }
    }
    trace
        .report
        .accept(crate::execution::report::View::SourceIntake, evidence);
    trace.report.update(|a| {
        a.source_eof = true;
        a.task_outcome = attempt.response().ok().map(|r| r.semantic.outcome().into());
    });
    trace.reported_settings(
        attempt
            .metadata()
            .and_then(|metadata| metadata.context.settings()),
    );
    trace.stage(Stage::Projection);
    if stream {
        flush_pending(lane, delivery, trace).await?;
    }
    let (frames, json_snapshot) = if stream {
        let frames = delivery.finish_stream(attempt);

        let frames = frames.map_err(projection_failed(trace, ConversionBoundary::EventEncode))?;
        (frames, None)
    } else {
        if let Some(usage) = attempt.response().ok().and_then(|r| r.semantic.usage()) {
            trace.reported_usage(usage);
        }
        let snapshot = delivery.encode_json_snapshot(attempt);

        let (response, batch) =
            snapshot.map_err(projection_failed(trace, ConversionBoundary::ResponseEncode))?;
        let snapshot = history
            .filter(|h| h.input.is_some())
            .map(|_| (response, batch.frames()[0].clone()));
        (batch, snapshot)
    };

    release_held(&mut held, lane, delivery, trace).await?;
    let reservation = if let Some(history) = history.filter(|h| h.input.is_some()) {
        let (response, body) = match json_snapshot {
            Some(snapshot) => snapshot,
            None => {
                let snapshot = delivery.snapshot(attempt);

                let (response, body) = snapshot.map_err(|_| ApiError::upstream())?;
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
async fn error_body(mut upstream: reqwest::Response, trace: &Trace) -> (Vec<u8>, Observation) {
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
                return (body, Observation::Interrupted);
            }
        };
        if body.len() + chunk.len() > LIMIT {
            return (vec![], Observation::OverLimit);
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
    (body, Observation::Complete)
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
        let class = match crate::provider::classify_status(status) {
            crate::provider::StatusClass::Failure(c) => c,
            _ => unreachable!(),
        };
        trace.report.failure(
            None,
            &crate::execution::AttemptError::Status { status, class },
        );
        trace.report.source_failure(SourceFailureEvidence::http(
            status,
            b"",
            Observation::IntentionallyUnread,
        ));
        if !(400..600).contains(&status) {
            return Err(ApiError::status(status));
        }
        let retry_after = crate::gateway::error::retry_after(upstream.headers());
        *lane.state.failure_head.lock().expect("failure head") =
            Some(ApiError::failure(status, b"", retry_after));
        // These statuses are authoritative. Reading an optional error body must
        // not turn an auth refusal into quota or timeout retry permission.
        if matches!(status, 401 | 403) {
            return Err(ApiError::failure(status, b"", retry_after));
        }
        lane.state
            .waiting_upstream
            .store(true, std::sync::atomic::Ordering::Release);
        trace.report.source_failure(SourceFailureEvidence::http(
            status,
            b"",
            Observation::Pending,
        ));
        let (body, observation) = error_body(upstream, trace).await;
        trace
            .report
            .source_failure(SourceFailureEvidence::http(status, &body, observation));
        lane.state
            .waiting_upstream
            .store(false, std::sync::atomic::Ordering::Release);
        return Err(ApiError::failure(status, &body, retry_after));
    }
    // A 2xx that is not an API reply (an HTML page, a missing or foreign type)
    // fails over like a 5xx.
    let report = trace.report.clone();
    let not_api = || {
        report.failure(
            None,
            &crate::execution::AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::UnexpectedResponseMediaType,
            ),
        );
        ApiError::transport(crate::provider::ErrorClass::Upstream)
    };
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
    attempt.begin(status, content_type).map_err(|error| {
        trace.decode_error(&error);
        ApiError::upstream()
    })?;
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
    )
    .with_report(trace.report.clone())
    .with_request(&candidate.endpoint, request)
    .with_coalescing(stream.then_some(limits.delta_coalescing).flatten());
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
    if let Err(error) = result
        && error.health
    {
        // Error delivery can itself stall or be cancelled. Preserve the failure
        // before flushing pending output, not only when this future returns.
        *lane.state.failure_head.lock().expect("upstream failure") = Some(error);
    }
    trace.coalescing_peak(delivery.coalescing_peak_bytes());

    if result.is_err() {
        // An unpublished failed attempt must not acquire commitment by flushing.
        if stream
            && delivery.has_open_pending()
            && lane
                .state
                .published
                .load(std::sync::atomic::Ordering::Acquire)
        {
            let _ = flush_pending(lane, &mut delivery, trace).await;
        }
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
