//! OpenAI request envelopes, controls and header projection.
use crate::protocol::openai::adapter::{Adapter, AdapterError, Request};
use crate::{
    lowering::generation::RepresentationError,
    protocol::openai::{
        self, CodecError, DecodeDirection, IgnoredFields, Profile, TargetCaps,
        chat::envelope as chat_envelope, lowering::lower_request, responses::envelope,
    },
    semantic::{
        context::{DeliveryIntent, ExecutionHints, StreamOptions},
        task::generation::{ToolChoice, ToolDefinition, ToolKind},
        value::Presence,
    },
};
use serde_json::Value;

fn with_effort_mapping(
    request: &Request,
    mapping: &crate::protocol::reasoning::ReasoningEffortMapping,
) -> Result<Option<Request>, CodecError> {
    mapping.validate()?;
    if openai::reasoning::requires_extended_controls(request.task.semantic.settings()) {
        return Err(CodecError::Unsupported("reasoning controls".into()));
    }
    let mut items = request.task.semantic.items().to_vec();
    for (_, item) in &mut items {
        if let crate::semantic::task::generation::Item::ConfigurationUpdate(update) = item {
            openai::reasoning::admit_configuration_mapping(update, Some(mapping), false)?;
        }
    }
    let mut settings = request.task.semantic.settings().clone();
    if let Some(control) = settings.effort.value() {
        if control.scope != mapping.scope {
            return Err(CodecError::Unsupported("reasoning mapping scope".into()));
        }
        let mapped = mapping.encode(control.level)?;
        settings.effort = Presence::Value(crate::semantic::task::generation::EffortControl {
            scope: control.scope,
            level: mapped,
        });
    }
    if &settings == request.task.semantic.settings() && items == request.task.semantic.items() {
        return Ok(None);
    }
    let mut copy = request.clone();
    copy.task.semantic = copy
        .task
        .semantic
        .with_settings(settings)?
        .with_items(items)?;
    Ok(Some(copy))
}

/// OpenAI requires a schema name; Messages and Gemini formats have none. The fixed
/// name is a wire label, like the fixed `max_tokens` default on Messages targets.
fn with_schema_name(request: &Request) -> Result<Option<Request>, CodecError> {
    use crate::semantic::{
        task::generation::OutputConstraint,
        value::{Presence, Text},
    };
    let mut settings = request.task.semantic.settings().clone();
    let Presence::Value(OutputConstraint::JsonSchema {
        name: name @ None, ..
    }) = &mut settings.text.format
    else {
        return Ok(None);
    };
    *name = Some(Text::new("response", "schema name", 64).expect("static name"));
    let mut copy = request.clone();
    copy.task.semantic = copy.task.semantic.with_settings(settings)?;
    Ok(Some(copy))
}

impl Adapter {
    /// Closed decode: every field must be registered.
    pub fn decode_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        self.decode_in(bytes, DecodeDirection::Request)
    }
    /// Downstream client decode: unregistered fields are ignored and their names
    /// kept for diagnostics; nothing unregistered reaches the IR.
    pub fn decode_client_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        let ignored = IgnoredFields::default();
        let mut request = self.decode_in(bytes, DecodeDirection::Client(&ignored))?;
        request.ignored_fields = ignored.into_names();
        Ok(request)
    }
    fn decode_in(
        &self,
        bytes: &[u8],
        direction: DecodeDirection<'_>,
    ) -> Result<Request, CodecError> {
        let mut value = crate::protocol::json::decode(bytes)?;
        let cache_session = if let Some(session) = value
            .as_object_mut()
            .and_then(|o| o.shift_remove("session_id"))
        {
            if !self.adaptation.cache.session_id {
                return Err(CodecError::Unsupported("session_id".into()));
            }
            Some(crate::protocol::cache::CacheSession::new(
                session.as_str().ok_or(CodecError::Invalid("session_id"))?,
            )?)
        } else {
            None
        };
        let mut request = match self.protocol {
            Profile::Chat => {
                let decoded =
                    chat_envelope::decode_request_with(&value, &self.adaptation, direction)?;
                Request {
                    task: decoded.task,
                    model: decoded.context.model,
                    context: ExecutionHints {
                        cache: decoded.context.cache,
                        identity: decoded.context.identity,
                        metadata: decoded.context.metadata,
                        ..Default::default()
                    },
                    service_policy: decoded.context.service_policy,
                    delivery: DeliveryIntent {
                        stream: decoded.context.stream,
                        options: decoded.context.stream_options,
                    },
                    source: Some(self.protocol.into()),
                    conversation: None,
                    cache_session,
                    inferred_affinity: None,
                    ignored_fields: vec![],
                    n: decoded.context.n,
                }
            }
            Profile::Responses => {
                let decoded = envelope::decode_request_in(&value, direction)?;
                let options = match decoded.context.delivery.options {
                    Presence::Absent => Presence::Absent,
                    Presence::Null => Presence::Null,
                    Presence::Value(value) => Presence::Value(StreamOptions {
                        include_obfuscation: value.include_obfuscation,
                        ..Default::default()
                    }),
                };
                Request {
                    task: decoded.task,
                    model: decoded.context.model,
                    context: decoded.context.execution,
                    service_policy: decoded.context.service_policy,
                    delivery: DeliveryIntent {
                        stream: decoded.context.delivery.stream,
                        options,
                    },
                    source: Some(self.protocol.into()),
                    conversation: None,
                    cache_session,
                    inferred_affinity: None,
                    ignored_fields: vec![],
                    n: Presence::Absent,
                }
            }
        };
        request.delivery.validate()?;
        if let Some(scope) = &self.adaptation.scope {
            request.task.fidelity.bind_replay_origin(scope)?;
        }
        for (_, item) in request.task.semantic.items() {
            if let crate::semantic::task::generation::Item::Message(m) = item {
                for p in &m.parts {
                    if matches!(
                        p.content,
                        crate::semantic::task::generation::ContentPart::Audio(_)
                            | crate::semantic::task::generation::ContentPart::AudioReference(_)
                    ) {
                        request.task.fidelity.record_audio(
                            p.id,
                            &p.content,
                            self.adaptation.scope.clone(),
                        )?;
                    }
                }
            }
        }
        Ok(request)
    }
}
impl Adapter {
    /// Closed Provider header projection; no inbound header map or runtime target selection.
    pub(crate) fn request_headers(
        &self,
        request: &Request,
        mut cache: crate::protocol::cache::CacheProjection,
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
        contract: &TargetCaps,
    ) -> Result<Value, AdapterError> {
        self.encode_request_with_rules(request, model, contract)
            .map(|(value, _)| value)
    }
    /// The body plus the names of the degradable rules applied to it.
    pub fn encode_request_with_rules(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetCaps,
    ) -> Result<(Value, Vec<&'static str>), AdapterError> {
        self.encode_request_with_evidence(request, model, contract)
            .map(|projection| (projection.value, projection.rules))
    }
    pub(crate) fn encode_request_with_evidence(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetCaps,
    ) -> Result<crate::protocol::RequestProjection, AdapterError> {
        use crate::lowering::generation::RequestControlLoss;
        // Compatibility and carrier support are separate: a same-issuer value
        // without a target carrier must fail lowering, not disappear silently.
        let issuer = contract
            .replay_origin
            .as_ref()
            .or(self.adaptation.scope.as_ref());
        let stripped = request.without_foreign_reasoning(issuer)?;
        let mut rules = vec![];
        let mut controls = vec![];
        if stripped.is_some() {
            let chat = self.protocol == Profile::Chat;
            rules.push(if chat {
                "chat.transfer-visible-history.v1"
            } else {
                "responses.transfer-visible-history.v1"
            });
        }
        let request = stripped.as_ref().unwrap_or(request);
        if self.adaptation.reasoning_policy.is_some() && self.adaptation.effort_mapping.is_some() {
            return Err(CodecError::Invalid("overlapping reasoning policies").into());
        }
        let mut projected = std::borrow::Cow::Borrowed(request);
        if let Some(policy) = &self.adaptation.reasoning_policy
            && let Some(semantic) = policy.project_request(&request.task.semantic)?
        {
            controls.push(policy.receipt(&request.task.semantic, &semantic)?);
            projected.to_mut().task.semantic = semantic;
            rules.push(policy.rule);
        }
        let request = projected.as_ref();
        if self.protocol == Profile::Chat
            && self.adaptation.rules.chat_system_instructions
            && request.task.semantic.instructions().value().is_some()
        {
            rules.push("chat.instructions-as-system.v1");
            controls.push(RequestControlLoss::Role {
                rule: "chat.instructions-as-system.v1",
                input: crate::semantic::task::generation::InstructionAuthority::Developer,
                output: crate::semantic::task::generation::InstructionAuthority::System,
            });
        }
        if self.adaptation.rules.reject_disabled_reasoning
            && (request.task.semantic.reasoning().activation.value()
                == Some(&crate::semantic::task::generation::ReasoningActivation::Disabled)
                || request.task.semantic.items().iter().any(|(_, item)| matches!(
                    item, crate::semantic::task::generation::Item::ConfigurationUpdate(update)
                        if update.activation == Some(crate::semantic::task::generation::ReasoningActivation::Disabled)
                )))
        {
            return Err(CodecError::Unsupported("reasoning activation".into()).into());
        }
        if openai::reasoning::requires_extended_controls(request.task.semantic.settings()) {
            return Err(CodecError::Unsupported("reasoning controls".into()).into());
        }
        let mapped = self
            .adaptation
            .effort_mapping
            .map(|mapping| with_effort_mapping(request, mapping))
            .transpose()?
            .flatten();
        if let Some(mapped_request) = &mapped {
            controls.push(RequestControlLoss::Reasoning {
                rule: self
                    .adaptation
                    .effort_mapping
                    .expect("selected mapping")
                    .rule,
                input: crate::lowering::generation::ReasoningControls::request(
                    &request.task.semantic,
                ),
                output: crate::lowering::generation::ReasoningControls::request(
                    &mapped_request.task.semantic,
                ),
                preset_tokens: None,
            });
            rules.push(
                self.adaptation
                    .effort_mapping
                    .expect("selected mapping")
                    .rule,
            );
        }
        let request = mapped.as_ref().unwrap_or(request);
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
                            detail: Some(_)
                        }
                    )
                })
        {
            return Err(
                CodecError::Unsupported("image detail without target policy".into()).into(),
            );
        }
        let named = with_schema_name(request)?;
        if named.is_some() {
            rules.push("openai.default-schema-name.v1");
        }
        let request = named.as_ref().unwrap_or(request);
        // Messages permits this named loss only on targets without an error carrier.
        // Keep the source and every native candidate unchanged.
        let errors = if request.source == Some(crate::adapter::ClientProtocol::Messages) {
            let mut copy = request.clone();
            copy.drop_tool_error_flags()?.then_some(copy)
        } else {
            None
        };
        if errors.is_some() {
            rules.push("openai.omit-tool-result-errors.v1");
        }
        let request = errors.as_ref().unwrap_or(request);
        request.check_context(contract.standard_context)?;
        if self.adaptation.rules.responses_siwc {
            if self.protocol != Profile::Responses {
                return Err(CodecError::ProfileMismatch.into());
            }
            super::siwc::check(request)?;
        }
        if request.delivery.streaming()
            && request
                .task
                .semantic
                .settings()
                .audio
                .value()
                .is_some_and(|a| a.format != crate::semantic::task::generation::AudioFormat::Pcm16)
        {
            return Err(CodecError::Unsupported("streaming audio requires pcm16".into()).into());
        }
        let contract = self.contract(contract);
        let custom_output = match request.task.semantic.tool_choice() {
            Some(ToolChoice::None | ToolChoice::Specific(_)) => false,
            Some(ToolChoice::Allowed { tools, .. }) => {
                tools.iter().any(|tool| tool.kind == ToolKind::Custom)
            }
            _ => true,
        };
        if self.protocol == Profile::Chat
            && (request.delivery.streaming() || contract.adaptation.rules.responses_forced_stream)
            && custom_output
            && request
                .task
                .semantic
                .tools()
                .iter()
                .flat_map(ToolDefinition::leaves)
                .any(|tool| matches!(tool, ToolDefinition::Custom(_)))
        {
            return Err(CodecError::Unsupported("Chat custom tool streaming".into()).into());
        }
        let mut context = request.context.clone();
        contract.cache.project_request(
            &mut context,
            request.conversation.as_ref(),
            request.inferred_affinity,
        )?;
        rules.extend(
            contract
                .cache
                .losses(&request.context)
                .into_iter()
                .map(|r| r.name()),
        );
        if !contract.identity_hints
            && (!context.identity.user.is_absent()
                || !context.identity.safety_identifier.is_absent())
        {
            return Err(CodecError::Unsupported("identity hints".into()).into());
        }
        if request.cache_session.is_some() && !contract.cache.session_id {
            return Err(CodecError::Unsupported("session_id".into()).into());
        }
        // Candidate preflight uses this same encoder, so invalid header carriers cannot reach I/O.
        self.request_headers(request, contract.cache)?;
        if self.adaptation.rules.reject_reasoning_controls
            && (request.task.semantic.reasoning().presence()
                == crate::semantic::task::generation::ReasoningPresence::Present
                || request.task.semantic.reasoning().encrypted_output()
                || !request.task.semantic.settings().effort.is_absent())
        {
            return Err(CodecError::Unsupported("reasoning controls".into()).into());
        }
        if self.protocol == Profile::Chat && !context.max_tool_calls.is_absent() {
            return Err(RepresentationError::UnmigratedSemantic.into());
        }
        let target = lower_request(
            &request.task.semantic,
            &request.task.fidelity,
            self.protocol,
            contract,
        )?;
        for evidence in target
            .projection()
            .iter()
            .filter_map(|stage| stage.evidence())
        {
            if !rules.contains(&evidence.rule) {
                rules.push(evidence.rule);
            }
            controls.push(crate::lowering::generation::RequestControlLoss::Projection(
                evidence,
            ));
        }
        let mut value = match self.protocol {
            Profile::Chat => {
                let mut options = request.delivery.options.clone();
                // Cross-protocol delivery still needs actual upstream usage; request
                // its wire report rather than synthesizing any counters.
                if request.source != Some(self.protocol.into()) && request.delivery.streaming() {
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
                        cache: context.cache,
                        identity: context.identity,
                        metadata: context.metadata,
                        service_policy: request.service_policy.clone(),
                    },
                )?
            }
            Profile::Responses => {
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
            if self.protocol != Profile::Responses {
                return Err(CodecError::ProfileMismatch.into());
            }
            value["stream"] = Value::Bool(true);
            // Empty instructions carry no new task instruction; never substitute a prompt.
            if !self.adaptation.rules.responses_siwc && value.get("instructions").is_none() {
                value["instructions"] = Value::String(String::new());
            }
        }
        if self.protocol == Profile::Chat
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
        Ok(crate::protocol::RequestProjection {
            value,
            rules,
            controls,
        })
    }
}

impl Request {
    pub fn check(
        &self,
        contract: &TargetCaps,
    ) -> Result<crate::semantic::task::generation::GenerationRequirements, AdapterError> {
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
    fn siwc_rejects_final_controls_and_ungrouped_tools_instead_of_dropping_them() {
        let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let target = Adapter::new(Profile::Responses, Dialect::Siwc, None);
        let contract = TargetCaps::full();
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
            ("safety_identifier", serde_json::json!("identity")),
            ("user", Value::Null),
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
    }
}
