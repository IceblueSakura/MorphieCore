//! Prepare one immutable embedding attempt before resolving credentials. No I/O or retries.
use super::{AttemptError, task::PreparedTaskRequest};
use crate::{
    protocol::embedding::{
        Request,
        projection::{AdaptationRecord, DeliveryPolicy, Expectation},
    },
    provider::ProviderDefinition,
    topology::embeddings::EmbeddingRoute,
};

pub struct PreparedEmbedding {
    pub request: PreparedTaskRequest,
    pub expectation: Expectation,
    pub delivery: DeliveryPolicy,
    pub adaptations: Vec<AdaptationRecord>,
}
pub fn prepare(
    route: &EmbeddingRoute,
    provider: &ProviderDefinition,
    request: &Request,
) -> Result<PreparedEmbedding, AttemptError> {
    if request.model != route.model.as_str()
        || provider.id != route.endpoint.provider
        || provider.origin != route.endpoint.target.origin
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::EmbeddingBindingMismatch,
        ));
    }
    let projected = route.endpoint.contract.prepare(request).map_err(|_| {
        AttemptError::Protocol(crate::execution::attempt::ProtocolReason::EmbeddingAdmission)
    })?;
    let value = route
        .endpoint
        .profile
        .encode_request(&projected.target, &route.endpoint.upstream_model)?;
    crate::semantic::value::json_size(&value, route.endpoint.execution.request_body_limit)
        .map_err(|_| AttemptError::Limit)?;
    let body = serde_json::to_vec(&value).map_err(|_| {
        AttemptError::Protocol(crate::execution::attempt::ProtocolReason::EmbeddingSerialization)
    })?;
    Ok(PreparedEmbedding {
        request: PreparedTaskRequest::new(
            provider,
            route.endpoint.target.path.clone(),
            crate::provider::ProviderOperation::Embeddings,
            vec![
                ("content-type".into(), "application/json".into()),
                ("accept".into(), "application/json".into()),
            ],
            body,
        ),
        expectation: projected.expectation,
        delivery: projected.delivery,
        adaptations: projected.adaptations,
    })
}
