//! OpenAI request envelopes, controls and header projection.
use crate::{
    protocol::{
        ConversionFailure, GenerationRequestEnvelope as Request,
        conversion::generation::RepresentationError,
        failure::CarrierReason,
        openai::{
            self, CodecError, GenerationApi, TargetContract, chat::envelope as chat_envelope,
            codec::GenerationCodec, lowering::lower_request, responses::envelope,
        },
    },
    semantic::{
        task::generation::{ToolChoice, ToolDefinition, ToolKind},
        value::Presence,
    },
};
use serde_json::Value;
use std::borrow::Cow;

fn with_effort_mapping<'a>(
    request: Cow<'a, Request>,
    mapping: &crate::protocol::reasoning::ReasoningEffortMapping,
    profile: GenerationApi,
    native_thinking: bool,
) -> Result<(Cow<'a, Request>, bool), CodecError> {
    mapping.validate()?;
    if openai::dialect::bailian::extended(
        request.task.semantic.settings(),
        profile,
        native_thinking,
    ) {
        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
    }
    let mut items = Cow::Borrowed(request.task.semantic.items());
    for (index, (_, item)) in request.task.semantic.items().iter().enumerate() {
        if let crate::semantic::task::generation::Item::ConfigurationUpdate(update) = item {
            let mut mapped = *update;
            openai::reasoning::admit_configuration_mapping(&mut mapped, Some(mapping), false)?;
            if mapped != *update {
                items.to_mut()[index].1 =
                    crate::semantic::task::generation::Item::ConfigurationUpdate(mapped);
            }
        }
    }
    let mut settings = request.task.semantic.settings().clone();
    if let Some(control) = settings.effort.value() {
        if control.scope != mapping.scope {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning));
        }
        let mapped = mapping.encode(control.level)?;
        settings.effort = Presence::Value(crate::semantic::task::generation::EffortControl {
            scope: control.scope,
            level: mapped,
        });
    }
    if &settings == request.task.semantic.settings() && matches!(items, Cow::Borrowed(_)) {
        return Ok((request, false));
    }
    let items = match items {
        Cow::Owned(items) => Some(items),
        Cow::Borrowed(_) => None,
    };
    let mut request = request.into_owned();
    request.task.semantic = request.task.semantic.with_settings(settings)?;
    if let Some(items) = items {
        request.task.semantic = request.task.semantic.with_items(items)?;
    }
    Ok((Cow::Owned(request), true))
}

/// OpenAI requires a schema name; Messages and Gemini formats have none. The fixed
/// name is a wire label, not a new semantic output constraint.
fn with_schema_name(request: Cow<'_, Request>) -> Result<(Cow<'_, Request>, bool), CodecError> {
    use crate::semantic::{
        task::generation::OutputConstraint,
        value::{Presence, Text},
    };
    if !matches!(
        request.task.semantic.settings().text.format,
        Presence::Value(OutputConstraint::JsonSchema { name: None, .. })
    ) {
        return Ok((request, false));
    }
    let mut settings = request.task.semantic.settings().clone();
    let Presence::Value(OutputConstraint::JsonSchema {
        name: name @ None, ..
    }) = &mut settings.text.format
    else {
        unreachable!("checked schema name");
    };
    *name = Some(Text::new("response", "schema name", 64).expect("static name"));
    let mut request = request.into_owned();
    request.task.semantic = request.task.semantic.with_settings(settings)?;
    Ok((Cow::Owned(request), true))
}

fn with_tool_error_status(
    request: Cow<'_, Request>,
) -> Result<(Cow<'_, Request>, bool), CodecError> {
    use crate::semantic::{task::generation::*, value::Text};
    if !request
            .task
            .semantic
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolResult(result) | Item::CustomResult(result) if result.is_error.is_some()))
    {
        return Ok((request, false));
    }
    let mut next = request
        .task
        .semantic
        .items()
        .iter()
        .flat_map(|(_, item)| item.part_ids())
        .filter(|id| id.scope() == LocalScope::DERIVED)
        .map(PartId::get)
        .max()
        .unwrap_or(0);
    let mut items = request.task.semantic.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolResult(result) | Item::CustomResult(result) = item
            && let Some(is_error) = result.is_error.take()
        {
            // This reports the supplied flag, not an inferred execution conclusion.
            let prefix = format!("[tool result is_error={is_error}]\n");
            match &mut result.output {
                ToolOutput::Text(text) => text.insert_str(0, &prefix),
                ToolOutput::Structured(value) => {
                    result.output = ToolOutput::Text(format!("{prefix}{}", value.value()));
                }
                ToolOutput::Parts(parts) => {
                    next = next.checked_add(1).ok_or(CodecError::Limit)?;
                    parts.insert(
                        0,
                        Part::new(
                            PartId::scoped(LocalScope::DERIVED, next),
                            InputPart::Text(
                                Text::new(prefix, "tool error status", 64)
                                    .map_err(|_| CodecError::Limit)?,
                            ),
                        ),
                    );
                }
            }
        }
    }
    let mut request = request.into_owned();
    request.task.semantic = request.task.semantic.with_items(items)?;
    Ok((Cow::Owned(request), true))
}

impl GenerationCodec {
    /// Closed Provider header projection; no inbound header map or runtime target selection.
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        mut cache: crate::protocol::cache::CacheTargetPolicy,
    ) -> Result<Vec<(String, String)>, CodecError> {
        if !self.adaptation.rules.opencode_go_headers {
            return Ok(vec![]);
        }
        cache.intersect(self.adaptation.cache);
        let session = cache.opencode_session(
            request.conversation.as_ref(),
            request.cache_session.as_ref(),
            request.inferred_affinity,
        )?;
        let mut headers = vec![(
            "user-agent".into(),
            concat!("MorphieCore/", env!("CARGO_PKG_VERSION")).into(),
        )];
        headers.push(("x-opencode-session".into(), session));
        Ok(headers)
    }
    pub fn encode_request(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetContract,
    ) -> Result<Value, ConversionFailure> {
        self.encode_request_with_evidence(request, model, contract)
            .map(|projection| projection.value)
    }
    /// The body plus the names of the degradable rules applied to it.
    pub fn encode_request_with_rules(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetContract,
    ) -> Result<(Value, Vec<&'static str>), ConversionFailure> {
        self.encode_request_with_evidence(request, model, contract)
            .map(|projection| {
                let rules = projection.evidence.rule_names();
                (projection.value, rules)
            })
    }
    pub(crate) fn encode_request_with_evidence(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetContract,
    ) -> Result<
        crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        ConversionFailure,
    > {
        let result = (|| {
            use crate::protocol::conversion::generation::Effect;
            if self.adaptation.rules.bailian_requests
                && self.adaptation.effort_mapping.is_none()
                && let Some(mapping) =
                    openai::dialect::bailian::effort_mapping(model, self.protocol)
            {
                let mut bound = self.clone();
                bound.adaptation.effort_mapping = Some(mapping);
                return bound.encode_request_with_evidence(request, model, contract);
            }
            // History acceptance and its exact wire carrier are separate facts.
            let issuer = contract
                .replay_origin
                .as_ref()
                .or(self.adaptation.scope.as_ref());
            let stripped = if self.protocol == GenerationApi::Responses
                && self.adaptation.rules.bailian_requests
                && contract.reasoning_history
                    == Some(crate::protocol::reasoning::ReasoningHistory::Accepted)
            {
                request.without_uncarried_reasoning(issuer, |reasoning| {
                    let omit = !openai::dialect::bailian::responses_reasoning_carrier(reasoning);
                    if omit && request.task.semantic.reasoning().read_history.value() == Some(&true)
                    {
                        return Err(CodecError::Unsupported(CarrierReason::Reasoning));
                    }
                    Ok(omit)
                })?
            } else {
                request.without_foreign_reasoning(issuer)?
            };

            let mut controls = vec![];
            if stripped.is_some() {
                let chat = self.protocol == GenerationApi::Chat;
                controls.push(crate::protocol::conversion::generation::Effect::new(
                    if chat {
                        "chat.transfer-visible-history.v1"
                    } else {
                        "responses.transfer-visible-history.v1"
                    },
                    crate::protocol::conversion::generation::Field::Replay,
                    crate::protocol::conversion::generation::ConversionBoundary::RequestProject,
                    crate::protocol::conversion::generation::Change::Omit,
                ));
            }
            let mut projected = stripped.map_or(Cow::Borrowed(request), Cow::Owned);
            if (self.protocol == GenerationApi::Chat || self.adaptation.rules.bailian_requests)
                && let Some((semantic, effects)) = openai::lowering::strictness::project(
                    &projected.task.semantic,
                    contract.semantics.strict_tools && !self.adaptation.rules.bailian_requests,
                )?
            {
                projected.to_mut().task.semantic = semantic;
                controls.extend(effects);
            }
            if self.adaptation.reasoning_policy.is_some()
                && self.adaptation.effort_mapping.is_some()
            {
                return Err(CodecError::invalid().into());
            }
            if let Some(policy) = &self.adaptation.reasoning_policy
                && let Some(semantic) =
                    policy.project_request(&projected.task.semantic, |settings| {
                        use crate::{
                            protocol::reasoning::ReasoningConversion, semantic::task::generation::*,
                        };
                        match policy.conversion {
                            ReasoningConversion::BudgetToEffort { .. } => {
                                self.protocol == GenerationApi::Chat
                                    && self.adaptation.rules.thinking_controls
                            }
                            _ => {
                                contract.semantics.reasoning
                                    && !self.adaptation.rules.reject_reasoning_controls
                                    && settings.effort.value().is_some_and(|effort| {
                                        effort.scope == EffortScope::Reasoning
                                            && contract
                                                .model_limits
                                                .check_values(None, [effort.level])
                                                .is_ok()
                                    })
                            }
                        }
                    })?
            {
                controls.push(policy.receipt(&projected.task.semantic, &semantic)?);
                projected.to_mut().task.semantic = semantic;
            }
            if self.protocol == GenerationApi::Responses && self.adaptation.rules.bailian_requests {
                use crate::{
                    protocol::conversion::generation::{Change, ConversionBoundary, Field},
                    semantic::task::generation::{
                        ReasoningActivation, ReasoningDisplay, ReasoningSummary,
                    },
                };
                let reasoning = projected.task.semantic.reasoning();
                let summary =
                    reasoning.presentation.visibility.value() == Some(&ReasoningDisplay::Summary);
                if summary || reasoning.encrypted_output() {
                    let mut settings = projected.task.semantic.settings().clone();
                    if summary {
                        // Auto resolves to the fixed native summarizer (or none).
                        if matches!(
                            reasoning.summary(),
                            Some(ReasoningSummary::Concise | ReasoningSummary::Detailed)
                        ) {
                            controls.push(Effect::new(
                                "responses.omit-reasoning-summary.v1",
                                Field::ReasoningSummary,
                                ConversionBoundary::RequestProject,
                                Change::Omit,
                            ));
                        }
                        settings.reasoning.presentation = Default::default();
                    }
                    if reasoning.encrypted_output() {
                        if reasoning.activation.value() != Some(&ReasoningActivation::Disabled) {
                            controls.push(Effect::new(
                                "responses.omit-encrypted-reasoning-include.v1",
                                Field::Replay,
                                ConversionBoundary::RequestProject,
                                Change::Omit,
                            ));
                        }
                        settings.reasoning = settings.reasoning.with_encrypted_output(false);
                    }
                    let semantic = projected
                        .task
                        .semantic
                        .clone()
                        .with_settings(settings)
                        .map_err(CodecError::from)?;
                    projected.to_mut().task.semantic = semantic;
                }
            }
            let request = projected.as_ref();
            if self.adaptation.rules.reject_disabled_reasoning
            && (request.task.semantic.reasoning().activation.value()
                == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                || request.task.semantic.items().iter().any(|(_, item)| matches!(
                    item, crate::semantic::task::generation::Item::ConfigurationUpdate(update)
                        if update.activation == Some(crate::semantic::task::generation::ReasoningActivation::Disabled)
                )))
        {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning).into());
        }
            if self.adaptation.rules.bailian_requests {
                openai::dialect::bailian::check(request, self.protocol, model)?;
            }
            if openai::dialect::bailian::extended(
                request.task.semantic.settings(),
                self.protocol,
                self.adaptation.rules.thinking_controls,
            ) {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning).into());
            }
            if let Some(mapping) = self.adaptation.effort_mapping {
                let input = crate::protocol::conversion::generation::ReasoningControls::request(
                    &projected.task.semantic,
                );
                let changed;
                (projected, changed) = with_effort_mapping(
                    projected,
                    mapping,
                    self.protocol,
                    self.adaptation.rules.thinking_controls,
                )?;
                if changed {
                    controls.push(Effect::Reasoning {
                        rule: crate::protocol::conversion::generation::RuleId::new(mapping.rule),
                        input,
                        output: crate::protocol::conversion::generation::ReasoningControls::request(
                            &projected.task.semantic,
                        ),
                        preset_tokens: None,
                        owner: crate::protocol::conversion::generation::Owner::Root,
                    });
                }
            }
            let request = projected.as_ref();
            if self.adaptation.rules.omit_image_detail
                && request
                    .task
                    .semantic
                    .items()
                    .iter()
                    .flat_map(|(_, item)| item.resource_uses())
                    .any(|resource| {
                        matches!(
                            resource.description,
                            crate::semantic::task::generation::ResourceDescription::Image {
                                detail: Some(_),
                                ..
                            }
                        )
                    })
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable).into());
            }
            let (projected, changed) = with_schema_name(projected)?;
            if changed {
                controls.push(crate::protocol::conversion::generation::Effect::new(
                    "openai.default-schema-name.v1",
                    crate::protocol::conversion::generation::Field::SchemaName,
                    crate::protocol::conversion::generation::ConversionBoundary::RequestProject,
                    crate::protocol::conversion::generation::Change::Default(
                        crate::protocol::conversion::generation::DefaultBasis::SelectedPolicy,
                    ),
                ));
            }
            // Without a native flag, preserve the failure distinction in model-visible text.
            // Keep source values, execution facts and every native candidate unchanged.
            let (projected, changed) = with_tool_error_status(projected)?;
            if changed {
                controls.push(crate::protocol::conversion::generation::Effect::Field {
                    rule: crate::protocol::conversion::generation::RuleId::new(
                        "openai.wrap-tool-result-errors.v1",
                    ),
                    field: ("generation.tool_results.is_error").into(),
                    boundary: ("request.encode").into(),
                    change: ("wrap").into(),
                    owner: crate::protocol::conversion::generation::Owner::Root,
                });
            }
            let request = projected.as_ref();
            request.check_context(contract.standard_context)?;
            let contract = self.contract(contract);
            if self.adaptation.rules.responses_siwc {
                if self.protocol != GenerationApi::Responses {
                    return Err(CodecError::ProfileMismatch.into());
                }
                openai::responses::dialect::siwc::check(request, contract.cache)?;
            }
            if request.delivery.streaming()
                && request
                    .task
                    .semantic
                    .settings()
                    .audio
                    .value()
                    .is_some_and(|a| {
                        a.format != crate::semantic::task::generation::AudioFormat::Pcm16
                    })
            {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable).into());
            }
            let custom_output = match request.task.semantic.tool_choice() {
                Some(ToolChoice::None | ToolChoice::Specific(_)) => false,
                Some(ToolChoice::Allowed { tools, .. }) => {
                    tools.iter().any(|tool| tool.kind == ToolKind::Custom)
                }
                _ => true,
            };
            if self.protocol == GenerationApi::Chat
                && (request.delivery.streaming()
                    || contract.adaptation.rules.responses_forced_stream)
                && custom_output
                && request
                    .task
                    .semantic
                    .tools()
                    .iter()
                    .flat_map(ToolDefinition::leaves)
                    .any(|tool| matches!(tool, ToolDefinition::Custom(_)))
            {
                return Err(CodecError::Unsupported(CarrierReason::Tools).into());
            }
            let mut context = request.context.clone();
            contract.cache.project_request(
                &mut context,
                request.conversation.as_ref(),
                request.inferred_affinity,
            )?;

            controls.extend(
                contract
                    .cache
                    .applied_rules(&request.context)
                    .into_iter()
                    .filter_map(|r| r.evidence()),
            );
            use crate::protocol::attribution::{self, Carrier};
            // SIWC has no identity carrier whatever the configured contract says.
            let identity = attribution::project(
                request,
                if contract.identity_hints && !self.adaptation.rules.responses_siwc {
                    Carrier::OpenAi
                } else {
                    Carrier::None
                },
            )?;
            context.identity = identity.identity;
            for rule in identity.losses {
                controls.extend(rule.evidence());
            }
            if request.cache_session.is_some() && !contract.cache.session_id {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable).into());
            }
            // Candidate preflight uses this same encoder, so invalid header carriers cannot reach I/O.
            self.request_headers(request, contract.cache)?;
            if self.adaptation.rules.reject_reasoning_controls
                && (request.task.semantic.reasoning().presence()
                    == crate::semantic::task::generation::ReasoningPresence::Present
                    || request.task.semantic.reasoning().encrypted_output()
                    || !request.task.semantic.settings().effort.is_absent())
            {
                return Err(CodecError::Unsupported(CarrierReason::Reasoning).into());
            }
            if self.protocol == GenerationApi::Chat && !context.max_tool_calls.is_absent() {
                return Err(RepresentationError::UnmigratedSemantic.into());
            }
            let target = lower_request(
                &request.task.semantic,
                &request.task.fidelity,
                self.protocol,
                contract,
            )
            .map_err(|error| {
                crate::protocol::ConversionFailure::from(error)
                    .at(crate::protocol::conversion::generation::ConversionBoundary::RequestProject)
            })?;
            for evidence in target.projection().iter().cloned() {
                controls.push(evidence);
            }
            let mut value = match self.protocol {
                GenerationApi::Chat => {
                    let mut options = request.delivery.options.clone();
                    // Cross-protocol delivery still needs actual upstream usage; request
                    // its wire report rather than synthesizing any counters.
                    if request.source != Some(self.protocol.into()) && request.delivery.streaming()
                    {
                        let mut common = options.value().cloned().unwrap_or_default();
                        common.include_usage = Presence::Value(true);
                        options = Presence::Value(common);
                    }
                    chat_envelope::encode_request(
                        &target,
                        &chat_envelope::RequestContext {
                            model: model.into(),
                            n: if request.source == Some(self.protocol.into()) {
                                request.n.clone()
                            } else {
                                Presence::Absent
                            },
                            stream: request.delivery.stream.clone(),
                            stream_options: options,
                            store: context.store,
                            cache: context.cache,
                            identity: context.identity,
                            metadata: context.metadata,
                            service_policy: request.service_policy.clone(),
                        },
                    )?
                }
                GenerationApi::Responses => {
                    let options = match &request.delivery.options {
                        Presence::Absent => Presence::Absent,
                        Presence::Null => Presence::Null,
                        Presence::Value(value) => Presence::Value(envelope::StreamOptions {
                            include_obfuscation: value.include_obfuscation.clone(),
                        }),
                    };
                    envelope::encode_request(
                        &target,
                        &envelope::RequestContext {
                            model: model.into(),
                            execution: context,
                            service_policy: request.service_policy.clone(),
                            delivery: envelope::Delivery {
                                stream: request.delivery.stream.clone(),
                                options,
                            },
                        },
                    )?
                }
            };
            if self.adaptation.rules.responses_forced_stream {
                if self.protocol != GenerationApi::Responses {
                    return Err(CodecError::ProfileMismatch.into());
                }
                value["stream"] = Value::Bool(true);
                // Empty instructions carry no new task instruction; never substitute a prompt.
                if !self.adaptation.rules.responses_siwc && value.get("instructions").is_none() {
                    value["instructions"] = Value::String(String::new());
                }
            }
            if self.protocol == GenerationApi::Chat
                && self.adaptation.rules.reasoning_alias
                && let Some(messages) = value.get_mut("messages").and_then(Value::as_array_mut)
            {
                for message in messages {
                    openai::dialect::adapter_shapes::encode_message(message);
                }
            }
            if let Some(session) = &request.cache_session
                && !self.adaptation.rules.opencode_go_headers
            {
                value["session_id"] = serde_json::json!(session.as_str());
            }
            if self.adaptation.rules.require_parameters {
                // Fixed adapter policy, not a user-controlled routing extension.
                value["provider"] = serde_json::json!({"require_parameters": true});
            }
            crate::semantic::value::json_size(
                &value,
                crate::semantic::task::generation::MAX_TOTAL_BYTES,
            )
            .map_err(|_| CodecError::Limit)?;
            Ok(crate::protocol::conversion::generation::Conversion {
                value,
                evidence: controls.into_iter().collect(),
            })
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
    }
}

impl Request {
    pub fn check(
        &self,
        contract: &TargetContract,
    ) -> Result<crate::semantic::task::generation::GenerationRequirements, ConversionFailure> {
        self.check_context(contract.standard_context)?;
        Ok(crate::protocol::openai::lowering::check(
            &self.task.semantic,
            contract.clone(),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::openai::Dialect;

    #[test]
    fn error_wrapping_keeps_media_owners_and_rejects_growth_beyond_the_text_bound() {
        use crate::semantic::task::generation::*;
        let body = serde_json::json!({"model":"m","max_tokens":64,"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"lookup","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call","is_error":true,
                "content":[{"type":"image","source":{"type":"url","url":"https://example.invalid/image.png"}}]}]}
        ]});
        let mut source = crate::adapter::ClientAdapter::Messages
            .decode_client_request(body.to_string().as_bytes())
            .unwrap();
        source.source = None; // Typed error semantics do not depend on the source protocol label.
        let before = source.clone();
        let (projected, changed) = with_tool_error_status(Cow::Borrowed(&source)).unwrap();
        assert!(changed);
        let Item::ToolResult(result) = &projected.task.semantic.items()[1].1 else {
            panic!("result")
        };
        let ToolOutput::Parts(parts) = &result.output else {
            panic!("parts")
        };
        assert_eq!(
            parts[0].content.as_text(),
            Some("[tool result is_error=true]\n")
        );
        assert_eq!(parts[0].id.scope(), LocalScope::DERIVED);
        let Item::ToolResult(original) = &source.task.semantic.items()[1].1 else {
            panic!("source result")
        };
        let ToolOutput::Parts(original_parts) = &original.output else {
            panic!("source parts")
        };
        assert_eq!(&parts[1..], original_parts);
        assert_eq!(source, before);
        let wire = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None)
            .encode_request(&source, "m", &TargetContract::full())
            .unwrap();
        assert_eq!(
            wire["input"][1]["output"][0]["text"],
            "[tool result is_error=true]\n"
        );
        assert_eq!(
            wire["input"][1]["output"][1]["image_url"],
            "https://example.invalid/image.png"
        );
        let mut items = source.task.semantic.items().to_vec();
        let Item::ToolResult(result) = &mut items[1].1 else {
            unreachable!()
        };
        result.output = ToolOutput::Text("x".repeat(MAX_TEXT_BYTES));
        source.task.semantic = source.task.semantic.with_items(items).unwrap();
        let oversized_source = source.clone();
        assert!(with_tool_error_status(Cow::Borrowed(&source)).is_err());
        assert_eq!(source, oversized_source);
    }

    #[test]
    fn schema_naming_and_tool_error_wrapping_are_candidate_local() {
        use serde_json::json;
        for profile in [GenerationApi::Chat, GenerationApi::Responses] {
            let target = GenerationCodec::new(profile, Dialect::Standard, None);
            for marker in [None, Some(false), Some(true)] {
                let mut result =
                    json!({"type":"tool_result","tool_use_id":"call","content":"feedback"});
                if let Some(marker) = marker {
                    result["is_error"] = json!(marker);
                }
                let body = json!({"model":"m","max_tokens":64,
                "output_config":{"format":{"type":"json_schema","schema":{
                    "type":"object","properties":{"ok":{"type":"boolean"}},
                    "required":["ok"],"additionalProperties":false}}},
                "messages":[
                    {"role":"assistant","content":[{"type":"tool_use","id":"call","name":"lookup","input":{}}]},
                    {"role":"user","content":[result]}
                ]});
                let source = crate::adapter::ClientAdapter::Messages
                    .decode_client_request(body.to_string().as_bytes())
                    .unwrap();
                let before = source.clone();
                let projected = target
                    .encode_request_with_evidence(&source, "m", &TargetContract::full())
                    .unwrap();
                assert!(
                    projected
                        .evidence
                        .contains_rule("openai.default-schema-name.v1")
                );
                assert_eq!(
                    projected
                        .evidence
                        .contains_rule("openai.wrap-tool-result-errors.v1"),
                    marker.is_some()
                );
                let content = marker.map_or_else(
                    || "feedback".into(),
                    |flag| format!("[tool result is_error={flag}]\nfeedback"),
                );
                match profile {
                    GenerationApi::Chat => {
                        assert_eq!(
                            projected.value["response_format"]["json_schema"]["name"],
                            "response"
                        );
                        assert_eq!(
                            projected.value["messages"][1],
                            json!({"role":"tool","tool_call_id":"call","content":content})
                        );
                    }
                    GenerationApi::Responses => {
                        assert_eq!(projected.value["text"]["format"]["name"], "response");
                        assert_eq!(
                            projected.value["input"][1],
                            json!({"type":"function_call_output","call_id":"call","output":content})
                        );
                    }
                }
                assert_eq!(source, before);
                let mut invalid = source.clone();
                invalid.context.metadata = Presence::Value(Default::default());
                let before = invalid.clone();
                let mut caps = TargetContract::full();
                caps.standard_context = false;
                assert!(matches!(
                    target.encode_request(&invalid, "m", &caps),
                    Err(ConversionFailure {
                        cause: crate::protocol::ConversionCause::Representation(
                            RepresentationError::UnmigratedSemantic
                        ),
                        ..
                    })
                ));
                assert_eq!(invalid, before);
            }
        }
    }

    #[test]
    fn siwc_rejects_final_controls_and_ungrouped_tools_instead_of_dropping_them() {
        let client = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let target = GenerationCodec::new(GenerationApi::Responses, Dialect::Siwc, None);
        let contract = TargetContract::full();
        let body = serde_json::json!({"model":"synthetic","input":"hello",
            "tools":[{"type":"namespace","name":"ops","description":"operations",
                "tools":[{"type":"function","name":"status","strict":false}]}]});
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
        let wire = target
            .encode_request(&request, "selected-slug", &contract)
            .unwrap();
        assert_eq!(wire["stream"], true);
        assert_eq!(wire["store"], false);
        assert!(wire["input"].is_array());
        assert_eq!(wire["tools"], body["tools"]);
        for (field, value) in [
            ("max_output_tokens", serde_json::json!(32)),
            ("temperature", serde_json::json!(0.5)),
            ("top_p", serde_json::json!(0.8)),
            ("max_tool_calls", serde_json::json!(1)),
            ("metadata", serde_json::json!({})),
            ("background", serde_json::json!(false)),
            ("conversation", Value::Null),
            ("previous_response_id", Value::Null),
            ("prompt", Value::Null),
            ("moderation", Value::Null),
            ("prompt_cache_retention", serde_json::json!("24h")),
        ] {
            let mut edited = body.clone();
            edited[field] = value;
            let request = client
                .decode_request(edited.to_string().as_bytes())
                .unwrap();
            assert!(
                target
                    .encode_request(&request, "selected-slug", &contract)
                    .is_err(),
                "{field}"
            );
        }
        // Identity hints are omitted, not sent and not rejected.
        for (field, value) in [
            ("safety_identifier", serde_json::json!("identity")),
            ("user", Value::Null),
        ] {
            let mut edited = body.clone();
            edited[field] = value;
            let request = client
                .decode_request(edited.to_string().as_bytes())
                .unwrap();
            let wire = target
                .encode_request(&request, "selected-slug", &contract)
                .unwrap();
            assert!(wire.get(field).is_none(), "{field}");
        }
        let mut edited = request.clone();
        let mut settings = edited.task.semantic.settings().clone();
        settings.controls.max_output_tokens = Some(8);
        edited.task.semantic = edited.task.semantic.with_settings(settings).unwrap();
        assert!(
            target
                .encode_request(&edited, "selected-slug", &contract)
                .is_err()
        );
        let mut flat = body;
        flat["tools"] = serde_json::json!([{"type":"function","name":"status","strict":false}]);
        let flat = client.decode_request(flat.to_string().as_bytes()).unwrap();
        assert!(
            target
                .encode_request(&flat, "selected-slug", &contract)
                .is_err()
        );
        let mut approved = contract.clone();
        approved.cache.omit_day_retention = true;
        let mut extended = request.clone();
        extended.context.cache.prompt_cache_retention =
            Presence::Value(crate::semantic::context::CacheRetention::Day);
        let before = extended.clone();
        let projected = target
            .encode_request_with_evidence(&extended, "selected-slug", &approved)
            .unwrap();
        assert!(projected.value.get("prompt_cache_retention").is_none());
        assert!(projected.evidence.iter().any(|loss| matches!(loss,
            e
                if e.rule() == "cache.omit-day-retention.v1" && e.semantic_field() == crate::protocol::conversion::generation::Field::CacheRetention
        )));
        assert_eq!(extended, before);
        // The exception does not move SIWC validation after the generic empty-options loss.
        extended.context.cache.prompt_cache_options = Presence::Value(Default::default());
        assert!(
            target
                .encode_request(&extended, "selected-slug", &approved)
                .is_err()
        );
        extended.context.cache.prompt_cache_options = Presence::Absent;
        extended.context.cache.prompt_cache_retention =
            Presence::Value(crate::semantic::context::CacheRetention::InMemory);
        assert!(
            target
                .encode_request(&extended, "selected-slug", &approved)
                .is_err()
        );
    }
}
