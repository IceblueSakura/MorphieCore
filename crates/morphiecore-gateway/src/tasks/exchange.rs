//! One-attempt standalone HTTP mechanics. Codecs and publication remain operation-owned.
#[cfg(test)]
mod tests;
use super::BoundTask;
use crate::{
    ApiError, Runtime,
    capture::{BodyCapture, Boundary},
    diagnostics::{Stage, Trace, upstream::Response},
    execution::{AttemptError, attempt::ProtocolReason, task::PreparedTaskRequest},
    protocol::failure::{Observation, Resource, SourceFailureEvidence},
    transport::http::TransportError,
};
use tokio::time::Instant;

pub(super) async fn send<R>(
    runtime: &Runtime,
    entry: &BoundTask<R>,
    prepared: PreparedTaskRequest,
    deadline: Instant,
    limit: usize,
    trace: &mut Trace,
) -> Result<Receiver, ApiError> {
    trace.stage(Stage::Prepare);
    trace.executor(entry.provider.id.as_str());
    trace.named_account(entry.account.as_deref());
    let (secret, grant) = entry.secret.resolve_for_request().await?;
    let revision = grant.as_ref().map(|grant| grant.material_revision);
    if let (Some(scopes), Some(revision)) = (&entry.health, revision) {
        runtime.executors.verified_material(scopes, revision);
    }
    let mut prepared = prepared
        .authorize(&entry.provider, &secret)
        .map_err(|error| {
            trace.report.failure(None, &error);
            ApiError::invalid()
        })?;
    if let Some(grant) = grant {
        prepared.safe_headers.extend(
            crate::credential::subscription::headers(entry.provider.auth, &grant)
                .map_err(|_| ApiError::upstream())?,
        );
    }

    if Instant::now() >= deadline {
        return Err(ApiError::timeout());
    }
    trace.stage(Stage::Connect);
    if let Some(capture) = &trace.capture {
        capture.upstream_request(&prepared, trace.capture_attempt);
    }
    trace.sent();

    let upstream = trace
        .send_http(
            &runtime.transport,
            prepared,
            deadline.saturating_duration_since(Instant::now()),
        )
        .await
        .map_err(|error| {
            trace.report.failure(None, &AttemptError::Transport(error));
            ApiError::transport_fact(error)
        });
    let result = upstream.and_then(|upstream| Receiver::new(upstream, limit, trace));
    if let (Some(scopes), Err(error)) = (&entry.health, &result)
        && error.recovery.affects_health
    {
        let now = Instant::now();
        let failure = crate::routing::executors::FailureObservation {
            error: *error,
            at: now,
            material_revision: revision,
        };
        runtime.executors.record_target_failures(
            entry.provider.id.as_str(),
            scopes,
            &[failure],
            now,
        );
    }
    result
}

/// Bounded receive state only: no retries, semantic terminal inference or delivery.
pub(super) struct Receiver {
    upstream: Response,
    media_type: String,
    length: Option<u64>,
    limit: usize,
    received: usize,
    capture: Option<BodyCapture>,
    stream_capture: bool,
}
impl Receiver {
    pub(super) fn new(
        upstream: impl Into<Response>,
        limit: usize,
        trace: &mut Trace,
    ) -> Result<Self, ApiError> {
        let upstream = upstream.into();
        trace.stage(Stage::ResponseHead);
        let status = upstream.status().as_u16();
        trace.head(status);
        if status != 200 {
            let source = SourceFailureEvidence::http(status, b"", Observation::IntentionallyUnread);
            trace.report.source_failure(source);
            // Never wait for an optional error body after an authoritative failed head.
            return Err(ApiError::classified(
                status,
                source.kind,
                crate::error::retry_after(upstream.headers()),
            ));
        }
        let invalid = |trace: &mut Trace, reason| {
            trace.report.failure(None, &AttemptError::Protocol(reason));
            ApiError::upstream()
        };
        let headers = upstream.headers();
        let mut types = headers.get_all("content-type").iter();
        let media_type = types
            .next()
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| invalid(trace, ProtocolReason::UnexpectedResponseMediaType))?
            .to_owned();
        if types.next().is_some() || headers.contains_key("content-encoding") {
            return Err(invalid(trace, ProtocolReason::UnexpectedResponseMediaType));
        }
        let mut lengths = headers.get_all("content-length").iter();
        let length = lengths
            .next()
            .map(|value| {
                let raw = value
                    .to_str()
                    .map_err(|_| invalid(trace, ProtocolReason::ResponseLength))?;
                if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(invalid(trace, ProtocolReason::ResponseLength));
                }
                raw.parse::<u64>()
                    .map_err(|_| invalid(trace, ProtocolReason::ResponseLength))
            })
            .transpose()?;
        if lengths.next().is_some() || length.is_some_and(|n| n == 0 || n > limit as u64) {
            return Err(invalid(trace, ProtocolReason::ResponseLength));
        }
        let media: mime::Mime = media_type
            .parse()
            .map_err(|_| invalid(trace, ProtocolReason::UnexpectedResponseMediaType))?;
        let capture = trace.capture.as_ref().map(|capture| {
            capture.body(
                Boundary::UpstreamResponse,
                Some(trace.capture_attempt),
                media.essence_str(),
            )
        });
        let stream_capture = media.essence_str() == "text/event-stream";
        trace.stage(Stage::Intake);
        Ok(Self {
            upstream,
            media_type,
            length,
            limit,
            received: 0,
            capture,
            stream_capture,
        })
    }
    pub(super) fn media_type(&self) -> &str {
        &self.media_type
    }
    pub(super) fn require_media(&self, expected: &str) -> Result<(), ApiError> {
        let media: mime::Mime = self.media_type.parse().map_err(|_| ApiError::upstream())?;
        if media.essence_str() != expected
            || media
                .params()
                .any(|(key, value)| key != mime::CHARSET || value != mime::UTF_8)
        {
            return Err(ApiError::upstream());
        }
        Ok(())
    }
    pub(super) async fn next(
        &mut self,
        trace: &mut Trace,
    ) -> Result<Option<bytes::Bytes>, ApiError> {
        let chunk = self.upstream.chunk().await.map_err(|error| {
            let cause = if error.is_timeout() {
                TransportError::Timeout
            } else {
                TransportError::Read
            };
            trace.report.failure(None, &AttemptError::Transport(cause));
            if error.is_timeout() {
                ApiError::timeout()
            } else {
                ApiError::upstream()
            }
        })?;
        if let Some(chunk) = &chunk {
            self.received = self
                .received
                .checked_add(chunk.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| {
                    trace
                        .report
                        .failure(None, &AttemptError::Resource(Resource::Bytes));
                    ApiError::upstream()
                })?;

            if self.stream_capture
                && let Some(capture) = &mut self.capture
            {
                capture.feed(chunk);
            }
        } else {
            trace.stage(Stage::Terminal);
            if self
                .length
                .is_some_and(|length| length != self.received as u64)
            {
                trace.report.failure(
                    None,
                    &AttemptError::Protocol(ProtocolReason::ResponseLength),
                );
                return Err(ApiError::upstream());
            }

            if self.stream_capture
                && let Some(capture) = &mut self.capture
            {
                capture.finish();
            }
        }
        Ok(chunk)
    }
    pub(super) async fn collect(mut self, trace: &mut Trace) -> Result<Vec<u8>, ApiError> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.next(trace).await? {
            bytes.extend_from_slice(&chunk);
        }
        if !self.stream_capture
            && let Some(capture) = &mut self.capture
        {
            capture.feed(&bytes);
            capture.finish();
        }
        Ok(bytes)
    }
}

pub(super) fn parse_sse_json(
    event: &crate::protocol::sse::SseEvent,
    allowed_names: &[&str],
    check_openai_type: bool,
) -> Result<serde_json::Value, ApiError> {
    let name = event.event();
    if let Some(name) = name
        && name != "message"
        && !allowed_names.contains(&name)
    {
        return Err(ApiError::upstream());
    }
    let value =
        crate::protocol::json::decode(event.data().as_bytes()).map_err(|_| ApiError::upstream())?;
    if check_openai_type
        && let Some(name) = name
        && name != "message"
        && value.get("type").and_then(serde_json::Value::as_str) != Some(name)
    {
        return Err(ApiError::upstream());
    }
    Ok(value)
}

pub(super) enum SseStep {
    Ignore,
    EmitDelta(bytes::Bytes),
    HoldTerminal(bytes::Bytes),
}

pub(super) async fn stream_sse<D>(
    mut receiver: Receiver,
    limits: &crate::Limits,
    lane: &crate::body::Lane<'_>,
    trace: &mut Trace,
    mut decoder: D,
    mut on_event: impl FnMut(&mut D, &crate::protocol::sse::SseEvent) -> Result<SseStep, ApiError>,
    finish: impl FnOnce(&D) -> Result<(), ApiError>,
) -> Result<(), ApiError> {
    receiver.require_media("text/event-stream")?;
    let mut framer = crate::protocol::sse::SseDecoder::new(limits.event_bytes);
    let mut events = 0usize;
    let mut terminal = None;
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
                if events > limits.max_events || event.data().is_empty() || terminal.is_some() {
                    return Err(ApiError::upstream());
                }
                match on_event(&mut decoder, &event)? {
                    SseStep::Ignore => {}
                    SseStep::EmitDelta(bytes) => {
                        crate::body::send_frame(lane, bytes, trace).await?;
                    }
                    SseStep::HoldTerminal(bytes) => {
                        terminal = Some(bytes);
                    }
                }
            }
        }
    }
    trace.stage(Stage::Terminal);
    framer.finish_strict().map_err(|_| ApiError::upstream())?;
    finish(&decoder)?;
    let final_bytes = terminal.ok_or_else(ApiError::upstream)?;
    trace.stage(Stage::Projection);
    crate::body::send_final_frame(lane, final_bytes, trace).await?;
    Ok(())
}
