//! Opt-in bounded private context. Nothing is persisted by the application.
mod redaction;
#[cfg(test)]
mod tests;
use axum::body::{Body, Bytes};
use futures_util::Stream;
use opentelemetry::{
    Key,
    logs::{AnyValue, Severity},
};
use redaction::sanitize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

#[derive(Clone, Copy)]
pub(super) enum Boundary {
    ClientRequest,
    UpstreamRequest,
    UpstreamResponse,
    ClientResponse,
}
impl Boundary {
    fn index(self) -> usize {
        match self {
            Self::ClientRequest => 0,
            Self::UpstreamRequest => 1,
            Self::UpstreamResponse => 2,
            Self::ClientResponse => 3,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::ClientRequest => "client_request",
            Self::UpstreamRequest => "upstream_request",
            Self::UpstreamResponse => "upstream_response",
            Self::ClientResponse => "client_response",
        }
    }
}
#[derive(Clone)]
pub(super) struct Sink(usize);
impl Sink {
    pub(super) fn new(limit: usize) -> Self {
        Self(limit)
    }
    pub(super) fn exchange(&self) -> Exchange {
        Exchange(Arc::new(ExchangeState {
            limit: self.0,
            context: Mutex::new(Captured::default()),
        }))
    }
}
#[derive(Default)]
struct Slot {
    units: VecDeque<String>,
    retained: usize,
    observed: u64,
    truncated: bool,
    incomplete: bool,
    redacted: bool,
    eof: bool,
    omission: Option<&'static str>,
}
type Attributes = Vec<(Key, AnyValue)>;
struct Followup {
    sink: super::diagnostics::Sink,
    attributes: Attributes,
    severity: Severity,
}
#[derive(Default)]
struct Captured {
    slots: [Slot; 4],
    attempt: Option<usize>,
    followup: Option<Followup>,
}
struct ExchangeState {
    limit: usize,
    context: Mutex<Captured>,
}
#[derive(Clone)]
pub(super) struct Exchange(Arc<ExchangeState>);
impl Exchange {
    pub(super) fn begin(&self, attempt: usize) {
        let mut context = self.0.context.lock().expect("private diagnostic context");
        if context.attempt != Some(attempt) {
            context.slots[1] = Slot::default();
            context.slots[2] = Slot::default();
            context.attempt = Some(attempt);
        }
    }
    pub(super) fn followup(
        &self,
        sink: super::diagnostics::Sink,
        mut attributes: Attributes,
        severity: Severity,
    ) {
        // Client output belongs to the request, not necessarily the failed send.
        attributes.retain(|(key, _)| {
            !matches!(
                key.as_str(),
                "attempt" | "provider" | "account" | "upstream_model" | "stage"
            )
        });
        attributes.push(("stage".into(), AnyValue::from("client_response")));
        self.0
            .context
            .lock()
            .expect("private diagnostic context")
            .followup = Some(Followup {
            sink,
            attributes,
            severity,
        });
    }
    pub(super) fn context(&self) -> Vec<(&'static str, AnyValue)> {
        let context = self.0.context.lock().expect("private diagnostic context");
        [
            Boundary::ClientRequest,
            Boundary::UpstreamRequest,
            Boundary::UpstreamResponse,
            Boundary::ClientResponse,
        ]
        .map(|boundary| {
            (
                boundary.label(),
                slot_body(&context.slots[boundary.index()]),
            )
        })
        .into()
    }
    fn client_finished(&self) {
        let (followup, body) = {
            let mut context = self.0.context.lock().expect("private diagnostic context");
            (context.followup.take(), slot_body(&context.slots[3]))
        };
        if let Some(followup) = followup {
            followup.sink.emit(
                "gateway.failure.response",
                followup.severity,
                followup.attributes,
                [("client_response", body)].into_iter().collect(),
            );
        }
    }
    pub(super) fn upstream_request(
        &self,
        prepared: &crate::execution::UpstreamRequest,
        attempt: usize,
    ) {
        self.begin(attempt);
        self.json(Boundary::UpstreamRequest, Some(attempt), &prepared.body);
    }
    fn emit(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        status: &'static str,
        value: Option<Value>,
        redacted: bool,
    ) {
        let mut context = self.0.context.lock().expect("private diagnostic context");
        if attempt.is_some() && attempt != context.attempt {
            return;
        }
        let slot = &mut context.slots[boundary.index()];
        slot.redacted |= redacted;
        match status {
            "source_eof" => slot.eof = true,
            "body_unit" => {
                let mut text =
                    serde_json::to_string(&value.expect("sanitized body")).expect("JSON body");
                let budget = self.0.limit / 4;
                if text.len() > budget {
                    let mut end = budget;
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    text.truncate(end);
                    text.shrink_to_fit();
                    slot.truncated = true;
                }
                while slot.units.len() >= 4 || slot.retained + text.len() > budget {
                    let old = slot.units.pop_front().expect("retained units");
                    slot.retained -= old.len();
                    slot.truncated = true;
                }
                slot.retained += text.len();
                slot.units.push_back(text);
            }
            "body_truncated" => {
                slot.truncated = true;
                slot.incomplete = true;
                slot.omission.get_or_insert(status);
            }
            _ => {
                slot.incomplete = true;
                slot.omission.get_or_insert(status);
            }
        }
    }
    fn observed(&self, boundary: Boundary, attempt: Option<usize>, bytes: usize) {
        let mut context = self.0.context.lock().expect("private diagnostic context");
        if attempt.is_none() || attempt == context.attempt {
            let slot = &mut context.slots[boundary.index()];
            slot.observed = slot.observed.saturating_add(bytes as u64);
        }
    }
    pub(super) fn json(&self, boundary: Boundary, attempt: Option<usize>, bytes: &[u8]) -> bool {
        self.observed(boundary, attempt, bytes.len());
        self.json_unit(boundary, attempt, bytes, true)
    }
    fn json_unit(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        bytes: &[u8],
        report_gap: bool,
    ) -> bool {
        if bytes.len() > self.0.limit {
            if report_gap {
                self.emit(boundary, attempt, "body_truncated", None, false);
            }
            return false;
        }
        let Ok(mut value) = crate::protocol::json::decode(bytes) else {
            if report_gap {
                self.emit(boundary, attempt, "unsafe_body", None, false);
            }
            return false;
        };
        // A fragment of serialized arguments is not an independently sanitizable unit.
        let kind = value
            .get("type")
            .or_else(|| value.get("event_type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if kind.contains("arguments.delta")
            || kind == "arguments_delta"
            || value.pointer("/delta/type").and_then(Value::as_str) == Some("input_json_delta")
            || value.pointer("/delta/type").and_then(Value::as_str) == Some("arguments_delta")
            || value
                .get("choices")
                .and_then(Value::as_array)
                .is_some_and(|choices| {
                    choices.iter().any(|choice| {
                        choice.pointer("/delta/tool_calls").is_some()
                            || choice.pointer("/delta/function_call").is_some()
                    })
                })
        {
            if report_gap {
                self.emit(boundary, attempt, "unsafe_fragment", None, false);
            }
            return false;
        }
        let mut redacted = false;
        if !sanitize(&mut value, &mut redacted, 0) {
            if report_gap {
                self.emit(boundary, attempt, "unsafe_body", None, false);
            }
            return false;
        }
        self.emit(boundary, attempt, "body_unit", Some(value), redacted);
        true
    }
    pub(super) fn body(
        &self,
        boundary: Boundary,
        attempt: Option<usize>,
        content_type: &str,
    ) -> BodyCapture {
        let media = content_type.parse::<mime::Mime>().ok();
        let essence = media.as_ref().map(mime::Mime::essence_str).unwrap_or("");
        let mode = if essence == "text/event-stream" {
            Mode::Sse(crate::transport::sse::SseDecoder::new(self.0.limit))
        } else if essence == "application/json" || essence.ends_with("+json") {
            Mode::Json(vec![])
        } else {
            Mode::Unsupported
        };
        BodyCapture {
            exchange: self.clone(),
            boundary,
            attempt,
            mode,
            ended: false,
            rejected: false,
        }
    }
}
enum Mode {
    Json(Vec<u8>),
    Sse(crate::transport::sse::SseDecoder),
    Unsupported,
}
pub(super) struct BodyCapture {
    exchange: Exchange,
    boundary: Boundary,
    attempt: Option<usize>,
    mode: Mode,
    ended: bool,
    rejected: bool,
}
impl BodyCapture {
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        self.exchange
            .observed(self.boundary, self.attempt, bytes.len());
        if self.rejected && !matches!(self.mode, Mode::Sse(_)) {
            return;
        }
        match &mut self.mode {
            Mode::Json(buffer) => {
                if bytes.len() > (self.exchange.0.limit).saturating_sub(buffer.len()) {
                    buffer.clear();
                    self.rejected = true;
                    self.exchange
                        .emit(self.boundary, self.attempt, "body_truncated", None, false);
                } else {
                    buffer.extend_from_slice(bytes);
                }
            }
            Mode::Sse(decoder) => {
                let mut rest = bytes;
                while !rest.is_empty() {
                    match decoder.push_until_event(rest) {
                        Ok((event, used)) if used > 0 => {
                            rest = &rest[used..];
                            if let Some(event) = event
                                && event.data() != "[DONE]"
                            {
                                // One sticky gap marker leaves the bounded quota
                                // available for later complete, sanitized units.
                                self.rejected |= !self.exchange.json_unit(
                                    self.boundary,
                                    self.attempt,
                                    event.data().as_bytes(),
                                    !self.rejected,
                                );
                            }
                        }
                        _ => {
                            self.rejected = true;
                            self.exchange.emit(
                                self.boundary,
                                self.attempt,
                                "unsafe_stream",
                                None,
                                false,
                            );
                            // Framing cannot recover. A rejected JSON unit above
                            // only marks a coverage gap; later framed units remain safe to inspect.
                            self.mode = Mode::Unsupported;
                            break;
                        }
                    }
                }
            }
            Mode::Unsupported => {
                self.rejected = true;
                self.exchange
                    .emit(self.boundary, self.attempt, "unsupported_body", None, false);
            }
        }
    }
    pub(super) fn finish(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        if let Mode::Json(buffer) = &self.mode
            && !self.rejected
        {
            self.rejected = !self
                .exchange
                .json_unit(self.boundary, self.attempt, buffer, true);
        }
        if let Mode::Sse(decoder) = &mut self.mode
            && decoder.finish_strict().is_err()
        {
            self.rejected = true;
        }
        self.exchange.emit(
            self.boundary,
            self.attempt,
            if self.rejected {
                "incomplete"
            } else {
                "source_eof"
            },
            None,
            false,
        );
        if matches!(self.boundary, Boundary::ClientResponse) {
            self.exchange.client_finished();
        }
    }
}
impl Drop for BodyCapture {
    fn drop(&mut self) {
        if !self.ended {
            self.exchange
                .emit(self.boundary, self.attempt, "incomplete", None, false);
            if matches!(self.boundary, Boundary::ClientResponse) {
                self.exchange.client_finished();
            }
        }
    }
}
struct ObservedBody {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, axum::Error>> + Send>>,
    capture: BodyCapture,
}
impl Stream for ObservedBody {
    type Item = Result<Bytes, axum::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                self.capture.feed(&bytes);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.capture.rejected = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.capture.finish();
                Poll::Ready(None)
            }
            other => other,
        }
    }
}
pub(super) fn observe(body: Body, capture: BodyCapture) -> Body {
    Body::from_stream(ObservedBody {
        stream: Box::pin(body.into_data_stream()),
        capture,
    })
}

fn slot_body(slot: &Slot) -> AnyValue {
    [
        (
            "snippet",
            AnyValue::from(
                slot.units
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ),
        (
            "observed_bytes",
            AnyValue::from(slot.observed.min(i64::MAX as u64) as i64),
        ),
        ("retained_bytes", AnyValue::from(slot.retained as i64)),
        ("truncated", AnyValue::from(slot.truncated)),
        ("incomplete", AnyValue::from(slot.incomplete)),
        ("redacted", AnyValue::from(slot.redacted)),
        ("eof", AnyValue::from(slot.eof)),
        ("omission", AnyValue::from(slot.omission.unwrap_or("none"))),
    ]
    .into_iter()
    .collect()
}
