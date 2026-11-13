//! Google AI Studio bindings. Explicit public models and their native Google endpoint candidates.
use crate::lowering::disposition::Dispositions;
use crate::{
    protocol::{
        google::{generate_content, interactions},
        model_limits::ModelLimits,
        reasoning::{ReasoningConversion, ReasoningRequestPolicy},
    },
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, catalog},
    topology::*,
};
pub struct GoogleBinding {
    pub model: &'static str,
    pub upstream: &'static str,
    pub released_at: u64,
    pub developer: &'static str,
}
/// Selected models; registration is not account or inference evidence.
pub const GOOGLE_BINDINGS: &[GoogleBinding] = &[
    GoogleBinding {
        model: "gemini-3.8-flash",
        upstream: "gemini-3.8-flash",
        // https://ai.google.dev/gemini-api/docs/changelog (2026-09-02).
        released_at: 1_788_307_200,
        developer: "Google",
    },
    GoogleBinding {
        model: "gemini-4-argon",
        upstream: "gemini-4-argon",
        // https://ai.google.dev/gemini-api/docs/changelog (2026-10-13).
        released_at: 1_791_849_600,
        developer: "Google",
    },
    GoogleBinding {
        model: "gemini-4-hydrogen",
        upstream: "gemini-4-hydrogen",
        // https://ai.google.dev/gemini-api/docs/changelog (2026-11-12).
        released_at: 1_794_441_600,
        developer: "Google",
    },
    GoogleBinding {
        model: "gemini-4-carbon",
        upstream: "gemini-4-carbon",
        // https://ai.google.dev/gemini-api/docs/changelog (2026-11-10).
        released_at: 1_794_268_800,
        developer: "Google",
    },
];
impl GoogleBinding {
    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    fn reasoning_policy(&self) -> Option<ReasoningRequestPolicy> {
        use crate::semantic::task::generation::{EffortLevel::*, EffortScope};
        // Scope approximation is named per selected model, not a response-effort equivalence.
        let rule = match self.model {
            "gemini-3.8-flash" => "google.gemini-3.8-flash.response-effort-to-thinking.v1",
            "gemini-4-argon" => "google.gemini-4-argon.response-effort-to-thinking.v1",
            "gemini-4-hydrogen" => "google.gemini-4-hydrogen.response-effort-to-thinking.v1",
            "gemini-4-carbon" => "google.gemini-4-carbon.response-effort-to-thinking.v1",
            _ => return None,
        };
        Some(ReasoningRequestPolicy {
            canonical_model: self.model.into(),
            rule,
            conversion: ReasoningConversion::Scope {
                from: EffortScope::Response,
                to: EffortScope::Reasoning,
                levels: if self.model == "gemini-4-hydrogen" {
                    &[(Low, Low), (Medium, Medium), (High, High), (XHigh, XHigh)]
                } else {
                    &[(Low, Low), (Medium, Medium), (High, High)]
                },
            },
        })
    }
    fn accepted_contract(&self) -> Dispositions {
        let mut contract = generate_content::semantic_contract();
        // The existing Interactions candidate retains its official image-result carrier.
        contract.set_native(
            crate::semantic::task::generation::Feature::ToolResultMedia,
            true,
        );
        contract.set_native(
            crate::semantic::task::generation::Feature::ResponseEffort,
            self.reasoning_policy().is_some(),
        );
        contract
    }

    fn model_limits(&self) -> ModelLimits {
        use crate::semantic::task::generation::EffortLevel::{High, Low, Medium, XHigh};
        let levels = if self.model == "gemini-4-hydrogen" {
            vec![Low, Medium, High, XHigh]
        } else {
            vec![Low, Medium, High]
        };
        // Gemini 4 is a unified input + thinking + output window, not a separate input allowance.
        ModelLimits {
            allowed_effort: Some(levels.into()),
            reject_disabled_reasoning: self.model != "gemini-3.8-flash",
            context_window_tokens: (self.model != "gemini-3.8-flash").then_some(1_048_576),
            maximum_input_tokens: (self.model == "gemini-3.8-flash").then_some(1_048_576),
            maximum_output_tokens: Some(65_536),
        }
    }
    pub fn provider(&self) -> ProviderDefinition {
        catalog::google()
    }
    pub fn metadata(&self) -> ModelMetadata {
        ModelMetadata::new(self.released_at, self.developer).expect("static publication")
    }
    pub fn endpoint(&self) -> Endpoint {
        let provider = self.provider();
        let collection = provider
            .generation_entry(crate::provider::GenerationOperation::GoogleGenerateContent)
            .expect("declared path")
            .path;
        let path = crate::provider::GenerationOperation::GoogleGenerateContent
            .resolve_path(collection, self.upstream, true)
            .expect("static path");
        Endpoint {
            bound_replay_scope: None,
            id: EndpointId::new(&format!("google-{}-generate-content", self.model))
                .expect("static id"),
            provider: provider.id,
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            task: TaskKind::Generation,
            protocol: GenerationProtocol::GoogleGenerateContent,
            upstream_model: self.upstream.into(),
            canonical_model: ModelId::new(self.model).expect("static model"),
            representation: generate_content::TargetContract {
                dispositions: (generate_content::semantic_contract())
                    .with_native(
                        crate::semantic::task::generation::Feature::Temperature,
                        false,
                    )
                    .with_native(crate::semantic::task::generation::Feature::TopP, false),
                sampling: crate::protocol::google::SamplingPolicy::Ignored,
                model_limits: self.model_limits(),
                reasoning_policy: self.reasoning_policy(),
                messages_terminal_projection: self.model == "gemini-3.8-flash",
                ..Default::default()
            }
            .into(),
            execution: ExecutionContract {
                streaming: true,
                request_body_limit: super::GENERATION_REQUEST_BYTES,
                response_body_limit: super::GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: super::GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        }
    }
    pub fn route(&self) -> Route {
        Route {
            id: RouteId::new(&format!("google-{}", self.model)).expect("static route"),
            task: TaskKind::Generation,
            endpoints: vec![self.endpoint().id, self.interactions_endpoint().id],
            policy: RoutePolicy {
                candidates: CandidatePolicy::SkipUnrepresentable,
                ..RoutePolicy::default()
            },
        }
    }
    pub fn interactions_endpoint(&self) -> Endpoint {
        let mut endpoint = self.endpoint();
        endpoint.id =
            EndpointId::new(&format!("google-{}-interactions", self.model)).expect("static id");
        endpoint.protocol = GenerationProtocol::GoogleInteractions;
        endpoint.target.path = self
            .provider()
            .generation_entry(crate::provider::GenerationOperation::GoogleInteractions)
            .expect("declared path")
            .path
            .clone();
        endpoint.representation = interactions::TargetContract {
            sampling: crate::protocol::google::SamplingPolicy::Ignored,
            model_limits: self.model_limits(),
            reasoning_policy: self.reasoning_policy(),
            messages_terminal_projection: self.model == "gemini-3.8-flash",
            ..Default::default()
        }
        .into();
        endpoint
    }
    pub fn public_model(&self) -> PublicModel {
        let model = ModelId::new(self.model).expect("static model");
        PublicModel::new(
            model.clone(),
            model,
            TaskKind::Generation,
            self.route().id,
            self.accepted_contract(),
        )
    }
    pub fn canonical_model(&self) -> CanonicalModel {
        CanonicalModel {
            id: ModelId::new(self.model).expect("static model"),
            task: TaskKind::Generation,
            contract: self.accepted_contract(),
        }
    }
}
