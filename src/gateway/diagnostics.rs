//! Request-local attribution and bounded Generation timelines, independent of export.
mod telemetry;
use crate::execution::AttemptError;
use opentelemetry::logs::{AnyValue, Severity};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Instant,
};
pub(super) use telemetry::Sink;
pub use telemetry::{DiagnosticsConfig, LogLevel};
type Attributes = Vec<(opentelemetry::Key, AnyValue)>;

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
    public_model: Option<String>,
    upstream_model: Option<String>,
    stage: Stage,
    outcome: Outcome,
    status: Option<u16>,
    active: bool,
    failure_logged: bool,
    emitted: Option<Emitted>,
    started: Instant,
    attempt_started: Instant,
    sequence: AtomicI64,
    seen_stages: u16,
    sends: i64,
    terminal_validated: bool,
    finish_validated: bool,
    published: bool,
    first_handoff_ms: Option<i64>,
    final_handoff: bool,
    response_outcome: Option<&'static str>,
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
        trace.timeline("generation.request.started", Severity::Debug, vec![]);
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
            public_model: None,
            upstream_model: None,
            stage: Stage::Admission,
            outcome: Outcome::Interrupted,
            status: None,
            active: false,
            failure_logged: false,
            emitted: None,
            started: Instant::now(),
            attempt_started: Instant::now(),
            sequence: AtomicI64::new(0),
            seen_stages: 0,
            sends: 0,
            terminal_validated: false,
            finish_validated: false,
            published: false,
            first_handoff_ms: None,
            final_handoff: false,
            response_outcome: None,
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
        self.attempt_started = Instant::now();
        self.response_outcome = None;
        self.status = None;
        self.stage = Stage::Prepare;
        self.seen_stages = 0;
        self.terminal_validated = false;
        self.finish_validated = false;
        self.provider = None;
        self.account = None;
        self.upstream_model = None;
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
        self.report.attempt().source_failure.is_none()
            && matches!(
                self.report.cause().map(|f| f.cause),
                Some(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::BodyConsumerClosed
                ))
            )
    }
    fn failure_code(&self) -> Option<&'static str> {
        let source = self.report.attempt().source_failure;
        source
            .filter(|s| s.kind != crate::protocol::failure::SourceFailureKind::Other)
            .map(|s| crate::execution::failure::source_code(&s.kind))
            .or_else(|| self.report.cause().map(|f| f.cause.code()))
            .or_else(|| source.map(|s| crate::execution::failure::source_code(&s.kind)))
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
        self.public_model = Some(public.to_owned());
        self.upstream_model = Some(upstream.to_owned());
    }
    pub(super) fn generation(&mut self, public: &str, client: crate::protocol::ClientProtocol) {
        self.public_model = Some(public.to_owned());
        self.operation = match client {
            crate::protocol::ClientProtocol::Chat => "chat",
            crate::protocol::ClientProtocol::Responses => "responses",
            crate::protocol::ClientProtocol::Messages => "messages",
        };
    }
    pub(super) fn planning_failure(&mut self, cause: &crate::execution::plan::PlanError) {
        use crate::execution::plan::{PlanError, RejectionReason};
        let reasons: Vec<_> = match cause {
            PlanError::CandidateRejected(reason) => vec![reason],
            PlanError::NoCandidate { rejections } => rejections.iter().map(|r| &r.reason).collect(),
            _ => vec![],
        };
        for (index, reason) in reasons.into_iter().enumerate() {
            if index == 0
                && let RejectionReason::Conversion(failure) = reason
            {
                self.report.failure(
                    failure.boundary,
                    &AttemptError::Conversion(failure.as_ref().clone()),
                );
            }
            self.timeline(
                "generation.candidate.rejected",
                Severity::Debug,
                vec![
                    ("rejection.index".into(), AnyValue::from(index as i64)),
                    ("error.type".into(), AnyValue::from(reason.code())),
                ],
            );
        }
    }
    pub(super) fn stage(&mut self, stage: Stage) {
        self.stage = stage;
        let bit = 1 << stage as u16;
        if self.seen_stages & bit == 0 {
            self.seen_stages |= bit;
            self.timeline(
                "generation.stage",
                Severity::Debug,
                vec![("stage".into(), AnyValue::from(<&'static str>::from(stage)))],
            );
        }
    }
    pub(super) fn outcome(&mut self, outcome: Outcome) {
        self.outcome = outcome;
    }
    pub(super) fn head(&mut self, status: u16) {
        self.status = Some(status);
    }
    pub(super) fn sent(&mut self) {
        self.sends = self.sends.saturating_add(1);
        self.timeline("generation.upstream.sent", Severity::Debug, vec![]);
    }
    pub(super) fn terminal_validated(&mut self) {
        if !self.terminal_validated {
            self.terminal_validated = true;
            self.timeline("generation.upstream.terminal", Severity::Debug, vec![]);
        }
    }
    pub(super) fn finish_validated(&mut self) {
        self.finish_validated = true;
        self.timeline("generation.upstream.finished", Severity::Debug, vec![]);
    }
    pub(super) fn response_outcome(&mut self, outcome: crate::semantic::task::generation::Outcome) {
        use crate::semantic::task::generation::Outcome;
        self.response_outcome = Some(match outcome {
            Outcome::Completed => "completed",
            Outcome::Incomplete => "incomplete",
            Outcome::Failed => "failed",
            Outcome::Cancelled => "cancelled",
        });
    }
    pub(super) fn publishing(&mut self) {
        self.published = true;
    }
    pub(super) fn handoff(&mut self, final_frame: bool) {
        if self.first_handoff_ms.is_none() {
            self.first_handoff_ms = Some(self.elapsed_ms());
            self.timeline("generation.client.first_handoff", Severity::Debug, vec![]);
        }
        if final_frame {
            self.final_handoff = true;
            self.timeline("generation.client.final_handoff", Severity::Debug, vec![]);
        }
    }
    pub(super) fn decode_failure(
        &mut self,
        error: &AttemptError,
        attempt: &crate::execution::Attempt,
    ) {
        use crate::protocol::conversion::generation::ConversionBoundary;
        let boundary = if matches!(error, AttemptError::Sse(_)) {
            ConversionBoundary::EventDecode
        } else {
            ConversionBoundary::ResponseDecode
        };
        self.report
            .failure_with_context(Some(boundary), error, attempt.decode_failure());
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
    fn elapsed_ms(&self) -> i64 {
        self.started.elapsed().as_millis().min(i64::MAX as u128) as i64
    }
    fn attributes(&self) -> Attributes {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let mut attributes = vec![
            ("operation".into(), AnyValue::from(self.operation)),
            ("event.sequence".into(), AnyValue::from(sequence)),
            ("elapsed_ms".into(), AnyValue::from(self.elapsed_ms())),
            ("send_attempts".into(), AnyValue::from(self.sends)),
            (
                "upstream.terminal_validated".into(),
                AnyValue::from(self.terminal_validated),
            ),
            (
                "upstream.finish_validated".into(),
                AnyValue::from(self.finish_validated),
            ),
            ("client.published".into(), AnyValue::from(self.published)),
            (
                "client.handoff_complete".into(),
                AnyValue::from(self.final_handoff),
            ),
        ];
        for (name, value) in [
            ("request_id", self.request_id.as_deref()),
            ("agent_id", self.agent_id.as_deref()),
            ("provider", self.provider.as_deref()),
            ("account", self.account.as_deref()),
            ("model", self.public_model.as_deref()),
            ("upstream_model", self.upstream_model.as_deref()),
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
            attributes.push((
                "attempt.elapsed_ms".into(),
                AnyValue::from(
                    self.attempt_started
                        .elapsed()
                        .as_millis()
                        .min(i64::MAX as u128) as i64,
                ),
            ));
        }
        if let Some(outcome) = self.response_outcome {
            attributes.push(("response.outcome".into(), AnyValue::from(outcome)));
        }
        if let Some(ms) = self.first_handoff_ms {
            attributes.push(("client.first_handoff_ms".into(), AnyValue::from(ms)));
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
        attributes
    }
    fn timeline(&self, name: &'static str, severity: Severity, extra: Attributes) {
        if !matches!(self.operation, "chat" | "responses" | "messages") {
            return;
        }
        if let Some(sink) = &self.sink
            && sink.enabled(severity)
        {
            let mut attributes = self.attributes();
            attributes.extend(extra);
            sink.emit(name, severity, attributes, AnyValue::from(name));
        }
    }
    fn emit_failure(&mut self, code: &'static str, retry: bool) {
        use crate::protocol::conversion::generation::ConversionBoundary as Boundary;
        self.failure_logged = true;
        let Some(sink) = &self.sink else {
            return;
        };
        let attempt = self.report.attempt();
        let source = attempt.source_failure;
        let cause = self.report.cause();
        let decode = cause
            .as_ref()
            .and_then(|f| f.decode)
            .or_else(|| attempt.handling_failure.as_ref().and_then(|f| f.decode));
        let stage = if source.is_some() {
            Stage::Intake
        } else {
            match cause.as_ref().and_then(|f| f.boundary) {
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
        let status = source
            .and_then(|s| s.status_hint)
            .map(|s| s.get())
            .or(self.status)
            .or(self.report.request_error().map(|(_, s)| s));
        let severity = if retry || status.is_some_and(|s| (400..500).contains(&s)) {
            Severity::Warn
        } else {
            Severity::Error
        };
        if !sink.enabled(severity) {
            return;
        }
        if let Some(emitted) = &self.emitted {
            emitted.0.store(true, Ordering::Relaxed);
        }
        let mut attributes = self.attributes();
        attributes.extend([
            ("error.type".into(), AnyValue::from(code)),
            ("stage".into(), AnyValue::from(<&'static str>::from(stage))),
            ("retry.planned".into(), AnyValue::from(retry)),
        ]);
        if let Some(failure) = cause {
            if let Some(boundary) = failure.boundary {
                attributes.push((
                    "conversion.boundary".into(),
                    AnyValue::from(
                        serde_json::to_value(boundary)
                            .expect("closed boundary")
                            .as_str()
                            .expect("boundary label")
                            .to_owned(),
                    ),
                ));
            }
            if let Some(crate::protocol::CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::ToolConstraint {
                    declaration,
                    constraint,
                },
            )) = failure.cause.codec()
            {
                attributes.push((
                    "tool.declaration_index".into(),
                    AnyValue::from(*declaration as i64),
                ));
                attributes.push((
                    "tool.constraint".into(),
                    AnyValue::from(<&'static str>::from(*constraint)),
                ));
            }
        }
        if let Some(failure) = &attempt.handling_failure {
            attributes.push((
                "handling.error.type".into(),
                AnyValue::from(failure.cause.code()),
            ));
        }
        if let Some(context) = decode {
            attributes.push(("decode.operation".into(), AnyValue::from(context.operation)));
            if let Some(event) = context.last_event {
                attributes.push(("source.last_event.type".into(), AnyValue::from(event.kind)));
                attributes.push((
                    "source.last_event.ordinal".into(),
                    AnyValue::from(event.ordinal as i64),
                ));
                if let Some(sequence) = event.sequence.and_then(|s| i64::try_from(s).ok()) {
                    attributes.push((
                        "source.last_event.sequence".into(),
                        AnyValue::from(sequence),
                    ));
                }
            }
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
        let outcome = if self.cancelled_only() {
            "cancelled"
        } else if matches!(self.outcome, Outcome::Shutdown) {
            "shutdown"
        } else if matches!(self.outcome, Outcome::Timeout) {
            "timeout"
        } else if self.failure_code().is_some() || self.report.request_error().is_some() {
            "failed"
        } else {
            match self.outcome {
                Outcome::Complete => "complete",
                Outcome::Error => "failed",
                Outcome::Interrupted => "interrupted",
                Outcome::Timeout => "timeout",
                Outcome::Shutdown => "shutdown",
            }
        };
        self.timeline(
            "generation.request.finished",
            Severity::Info,
            vec![("outcome".into(), AnyValue::from(outcome))],
        );
    }
}
#[cfg(test)]
mod tests;
