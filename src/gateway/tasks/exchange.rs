//! One-attempt standalone HTTP mechanics. Codecs and publication remain operation-owned.
#[cfg(test)]
mod tests;
use super::BoundTask;
use crate::{
    execution::{AttemptError, attempt::ProtocolReason, task::PreparedTaskRequest},
    gateway::{
        ApiError, Runtime,
        capture::{BodyCapture, Boundary},
        diagnostics::{Stage, Trace},
    },
    protocol::failure::{Observation, Resource, SourceFailureEvidence},
    transport::http::TransportError,
};
use tokio::time::Instant;

pub(super) async fn send<R>(
    runtime: &Runtime,
    entry: &BoundTask<R>,
    prepared: PreparedTaskRequest,
    deadline: Instant,
    trace: &mut Trace,
) -> Result<reqwest::Response, ApiError> {
    trace.stage(Stage::Prepare);
    trace.executor(entry.provider.id.as_str());
    let (secret, grant) = entry.secret.resolve_for_request().await?;
    if grant.is_some() {
        return Err(ApiError::upstream());
    }
    let prepared = prepared
        .authorize(&entry.provider, &secret)
        .map_err(|error| {
            trace.report.failure(None, &error);
            ApiError::invalid()
        })?;
    trace.report.update(|attempt| attempt.prepared = true);
    trace.stage(Stage::Connect);
    if let Some(capture) = &trace.capture {
        capture.upstream_request(&prepared, trace.capture_attempt);
    }
    trace.report.update(|attempt| attempt.send_started = true);
    runtime
        .transport
        .send(prepared, deadline.saturating_duration_since(Instant::now()))
        .await
        .map_err(|error| {
            trace.report.failure(None, &AttemptError::Transport(error));
            ApiError::transport(error.recovery_class())
        })
}

/// Bounded receive state only: no retries, semantic terminal inference or delivery.
pub(super) struct Receiver {
    upstream: reqwest::Response,
    media_type: String,
    length: Option<u64>,
    limit: usize,
    received: usize,
    capture: Option<BodyCapture>,
    stream_capture: bool,
}
impl Receiver {
    pub(super) fn new(
        upstream: reqwest::Response,
        limit: usize,
        trace: &mut Trace,
    ) -> Result<Self, ApiError> {
        trace.stage(Stage::ResponseHead);
        let status = upstream.status().as_u16();
        trace.head(status, upstream.headers());
        if let Some(capture) = &trace.capture {
            capture.head(
                Boundary::UpstreamResponse,
                Some(trace.capture_attempt),
                upstream.headers(),
                None,
            );
        }
        if status != 200 {
            let source = SourceFailureEvidence::http(status, b"", Observation::IntentionallyUnread);
            trace.report.source_failure(source);
            // Never wait for an optional error body after an authoritative failed head.
            return Err(ApiError::classified(
                status,
                source.kind,
                crate::gateway::error::retry_after(upstream.headers()),
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
        let capture = trace.capture.as_ref().and_then(|capture| {
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
            trace.received(chunk.len());
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
            trace.report.update(|attempt| attempt.source_eof = true);
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
