//! One acknowledged frame in flight. A chain owns one permit/deadline/trace;
//! each candidate owns a fresh decoder/delivery, and visibility forbids advancement.
#[cfg(test)]
mod tests;
use crate::{
    execution::ResponseDelivery,
    gateway::{
        ApiError, Limits,
        diagnostics::{Outcome, Stage, Trace},
    },
};
use axum::{
    body::{Body, Bytes},
    response::Response,
};
use futures_util::Stream;
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc, oneshot},
    task::AbortHandle,
    time::Instant,
};
pub(super) struct Chunk {
    bytes: Bytes,
    ack: oneshot::Sender<()>,
    evidence_ack: Option<crate::execution::report::FrameAck>,
    audio_content_type: Option<&'static str>,
    history: Option<Arc<Mutex<Option<crate::gateway::generation::history::Reservation>>>>,
}
/// The producer can revoke a queued reservation without another consumer poll.
/// The same lock serializes revocation with final acknowledgment/publication.
struct HistoryHandoff(Arc<Mutex<Option<crate::gateway::generation::history::Reservation>>>);
impl Drop for HistoryHandoff {
    fn drop(&mut self) {
        self.0.lock().expect("history handoff").take();
    }
}
pub(super) enum ResponseType {
    Json,
    EventStream,
    Audio,
}
pub(super) enum Message {
    Chunk(Chunk),
    Error(ApiError),
}
#[derive(Default)]
pub(super) struct FailureState {
    /// The first failure time survives later classification and error-body cleanup.
    pub(super) head: Option<crate::gateway::routing::executors::FailureObservation>,
    /// A later I/O interruption does not replace the classified upstream failure.
    interruption: Option<crate::gateway::routing::executors::FailureObservation>,
    pub(super) material_revision: Option<u64>,
    previous: Vec<crate::gateway::routing::executors::FailureObservation>,
}
#[derive(Default)]
pub(super) struct Status {
    error: Mutex<Option<ApiError>>,
    complete: AtomicBool,
    pub(super) published: AtomicBool,
    /// Explicit wait attribution; the last trace stage is not a wait state.
    pub(super) waiting_upstream: AtomicBool,
    pub(super) failures: Mutex<FailureState>,
    pub(super) recovery_ready: AtomicBool,
    pub(super) readiness: tokio::sync::Notify,
}
impl Status {
    pub(super) fn ready(&self) {
        self.recovery_ready.store(true, Ordering::Release);
        self.readiness.notify_waiters();
    }
    pub(super) fn retain_attempt(&self, error: ApiError) {
        let failures = self.take_failures(Some(error));
        self.failures.lock().expect("failure state").previous = failures;
    }
    pub(super) fn observe_failure(&self, error: ApiError) {
        let mut failures = self.failures.lock().expect("failure state");
        let material_revision = failures.material_revision;
        let observation = failures.head.get_or_insert_with(|| {
            crate::gateway::routing::executors::FailureObservation {
                error,
                at: Instant::now(),
                material_revision,
            }
        });
        observation.error = error;
    }
    pub(super) fn observe_interruption(&self, error: ApiError) {
        let mut failures = self.failures.lock().expect("failure state");
        let material_revision = failures.material_revision;
        failures.interruption.get_or_insert_with(|| {
            crate::gateway::routing::executors::FailureObservation {
                error,
                at: Instant::now(),
                material_revision,
            }
        });
    }
    pub(super) fn take_failures(
        &self,
        final_error: Option<ApiError>,
    ) -> Vec<crate::gateway::routing::executors::FailureObservation> {
        let mut state = self.failures.lock().expect("failure state");
        let mut failures = std::mem::take(&mut state.previous);
        failures.extend(state.head.take());
        failures.extend(state.interruption.take());
        if let Some(error) = final_error
            && error.recovery.affects_health
            && !failures.iter().any(|failure| failure.error == error)
        {
            failures.push(crate::gateway::routing::executors::FailureObservation {
                error,
                at: Instant::now(),
                material_revision: state.material_revision,
            });
        }
        failures
    }
    pub(super) fn timeout_error(&self) -> ApiError {
        let observed = {
            let failures = self.failures.lock().expect("failure state");
            failures.interruption.or(failures.head)
        };
        if let Some(failure) = observed {
            failure.error
        } else if self.waiting_upstream.load(Ordering::Acquire) {
            ApiError::transport_fact(crate::transport::http::TransportError::Timeout)
        } else {
            ApiError::timeout()
        }
    }
}
struct Output {
    first: Option<Chunk>,
    rx: mpsc::Receiver<Message>,
    status: Arc<Status>,
    abort: AbortHandle,
    ended: bool,
}
impl Stream for Output {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if self.status.error.lock().expect("delivery status").is_some() {
            self.ended = true;
            return Poll::Ready(Some(Err(std::io::Error::other("upstream stream failed"))));
        }
        let item = if let Some(first) = self.first.take() {
            Poll::Ready(Some(Message::Chunk(first)))
        } else {
            self.rx.poll_recv(cx)
        };
        match item {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Message::Chunk(chunk))) => {
                let mut history = chunk
                    .history
                    .as_ref()
                    .map(|h| h.lock().expect("history handoff"));
                let accepted = match chunk.evidence_ack {
                    Some(evidence) => evidence.acknowledge(|| chunk.ack.send(()).is_ok()),
                    None => chunk.ack.send(()).is_ok(),
                };
                if !accepted {
                    self.ended = true;
                    Poll::Ready(Some(Err(std::io::Error::other("delivery cancelled"))))
                } else {
                    if let Some(reservation) = history.as_mut().and_then(|h| h.take()) {
                        reservation.publish(Instant::now());
                    }
                    Poll::Ready(Some(Ok(chunk.bytes)))
                }
            }
            Poll::Ready(Some(Message::Error(_))) => {
                self.ended = true;
                Poll::Ready(Some(Err(std::io::Error::other("upstream stream failed"))))
            }
            Poll::Ready(None) => {
                self.ended = true;
                if self.status.complete.load(Ordering::Acquire) {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(std::io::Error::other("unfinished delivery"))))
                }
            }
        }
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
pub(super) struct Lane<'a> {
    pub(super) tx: &'a mpsc::Sender<Message>,
    pub(super) state: &'a Status,
}
pub(super) async fn send_frames(
    lane: &Lane<'_>,
    delivery: &mut ResponseDelivery,
    frames: crate::execution::report::EncodedBatch,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    for (bytes, evidence_ack) in frames.into_acknowledged_frames() {
        send_chunk(lane, bytes, None, None, evidence_ack, trace).await?;
        delivery.commit().map_err(|_| ApiError::upstream())?;
    }
    Ok(())
}
pub(super) async fn send_error_frames(
    lane: &Lane<'_>,
    frames: crate::execution::report::EncodedBatch,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    for (bytes, evidence_ack) in frames.into_acknowledged_frames() {
        send_chunk(lane, bytes, None, None, evidence_ack, trace).await?;
    }
    Ok(())
}
pub(super) async fn send_frame(
    lane: &Lane<'_>,
    bytes: Bytes,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    send_chunk(lane, bytes, None, None, None, trace).await
}
pub(super) async fn send_final_frames(
    lane: &Lane<'_>,
    delivery: &mut ResponseDelivery,
    frames: crate::execution::report::EncodedBatch,
    history: Option<crate::gateway::generation::history::Reservation>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let mut frames = frames.into_acknowledged_frames().peekable();
    if frames.peek().is_none() {
        return Err(ApiError::upstream());
    }
    let mut history = history;
    while let Some((bytes, evidence_ack)) = frames.next() {
        let final_history = if frames.peek().is_none() {
            history.take()
        } else {
            None
        };
        send_chunk(lane, bytes, None, final_history, evidence_ack, trace).await?;
        delivery.commit().map_err(|_| ApiError::upstream())?;
    }
    Ok(())
}
pub(super) async fn send_audio(
    lane: &Lane<'_>,
    audio: crate::semantic::value::AudioArtifact,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    let content_type = crate::protocol::openai::speech::response_content_type(&audio);
    send_chunk(
        lane,
        audio.into_data(),
        Some(content_type),
        None,
        None,
        trace,
    )
    .await
}
async fn send_chunk(
    lane: &Lane<'_>,
    bytes: Bytes,
    audio_content_type: Option<&'static str>,
    history: Option<crate::gateway::generation::history::Reservation>,
    evidence_ack: Option<crate::execution::report::FrameAck>,
    trace: &mut Trace,
) -> Result<(), ApiError> {
    trace.stage(Stage::Delivery);
    lane.state.waiting_upstream.store(false, Ordering::Release);
    let size = bytes.len();
    let (ack, seen) = oneshot::channel();
    let history =
        history.map(|reservation| HistoryHandoff(Arc::new(Mutex::new(Some(reservation)))));
    // Publication freezes advancement; acknowledgement alone proves body handoff.
    lane.state.published.store(true, Ordering::Release);
    trace.report.update(|a| a.published = true);
    lane.tx
        .send(Message::Chunk(Chunk {
            bytes,
            ack,
            evidence_ack,
            audio_content_type,
            history: history.as_ref().map(|h| h.0.clone()),
        }))
        .await
        .map_err(|_| {
            trace.report.failure(
                None,
                &crate::execution::AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::BodyConsumerClosed,
                ),
            );
            ApiError::upstream()
        })?;
    seen.await.map_err(|_| {
        trace.report.failure(
            None,
            &crate::execution::AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::BodyConsumerClosed,
            ),
        );
        ApiError::upstream()
    })?;
    trace.handed_off(size);
    Ok(())
}
/// The work behind one response body: Generation's attempt chain or a
/// standalone task exchange. The body owner only publishes what it produces.
pub(super) trait Produce: Send + 'static {
    fn produce(
        self,
        limits: &Limits,
        deadline: Instant,
        lane: &Lane<'_>,
        trace: &mut Trace,
    ) -> impl std::future::Future<Output = Result<(), ApiError>> + Send;
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn respond_producer(
    producer: impl Produce,
    response_type: ResponseType,
    limits: Limits,
    deadline: Instant,
    shutdown: tokio_util::sync::CancellationToken,
    permit: OwnedSemaphorePermit,
    mut trace: Trace,
) -> Result<Response, ApiError> {
    let (tx, mut rx) = mpsc::channel(1);
    let state = Arc::new(Status::default());
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        trace.outcome(Outcome::Interrupted);
        let _permit = permit;
        use crate::execution::{
            AttemptError,
            attempt::{DeadlineReason, DeliveryReason},
        };
        let lane = Lane {
            tx: &tx,
            state: &worker_state,
        };
        let result = tokio::select! {
            biased;
            _ = shutdown.cancelled() => Err(ApiError::shutdown()),
            _ = tx.closed() => {
                let closed = AttemptError::Delivery(DeliveryReason::BodyConsumerClosed);
                trace.report.failure(None, &closed);
                Err(ApiError::upstream())
            }
            result = producer.produce(&limits, deadline, &lane, &mut trace) => result,
            // Generation's own attempt deadline must record upstream evidence
            // before the outer safety deadline cancels its recovery lease.
            _ = tokio::time::sleep_until(deadline) => {
                let elapsed = AttemptError::Deadline(DeadlineReason::Exchange);
                trace.report.failure(None, &elapsed);
                Err(ApiError::timeout())
            }
        };
        match result {
            Ok(()) => {
                trace.stage(Stage::Complete);
                trace.outcome(Outcome::Complete);
                worker_state.complete.store(true, Ordering::Release);
            }
            Err(error) => {
                trace
                    .report
                    .request_failure(error.diagnostic_code(), error.status.as_u16());
                trace.outcome(match error.kind {
                    crate::gateway::error::ErrorKind::Timeout => Outcome::Timeout,
                    crate::gateway::error::ErrorKind::Shutdown => Outcome::Shutdown,
                    _ => Outcome::Error,
                });
                *worker_state.error.lock().expect("delivery status") = Some(error);
                let _ = tx.try_send(Message::Error(error));
            }
        }
    });
    let abort = worker.abort_handle();
    let first = match rx.recv().await {
        Some(Message::Chunk(chunk)) => {
            if let Some(error) = *state.error.lock().expect("delivery status") {
                return Err(error);
            }
            chunk
        }
        Some(Message::Error(error)) => return Err(error),
        _ => {
            return Err(state
                .error
                .lock()
                .expect("delivery status")
                .unwrap_or_else(ApiError::upstream));
        }
    };
    let content_type = match response_type {
        ResponseType::Audio => first.audio_content_type.ok_or_else(ApiError::upstream)?,
        ResponseType::Json => "application/json",
        ResponseType::EventStream => "text/event-stream",
    };
    // Conservatively freeze the candidate when HTTP response ownership leaves
    // this layer; encoding/queueing alone still never calls delivery.commit().
    state.published.store(true, Ordering::Release);
    let body = Body::from_stream(Output {
        first: Some(first),
        rx,
        status: state,
        abort,
        ended: false,
    });
    let mut response = Response::builder()
        .header("content-type", content_type)
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff");
    if matches!(response_type, ResponseType::EventStream) {
        response = response.header("x-accel-buffering", "no");
    }
    response.body(body).map_err(|_| ApiError::upstream())
}
