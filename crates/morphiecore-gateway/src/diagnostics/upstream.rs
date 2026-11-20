//! HTTP observation owns neither transport policy nor semantic response validity.
use super::Trace;
use crate::transport::http::{HttpTransport, TransportError, UpstreamRequest};
use opentelemetry::trace::Status;
use std::time::Duration;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct Exchange(Span);

impl Exchange {
    fn finish(&mut self, outcome: &'static str, error: Option<TransportError>) {
        let span = std::mem::replace(&mut self.0, Span::none());
        if let Some(dispatch) = span.with_subscriber(|(_, dispatch)| dispatch.clone()) {
            tracing::dispatcher::with_default(&dispatch, || {
                span.set_attribute("morphiecore.io.outcome", outcome);
                if let Some(error) = error {
                    span.set_attribute("error.type", error.code());
                    span.set_status(Status::error(error.code()));
                }
                drop(span);
            });
        }
    }
}

impl Drop for Exchange {
    fn drop(&mut self) {
        // Drop can mean cancellation, a rejected head, or an intentionally
        // unread body. None of those is an observed EOF.
        self.finish("not_drained", None);
    }
}

pub(crate) struct Response {
    inner: reqwest::Response,
    exchange: Exchange,
}

#[cfg(test)]
impl From<reqwest::Response> for Response {
    fn from(inner: reqwest::Response) -> Self {
        Self {
            inner,
            exchange: Exchange(Span::none()),
        }
    }
}

impl Response {
    pub(crate) fn status(&self) -> reqwest::StatusCode {
        self.inner.status()
    }

    pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
        self.inner.headers()
    }

    pub(crate) async fn chunk(&mut self) -> Result<Option<bytes::Bytes>, reqwest::Error> {
        let result = self.inner.chunk().await;
        match &result {
            Ok(Some(_)) => {}
            Ok(None) => self.exchange.finish("eof", None),
            Err(error) => self
                .exchange
                .finish("read_error", Some(TransportError::from_reqwest(error))),
        }
        result
    }
}

impl Trace {
    pub(crate) async fn send_http(
        &self,
        transport: &HttpTransport,
        prepared: UpstreamRequest,
        timeout: Duration,
    ) -> Result<Response, TransportError> {
        let mut exchange = Exchange(self.sink.as_ref().map_or_else(Span::none, |sink| {
            tracing::dispatcher::with_default(&sink.dispatch(), || {
                tracing::info_span!(target: "morphiecore.transport", parent: self.current_span(),
                    "gateway.upstream.http", otel.kind = "client")
            })
        }));
        match transport.send(prepared, timeout).await {
            Ok(inner) => {
                exchange.0.set_attribute(
                    "http.response.status_code",
                    i64::from(inner.status().as_u16()),
                );
                Ok(Response { inner, exchange })
            }
            Err(error) => {
                exchange.finish("send_error", Some(error));
                Err(error)
            }
        }
    }
}
