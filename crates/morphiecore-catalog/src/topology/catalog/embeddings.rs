//! Explicit text embedding bindings. No Generation or file-service admission is inferred.
use crate::{
    adapter::embeddings::UpstreamAdapter,
    protocol::embedding::projection::{
        Contract, DimensionSupport, EncodingSupport, ReportPolicy, UserPolicy,
    },
    provider::{CredentialBindingId, CredentialKind, ProviderOperation},
    semantic::task::embedding::TokenizerScope,
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        TopologyError,
        embeddings::{EmbeddingEndpoint, EmbeddingRoute, ProviderEntry},
    },
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub(super) provider: &'static str,
    pub max_inputs: usize,
    pub dimensions: Box<[usize]>,
    pub max_dimensions: usize,
    pub min_dimensions: usize,
    pub default_dimensions: usize,
    pub encodings: EncodingSupport,
    pub user: UserPolicy,
    pub tokenizer: Option<TokenizerScope>,
    pub max_input_tokens: Option<usize>,
    pub profile: UpstreamAdapter,
    pub released_at: u64,
    pub developer: &'static str,
}

impl EmbeddingBinding {
    pub(super) fn ids(&self) -> Result<(EndpointId, RouteId), TopologyError> {
        let id = format!("{}-{}-embeddings", self.provider, self.model);
        Ok((
            EndpointId::new(&id).map_err(|_| TopologyError::InvalidModelBinding)?,
            RouteId::new(&id).map_err(|_| TopologyError::InvalidModelBinding)?,
        ))
    }

    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }

    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn operation(&self) -> ProviderEntry {
        let provider = self.provider();
        let path = provider
            .entry(ProviderOperation::Embeddings)
            .expect("embedding entry")
            .path
            .clone();
        ProviderEntry {
            provider: provider.id,
            path,
        }
    }
    pub fn route(&self) -> EmbeddingRoute {
        let provider = self.provider();
        let canonical = ModelId::new(self.model).expect("static model");
        let (endpoint_id, route_id) = self.ids().expect("validated embedding identity");
        EmbeddingRoute {
            id: route_id,
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            endpoint: EmbeddingEndpoint {
                id: endpoint_id,
                provider: provider.id,
                target: EndpointTarget {
                    origin: provider.origin,
                    path: self.operation().path,
                },
                upstream_model: self.upstream.into(),
                canonical_model: canonical,
                profile: self.profile,
                contract: Contract {
                    max_inputs: self.max_inputs,
                    default_dimensions: self.default_dimensions,
                    encodings: self.encodings,
                    dimensions: if self.dimensions.is_empty() {
                        DimensionSupport::Range {
                            min: self.min_dimensions,
                            max: self.max_dimensions,
                        }
                    } else {
                        DimensionSupport::Selected(self.dimensions.to_vec())
                    },
                    user: self.user,
                    tokenizer: self.tokenizer,
                    max_input_tokens: self.max_input_tokens,
                    joint_text: self.profile == UpstreamAdapter::Google,
                    reports: if self.profile == UpstreamAdapter::Standard {
                        ReportPolicy::Strict
                    } else {
                        ReportPolicy::Compatible
                    },
                },
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
