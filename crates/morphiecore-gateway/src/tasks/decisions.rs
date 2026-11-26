//! Same-protocol text evaluation through the bounded standalone lifecycle.
use super::{
    TaskRoute,
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, DecisionEntry, Limits, Runtime, StartupError, body,
    diagnostics::{Stage, Trace},
    execution::task::PreparedTaskRequest,
    ingress::admission,
    protocol::decision::{Expectation, Native},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, ProviderId},
    topology::{CompiledTopology, ExecutionContract, decisions::DecisionRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};

mod compression;

const JSON_BYTES: usize = 4 << 20;
pub(crate) type BoundDecision = super::BoundTask<DecisionRoute>;

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
        .any(|entry| entry.route.native == native)
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
        .filter(|entry| entry.route.native == native)
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let route = &entry.route;
    let prepared = native
        .prepare(parsed.task, &route.upstream_model)
        .map_err(|_| ApiError::invalid())?;
    crate::semantic::value::json_size(&prepared.body, route.execution.request_body_limit).map_err(
        |_| {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                crate::error::ErrorKind::RequestTooLarge,
            )
        },
    )?;
    let body = serde_json::to_vec(&prepared.body).map_err(|_| ApiError::invalid())?;
    let operation = route.operation();
    let path = entry
        .provider
        .entry(operation)
        .ok_or_else(ApiError::upstream)?
        .path
        .clone();
    let wire = PreparedTaskRequest::new(
        &entry.provider,
        path,
        operation,
        vec![
            ("content-type".into(), "application/json".into()),
            ("accept".into(), "application/json".into()),
        ],
        body,
    );
    admitted
        .respond(runtime, entry, wire, Output(prepared.expectation))
        .await
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
        receiver.require_media("application/json")?;
        let bytes = receiver.collect(trace).await?;
        trace.stage(Stage::Projection);
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
