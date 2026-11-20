//! Authenticated text embeddings use one bounded static exchange and the shared handoff owner.
use super::{
    exchange::Receiver,
    lifecycle::{Admission, TaskOutput},
};
use crate::{
    ApiError, Credentials, EmbeddingEntry, Runtime, StartupError,
    adapter::embeddings::UpstreamAdapter,
    body,
    diagnostics::{Stage, Trace},
    ingress::{admission, capabilities},
    protocol::{
        embedding::{Encoding, Response as EmbeddingResponse},
        openai::embeddings as codec,
    },
    topology::{CompiledTopology, embeddings::EmbeddingRoute},
};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{collections::BTreeMap, sync::Arc};

pub(crate) type BoundEmbedding = super::BoundTask<EmbeddingRoute>;
pub(crate) fn bind(
    topology: &CompiledTopology,
    entries: Vec<EmbeddingEntry>,
    credentials: &Credentials,
) -> Result<BTreeMap<String, Arc<BoundEmbedding>>, StartupError> {
    super::bind(
        topology,
        entries,
        credentials,
        |entry: &EmbeddingEntry| topology.embedding_route(&entry.model),
        |entry| &entry.model,
    )
}
pub(crate) async fn handle(State(runtime): State<Arc<Runtime>>, request: Request) -> Response {
    ingress(runtime, request)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
async fn ingress(runtime: Arc<Runtime>, request: Request) -> Result<Response, ApiError> {
    let (admitted, _, bytes) = Admission::upload(
        &runtime,
        request,
        runtime.limits.request_bytes,
        admission::headers,
    )
    .await?;
    let parsed = codec::parse_request(&bytes).map_err(|_| ApiError::invalid())?;
    let entry = runtime
        .embeddings
        .get(parsed.model())
        .ok_or(ApiError::new(
            StatusCode::NOT_FOUND,
            crate::error::ErrorKind::ModelNotFound,
        ))?
        .clone();
    let request = parsed
        .into_request(entry.route.endpoint.contract.tokenizer)
        .map_err(|_| ApiError::invalid())?;
    let prepared = crate::execution::embeddings::prepare(&entry.route, &entry.provider, &request)
        .map_err(|_| ApiError::invalid())?;
    admitted
        .respond(
            runtime,
            entry,
            prepared.request,
            Output {
                expectation: prepared.expectation,
                delivery: prepared.delivery,
            },
        )
        .await
        .map(capabilities::notice)
}
async fn decode(
    receiver: Receiver,
    trace: &mut Trace,
    profile: UpstreamAdapter,
    encoding: Encoding,
) -> Result<EmbeddingResponse, ApiError> {
    receiver.require_media("application/json")?;
    let bytes = receiver.collect(trace).await?;
    profile
        .decode_response_as(&bytes, encoding)
        .map_err(|_| ApiError::upstream())
}
#[cfg(test)]
pub(crate) async fn receive_for_test(
    upstream: reqwest::Response,
    limit: usize,
    trace: &mut Trace,
    profile: UpstreamAdapter,
    encoding: Encoding,
) -> Result<EmbeddingResponse, ApiError> {
    decode(
        Receiver::new(
            upstream,
            limit.min(crate::semantic::value::JsonLimits::ENVELOPE.bytes),
            trace,
        )?,
        trace,
        profile,
        encoding,
    )
    .await
}
struct Output {
    expectation: crate::protocol::embedding::projection::Expectation,
    delivery: crate::protocol::embedding::projection::DeliveryPolicy,
}
impl TaskOutput for Output {
    type Route = EmbeddingRoute;
    const RESPONSE: body::ResponseType = body::ResponseType::Json;

    fn receive_limit(&self, _: &BoundEmbedding, _: &crate::Limits, wire_limit: usize) -> usize {
        wire_limit.min(crate::semantic::value::JsonLimits::ENVELOPE.bytes)
    }

    async fn publish(
        self,
        receiver: Receiver,
        entry: &BoundEmbedding,
        limits: &crate::Limits,
        lane: &body::Lane<'_>,
        trace: &mut Trace,
    ) -> Result<(), ApiError> {
        let Self {
            expectation,
            delivery,
        } = self;
        let response = decode(
            receiver,
            trace,
            entry.route.endpoint.profile,
            expectation.upstream_encoding,
        )
        .await?;
        expectation
            .validate_response(&response)
            .map_err(|_| ApiError::upstream())?;
        trace.stage(Stage::Projection);
        let projected =
            crate::protocol::embedding::projection::project_response(&response, &delivery)
                .map_err(|_| ApiError::upstream())?;

        let value = codec::encode_response_as(&projected.response, delivery.encoding)
            .map_err(|_| ApiError::upstream())?;
        crate::semantic::value::json_size(&value, limits.response_bytes)
            .map_err(|_| ApiError::upstream())?;
        let output = serde_json::to_vec(&value).map_err(|_| ApiError::upstream())?;
        body::send_final_frame(lane, output.into(), trace).await?;
        Ok(())
    }
}
