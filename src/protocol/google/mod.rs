//! Google generateContent upstream: request projection and streamed intake over the shared
//! IR. Upstream only; no client codec, OpenAI profile or wire switches.
mod logprobs;
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
        top_k: false,
        stop_sequences: false,
        presence_penalty: false,
        frequency_penalty: false,
        seed: false,
        logprobs: false,
        verbosity: false,
        truncation: false,
        structured_output: false,
        reasoning: true,
        response_effort: false,
        image_input: true,
        audio_input: true,
        video_input: true,
        audio_output: false,
        audio_history: false,
        file_input: true,
        parallel_tool_calls: true,
        strict_tools: false,
        normalization_default_tools: false,
        explicit_cache: false,
        automatic_prefix_cache: false,
    }
}

/// Pure codec support does not activate a model or widen a public promise.
pub fn representation_contract() -> GenerationSemanticContract {
    let mut contract = semantic_contract();
    contract.structured_output = true;
    contract.logprobs = true;
    contract.top_k = true;
    contract.stop_sequences = true;
    contract.presence_penalty = true;
    contract.frequency_penalty = true;
    contract.seed = true;
    contract
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
    pub reasoning_policy: Option<crate::protocol::reasoning::ReasoningRequestPolicy>,
    pub reasoning_as_text: bool,
}
impl Default for TargetCaps {
    fn default() -> Self {
        Self {
            semantics: semantic_contract(),
            cache: CacheProjection::default(),
            reasoning_policy: None,
            reasoning_as_text: false,
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
            return Err(CodecError::Unsupported("Gemini request context".into()));
        }
        Ok(request.cache_session.is_some())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub(crate) fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<crate::protocol::RequestProjection, AdapterError> {
        use crate::lowering::generation::RequestControlLoss as Loss;
        let mut projected = std::borrow::Cow::Borrowed(request);
        let mut reasoning_rule = None;
        let mut controls = vec![];
        if let Some(policy) = &self.reasoning_policy
            && let Some(semantic) = policy.project_request(&request.task.semantic)?
        {
            controls.push(policy.receipt(&request.task.semantic, &semantic)?);
            projected.to_mut().task.semantic = semantic;
            reasoning_rule = Some(policy.rule);
        }
        let request = projected.as_ref();
        if request.task.semantic.reasoning().budget_tokens.is_some()
            && !self.reasoning_policy.as_ref().is_some_and(|policy| {
                matches!(
                    policy.conversion,
                    crate::protocol::reasoning::ReasoningConversion::EffortToBudget { .. }
                )
            })
        {
            return Err(CodecError::Unsupported("undeclared model thinking budget".into()).into());
        }
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
        if !self.reasoning_as_text && rules.contains(&RequestRule::ReasoningAsText) {
            return Err(CodecError::Unsupported("reasoning as text disabled".into()).into());
        }
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
        for rule in &rules {
            use crate::semantic::task::generation::{InstructionAuthority, ReasoningDisplay};
            match rule {
                RequestRule::OmitVerbosity => controls.push(Loss::Verbosity {
                    rule: rule.name(),
                    input: request.task.semantic.text_options().verbosity.clone(),
                    output: Presence::Absent,
                }),
                RequestRule::SummarizeReasoning => controls.push(Loss::Summary {
                    rule: rule.name(),
                    input: request.task.semantic.reasoning().summary.clone(),
                    output: ReasoningDisplay::Summary,
                }),
                RequestRule::DeveloperAsSystem => controls.push(Loss::Role {
                    rule: rule.name(),
                    input: InstructionAuthority::Developer,
                    output: InstructionAuthority::System,
                }),
                _ => {}
            }
        }
        let mut rules: Vec<_> = rules.iter().map(|r| r.name()).collect();
        rules.extend(reasoning_rule);
        Ok(crate::protocol::RequestProjection {
            value: wire,
            rules,
            controls,
        })
    }
    pub(crate) fn request_headers(&self) -> Vec<(String, String)> {
        vec![(
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        )]
    }
}
