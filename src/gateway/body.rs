//! One acknowledged frame in flight. A chain owns one permit/deadline/trace;
//! each candidate owns a fresh decoder/delivery, and visibility forbids advancement.
#[cfg(test)]
mod tests;
#[cfg(test)]
use crate::gateway::BoundCandidate;
use crate::gateway::{
    ApiError, BoundEntry, Limits,
    diagnostics::{Outcome, Stage, Trace},
    generation::exchange::{Upstreams, produce_chain},
};
use crate::{adapter::Request, execution::ResponseDelivery};
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
    history: Option<crate::gateway::generation::history::Reservation>,
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
pub(super) struct Status {
    failed: AtomicBool,
    complete: AtomicBool,
    pub(super) published: AtomicBool,
    /// Explicit wait attribution; the last trace stage is not a wait state.
    pub(super) waiting_upstream: AtomicBool,
    /// Failed HTTP heads or stream failures survive interrupted error delivery.
    pub(super) failure_head: Mutex<Option<ApiError>>,
}
impl Status {
    pub(super) fn timeout_error(&self) -> ApiError {
        if let Some(failure) = *self.failure_head.lock().expect("failure head") {
            failure
        } else if self.waiting_upstream.load(Ordering::Acquire) {
            ApiError::transport(crate::provider::ErrorClass::Timeout)
        } else {
            ApiError::timeout()
        }
    }
}
struct Output {
    first: Option<Message>,
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
        if self.status.failed.load(Ordering::Acquire) {
            self.ended = true;
            return Poll::Ready(Some(Err(std::io::Error::other("upstream stream failed"))));
        }
        let item = if let Some(first) = self.first.take() {
            Poll::Ready(Some(first))
        } else {
            self.rx.poll_recv(cx)
        };
        match item {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Message::Chunk(chunk))) => {
                let accepted = match chunk.evidence_ack {
                    Some(evidence) => evidence.acknowledge(|| chunk.ack.send(()).is_ok()),
                    None => chunk.ack.send(()).is_ok(),
                };
                if !accepted {
                    self.ended = true;
                    Poll::Ready(Some(Err(std::io::Error::other("delivery cancelled"))))
                } else {
                    if let Some(history) = chunk.history {
                        history.publish(Instant::now());
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
    // Publication freezes advancement; acknowledgement alone proves body handoff.
    lane.state.published.store(true, Ordering::Release);
    trace.report.update(|a| a.published = true);
    lane.tx
        .send(Message::Chunk(Chunk {
            bytes,
            ack,
            evidence_ack,
            audio_content_type,
            history,
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
pub(super) enum Producer {
    Generation {
        entry: Arc<BoundEntry>,
        request: Box<Request>,
        source: Upstreams,
    },
    Image {
        runtime: Arc<crate::gateway::Runtime>,
        entry: Arc<crate::gateway::tasks::images::BoundImage>,
        request: crate::adapter::images::Request,
    },
    Speech {
        runtime: Arc<crate::gateway::Runtime>,
        entry: Arc<crate::gateway::tasks::speech::BoundSpeech>,
        request: crate::adapter::speech::Request,
    },
    Transcription {
        runtime: Arc<crate::gateway::Runtime>,
        entry: Arc<crate::gateway::tasks::transcription::BoundTranscription>,
        request: crate::adapter::transcription::Request,
    },
    Embedding {
        runtime: Arc<crate::gateway::Runtime>,
        entry: Arc<crate::gateway::tasks::embeddings::BoundEmbedding>,
        prepared: crate::execution::embeddings::PreparedEmbedding,
    },
}
impl Producer {
    async fn produce(
        self,
        limits: &Limits,
        deadline: Instant,
        tx: &mpsc::Sender<Message>,
        state: &Status,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        match self {
            Self::Generation {
                entry,
                request,
                source,
            } => produce_chain(source, &entry, &request, limits, deadline, tx, state, trace).await,
            Self::Embedding {
                runtime,
                entry,
                prepared,
            } => {
                crate::gateway::tasks::embeddings::produce(
                    runtime,
                    entry,
                    prepared,
                    deadline,
                    &Lane { tx, state },
                    trace,
                )
                .await
            }
            Self::Image {
                runtime,
                entry,
                request,
            } => {
                crate::gateway::tasks::images::produce(
                    runtime,
                    entry,
                    request,
                    deadline,
                    &Lane { tx, state },
                    trace,
                )
                .await
            }
            Self::Transcription {
                runtime,
                entry,
                request,
            } => {
                crate::gateway::tasks::transcription::produce(
                    runtime,
                    entry,
                    request,
                    deadline,
                    &Lane { tx, state },
                    trace,
                )
                .await
            }
            Self::Speech {
                runtime,
                entry,
                request,
            } => {
                crate::gateway::tasks::speech::produce(
                    runtime,
                    entry,
                    request,
                    deadline,
                    &Lane { tx, state },
                    trace,
                )
                .await
            }
        }
    }
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn respond(
    entry: Arc<BoundEntry>,
    candidate: Arc<BoundCandidate>,
    request: Request,
    upstream: reqwest::Response,
    limits: &Limits,
    deadline: Instant,
    shutdown: tokio_util::sync::CancellationToken,
    permit: OwnedSemaphorePermit,
    trace: Trace,
) -> Result<Response, ApiError> {
    respond_source(
        entry,
        request,
        Upstreams::Observed {
            candidate,
            response: upstream,
        },
        limits.clone(),
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn respond_source(
    entry: Arc<BoundEntry>,
    request: Request,
    source: Upstreams,
    limits: Limits,
    deadline: Instant,
    shutdown: tokio_util::sync::CancellationToken,
    permit: OwnedSemaphorePermit,
    trace: Trace,
) -> Result<Response, ApiError> {
    let stream = request.delivery.streaming();
    respond_producer(
        Producer::Generation {
            entry,
            request: Box::new(request),
            source,
        },
        if stream {
            ResponseType::EventStream
        } else {
            ResponseType::Json
        },
        limits,
        deadline,
        shutdown,
        permit,
        trace,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn respond_producer(
    producer: Producer,
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
        let result = tokio::select! {
            biased;
            _=shutdown.cancelled()=>Err(ApiError::shutdown()),
            _=tx.closed()=>{trace.report.failure(None,&crate::execution::AttemptError::Delivery(crate::execution::attempt::DeliveryReason::BodyConsumerClosed));Err(ApiError::upstream())},
            result=producer.produce(&limits,deadline,&tx,&worker_state,&mut trace)=>result,
            // Generation's own attempt deadline must record upstream evidence
            // before the outer safety deadline cancels its recovery lease.
            _=tokio::time::sleep_until(deadline)=>{trace.report.failure(None,&crate::execution::AttemptError::Deadline(crate::execution::attempt::DeadlineReason::Exchange));Err(ApiError::timeout())},
        };
        match result {
            Ok(()) => {
                trace.stage(Stage::Complete);
                trace.outcome(Outcome::Complete);
                worker_state.complete.store(true, Ordering::Release);
            }
            Err(error) => {
                trace.outcome(match error.code {
                    "upstream_timeout" => Outcome::Timeout,
                    "shutting_down" => Outcome::Shutdown,
                    _ => Outcome::Error,
                });
                worker_state.failed.store(true, Ordering::Release);
                let _ = tx.try_send(Message::Error(error));
            }
        }
    });
    let abort = worker.abort_handle();
    let first = match rx.recv().await {
        Some(Message::Chunk(chunk)) if !state.failed.load(Ordering::Acquire) => {
            Message::Chunk(chunk)
        }
        Some(Message::Error(error)) => return Err(error),
        _ => return Err(ApiError::upstream()),
    };
    let content_type = match (&response_type, &first) {
        (ResponseType::Audio, Message::Chunk(chunk)) => {
            chunk.audio_content_type.ok_or_else(ApiError::upstream)?
        }
        (ResponseType::Json, _) => "application/json",
        (ResponseType::EventStream, _) => "text/event-stream",
        _ => return Err(ApiError::upstream()),
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
    Response::builder()
        .header("content-type", content_type)
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .body(body)
        .map_err(|_| ApiError::upstream())
}
