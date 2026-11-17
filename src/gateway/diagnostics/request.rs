//! Request spans follow body consumption, never just handler completion.
use super::Sink;
use axum::{
    body::{Body, HttpBody},
    response::Response,
};
use bytes::Bytes;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tracing::Span;

pub(in crate::gateway) struct RequestTrace {
    span: Option<Span>,
    dispatch: tracing::Dispatch,
    status: Option<u16>,
}
impl RequestTrace {
    pub(in crate::gateway) fn new(
        sink: &Sink,
        operation: &'static str,
        request_id: Option<&str>,
    ) -> Self {
        let dispatch = sink.dispatch();
        let span = tracing::dispatcher::with_default(&dispatch, || {
            let span = tracing::info_span!(target: "morphiecore.request", parent: None, "gateway.request",
                otel.kind = "server",
                otel.status_code = tracing::field::Empty,
                "morphiecore.operation" = operation,
                "morphiecore.request_id" = request_id,
                "http.response.status_code" = tracing::field::Empty,
                "morphiecore.delivery.outcome" = tracing::field::Empty);
            span.in_scope(|| tracing::info!(name: "gateway.http.started", target: "morphiecore.request",
                { morphiecore.operation = operation, morphiecore.request_id = request_id }, "request started"));
            span
        });
        Self {
            span: Some(span),
            dispatch,
            status: None,
        }
    }
    pub(in crate::gateway) fn span(&self) -> Span {
        self.span.as_ref().expect("active request").clone()
    }
    pub(in crate::gateway) fn response(mut self, response: Response) -> Response {
        self.status = Some(response.status().as_u16());
        self.span
            .as_ref()
            .expect("active request")
            .record("http.response.status_code", response.status().as_u16());
        let (parts, body) = response.into_parts();
        if body.is_end_stream() {
            self.finish("body_complete");
            return Response::from_parts(parts, body);
        }
        Response::from_parts(parts, Body::new(Observed { body, trace: self }))
    }
    fn finish(&mut self, outcome: &'static str) {
        let Some(span) = self.span.take() else {
            return;
        };
        tracing::dispatcher::with_default(&self.dispatch, || {
            span.in_scope(|| {
                span.record("morphiecore.delivery.outcome", outcome);
                if self.status.is_some_and(|status| status >= 500) || outcome == "body_error" {
                    span.record("otel.status_code", "ERROR");
                }
                tracing::info!(name: "gateway.http.finished", target: "morphiecore.request",
                    { http.response.status_code = self.status, morphiecore.delivery.outcome = outcome }, "request ended");
            });
            drop(span);
        });
    }
}
impl Drop for RequestTrace {
    fn drop(&mut self) {
        // No EOF observation means consumer closure, not proof of inference cancellation.
        self.finish("consumer_closed");
    }
}
struct Observed {
    body: Body,
    trace: RequestTrace,
}
impl HttpBody for Observed {
    type Data = Bytes;
    type Error = axum::Error;
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, axum::Error>>> {
        let this = self.get_mut();
        let result =
            tracing::dispatcher::with_default(&this.trace.dispatch, || match &this.trace.span {
                Some(span) => span.in_scope(|| Pin::new(&mut this.body).poll_frame(cx)),
                None => Pin::new(&mut this.body).poll_frame(cx),
            });
        match &result {
            Poll::Ready(Some(Err(_))) => this.trace.finish("body_error"),
            Poll::Ready(None) => this.trace.finish("body_complete"),
            Poll::Ready(Some(Ok(_))) if this.body.is_end_stream() => {
                this.trace.finish("body_complete")
            }
            _ => {}
        }
        result
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        self.body.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::LogLevel;
    use opentelemetry::trace::Status;

    #[tokio::test]
    async fn body_end_and_consumer_drop_close_spans_without_changing_payloads() {
        // WARN logging must not disable INFO-level server spans.
        let (sink, logs, spans) = Sink::memory_signals(LogLevel::Warn);
        for (index, consume) in [true, false].into_iter().enumerate() {
            let observation = RequestTrace::new(&sink, "responses", Some("synthetic-request"));
            let (send, receive) = tokio::sync::oneshot::channel::<Bytes>();
            let body = Body::from_stream(futures_util::stream::once(async move {
                receive
                    .await
                    .map_err(|_| std::io::Error::other("synthetic source closed"))
            }));
            let response = observation.response(Response::new(body));
            assert_eq!(spans.get_finished_spans().unwrap().len(), index);
            if consume {
                send.send(Bytes::from_static(b"synthetic-payload")).unwrap();
                assert_eq!(
                    axum::body::to_bytes(response.into_body(), 1024)
                        .await
                        .unwrap(),
                    "synthetic-payload"
                );
            } else {
                drop(response);
                assert!(send.send(Bytes::new()).is_err());
            }
        }
        let spans = spans.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 2);
        for (span, outcome) in spans.iter().zip(["body_complete", "consumer_closed"]) {
            assert_eq!(span.status, Status::Unset);
            assert!(
                span.attributes
                    .iter()
                    .any(|kv| kv.key.as_str() == "morphiecore.delivery.outcome"
                        && kv.value.as_str() == outcome)
            );
        }
        assert!(logs.get_emitted_logs().unwrap().is_empty());
    }

    #[tokio::test]
    async fn body_failure_is_closed_and_third_party_errors_never_enter_export() {
        let (sink, logs, spans) = Sink::memory_signals(LogLevel::Info);
        let observation = RequestTrace::new(&sink, "responses", Some("synthetic-request"));
        tracing::dispatcher::with_default(&sink.dispatch(), || {
            observation.span().in_scope(|| {
                tracing::error!(target: "reqwest", "synthetic-foreign-secret");
            });
        });
        let body = Body::from_stream(futures_util::stream::once(async {
            Err::<Bytes, _>(std::io::Error::other("synthetic-body-secret"))
        }));
        let response = observation.response(Response::new(body));
        assert!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .is_err()
        );
        let spans = spans.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert!(matches!(spans[0].status, Status::Error { .. }));
        let logs = logs.get_emitted_logs().unwrap();
        assert_eq!(logs.len(), 2);
        for log in &logs {
            let context = log.record.trace_context().unwrap();
            assert_eq!(context.trace_id, spans[0].span_context.trace_id());
            assert_eq!(context.span_id, spans[0].span_context.span_id());
        }
        let exported = format!("{logs:?} {spans:?}");
        assert!(!exported.contains("synthetic-foreign-secret"));
        assert!(!exported.contains("synthetic-body-secret"));
        assert!(exported.contains("body_error"));
    }
}
