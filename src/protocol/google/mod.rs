//! Google generateContent upstream: request projection and streamed intake over the shared
//! IR. Upstream only; no client codec, OpenAI profile or wire switches.
mod request;
mod signature;
mod stream;
pub use request::RequestRule;
pub use stream::GenerationSseDecoder;

use crate::{
    adapter::{AdapterError, Request},
    protocol::{
        CodecError,
        cache::CacheProjection,
        fidelity::FidelityRecords,
        native::{ReplayTarget, group_client_history},
    },
    semantic::{
        context::ExecutionHints,
        task::generation::{GenerationRequest, GenerationSemanticContract},
        value::{Presence, json_size},
    },
};
use serde_json::Value;

/// Google Files URLs select credential-owned state, not public media.
/// Reject the API host regardless of path spelling or version.
/// https://ai.google.dev/gemini-api/docs/files
pub(crate) fn check_public_media_url(raw: &str) -> Result<(), CodecError> {
    let url = url::Url::parse(raw).map_err(|_| CodecError::Invalid("Google media URL"))?;
    if url
        .host_str()
        .is_some_and(|host| host.trim_end_matches('.') == "generativelanguage.googleapis.com")
    {
        return Err(CodecError::Unsupported(
            "Google credential-owned file URI".into(),
        ));
    }
    Ok(())
}

/// The implemented semantic slice, not a Provider/model capability inventory.
pub fn semantic_contract() -> GenerationSemanticContract {
    GenerationSemanticContract {
        compaction: false,
        instructions: true,
        temperature: true,
        max_output_tokens: true,
        tools: true,
        custom_tools: false,
        structured_arguments: true,
        structured_tool_results: false,
        tool_execution_reports: true,
        tool_result_errors: true,
        tool_result_media: true,
        text_metadata: false,
        top_p: true,
        logprobs: false,
        verbosity: false,
        truncation: false,
        structured_output: false,
        reasoning: true,
        image_input: true,
        audio_input: true,
        video_input: true,
        audio_output: false,
        audio_history: false,
        file_input: true,
        parallel_tool_calls: true,
        strict_tools: false,
    }
}

/// Pure IR → `streamGenerateContent` body for one target, with the rules applied.
pub fn encode_generation_request(
    request: &GenerationRequest,
    fidelity: &FidelityRecords,
    target: &ReplayTarget,
) -> Result<(Value, Vec<RequestRule>), CodecError> {
    let request = group_client_history(request)?;
    request.validate()?;
    fidelity.require_reasoning_replay(&request, &target.origin())?;
    let mut rules = Default::default();
    let value = request::project(&request, fidelity, target, &mut rules)?;
    json_size(&value, crate::semantic::task::generation::MAX_TOTAL_BYTES)
        .map_err(|_| CodecError::Limit)?;
    Ok((value, rules.into_iter().collect()))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCaps {
    pub semantics: GenerationSemanticContract,
    pub cache: CacheProjection,
}
impl Default for TargetCaps {
    fn default() -> Self {
        Self {
            semantics: semantic_contract(),
            cache: CacheProjection::default(),
        }
    }
}
impl TargetCaps {
    fn check_context(&self, request: &Request) -> Result<bool, CodecError> {
        request.check_grouping()?;
        request.context.validate()?;
        request.delivery.validate()?;
        let mut context = request.context.clone();
        self.cache.project(&mut context)?;
        // Explicit stateless defaults do not ask the Provider for storage.
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
        if context != ExecutionHints::default()
            || request.delivery.stream == Presence::Null
            || request.n.value().is_some_and(|n| *n != 1)
        {
            return Err(CodecError::Unsupported("Gemini request context".into()));
        }
        Ok(request.cache_session.is_some())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub(crate) fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<(Value, Vec<&'static str>), AdapterError> {
        // Reasoning without this target's value is converted inside the projection.
        let stripped = request
            .without_foreign_replay(Some(&target.origin()), false)?
            .map(|(copy, _)| copy);
        let foreign = stripped.is_some();
        let request = stripped.as_ref().unwrap_or(request);
        let cache_key = self.check_context(request)?;
        request.check_semantic(&self.semantics)?;
        request.check_semantic(&semantic_contract())?;
        let semantic = &request.task.semantic;
        let mut settings = semantic.settings().clone();
        // Include selects the downstream opaque carrier, not a native control.
        settings.reasoning = settings.reasoning.with_encrypted_output(false);
        let semantic = semantic
            .clone()
            .with_settings(settings)
            .map_err(CodecError::from)?;
        let (wire, mut rules) =
            encode_generation_request(&semantic, &request.task.fidelity, target)?;
        if foreign {
            rules.push(RequestRule::OmitForeignReplay);
        }
        if cache_key {
            rules.push(RequestRule::OmitCacheKey);
        }
        rules.sort();
        rules.dedup();
        Ok((wire, rules.iter().map(|r| r.name()).collect()))
    }
    pub(crate) fn request_headers(&self) -> Vec<(String, String)> {
        vec![(
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        )]
    }
}
