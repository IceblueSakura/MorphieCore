//! Qualified Decision bindings. Task activation belongs to the deployment.
use crate::{
    protocol::decision::{Native, Target},
    provider::{CredentialBindingId, CredentialKind, ProviderId},
    topology::{
        ExecutionContract, ModelId,
        decisions::{DecisionRoute, SystemOneMetadata},
    },
};

pub struct DecisionBinding {
    pub id: &'static str,
    pub model: &'static str,
    pub provider: &'static str,
    pub client: Native,
    target: Target,
    metadata: Option<SystemOneMetadata>,
}

pub fn decision_bindings() -> &'static [DecisionBinding] {
    static BINDINGS: &[DecisionBinding] = &[
        DecisionBinding {
            id: "openai-luna-decisions",
            model: "gpt-6-luna",
            provider: "openai",
            client: Native::OpenAi,
            target: Target::Native(Native::OpenAi),
            metadata: None,
        },
        DecisionBinding {
            id: "typesafe-jev-1-14-system-one",
            model: "jev-1.14.0",
            provider: "typesafe",
            client: Native::SystemOne,
            target: Target::Native(Native::SystemOne),
            // https://docs.typesafe.ai/changelog
            metadata: Some(SystemOneMetadata {
                description: "General-purpose System One model for typed decisions.",
                release_date: "2026-10-14",
            }),
        },
        DecisionBinding {
            id: "google-gemini-decision-1-system-one-noul",
            model: "gemini-decision-1",
            provider: "google",
            client: Native::SystemOne,
            target: Target::Gemini,
            // https://aistudio.google.com/docs/changelog#11-11-2026
            metadata: Some(SystemOneMetadata {
                description: "Gemini Decision 1; plain-text Noul only.",
                release_date: "2026-11-11",
            }),
        },
    ];
    BINDINGS
}

impl DecisionBinding {
    pub fn route(&self, credential: CredentialBindingId) -> DecisionRoute {
        DecisionRoute {
            model: ModelId::new(self.model).expect("static Decision model"),
            upstream_model: self.model.into(),
            provider: ProviderId::new(self.provider).expect("static Decision provider"),
            target: self.target,
            client: Some(self.client),
            credential,
            execution: ExecutionContract {
                streaming: false,
                credential_kind: CredentialKind::ApiKey,
                request_body_limit: 4 << 20,
                response_body_limit: 4 << 20,
                timeout_ms: 60_000,
            },
            system_one_metadata: self.metadata.clone(),
        }
    }
}
