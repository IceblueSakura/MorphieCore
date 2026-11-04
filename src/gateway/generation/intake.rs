//! One attempt's bounded upstream I/O and semantic projection, never candidate selection.
use crate::{
    execution::{Attempt, ResponseDelivery},
    gateway::{
        ApiError, BoundCandidate, BoundEntry, Limits,
        body::{Lane, send_frames},
        diagnostics::{Stage, Trace},
    },
    protocol::{
        GenerationRequestEnvelope as Request,
        conversion::generation::ConversionBoundary,
        failure::{Observation, SourceFailureEvidence},
        openai::obfuscation::Obfuscation,
        stream::SseLimits,
    },
};
use bytes::Bytes;
/// Delivery can finish honestly without establishing successful upstream work.
#[derive(Clone, Copy)]
pub(in crate::gateway) enum Completion {
    Eligible,
    Failed(ApiError),
    Neutral,
}
pub(in crate::gateway) struct Produced {
    pub completion: Completion,
    pub successor: Option<Request>,
}
/// While another candidate could still answer, lifecycle-only frames wait for the
/// first content so a pre-content upstream failure can fail over unseen.
const HOLD_BYTES: usize = 64 << 10;
const HOLD_TIME: std::time::Duration = std::time::Duration::from_secs(20);
fn lifecycle_only(event: &crate::semantic::task::generation::StreamEvent) -> bool {
    !crate::execution::delivery::content_event(event)
}
fn source_failure(
    provider: &str,
    status: u16,
    source: SourceFailureEvidence,
    retry_after: Option<u64>,
    observed_http: bool,
    trace: &Trace,
) -> ApiError {
    let (failure, source) =
        ApiError::interpret_source(provider, status, source, retry_after, observed_http);
    trace.report.source_failure(source);
    failure
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
) -> Result<Completion, ApiError> {
    lane.state.observe_failure(failure);
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
    provider: &str,
    attempt: &mut Attempt,
    delivery: &mut ResponseDelivery,
    lane: &Lane<'_>,
    stream: bool,
    hold: bool,
    limit: usize,
    history: Option<&super::history::Pending>,
    trace: &mut Trace,
) -> Result<Completion, ApiError> {
    let retry_after = crate::gateway::error::retry_after(upstream.headers());
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
                let class=crate::transport::http::TransportError::from_reqwest(&e);
                trace.report.failure(None,&crate::execution::AttemptError::Transport(class));
                let failure = ApiError::transport_fact(class);
                lane.state.observe_interruption(failure);
                failure
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
                    let Some(failure) = error.source_failure().map(|source| {
                        source_failure(
                            provider,
                            source.status_hint.map_or(500, |s| s.get()),
                            source,
                            retry_after,
                            false,
                            trace,
                        )
                    }) else {
                        return Err(ApiError::malformed());
                    };
                    return fail_classified(lane, delivery, stream, failure, trace).await;
                }
            };
            let mut retry_failure = None;
            if let Some(source) = attempt.reported_failure() {
                let failure = source_failure(
                    provider,
                    source.status_hint.map_or(500, |s| s.get()),
                    source,
                    retry_after,
                    false,
                    trace,
                );
                lane.state.observe_failure(failure);
                if hold
                    && failure.recovery.retryable
                    && !lane
                        .state
                        .published
                        .load(std::sync::atomic::Ordering::Acquire)
                {
                    retry_failure = Some(failure);
                }
            }
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
            if let Some(failure) = retry_failure {
                return Err(failure);
            }
            if attempt.recovery_ready_terminal() {
                lane.state.ready();
            }
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
                if delivery.content_ready() {
                    lane.state.ready();
                }
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
            } else if !events.is_empty() {
                delivery
                    .observe_events(attempt, &events)
                    .map_err(projection_failed(trace, ConversionBoundary::EventProject))?;
                if delivery.content_ready() {
                    lane.state.ready();
                }
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
    if let Err(error) = attempt.finish() {
        trace.decode_error(&error);
        if let Some(source) = error.source_failure() {
            let failure = source_failure(
                provider,
                source.status_hint.map_or(500, |s| s.get()),
                source,
                retry_after,
                false,
                trace,
            );
            return fail_classified(lane, delivery, stream, failure, trace).await;
        }
        return Err(ApiError::malformed());
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
    // A valid failed task still established source facts and EOF. Client error
    // presentation must not erase them or become the authoritative source cause.
    let mut failure = None;
    if let Some(source) = attempt.reported_failure() {
        failure = Some(source_failure(
            provider,
            source.status_hint.map_or(500, |s| s.get()),
            source,
            retry_after,
            false,
            trace,
        ));
        if let Some(failure) = failure {
            lane.state.observe_failure(failure);
            if hold
                && failure.recovery.retryable
                && !lane
                    .state
                    .published
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(failure);
            }
            if !delivery.carries_failed_responses() {
                return fail_classified(lane, delivery, stream, failure, trace).await;
            }
        }
    }
    let completion = {
        use crate::semantic::task::generation::{IncompleteReason, Outcome};
        let response = attempt.response().map_err(|_| ApiError::upstream())?;
        if let Some(failure) = failure {
            Completion::Failed(failure)
        } else if response.semantic.outcome() == Outcome::Completed
            || (response.semantic.outcome() == Outcome::Incomplete
                && response.semantic.details().incomplete
                    == Some(IncompleteReason::MaxOutputTokens))
        {
            Completion::Eligible
        } else {
            Completion::Neutral
        }
    };
    if matches!(completion, Completion::Eligible) {
        lane.state.ready();
    }
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
    Ok(completion)
}
/// Error bodies are read for classification only, within the attempt deadline.
async fn error_body(
    mut upstream: reqwest::Response,
    lane: &Lane<'_>,
    trace: &Trace,
) -> (Vec<u8>, Observation) {
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
            Err(error) => {
                lane.state.observe_interruption(ApiError::transport_fact(
                    crate::transport::http::TransportError::from_reqwest(&error),
                ));
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
    original: &Request,
    upstream: reqwest::Response,
    limits: &Limits,
    lane: &Lane<'_>,
    hold: bool,
    history: Option<&super::history::Pending>,
    trace: &mut Trace,
) -> Result<Produced, ApiError> {
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
        lane.state.observe_failure(source_failure(
            candidate.provider.id.as_str(),
            status,
            SourceFailureEvidence::http(status, b"", Observation::Pending),
            retry_after,
            true,
            trace,
        ));
        lane.state
            .waiting_upstream
            .store(true, std::sync::atomic::Ordering::Release);
        trace.report.source_failure(SourceFailureEvidence::http(
            status,
            b"",
            Observation::Pending,
        ));
        let budget = limits
            .provider_recovery
            .get(candidate.provider.id.as_str())
            .cloned()
            .unwrap_or_default()
            .recovery_timeout_ms;
        let cleanup_deadline = {
            let failures = lane.state.failures.lock().expect("failure state");
            failures
                .head
                .as_ref()
                .filter(|failure| failure.error.recovery.retryable && budget > 0)
                .map(|failure| failure.at + std::time::Duration::from_millis(budget))
        };
        let (body, observation) = match cleanup_deadline {
            Some(until) => tokio::time::timeout_at(until, error_body(upstream, lane, trace))
                .await
                .unwrap_or((vec![], Observation::Interrupted)),
            None => error_body(upstream, lane, trace).await,
        };
        let source = SourceFailureEvidence::http(status, &body, observation);
        lane.state
            .waiting_upstream
            .store(false, std::sync::atomic::Ordering::Release);
        let failure = source_failure(
            candidate.provider.id.as_str(),
            status,
            source,
            retry_after,
            true,
            trace,
        );
        lane.state.observe_failure(failure);
        return Err(failure);
    }
    // Malformed successful replies do not become upstream health evidence.
    let report = trace.report.clone();
    let not_api = || {
        report.failure(
            None,
            &crate::execution::AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::UnexpectedResponseMediaType,
            ),
        );
        ApiError::malformed()
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
    let upstream_stream = stream || candidate.endpoint.protocol.requires_streaming_intake();
    #[cfg(test)]
    let upstream_stream = upstream_stream || limits.force_upstream_stream;
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
        ApiError::malformed()
    })?;
    let options = request
        .delivery
        .options
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let padding = if stream && options.obfuscation() {
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                crate::gateway::error::ErrorKind::EntropyUnavailable,
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
    .with_request(&candidate.endpoint, request, original)
    .with_coalescing(stream.then_some(limits.delta_coalescing).flatten());
    if let Some(history) = history {
        delivery = delivery.with_storage(&history.id, history.options.metadata());
    }
    let result = produce(
        upstream,
        candidate.provider.id.as_str(),
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
    let completion = result?;
    let successor = (matches!(completion, Completion::Eligible)
        && request.inferred_affinity.is_some())
    .then(|| delivery.cache_successor(&attempt, request))
    .flatten();
    Ok(Produced {
        completion,
        successor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
    use futures_util::StreamExt;

    #[tokio::test]
    async fn retryable_failed_terminal_records_its_usage_before_discarding_output() {
        let response = serde_json::json!({"id":"failed-attempt","object":"response","created_at":1,
            "model":"fixture-model","status":"failed","output":[],
            "error":{"code":"server_error","message":"synthetic failure"},
            "usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}});
        let events = crate::gateway::tests::responses_wire::response_events(response);
        let wire = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        let chunks =
            futures_util::stream::once(async move { Ok::<_, std::io::Error>(Bytes::from(wire)) })
                .chain(futures_util::stream::pending());
        let upstream = reqwest::Response::from(
            axum::http::Response::builder()
                .header("content-type", "text/event-stream")
                .header("retry-after", "7")
                .body(reqwest::Body::wrap_stream(chunks))
                .unwrap(),
        );
        let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Siwc, None);
        let mut attempt = Attempt::new(codec, 1 << 20, SseLimits::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let client = crate::adapter::ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
            TargetContract::full(),
        );
        let mut delivery = ResponseDelivery::new(
            client,
            "fixture-model",
            SseLimits::default(),
            Default::default(),
            Obfuscation::Disabled,
        );
        let state = crate::gateway::body::Status::default();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let lane = Lane {
            tx: &tx,
            state: &state,
        };
        let mut trace = Trace::for_request(
            None,
            &axum::extract::Request::new(axum::body::Body::empty()),
        );
        trace.report.begin(1);
        let error = produce(
            upstream,
            "chatgpt",
            &mut attempt,
            &mut delivery,
            &lane,
            false,
            true,
            1 << 20,
            None,
            &mut trace,
        )
        .await
        .err()
        .unwrap();
        assert!(error.recovery.retryable);
        assert_eq!(error.retry_after, Some(7));
        let usage = serde_json::to_value(trace.report.attempt().source_usage).unwrap();
        assert_eq!(usage["reports"][0]["counts"]["input_tokens"], 7);
        assert_eq!(usage["reports"][0]["counts"]["output_tokens"], 3);
        assert!(!state.published.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn chat_error_evidence_agrees_across_json_and_sse_and_rejects_malformed_known_values() {
        for malformed in [
            None,
            Some("code"),
            Some("type"),
            Some("message"),
            Some("missing_message"),
            Some("null_message"),
            Some("error_type"),
            Some("metadata"),
        ] {
            let mut value = serde_json::json!({"error":{"code":500,"message":"synthetic"},"error_type":"authentication"});
            if let Some(field) = malformed {
                if field == "missing_message" {
                    value["error"].as_object_mut().unwrap().remove("message");
                } else if field == "null_message" {
                    value["error"]["message"] = serde_json::Value::Null;
                } else if field == "metadata" {
                    value["error"]["metadata"] = serde_json::json!({"error_type":42});
                } else {
                    value["error"][field] = serde_json::json!(false);
                }
            }
            for stream in [false, true] {
                let codec = GenerationCodec::new(GenerationApi::Chat, Dialect::OpenRouter, None);
                let error = if stream {
                    let mut attempt = Attempt::new(codec, 1 << 20, SseLimits::default());
                    attempt.begin(200, "text/event-stream").unwrap();
                    attempt
                        .push(format!("data: {value}\n\n").as_bytes())
                        .unwrap_err()
                } else {
                    crate::execution::AttemptError::Codec(
                        codec
                            .decode_response(&serde_json::to_vec(&value).unwrap())
                            .unwrap_err(),
                    )
                };
                if malformed.is_some() {
                    assert!(error.source_failure().is_none(), "{stream}: {malformed:?}");
                } else {
                    let source = error.source_failure().unwrap();
                    let (error, _) =
                        ApiError::interpret_source("openrouter", 500, source, None, false);
                    assert_eq!(
                        error.recovery.class,
                        Some(crate::provider::RecoveryClass::Auth)
                    );
                    assert!(!error.recovery.retryable);
                }
            }
        }
    }

    #[test]
    fn native_failed_envelopes_keep_provider_evidence_and_standard_error_carriers() {
        for (dialect, code, canonical, expected, class) in [
            (
                Dialect::Siwc,
                "subscription_sharing_usage_limit_exceeded",
                None,
                "rate_limit_exceeded",
                crate::provider::RecoveryClass::Quota,
            ),
            (
                Dialect::OpenRouter,
                "server_error",
                Some("authentication"),
                "server_error",
                crate::provider::RecoveryClass::Auth,
            ),
        ] {
            let mut wire = crate::gateway::tests::responses_wire::subscription_response(true);
            wire["status"] = serde_json::json!("failed");
            wire["output"] = serde_json::json!([]);
            wire["error"] = serde_json::json!({"code":code,"message":"synthetic failure"});
            if let Some(canonical) = canonical {
                wire["error_type"] = serde_json::json!(canonical);
            }
            let codec = GenerationCodec::new(GenerationApi::Responses, dialect, None);
            let source = codec
                .decode_response(&serde_json::to_vec(&wire).unwrap())
                .unwrap();
            assert_eq!(
                source.semantic.outcome(),
                crate::semantic::task::generation::Outcome::Failed
            );
            let evidence = source.metadata.source_failure.unwrap();
            let (failure, _) = ApiError::interpret_source(
                if canonical.is_some() {
                    "openrouter"
                } else {
                    "chatgpt"
                },
                500,
                evidence,
                None,
                false,
            );
            assert_eq!(failure.recovery.class, Some(class));
            assert!(!failure.recovery.retryable);
            let standard = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
            let output = standard
                .encode_response(&source, &TargetContract::full())
                .unwrap();
            assert_eq!(output["status"], "failed");
            assert_eq!(output["error"]["code"], expected);
            assert_eq!(
                source
                    .semantic
                    .details()
                    .error
                    .as_ref()
                    .unwrap()
                    .code
                    .as_ref()
                    .unwrap()
                    .as_str(),
                code
            );
        }
    }

    #[tokio::test]
    async fn validated_failed_terminal_is_observed_before_eof_or_trailing_rejection() {
        for malformed in [false, true] {
            let mut response = crate::gateway::tests::responses_wire::subscription_response(true);
            let mut events =
                crate::gateway::tests::responses_wire::response_events(response.clone());
            response["status"] = serde_json::json!("failed");
            response["error"] = serde_json::json!({
                "code":"rate_limit_exceeded","message":"synthetic limit"
            });
            let last = events.last_mut().unwrap();
            last["type"] = serde_json::json!("response.failed");
            last["response"] = response;
            let mut wire = events
                .iter()
                .map(|event| {
                    format!(
                        "event: {}\ndata: {event}\n\n",
                        event["type"].as_str().unwrap()
                    )
                })
                .collect::<String>();
            if malformed {
                wire.push_str("data: {invalid}\n\n");
            }
            let chunks = futures_util::stream::once(std::future::ready(Ok::<_, std::io::Error>(
                Bytes::from(wire),
            )))
            .chain(futures_util::stream::pending());
            let upstream = reqwest::Response::from(
                axum::http::Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body(reqwest::Body::wrap_stream(chunks))
                    .unwrap(),
            );
            let client = crate::adapter::ClientAdapter::openai(
                GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None),
                TargetContract::full(),
            );
            let mut attempt = Attempt::new(
                GenerationCodec::new(GenerationApi::Responses, Dialect::Siwc, None),
                1 << 20,
                SseLimits::default(),
            );
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = ResponseDelivery::new(
                client,
                "fixture-model",
                SseLimits::default(),
                Default::default(),
                Obfuscation::Disabled,
            );
            let state = crate::gateway::body::Status::default();
            let (tx, _rx) = tokio::sync::mpsc::channel(1);
            let lane = Lane {
                tx: &tx,
                state: &state,
            };
            let mut trace = Trace::for_request(
                None,
                &axum::extract::Request::new(axum::body::Body::empty()),
            );
            {
                let producer = produce(
                    upstream,
                    "chatgpt",
                    &mut attempt,
                    &mut delivery,
                    &lane,
                    false,
                    false,
                    1 << 20,
                    None,
                    &mut trace,
                );
                tokio::pin!(producer);
                let polled = futures_util::poll!(&mut producer);
                if malformed {
                    assert!(matches!(polled, std::task::Poll::Ready(Err(_))));
                } else {
                    assert!(polled.is_pending());
                }
                let failure = state
                    .failures
                    .lock()
                    .unwrap()
                    .head
                    .expect("validated upstream failure");
                assert_eq!(
                    failure.error.recovery.class,
                    Some(crate::provider::RecoveryClass::RateLimit)
                );
            }
            assert!(
                attempt.response().is_err(),
                "failure evidence cannot certify EOF"
            );
        }
    }
}
