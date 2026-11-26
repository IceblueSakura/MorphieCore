//! Decision text evaluation through the bounded standalone exchange.
use super::{
    TaskRoute,
    exchange::{self, Receiver},
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, DecisionEntry, Gateway, Limits, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    execution::task::PreparedTaskRequest,
    ingress::admission,
    protocol::decision::{Expectation, Native, Target, gemini},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, ProviderId},
    semantic::task::decision::{Evaluation, Request as DecisionRequest},
    topology::{CompiledTopology, ExecutionContract, decisions::DecisionRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};
use tokio::time::Instant;

mod compression;

const JSON_BYTES: usize = 4 << 20;
pub(crate) type BoundDecision = super::BoundTask<DecisionRoute>;

/// Sanitized failure of an embedded evaluation; no upstream text or payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Decision evaluation failed: {code} ({status})")]
pub struct DecisionExecutionError {
    pub status: StatusCode,
    pub code: &'static str,
}

impl From<ApiError> for DecisionExecutionError {
    fn from(error: ApiError) -> Self {
        Self {
            status: error.status,
            code: error.code(),
        }
    }
}

impl TaskRoute for DecisionRoute {
    fn target(&self) -> (&ProviderId, &CredentialBindingId) {
        (&self.provider, &self.credential)
    }

    fn retarget(&mut self, provider: &ProviderDefinition) -> Result<(), StartupError> {
        if provider.id != self.provider {
            return Err(StartupError::Credentials);
        }
        if provider
            .entry(self.operation())
            .is_none_or(|entry| entry.auth.kind() != CredentialKind::ApiKey)
        {
            return Err(StartupError::Binding);
        }
        Ok(())
    }

    fn delivery(&self) -> (&str, &str, &ExecutionContract) {
        (self.model.as_str(), &self.upstream_model, &self.execution)
    }
}

pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<DecisionEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundDecision>>, StartupError> {
    super::bind(
        topology,
        entries,
        credentials,
        |entry: &DecisionEntry| topology.decision_route(&entry.model),
        |entry| &entry.model,
    )
}

pub(crate) async fn openai(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request, Native::OpenAi)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}

pub(crate) async fn system_one(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request, Native::SystemOne)
        .await
        .unwrap_or_else(ApiError::into_system_one_response)
}

async fn ingress(
    runtime: Arc<Runtime>,
    mut request: Request,
    native: Native,
) -> Result<Response, ApiError> {
    if !runtime
        .decisions
        .values()
        .any(|entry| entry.route.target == Target::Native(native))
    {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ));
    }
    if native == Native::SystemOne {
        for header in [
            "typesafe-danger-zone-images",
            "typesafe-danger-zone-long-context",
        ] {
            let mut values = request.headers().get_all(header).iter();
            if values
                .next()
                .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"false"))
                || values.next().is_some()
            {
                return Err(ApiError::invalid());
            }
        }
    }
    let zstd = native == Native::SystemOne && compression::consume_encoding(request.headers_mut())?;
    let limit = runtime.limits.request_bytes.min(JSON_BYTES);
    let (admitted, _, bytes) =
        Admission::upload(&runtime, request, limit, admission::headers).await?;
    let bytes = if zstd {
        compression::decode(
            bytes,
            limit,
            &runtime.shutdown,
            admitted.deadline(&runtime.limits, u64::MAX),
        )
        .await?
    } else {
        bytes
    };
    let parsed = native
        .decode_request(&bytes)
        .map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .decisions
        .get(&parsed.model)
        .filter(|entry| entry.route.target == Target::Native(native))
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let prepared = native
        .prepare(parsed.task, &entry.route.upstream_model)
        .map_err(|_| ApiError::invalid())?;
    let wire = prepare_wire(&entry, &prepared.body, JSON_BYTES)?;
    admitted
        .respond(runtime, entry, wire, Output(prepared.expectation))
        .await
}

impl Gateway {
    /// Evaluate an explicitly bound Gemini Decision target from a trusted Rust embed.
    /// This does not register an HTTP route or select a fallback target.
    pub async fn evaluate_gemini_decision(
        &self,
        model: &str,
        request: DecisionRequest,
    ) -> Result<Evaluation, DecisionExecutionError> {
        let runtime = &self.state;
        let entry = runtime
            .decisions
            .get(model)
            .filter(|e| e.route.target == Target::Gemini)
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    crate::error::ErrorKind::ModelNotFound,
                )
            })?;
        let started = Instant::now();
        let _permit = admission::acquire_permit(runtime)?;
        let deadline = started
            + runtime
                .limits
                .exchange_timeout
                .min(std::time::Duration::from_millis(
                    entry.route.execution.timeout_ms,
                ));
        let mut trace = Trace::embedded_decision(runtime);
        trace.begin_candidate(1);
        trace.models(model, &entry.route.upstream_model);
        let exchange = async {
            let prepared = gemini::prepare(request, &entry.route.upstream_model)
                .map_err(|_| ApiError::invalid())?;
            let wire = prepare_wire(entry, &prepared.body, runtime.limits.request_bytes)?;
            let receiver = exchange::send(
                runtime,
                entry,
                wire,
                deadline,
                receive_limit(entry, &runtime.limits),
                &mut trace,
            )
            .await?;
            let bytes = receive_json(receiver, &mut trace).await?;
            let evaluation = prepared
                .expectation
                .complete(&bytes)
                .map_err(|_| ApiError::malformed())?;
            trace.terminal_validated();
            Ok(evaluation)
        };
        let result: Result<_, ApiError> = tokio::select! {
            biased;
            _ = runtime.shutdown.cancelled() => Err(ApiError::shutdown()),
            result = tokio::time::timeout_at(deadline, exchange) => result.unwrap_or_else(|_| Err(ApiError::timeout())),
        };
        trace.end_candidate(result.as_ref().err().map(|e| e.diagnostic_code()), false);
        use crate::{diagnostics::Outcome, error::ErrorKind};
        trace.outcome(match result.as_ref().map_err(|e| e.kind) {
            Ok(_) => Outcome::Complete,
            Err(ErrorKind::Shutdown) => Outcome::Shutdown,
            Err(ErrorKind::Timeout) => Outcome::Timeout,
            Err(_) => Outcome::Error,
        });
        result.map_err(DecisionExecutionError::from)
    }
}

fn prepare_wire(
    entry: &BoundDecision,
    body: &serde_json::Value,
    limit: usize,
) -> Result<PreparedTaskRequest, ApiError> {
    let route = &entry.route;
    let limit = limit
        .min(JSON_BYTES)
        .min(route.execution.request_body_limit);
    crate::semantic::value::json_size(body, limit).map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            crate::error::ErrorKind::RequestTooLarge,
        )
    })?;
    let body = serde_json::to_vec(body).map_err(|_| ApiError::invalid())?;
    let operation = route.operation();
    let path = entry
        .provider
        .entry(operation)
        .ok_or_else(ApiError::upstream)?
        .path
        .clone();
    Ok(PreparedTaskRequest::new(
        &entry.provider,
        path,
        operation,
        vec![
            ("content-type".into(), "application/json".into()),
            ("accept".into(), "application/json".into()),
        ],
        body,
    ))
}

fn receive_limit(entry: &BoundDecision, limits: &Limits) -> usize {
    JSON_BYTES
        .min(entry.route.execution.response_body_limit)
        .min(limits.response_bytes)
}

async fn receive_json(receiver: Receiver, trace: &mut Trace) -> Result<Vec<u8>, ApiError> {
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await?;
    trace.stage(Stage::Projection);
    Ok(bytes)
}

struct Output(Expectation);
impl TaskOutput for Output {
    type Route = DecisionRoute;

    fn response_type(&self) -> body::ResponseType {
        body::ResponseType::Json
    }

    fn receive_limit(&self, _: &BoundDecision, _: &Limits, wire_limit: usize) -> usize {
        wire_limit.min(JSON_BYTES)
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundDecision,
        limits: &Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let bytes = receive_json(receiver, trace).await?;
        let value = self
            .0
            .complete(&bytes, entry.route.model.as_str())
            .map_err(|_| ApiError::malformed())?;
        crate::semantic::value::json_size(&value, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let bytes = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, bytes.into(), trace).await?;
        Ok(())
    }
}
