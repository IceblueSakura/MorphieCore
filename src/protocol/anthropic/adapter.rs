//! Native target projection and dialect policy; no OpenAI profile or wire switches.
use super::{Profile, ReplayTarget};
use crate::{
    adapter::{AdapterError, Request},
    protocol::{CodecError, cache::CacheProjection, extensions::CustomSections},
    semantic::{
        context::ExecutionHints, task::generation::GenerationSemanticContract, value::Presence,
    },
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    Standard,
    OpenCodeGo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCaps {
    pub profile: Profile,
    pub dialect: Dialect,
    pub semantics: GenerationSemanticContract,
    pub cache: CacheProjection,
}
impl TargetCaps {
    pub fn new(profile: Profile, dialect: Dialect) -> Self {
        Self {
            profile,
            dialect,
            semantics: profile.semantic_contract(),
            cache: CacheProjection {
                session_id: dialect == Dialect::OpenCodeGo,
                ..Default::default()
            },
        }
    }
    fn check_context(&self, request: &Request) -> Result<(), CodecError> {
        request.check_grouping()?;
        request.context.validate()?;
        request.delivery.validate()?;
        let mut context = request.context.clone();
        self.cache.project(&mut context)?;
        // Explicit stateless defaults do not ask the native Provider for storage.
        if context.store == Presence::Value(false) {
            context.store = Presence::Absent;
        }
        if context.background == Presence::Value(false) {
            context.background = Presence::Absent;
        }
        for value in [
            &mut context.previous_response_id,
            &mut context.conversation,
            &mut context.prompt,
            &mut context.moderation,
        ] {
            if *value == Presence::Null {
                *value = Presence::Absent;
            }
        }
        let go = self.dialect == Dialect::OpenCodeGo;
        if go {
            self.cache.opencode_session(
                request.conversation.as_ref(),
                request.cache_session.as_ref(),
                request.inferred_affinity,
            )?;
        }
        if context != ExecutionHints::default()
            || request.delivery.stream == Presence::Null
            || request.extensions != CustomSections::default()
            || request.n.value().is_some_and(|n| *n != 1)
            || !go && request.cache_session.is_some()
        {
            return Err(CodecError::Unsupported("native request context".into()));
        }
        Ok(())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub(crate) fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<Value, AdapterError> {
        let stripped = request.without_foreign_replay(Some(&target.origin()))?;
        let request = stripped.as_ref().unwrap_or(request);
        self.check_context(request)?;
        request.check_semantic(&self.semantics)?;
        request.check_semantic(&self.profile.semantic_contract())?;
        let mut semantic = self.profile.group_client_history(&request.task.semantic)?;
        let mut settings = semantic.settings().clone();
        // Include selects the downstream opaque carrier, not a native control.
        settings.reasoning = settings.reasoning.with_encrypted_output(false);
        if self.dialect == Dialect::OpenCodeGo && settings.reasoning != Default::default() {
            return Err(
                CodecError::Unsupported("unverified native reasoning controls".into()).into(),
            );
        }
        semantic = semantic.with_settings(settings).map_err(CodecError::from)?;
        let mut wire = self
            .profile
            .encode_generation_request(&semantic, &request.task.fidelity, target)?
            .value;
        if let Some(stream) = request.delivery.stream.value() {
            wire["stream"] = serde_json::json!(stream);
        }
        Ok(wire)
    }
    pub(crate) fn request_headers(
        &self,
        request: &Request,
    ) -> Result<Vec<(String, String)>, CodecError> {
        let mut headers = vec![
            ("anthropic-version".into(), "2023-06-01".into()),
            (
                "user-agent".into(),
                concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
            ),
        ];
        if self.dialect == Dialect::OpenCodeGo {
            let session = self.cache.opencode_session(
                request.conversation.as_ref(),
                request.cache_session.as_ref(),
                request.inferred_affinity,
            )?;
            headers.push(("x-opencode-session".into(), session));
        }
        Ok(headers)
    }
}
