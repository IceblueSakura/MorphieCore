//! Native target projection and dialect policy; no OpenAI profile or wire switches.
use super::{Profile, RequestRule};
use crate::{
    protocol::{
        CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
        cache::CacheTargetPolicy,
        failure::CarrierReason,
        native::{ReplayTarget, group_client_history, normalize_stateless_defaults},
    },
    semantic::{
        context::ExecutionHints,
        task::generation::{GenerationFeatureSet, ToolDefinition},
        value::Presence,
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
    Reject,
    /// Evidenced Claude 5.x policy, selected by a trusted binding.
    Fixed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContract {
    pub profile: Profile,
    pub dialect: Dialect,
    pub semantics: GenerationFeatureSet,
    pub model_limits: crate::protocol::model_limits::ModelLimits,
    pub cache: CacheTargetPolicy,
    pub reasoning_policy: Option<crate::protocol::reasoning::ReasoningRequestPolicy>,
    pub reasoning_as_text: bool,
    pub carriers: super::Carriers,
    pub sampling: SamplingPolicy,
    pub manual_thinking: bool,
    /// Trusted insertion policy, independent of the model maximum.
    pub default_output_tokens: Option<u64>,
    pub automatic_cache_default: Option<crate::semantic::task::generation::PrefixCacheTtl>,
}
impl TargetContract {
    pub fn new(profile: Profile, dialect: Dialect) -> Self {
        Self {
            profile,
            dialect,
            semantics: profile.semantic_contract(),
            model_limits: Default::default(),
            reasoning_policy: None,
            reasoning_as_text: false,
            carriers: Default::default(),
            sampling: SamplingPolicy::Reject,
            manual_thinking: true,
            default_output_tokens: None,
            automatic_cache_default: None,
            cache: CacheTargetPolicy {
                session_id: dialect == Dialect::OpenCodeGo,
                ..Default::default()
            },
        }
    }
    pub fn validate(&self) -> Result<(), CodecError> {
        self.model_limits.validate()?;
        if self.default_output_tokens.is_some_and(|default| {
            default == 0
                || self
                    .model_limits
                    .maximum_output_tokens
                    .is_some_and(|maximum| default > maximum)
        }) {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
    fn check_context(&self, request: &Request) -> Result<(), CodecError> {
        request.check_grouping()?;
        request.context.validate()?;
        request.delivery.validate()?;
        if request.service_policy == Presence::Null {
            return Err(CodecError::invalid());
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
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        Ok(())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        ConversionFailure,
    > {
        let result = (|| {
            self.validate()?;
            request.task.semantic.validate().map_err(CodecError::from)?;
            let mut projected = std::borrow::Cow::Borrowed(request);
            let mut reasoning_rule = None;
            let mut controls = vec![];
            let sampling_loss = if self.sampling == SamplingPolicy::Fixed {
                let settings = request.task.semantic.settings();
                if settings.controls.top_k.is_some() {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable).into());
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
                && let Some(semantic) =
                    policy.project_request(&projected.task.semantic, |settings| {
                        use crate::{
                            protocol::reasoning::ReasoningConversion, semantic::task::generation::*,
                        };
                        match policy.conversion {
                            ReasoningConversion::BudgetToEffort { .. } => {
                                self.manual_thinking
                                    && self.semantics.reasoning
                                    && settings.reasoning.budget_tokens.is_some_and(|budget| {
                                        budget >= 1024
                                            && settings
                                                .controls
                                                .max_output_tokens
                                                .or(self.default_output_tokens)
                                                .is_some_and(|ceiling| budget < ceiling)
                                    })
                                    && matches!(
                                        settings.reasoning.mode,
                                        Presence::Absent | Presence::Value(ReasoningMode::Budgeted)
                                    )
                            }
                            _ => settings.effort.value().is_some_and(|effort| {
                                self.semantics.response_effort
                                    && effort.scope == EffortScope::Response
                                    && super::request::native_effort(effort.level).is_ok()
                                    && self.model_limits.check_values(None, [effort.level]).is_ok()
                            }),
                        }
                    })?
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
                    settings.text.presence = Presence::Absent;
                }
                copy.task.semantic = copy
                    .task
                    .semantic
                    .clone()
                    .with_settings(settings)
                    .map_err(CodecError::from)?;
                Some(
                    crate::protocol::conversion::generation::Effect::MessagesVerbosity {
                        input: verbosity,
                        output: Presence::Absent,
                        owner: crate::protocol::conversion::generation::Owner::Root,
                    },
                )
            };
            let request = projected.as_ref();
            if !self.manual_thinking && request.task.semantic.reasoning().budget_tokens.is_some() {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning).into());
            }
            let stripped = request.without_foreign_reasoning(Some(&target.origin()))?;
            let foreign = stripped.is_some();
            let request = stripped.as_ref().unwrap_or(request);
            self.check_context(request)?;
            request.check_semantic(&self.semantics)?;
            let mut carrier_rules = std::collections::BTreeSet::new();
            let carried = super::carriers::project(
                &request.task.semantic,
                self.carriers,
                &mut carrier_rules,
            )?
            .map(|semantic| {
                let mut copy = request.clone();
                copy.task.semantic = semantic;
                copy
            });
            let request = carried.as_ref().unwrap_or(request);
            request.check_semantic(&self.profile.representation_contract())?;
            let mut semantic = group_client_history(&request.task.semantic, true)?;
            let mut settings = semantic.settings().clone();
            let default_output = settings.controls.max_output_tokens.is_none();
            if default_output {
                settings.controls.max_output_tokens = Some(
                    self.default_output_tokens
                        .ok_or(CodecError::Unsupported(CarrierReason::Unavailable))?,
                );
            }
            let default_cache = settings.automatic_prefix_cache.is_none()
                && !settings
                    .instructions
                    .value()
                    .is_some_and(|root| root.parts.iter().any(|part| part.cache.is_some()))
                && !semantic
                    .items()
                    .iter()
                    .any(|(_, i)| i.has_cache_breakpoints())
                && !semantic
                    .tools()
                    .iter()
                    .flat_map(|t| t.leaves())
                    .any(|t| matches!(t, ToolDefinition::Function(t) if t.cache.is_some()))
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
                &self.model_limits,
            )?;
            let mut selected = Vec::new();
            if default_output {
                selected.push(RequestRule::DefaultMaxTokens);
            }
            if default_cache {
                selected.push(RequestRule::DefaultAutomaticCache);
            }
            let attribution = crate::protocol::attribution::project(
                request,
                crate::protocol::attribution::Carrier::Messages,
            )?;
            let identity = &attribution.identity.user;
            crate::protocol::anthropic::messages::wire::request::validate_user_id(identity)?;
            if !identity.is_absent() {
                let mut metadata = serde_json::Map::new();
                super::shape::put(&mut metadata, "user_id", identity, |value| {
                    serde_json::json!(value)
                });
                projection.value["metadata"] = metadata.into();
            }
            if sampling_loss.is_some() {
                selected.push(RequestRule::OmitSampling);
            }
            if !self.reasoning_as_text
                && projection
                    .evidence
                    .contains_rule(RequestRule::ReasoningAsText.name())
            {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning).into());
            }
            if let Some(stream) = request.delivery.stream.value() {
                projection.value["stream"] = serde_json::json!(stream);
            }
            if let Some(loss) = loss {
                controls.push(loss);
                selected.push(RequestRule::OmitVerbosity);
            }
            if let Some(rule) = reasoning_rule {
                selected.push(RequestRule::ReasoningPolicy(rule));
            }
            if foreign {
                selected.push(RequestRule::TransferVisibleHistory);
            }
            selected.extend(carrier_rules);
            selected.extend(
                self.cache
                    .applied_rules(&request.context)
                    .into_iter()
                    .chain(attribution.losses)
                    .map(RequestRule::Hint),
            );
            selected.sort();
            selected.dedup();
            for effect in controls {
                projection.evidence.prefer_summary(effect);
            }
            for rule in selected {
                if !projection.evidence.contains_rule(rule.name()) {
                    projection
                        .evidence
                        .push(rule.effect(&request.task.semantic));
                }
            }

            super::shape::bounded(&projection.value)?;
            Ok(projection)
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
    /// Fixed headers plus the betas the encoded body actually uses.
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        body: &serde_json::Value,
    ) -> Result<Vec<(String, String)>, CodecError> {
        let mut headers = vec![
            ("anthropic-version".into(), super::API_VERSION.into()),
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
