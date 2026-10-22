//! Explicit text embedding bindings. No Generation or file-service admission is inferred.
use crate::{
    adapter::embeddings::{Contract, DimensionSupport, Profile},
    provider::{
        CredentialBindingId, CredentialKind, ProviderDefinition, ProviderOperation, catalog,
    },
    topology::{
        EndpointId, EndpointTarget, ExecutionContract, ModelId, ModelMetadata, RouteId,
        embeddings::{EmbeddingEndpoint, EmbeddingRoute, ProviderEntry},
    },
};
pub struct EmbeddingBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub provider: fn() -> ProviderDefinition,
    pub max_inputs: usize,
    pub dimensions: &'static [usize],
    pub max_dimensions: usize,
    pub user: bool,
    pub profile: Profile,
    pub released_at: u64,
    pub developer: &'static str,
}
pub const EMBEDDING_BINDINGS: &[EmbeddingBinding] = &[
    // Selected existing trusted origin; workspace migration is a separate operator action.
    // https://help.aliyun.com/en/model-studio/text-embedding-synchronous-api
    EmbeddingBinding {
        model: "qwen3.7-text-embedding",
        upstream: "qwen3.7-text-embedding",
        provider: catalog::aliyun_dashscope_cn,
        max_inputs: 20,
        dimensions: &[256, 512, 768, 1024, 1536, 2048, 2560],
        max_dimensions: 2560,
        user: false,
        profile: Profile::DashScope,
        // https://platform.qianwenai.com/docs/changelog/models#2026年7月15日
        released_at: 1_784_073_600,
        developer: "Alibaba",
    },
    // https://openrouter.ai/docs/api/api-reference/embeddings/create-embeddings
    EmbeddingBinding {
        model: "text-embedding-3-small",
        upstream: "openai/text-embedding-3-small",
        provider: catalog::openrouter,
        max_inputs: 128,
        dimensions: &[],
        max_dimensions: 1536,
        user: true,
        profile: Profile::OpenRouter,
        // https://openai.com/index/new-embedding-models-and-api-updates/ (2024-01-25).
        released_at: 1_706_140_800,
        developer: "OpenAI",
    },
];
impl EmbeddingBinding {
    pub fn credential(&self) -> CredentialBindingId {
        (self.provider)()
            .credential_pool
            .expect("catalog credential pool")
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn operation(&self) -> ProviderEntry {
        let provider = (self.provider)();
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
        let provider = (self.provider)();
        let canonical = ModelId::new(self.model).expect("static model");
        let id = format!("{}-{}-embeddings", provider.id.as_str(), self.model);
        EmbeddingRoute {
            id: RouteId::new(&id).expect("static route"),
            model: canonical.clone(),
            canonical_model: canonical.clone(),
            endpoint: EmbeddingEndpoint {
                id: EndpointId::new(&id).expect("static endpoint"),
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
                    dimensions: if self.dimensions.is_empty() {
                        DimensionSupport::UpTo(self.max_dimensions)
                    } else {
                        DimensionSupport::Selected(self.dimensions.to_vec())
                    },
                    user: self.user,
                },
                credential: self.credential(),
                execution: ExecutionContract {
                    streaming: false,
                    retry_before_commit: false,
                    request_body_limit: 256 << 10,
                    response_body_limit: 4 << 20,
                    credential_kind: CredentialKind::ApiKey,
                    timeout_ms: 120_000,
                },
            },
        }
    }
}
