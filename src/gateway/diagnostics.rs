//! Request-local failure attribution, independent of execution and optional export.
mod telemetry;
use crate::execution::AttemptError;
use opentelemetry::logs::{AnyValue, Severity};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub use telemetry::DiagnosticsConfig;
pub(super) use telemetry::Sink;

#[derive(Clone, Default)]
pub(super) struct Emitted(Arc<AtomicBool>);
impl Emitted {
    pub(super) fn observed(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
pub(super) fn operation(path: &str) -> &'static str {
    match path {
        "/v1/chat/completions" => "chat",
        "/v1/responses" => "responses",
        "/v1/messages" => "messages",
        "/v1/embeddings" => "embeddings",
        "/v1/images/generations" => "images",
        "/v1/audio/speech" => "speech",
        "/v1/audio/transcriptions" => "transcription",
        _ => "other",
    }
}

#[derive(Clone, Copy, Default, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(super) enum Stage {
    #[default]
    Admission,
    Prepare,
    Connect,
    ResponseHead,
    Intake,
    Terminal,
    Projection,
    Delivery,
    Complete,
    HttpResponse,
}
#[derive(Clone, Copy, Default)]
pub(super) enum Outcome {
    #[default]
    Error,
    Interrupted,
    Timeout,
    Shutdown,
    Complete,
}

pub(super) struct Trace {
    pub(super) report: crate::execution::report::Report,
    pub(super) capture: Option<super::capture::Exchange>,
    pub(super) capture_attempt: usize,
    sink: Option<Sink>,
    request_id: Option<String>,
    agent_id: Option<String>,
    operation: &'static str,
    provider: Option<String>,
    account: Option<String>,
    models: Option<(String, String)>,
    stage: Stage,
    outcome: Outcome,
    status: Option<u16>,
    active: bool,
    failure_logged: bool,
    emitted: Option<Emitted>,
}
impl Trace {
    pub(super) fn for_request(sink: Option<&Sink>, request: &axum::extract::Request) -> Self {
        let mut trace = Self::build(
            sink,
            request
                .extensions()
                .get::<super::ingress::correlation::RequestId>()
                .map(|id| id.0.to_string()),
            request
                .extensions()
                .get::<super::ingress::auth::AuthenticatedAgent>()
                .map(|agent| agent.agent_id.to_string()),
        );
        trace.capture = request
            .extensions()
            .get::<super::capture::Exchange>()
            .cloned();
        trace.emitted = request.extensions().get::<Emitted>().cloned();
        trace.operation = operation(request.uri().path());
        trace
    }
    #[cfg(test)]
    pub(super) fn new(sink: Option<&Sink>) -> Self {
        Self::build(
            sink,
            super::ingress::correlation::RequestIds::new()
                .unwrap()
                .next()
                .map(|id| id.0.to_string()),
            None,
        )
    }
    #[cfg(test)]
    pub(super) fn generation_trace(sink: Option<&Sink>) -> Self {
        Self::new(sink)
    }
    fn build(sink: Option<&Sink>, request_id: Option<String>, agent_id: Option<String>) -> Self {
        Self {
            report: Default::default(),
            capture: None,
            capture_attempt: 0,
            sink: sink.cloned(),
            request_id,
            agent_id,
            operation: "other",
            provider: None,
            account: None,
            models: None,
            stage: Stage::Admission,
            outcome: Outcome::Interrupted,
            status: None,
            active: false,
            failure_logged: false,
            emitted: None,
        }
    }
    pub(super) fn rejected(
        sink: &Sink,
        request_id: Option<&super::ingress::correlation::RequestId>,
        agent: &super::ingress::auth::AuthenticatedAgent,
        capture: Option<super::capture::Exchange>,
        operation: &'static str,
        code: &'static str,
        status: u16,
    ) {
        let mut trace = Self::build(
            Some(sink),
            request_id.map(|id| id.0.to_string()),
            Some(agent.agent_id.to_string()),
        );
        trace.capture = capture;
        trace.operation = operation;
        trace.stage = Stage::HttpResponse;
        trace.report.request_failure(code, status);
    }
    pub(super) fn begin_candidate(&mut self, ordinal: usize) {
        self.report.begin();
        if let Some(capture) = &self.capture {
            capture.begin(ordinal);
        }
        self.capture_attempt = ordinal;
        self.status = None;
        self.stage = Stage::Prepare;
        self.provider = None;
        self.account = None;
        self.active = true;
        self.failure_logged = false;
    }
    pub(super) fn end_candidate(&mut self, fallback: Option<&'static str>, retry: bool) {
        if !self.active {
            return;
        }
        self.active = false;
        if let Some(code) = self
            .failure_code()
            .or(fallback.filter(|code| *code != "interrupted"))
            && !self.cancelled_only()
        {
            self.emit_failure(code, retry);
        }
    }
    fn cancelled_only(&self) -> bool {
        let a = self.report.attempt();
        a.source_failure.is_none()
            && matches!(
                self.report.cause().map(|f| f.cause),
                Some(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::BodyConsumerClosed
                ))
            )
    }
    fn failure_code(&self) -> Option<&'static str> {
        let a = self.report.attempt();
        a.source_failure
            .filter(|source| source.kind != crate::protocol::failure::SourceFailureKind::Other)
            .map(|s| crate::execution::failure::source_code(&s.kind))
            .or_else(|| self.report.cause().map(|f| f.cause.code()))
            .or_else(|| {
                a.source_failure
                    .map(|s| crate::execution::failure::source_code(&s.kind))
            })
    }
    pub(super) fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }
    pub(super) fn executor(&mut self, provider: &str) {
        self.provider = Some(provider.to_owned());
    }
    pub(super) fn named_account(&mut self, account: Option<&str>) {
        self.account = account.map(str::to_owned);
    }
    pub(super) fn models(&mut self, public: &str, upstream: &str) {
        self.models = Some((public.to_owned(), upstream.to_owned()));
    }
    pub(super) fn stage(&mut self, stage: Stage) {
        self.stage = stage;
    }
    pub(super) fn outcome(&mut self, outcome: Outcome) {
        self.outcome = outcome;
    }
    pub(super) fn head(&mut self, status: u16) {
        self.status = Some(status);
    }
    pub(super) fn decode_error(&mut self, error: &AttemptError) {
        use crate::protocol::conversion::generation::ConversionBoundary;
        let boundary = if matches!(error, AttemptError::Sse(_)) {
            ConversionBoundary::EventDecode
        } else {
            ConversionBoundary::ResponseDecode
        };
        self.report.failure(Some(boundary), error);
    }
    pub(super) fn projection_error(
        &mut self,
        boundary: crate::protocol::conversion::generation::ConversionBoundary,
        error: &AttemptError,
    ) {
        self.report.failure(Some(boundary), error);
    }
    fn emit_failure(&mut self, code: &'static str, retry: bool) {
        use crate::protocol::conversion::generation::ConversionBoundary as Boundary;
        self.failure_logged = true;
        let Some(sink) = &self.sink else {
            return;
        };
        if let Some(emitted) = &self.emitted {
            emitted.0.store(true, Ordering::Relaxed);
        }
        let source = self.report.attempt().source_failure;
        let stage = if source.is_some() {
            Stage::Intake
        } else {
            match self.report.cause().and_then(|failure| failure.boundary) {
                Some(Boundary::RequestDecode) => Stage::Admission,
                Some(Boundary::RequestProject | Boundary::RequestEncode) => Stage::Prepare,
                Some(Boundary::ResponseDecode | Boundary::EventDecode) => Stage::Intake,
                Some(
                    Boundary::ResponseProject
                    | Boundary::ResponseEncode
                    | Boundary::EventProject
                    | Boundary::EventEncode,
                ) => Stage::Projection,
                None => self.stage,
            }
        };
        let failure_status = source
            .and_then(|s| s.status_hint)
            .map(|s| s.get())
            .or(self.status)
            .or(self.report.request_error().map(|(_, status)| status));
        let severity = if retry || failure_status.is_some_and(|s| (400..500).contains(&s)) {
            Severity::Warn
        } else {
            Severity::Error
        };
        let mut attributes = vec![
            ("error.type".into(), AnyValue::from(code)),
            ("operation".into(), AnyValue::from(self.operation)),
            ("stage".into(), AnyValue::from(<&'static str>::from(stage))),
            ("retry.planned".into(), AnyValue::from(retry)),
        ];
        for (name, value) in [
            ("request_id", self.request_id.as_deref()),
            ("agent_id", self.agent_id.as_deref()),
            ("provider", self.provider.as_deref()),
            ("account", self.account.as_deref()),
            ("model", self.models.as_ref().map(|m| m.0.as_str())),
            ("upstream_model", self.models.as_ref().map(|m| m.1.as_str())),
        ] {
            if let Some(value) = value {
                attributes.push((name.into(), AnyValue::from(value.to_owned())));
            }
        }
        if self.capture_attempt > 0 {
            attributes.push((
                "attempt".into(),
                AnyValue::from(self.capture_attempt as i64),
            ));
        }
        if matches!(self.outcome, Outcome::Complete) {
            attributes.push(("client.handoff_complete".into(), AnyValue::from(true)));
        }
        if let Some(status) = self.status {
            attributes.push((
                "http.response.status_code".into(),
                AnyValue::from(status as i64),
            ));
        }
        if let Some((_, status)) = self.report.request_error() {
            attributes.push((
                "gateway.error.status_code".into(),
                AnyValue::from(status as i64),
            ));
        }
        let mut body = vec![("message", AnyValue::from(code))];
        if let Some(capture) = &self.capture {
            body.extend(capture.context());
            capture.followup(sink.clone(), attributes.clone(), severity);
        }
        sink.emit(
            "gateway.failure",
            severity,
            attributes,
            body.into_iter().collect(),
        );
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        self.end_candidate(Some("interrupted"), false);
        if !self.failure_logged
            && !self.cancelled_only()
            && !matches!(self.outcome, Outcome::Shutdown)
            && let Some(code) = self
                .failure_code()
                .or(self.report.request_error().map(|(code, _)| code))
                .or(matches!(self.outcome, Outcome::Error | Outcome::Timeout)
                    .then_some("gateway.request_failed"))
        {
            self.emit_failure(code, false);
        }
    }
}

#[cfg(test)]
mod tests;
