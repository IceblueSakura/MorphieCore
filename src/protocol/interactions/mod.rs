//! Stateless Google Interactions v1 request projection.
//! Source: <https://ai.google.dev/api/interactions-api-v1>.
mod request;
mod schema;
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
        native::{ReplayTarget, group_client_history, normalize_stateless_defaults},
    },
    semantic::{
        context::ExecutionHints,
        task::generation::{GenerationRequest, GenerationSemanticContract},
        value::{Presence, json_size},
    },
};
use serde_json::Value;

/// Full history as steps, always streamed and never stored by the upstream.
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

/// Default binding admission, kept separate from codec carrier availability.
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
        response_effort: false,
        image_input: true,
        audio_input: true,
        video_input: false,
        audio_output: false,
        audio_history: false,
        file_input: true,
        parallel_tool_calls: true,
        strict_tools: false,
    }
}

/// Pure codec support does not activate a model or widen a public promise.
pub fn representation_contract() -> GenerationSemanticContract {
    let mut contract = semantic_contract();
    contract.structured_output = true;
    contract
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
        normalize_stateless_defaults(&mut context);
        if context != ExecutionHints::default()
            || request.delivery.stream == Presence::Null
            || request.n.value().is_some_and(|n| *n != 1)
        {
            return Err(CodecError::Unsupported(
                "Interactions request context".into(),
            ));
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
        let mut admission = request.clone();
        let mut settings = admission.task.semantic.settings().clone();
        settings.text.verbosity = Presence::Absent;
        if settings.text.format.is_absent() {
            settings.text.presence = false;
        }
        admission.task.semantic = admission
            .task
            .semantic
            .with_settings(settings)
            .map_err(CodecError::from)?;
        admission.check_semantic(&self.semantics)?;
        admission.check_semantic(&representation_contract())?;
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
        rules.extend(
            self.cache
                .losses(&request.context)
                .into_iter()
                .map(RequestRule::Cache),
        );
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
