//! Native target projection and dialect policy; no OpenAI profile or wire switches.
use super::{NativeProjection, Profile, RequestRule};
use crate::{
    adapter::{AdapterError, Request},
    protocol::{
        CodecError,
        cache::CacheProjection,
        native::{ReplayTarget, group_client_history, normalize_stateless_defaults},
    },
    semantic::{
        context::ExecutionHints, task::generation::GenerationSemanticContract, value::Presence,
    },
};

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
    pub reasoning_policy: Option<&'static crate::protocol::reasoning::ReasoningRequestPolicy>,
}
impl TargetCaps {
    pub fn new(profile: Profile, dialect: Dialect) -> Self {
        Self {
            profile,
            dialect,
            semantics: profile.semantic_contract(),
            reasoning_policy: None,
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
        normalize_stateless_defaults(&mut context);
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
            || request.n.value().is_some_and(|n| *n != 1)
            || !go && request.cache_session.is_some()
        {
            return Err(CodecError::Unsupported("native request context".into()));
        }
        Ok(())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<NativeProjection, AdapterError> {
        request.task.semantic.validate().map_err(CodecError::from)?;
        let mut projected = std::borrow::Cow::Borrowed(request);
        let mut reasoning_rule = None;
        let mut controls = vec![];
        if let Some(policy) = self.reasoning_policy
            && let Some(semantic) = policy.project_request(&request.task.semantic)?
        {
            controls.push(policy.receipt(&request.task.semantic, &semantic)?);
            projected.to_mut().task.semantic = semantic;
            reasoning_rule = Some(policy.rule);
        }
        let verbosity = request.task.semantic.text_options().verbosity.clone();
        let loss = if verbosity.is_absent() {
            None
        } else {
            let copy = projected.to_mut();
            let mut settings = copy.task.semantic.settings().clone();
            settings.text.verbosity = Presence::Absent;
            if settings.text.format.is_absent() {
                settings.text.presence = false;
            }
            copy.task.semantic = copy
                .task
                .semantic
                .clone()
                .with_settings(settings)
                .map_err(CodecError::from)?;
            Some(
                crate::lowering::generation::RequestControlLoss::MessagesVerbosity {
                    input: verbosity,
                    output: Presence::Absent,
                },
            )
        };
        let request = projected.as_ref();
        // Native projection converts unsigned reasoning itself, inside the assistant turn.
        let stripped = request
            .without_foreign_replay(Some(&target.origin()), false)?
            .map(|(copy, _)| copy);
        let foreign = stripped.is_some();
        let request = stripped.as_ref().unwrap_or(request);
        self.check_context(request)?;
        request.check_semantic(&self.semantics)?;
        request.check_semantic(&self.profile.representation_contract())?;
        let mut semantic = group_client_history(&request.task.semantic)?;
        let mut settings = semantic.settings().clone();
        // Include selects the downstream opaque carrier, not a native control.
        settings.reasoning = settings.reasoning.with_encrypted_output(false);
        semantic = semantic.with_settings(settings).map_err(CodecError::from)?;
        let mut projection =
            self.profile
                .encode_generation_request(&semantic, &request.task.fidelity, target)?;
        if let Some(stream) = request.delivery.stream.value() {
            projection.value["stream"] = serde_json::json!(stream);
        }
        if let Some(loss) = loss {
            projection.control_losses.push(loss);
            projection.rules.push(RequestRule::OmitVerbosity);
        }
        if let Some(rule) = reasoning_rule {
            projection.rules.push(RequestRule::ReasoningPolicy(rule));
        }
        if foreign {
            projection.rules.push(RequestRule::OmitForeignReplay);
        }
        projection.rules.extend(
            self.cache
                .losses(&request.context)
                .into_iter()
                .map(RequestRule::Cache),
        );
        projection.rules.sort();
        projection.control_losses.extend(controls);
        for rule in &projection.rules {
            use crate::lowering::generation::RequestControlLoss as Loss;
            use crate::semantic::task::generation::{InstructionAuthority, ReasoningDisplay};
            let settings = request.task.semantic.settings();
            match rule {
                RequestRule::SummarizeReasoning => projection.control_losses.push(Loss::Summary {
                    rule: rule.name(),
                    input: settings.reasoning.summary.clone(),
                    output: ReasoningDisplay::Summary,
                }),
                RequestRule::DeveloperAsSystem => projection.control_losses.push(Loss::Role {
                    rule: rule.name(),
                    input: InstructionAuthority::Developer,
                    output: InstructionAuthority::System,
                }),
                RequestRule::OmitSampling => projection.control_losses.push(Loss::Sampling {
                    rule: rule.name(),
                    temperature: settings.controls.temperature().is_some(),
                    top_p: settings.controls.top_p().is_some(),
                }),
                _ => {}
            }
        }
        Ok(projection)
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
