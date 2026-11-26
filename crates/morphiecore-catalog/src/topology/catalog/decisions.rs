//! Qualified native Decision bindings. Task activation belongs to the deployment.
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
    pub native: Native,
    metadata: Option<SystemOneMetadata>,
}

pub fn decision_bindings() -> &'static [DecisionBinding] {
    static BINDINGS: &[DecisionBinding] = &[
        DecisionBinding {
            id: "openai-luna-decisions",
            model: "gpt-6-luna",
            provider: "openai",
            native: Native::OpenAi,
            metadata: None,
        },
        DecisionBinding {
            id: "typesafe-jev-1-14-system-one",
            model: "jev-1.14.0",
            provider: "typesafe",
            native: Native::SystemOne,
            // https://docs.typesafe.ai/changelog
            metadata: Some(SystemOneMetadata {
                description: "General-purpose System One model for typed decisions.",
                release_date: "2026-10-14",
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
            target: Target::Native(self.native),
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
