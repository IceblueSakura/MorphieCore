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

/// Beta names for the carriers present in an encoded request.
fn betas(body: &serde_json::Value) -> Vec<&'static str> {
    let messages = body["messages"].as_array().map_or(&[][..], Vec::as_slice);
    let blocks = || {
        messages
            .iter()
            .filter_map(|m| m["content"].as_array())
            .flatten()
    };
    [
        (
            messages.iter().any(|m| m.get("output_config").is_some()),
            "mid-conversation-output-config-2026-07-01",
        ),
        (
            body["thinking"].get("block_binding").is_some(),
            "thinking-binding-controls-2026-08-01",
        ),
        (
            blocks().any(|b| matches!(b["type"].as_str(), Some("tool_addition" | "tool_removal"))),
            "inline-tools-2026-09-15",
        ),
    ]
    .into_iter()
    .filter_map(|(used, name)| used.then_some(name))
    .collect()
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    Standard,
    OpenCodeGo,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SamplingPolicy {
    #[default]
    Native,
    /// Evidenced Claude 5.x policy, selected by a trusted binding.
    Fixed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetCaps {
    pub profile: Profile,
    pub dialect: Dialect,
    pub semantics: GenerationSemanticContract,
    pub cache: CacheProjection,
    pub reasoning_policy: Option<crate::protocol::reasoning::ReasoningRequestPolicy>,
    pub reasoning_as_text: bool,
    pub carriers: super::Carriers,
    pub sampling: SamplingPolicy,
    pub manual_thinking: bool,
    pub automatic_cache_default: Option<crate::semantic::task::generation::PrefixCacheTtl>,
}
impl TargetCaps {
    pub fn new(profile: Profile, dialect: Dialect) -> Self {
        Self {
            profile,
            dialect,
            semantics: profile.semantic_contract(),
            reasoning_policy: None,
            reasoning_as_text: false,
            carriers: Default::default(),
            sampling: SamplingPolicy::Native,
            manual_thinking: true,
            automatic_cache_default: None,
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
        if request.service_policy == Presence::Null {
            return Err(CodecError::Invalid("Messages service policy"));
        }
        if let Some(policy) = request.service_policy.value() {
            super::service::policy_wire(*policy)?;
        }
        let mut context = request.context.clone();
        self.cache.project(&mut context)?;
        normalize_stateless_defaults(&mut context);
        // Native attribution is encoded separately; other hints retain named loss.
        context.identity = Default::default();
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
        let sampling_loss = if self.sampling == SamplingPolicy::Fixed {
            let settings = request.task.semantic.settings();
            if settings.controls.top_k.is_some() {
                return Err(CodecError::Unsupported("target top_k".into()).into());
            }
            let values = (
                settings.controls.temperature().is_some(),
                settings.controls.top_p().is_some(),
            );
            if values.0 || values.1 {
                let mut settings = settings.clone();
                settings.controls = settings.controls.without_sampling();
                projected.to_mut().task.semantic = request
                    .task
                    .semantic
                    .clone()
                    .with_settings(settings)
                    .map_err(CodecError::from)?;
                Some(values)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(policy) = &self.reasoning_policy
            && let Some(semantic) = policy.project_request(&projected.task.semantic)?
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
        if !self.manual_thinking && request.task.semantic.reasoning().budget_tokens.is_some() {
            return Err(
                CodecError::Unsupported("manual thinking unsupported by target".into()).into(),
            );
        }
        let stripped = request.without_foreign_reasoning(Some(&target.origin()))?;
        let foreign = stripped.is_some();
        let request = stripped.as_ref().unwrap_or(request);
        self.check_context(request)?;
        request.check_semantic(&self.semantics)?;
        let mut carrier_rules = std::collections::BTreeSet::new();
        let carried =
            super::carriers::project(&request.task.semantic, self.carriers, &mut carrier_rules)?
                .map(|semantic| {
                    let mut copy = request.clone();
                    copy.task.semantic = semantic;
                    copy
                });
        let request = carried.as_ref().unwrap_or(request);
        request.check_semantic(&self.profile.representation_contract())?;
        let mut semantic = group_client_history(&request.task.semantic, true)?;
        let mut settings = semantic.settings().clone();
        let default_cache = settings.automatic_prefix_cache.is_none()
            && !crate::semantic::task::generation::GenerationRequirements::derive(&semantic)
                .explicit_cache
            && self.automatic_cache_default.is_some();
        if default_cache {
            settings.automatic_prefix_cache = self.automatic_cache_default;
        }
        // Include selects the downstream opaque carrier, not a native control.
        settings.reasoning = settings.reasoning.with_encrypted_output(false);
        semantic = semantic.with_settings(settings).map_err(CodecError::from)?;
        let mut projection = self.profile.encode_request_envelope(
            &semantic,
            &request.task.fidelity,
            target,
            &request.service_policy,
        )?;
        if default_cache {
            projection.rules.push(RequestRule::DefaultAutomaticCache);
        }
        if matches!(
            request.source,
            None | Some(crate::adapter::ClientProtocol::Messages)
        ) {
            let identity = &request.context.identity.user;
            super::request::validate_user_id(identity)?;
            if !identity.is_absent() {
                let mut metadata = serde_json::Map::new();
                super::shape::put(&mut metadata, "user_id", identity, |value| {
                    serde_json::json!(value)
                });
                projection.value["metadata"] = metadata.into();
            }
        }
        if sampling_loss.is_some() {
            projection.rules.push(RequestRule::OmitSampling);
        }
        if !self.reasoning_as_text && projection.rules.contains(&RequestRule::ReasoningAsText) {
            return Err(CodecError::Unsupported("reasoning as text disabled".into()).into());
        }
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
            projection.rules.push(RequestRule::TransferVisibleHistory);
        }
        projection.rules.extend(carrier_rules);
        projection.rules.extend(
            self.cache
                .losses(&request.context)
                .into_iter()
                .chain({
                    let mut context = request.context.clone();
                    if matches!(
                        request.source,
                        None | Some(crate::adapter::ClientProtocol::Messages)
                    ) {
                        context.identity.user = Presence::Absent;
                    }
                    crate::protocol::cache::identity_loss(&context)
                })
                .map(RequestRule::Hint),
        );
        projection.rules.sort();
        projection.control_losses.extend(controls);
        for rule in &projection.rules {
            use crate::lowering::generation::RequestControlLoss as Loss;
            use crate::semantic::task::generation::{InstructionAuthority, ReasoningDisplay};
            let settings = request.task.semantic.settings();
            match rule {
                RequestRule::Hint(hint) => {
                    projection
                        .control_losses
                        .extend(hint.evidence().map(Loss::Projection));
                }
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
                    temperature: sampling_loss.is_some_and(|v| v.0),
                    top_p: sampling_loss.is_some_and(|v| v.1),
                }),
                _ => {}
            }
        }
        super::shape::bounded(&projection.value)?;
        Ok(projection)
    }
    /// Fixed headers plus the betas the encoded body actually uses.
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        body: &serde_json::Value,
    ) -> Result<Vec<(String, String)>, CodecError> {
        let mut headers = vec![
            ("anthropic-version".into(), "2023-06-01".into()),
            (
                "user-agent".into(),
                concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
            ),
        ];
        let betas = betas(body);
        if !betas.is_empty() {
            headers.push(("anthropic-beta".into(), betas.join(",")));
        }
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
#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn betas_follow_the_encoded_carriers() {
        assert!(super::betas(&json!({"messages":[{"role":"user","content":"go"}]})).is_empty());
        let body = json!({"thinking":{"type":"adaptive",
                "block_binding":{"prefix_mismatch_behavior":"drop_block"}},
            "messages":[{"role":"user","content":"go"},
                {"role":"system","content":[{"type":"tool_removal",
                    "tool":{"type":"tool_reference","name":"read"}}],
                    "output_config":{"effort":"low"}}]});
        assert_eq!(
            super::betas(&body),
            [
                "mid-conversation-output-config-2026-07-01",
                "thinking-binding-controls-2026-08-01",
                "inline-tools-2026-09-15"
            ]
        );
    }
}
