//! Deployment bindings, not model discovery. Aliases and profiles are explicit.
use crate::{
    lowering::capabilities::SamplingConstraints,
    protocol::{openai::Dialect, reasoning::ReasoningHistory},
    topology::GenerationProtocol,
};
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestSlice {
    Canonical,
    RestrictedLuna,
    LimitedControls,
    /// Do not inherit a canonical model's endpoint-specific strict guarantee.
    NonStrictTools,
}
impl RequestSlice {
    pub fn apply(
        self,
        mut contract: crate::lowering::disposition::Dispositions,
    ) -> crate::lowering::disposition::Dispositions {
        if matches!(self, Self::RestrictedLuna) {
            contract.set_native(
                crate::semantic::task::generation::Feature::Temperature,
                false,
            );
            contract.set_native(crate::semantic::task::generation::Feature::TopP, false);
            contract.set_native(crate::semantic::task::generation::Feature::Logprobs, false);
            contract.set_native(
                crate::semantic::task::generation::Feature::Truncation,
                false,
            );
            contract.set_native(
                crate::semantic::task::generation::Feature::ParallelTools,
                false,
            );
        }
        if matches!(self, Self::LimitedControls) {
            // A coding-model listing does not establish these optional request controls.
            // Source: https://opencode.ai/docs/go/#endpoints
            contract.set_native(
                crate::semantic::task::generation::Feature::StructuredOutput,
                false,
            );
            // Readable reasoning history remains semantic input; unverified controls are
            // rejected by the target adapter rather than disabling this shared owner.
            contract.set_native(crate::semantic::task::generation::Feature::Logprobs, false);
            contract.set_native(crate::semantic::task::generation::Feature::Verbosity, false);
            contract.set_native(
                crate::semantic::task::generation::Feature::Truncation,
                false,
            );
            contract.set_native(
                crate::semantic::task::generation::Feature::ParallelTools,
                false,
            );
        }
        if matches!(self, Self::NonStrictTools) {
            contract.set_native(
                crate::semantic::task::generation::Feature::StrictTools,
                false,
            );
        }
        contract
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKeyBinding {
    pub(super) provider: &'static str,
    pub dialect: Dialect,
    pub endpoint_prefix: &'static str,
    /// Downstream label only; clients cannot choose a Provider through it.
    pub model: &'static str,
    /// Shared semantic identity, independent of upstream naming.
    pub canonical_model: &'static str,
    /// Exact model spelling accepted by this Provider, injected during encoding.
    pub upstream: &'static str,
    pub protocols: Box<[GenerationProtocol]>,
    pub(super) effort_mappings: Box<[(GenerationProtocol, &'static str)]>,
    /// Missing protocol members are explicitly unassessed.
    pub reasoning_history: Box<[(GenerationProtocol, ReasoningHistory)]>,
    pub public_slice: RequestSlice,
    pub endpoint_slice: RequestSlice,
    #[serde(default)]
    pub(super) limits: crate::protocol::model_limits::ModelLimits,
    #[serde(default)]
    pub(super) sampling_requires_reasoning_off: bool,
    #[serde(default)]
    pub(super) wire_overrides: Box<[(GenerationProtocol, super::data::WireOverrides)]>,
}
use super::{GENERATION_REQUEST_BYTES, GENERATION_RESPONSE_BYTES, GENERATION_TIMEOUT_MS, data};
use crate::{
    protocol::openai::{GenerationCodec, TargetContract},
    provider::{CredentialBindingId, CredentialKind},
    topology::{
        Endpoint, EndpointId, EndpointTarget, ExecutionContract, ModelId, PublicModel, RouteId,
        TopologyError,
    },
};
impl ApiKeyBinding {
    pub fn provider(&self) -> crate::provider::ProviderDefinition {
        super::data::provider(self.provider).clone()
    }

    pub fn credential(&self) -> CredentialBindingId {
        CredentialBindingId::new(self.provider().id.as_str()).expect("catalog Provider identity")
    }
    pub fn public_model(&self) -> PublicModel {
        PublicModel {
            id: ModelId::new(self.model).expect("static id"),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            route: RouteId::new(&format!("{}-generation", self.endpoint_prefix))
                .expect("static id"),
            contract: self
                .public_slice
                .apply(data::openai_model_contract(self.canonical_model)),
        }
    }
    pub fn endpoint(&self, protocol: GenerationProtocol) -> Result<Endpoint, TopologyError> {
        let provider = self.provider();
        let (family, path) = match protocol {
            GenerationProtocol::OpenAiChat => (
                crate::protocol::openai::GenerationApi::Chat,
                provider
                    .generation_entry(protocol.operation())
                    .expect("declared entry")
                    .path
                    .clone(),
            ),
            GenerationProtocol::OpenAiResponses => (
                crate::protocol::openai::GenerationApi::Responses,
                provider
                    .generation_entry(protocol.operation())
                    .expect("declared entry")
                    .path
                    .clone(),
            ),
            GenerationProtocol::AnthropicMessages | GenerationProtocol::GoogleInteractions => {
                return Err(TopologyError::ContractUnsatisfiable);
            }
        };
        let scope = crate::protocol::replay_envelope::issuer(provider.id.as_str(), self.upstream);
        let mut adapter = GenerationCodec::new(family, self.dialect, Some(scope.clone()));
        let overrides = self
            .wire_overrides
            .iter()
            .find(|(member, _)| *member == protocol)
            .map(|(_, rules)| *rules)
            .unwrap_or_default();
        overrides.apply(&mut adapter.adaptation.rules);
        let mut mappings = self
            .effort_mappings
            .iter()
            .filter(|(member, _)| *member == protocol);
        adapter.adaptation.effort_mapping = mappings
            .next()
            .map(|(_, mapping)| super::data::effort_mapping(mapping));
        if adapter.adaptation.effort_mapping.is_none()
            && self.dialect == crate::protocol::openai::Dialect::Bailian
        {
            adapter.adaptation.effort_mapping =
                crate::protocol::openai::dialect::bailian::effort_mapping(
                    self.canonical_model,
                    family,
                );
        }
        if mappings.next().is_some() {
            return Err(TopologyError::InvalidModelBinding);
        }
        let mut history = self
            .reasoning_history
            .iter()
            .filter(|(member, _)| *member == protocol);
        let reasoning_history = history.next().map(|(_, fact)| *fact);
        if history.next().is_some() {
            return Err(TopologyError::InvalidModelBinding);
        }
        let mut contract = TargetContract {
            reasoning_history,
            sampling: SamplingConstraints {
                requires_reasoning_off: self.sampling_requires_reasoning_off,
                ..Default::default()
            },
            dispositions: self
                .endpoint_slice
                .apply(data::openai_model_contract(self.canonical_model))
                .with_native(crate::semantic::task::generation::Feature::Identity, false)
                .with_native(crate::semantic::task::generation::Feature::Metadata, false)
                .with_native(
                    crate::semantic::task::generation::Feature::ServicePolicy,
                    false,
                )
                .with_native(
                    crate::semantic::task::generation::Feature::ToolBudget,
                    false,
                )
                .with_native(
                    crate::semantic::task::generation::Feature::Prediction,
                    false,
                ),
            replay_origin: Some(scope.clone()),
            cache: adapter.adaptation.cache,
            model_limits: self.limits.clone(),
            ..TargetContract::full()
        };
        if self.dialect == Dialect::Bailian {
            contract.dispositions.set(
                crate::semantic::task::generation::Feature::StrictTools,
                crate::protocol::openai::dialect::bailian::function_strictness(
                    self.upstream,
                    family,
                ),
            );
        }
        Ok(Endpoint {
            bound_replay_scope: None,
            id: self.endpoint_id(protocol),
            canonical_model: ModelId::new(self.canonical_model).expect("static id"),
            provider: provider.id.clone(),
            target: EndpointTarget {
                origin: provider.origin,
                path,
            },
            protocol,
            upstream_model: self.upstream.into(),
            representation: adapter.contract(&contract).into(),
            execution: ExecutionContract {
                streaming: true,
                request_body_limit: GENERATION_REQUEST_BYTES,
                response_body_limit: GENERATION_RESPONSE_BYTES,
                credential_kind: CredentialKind::ApiKey,
                timeout_ms: GENERATION_TIMEOUT_MS,
            },
            credential: self.credential(),
        })
    }
    pub fn endpoint_id(&self, protocol: GenerationProtocol) -> EndpointId {
        let suffix = match protocol {
            GenerationProtocol::OpenAiChat => "chat",
            GenerationProtocol::OpenAiResponses => "responses",
            GenerationProtocol::AnthropicMessages => "messages",
            GenerationProtocol::GoogleInteractions => "interactions",
        };
        EndpointId::new(&format!("{}-{suffix}", self.endpoint_prefix)).expect("static id")
    }
}
