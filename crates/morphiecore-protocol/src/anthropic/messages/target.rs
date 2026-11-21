//! Native target projection and dialect policy; no OpenAI profile or wire switches.
use super::Profile;
use crate::lowering::disposition::Dispositions;
use crate::{
    CodecError, ConversionFailure, GenerationRequestEnvelope as Request,
    cache::CacheTargetPolicy,
    failure::CarrierReason,
    native::{ReplayTarget, normalize_stateless_defaults},
    semantic::{
        context::ExecutionHints,
        task::generation::{GenerationRequest, Item, ReplayFormat},
    },
};

pub(crate) fn check_signed_configuration_prefix(
    source: &GenerationRequest,
    projected: &GenerationRequest,
) -> Result<(), CodecError> {
    let mut changed_prefix = false;
    for (id, item) in source.items() {
        match item {
            Item::ConfigurationUpdate(_) => {
                changed_prefix |= !projected
                    .items()
                    .iter()
                    .any(|(owner, projected)| owner == id && projected == item);
            }
            Item::Reasoning(reasoning)
                if changed_prefix
                    && reasoning.replay.as_ref().is_some_and(|value| {
                        matches!(
                            value.format(),
                            ReplayFormat::AnthropicMessagesThinking
                                | ReplayFormat::AnthropicMessagesRedactedThinking
                        )
                    }) =>
            {
                // A message-local marker belongs to subsequent thinking's prefix.
                // A root-parameter change alone does not invalidate that prefix.
                return Err(CodecError::Unsupported(CarrierReason::Reasoning));
            }
            _ => {}
        }
    }
    Ok(())
}

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
/// Target-specific dialect constraints and policy enforcement for Anthropic Messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetContract {
    pub profile: Profile,
    pub dialect: Dialect,
    pub reasoning_history: Option<crate::reasoning::ReasoningHistory>,
    pub dispositions: Dispositions,
    pub model_limits: crate::model_limits::ModelLimits,
    pub cache: CacheTargetPolicy,
    pub reasoning_policy: Option<crate::reasoning::ReasoningRequestPolicy>,
    /// Optional default output token ceiling injected when client omits `max_tokens`.
    pub default_output_tokens: Option<u64>,
    /// Default TTL for automatic turn-boundary cache marker placement.
    pub automatic_cache_default: Option<crate::semantic::task::generation::PrefixCacheTtl>,
}
impl TargetContract {
    pub fn new(profile: Profile, dialect: Dialect) -> Self {
        Self {
            profile,
            dialect,
            reasoning_history: None,
            dispositions: profile.semantic_contract(),
            model_limits: Default::default(),
            reasoning_policy: None,
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
        if let Some(policy) = request.service_policy.as_ref() {
            super::service::policy_wire(*policy)?;
        }
        let mut context = request.context.clone();
        normalize_stateless_defaults(&mut context);
        // Native attribution is encoded separately; other hints retain named loss.
        context.identity = Default::default();
        let go = self.dialect == Dialect::OpenCodeGo;
        if go {
            self.cache.session_id(
                request.conversation.as_ref(),
                request.cache_session.as_ref(),
                request.inferred_affinity,
                crate::cache::SessionNamespace::OpenCodeGo,
            )?;
            // Go validates every declaration even with tool_choice:none.
            // Nested composition remains legal; never erase a root constraint.
            for (declaration, tool) in request.task.semantic.function_tools().enumerate() {
                if tool.parameters.as_ref().is_some_and(|schema| {
                    ["anyOf", "allOf", "oneOf"]
                        .iter()
                        .any(|keyword| schema.root().get(*keyword).is_some())
                }) {
                    return Err(CodecError::Unsupported(CarrierReason::ToolConstraint {
                        declaration,
                        constraint: crate::failure::ToolConstraint::Composition,
                    }));
                }
            }
        }
        if context != ExecutionHints::default() || request.n.value().is_some_and(|n| *n != 1) {
            return Err(CodecError::Unsupported(CarrierReason::Unavailable));
        }
        Ok(())
    }
    /// The target labels come from the trusted endpoint binding, never the request.
    pub fn encode_request(
        &self,
        request: &Request,
        target: &ReplayTarget,
    ) -> Result<crate::conversion::generation::Conversion<serde_json::Value>, ConversionFailure>
    {
        let result = (|| {
            self.validate()?;
            request.task.semantic.validate().map_err(CodecError::from)?;
            let request_projection = crate::lowering::request::project(
                request,
                crate::lowering::request::Target::Messages(self),
                Some(target),
            )?;
            let request = request_projection.value.as_ref();
            let evidence = request_projection.evidence;
            self.check_context(request)?;
            let semantic = &request.task.semantic;
            let mut projection = self.profile.encode_request_envelope(
                semantic,
                &request.task.fidelity,
                target,
                &request.service_policy,
            )?;
            let identity = &request.context.identity.user;
            crate::anthropic::messages::wire::request::validate_user_id(identity)?;
            if let Some(value) = identity {
                let mut metadata = serde_json::Map::new();
                metadata.insert("user_id".into(), serde_json::json!(value));
                projection.value["metadata"] = metadata.into();
            }
            projection.value["stream"] = serde_json::json!(true);
            projection.evidence.extend_prefer_summary(evidence);

            super::shape::bounded(&projection.value)?;
            Ok(projection)
        })();
        result.map_err(|error: crate::ConversionFailure| {
            error.at(crate::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
    /// Fixed headers plus the betas the encoded body actually uses.
    pub fn request_headers(
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
            let session = self.cache.session_id(
                request.conversation.as_ref(),
                request.cache_session.as_ref(),
                request.inferred_affinity,
                crate::cache::SessionNamespace::OpenCodeGo,
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
