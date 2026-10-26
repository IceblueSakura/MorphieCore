//! OpenAI request envelopes, controls and header projection.
use crate::protocol::failure::CarrierReason;
use crate::protocol::openai::adapter::{Adapter, ConversionFailure, Request};
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
use std::borrow::Cow;

fn with_effort_mapping<'a>(
    request: Cow<'a, Request>,
    mapping: &crate::protocol::reasoning::ReasoningEffortMapping,
    profile: Profile,
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

impl Adapter {
    /// Closed decode: every field must be registered.
    pub fn decode_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        self.decode_in(bytes, DecodeDirection::Request, None)
    }
    /// Downstream client decode: unregistered fields are ignored and their names
    /// kept for diagnostics; nothing unregistered reaches the IR.
    pub fn decode_client_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        self.decode_client_history(bytes, None)
    }
    /// Compose caller-resolved typed Responses history with new client input.
    /// This pure codec entry point neither resolves response IDs nor authorizes storage.
    pub fn decode_client_history(
        &self,
        bytes: &[u8],
        history: Option<(
            &crate::protocol::DecodedRequest,
            &crate::protocol::DecodedResponse,
        )>,
    ) -> Result<Request, CodecError> {
        if history.is_some() && self.protocol != Profile::Responses {
            return Err(CodecError::ProfileMismatch);
        }
        let ignored = IgnoredFields::default();
        let mut request = self.decode_in(bytes, DecodeDirection::Client(&ignored), history)?;
        request.ignored_fields = ignored.into_names();
        Ok(request)
    }
    fn decode_in(
        &self,
        bytes: &[u8],
        direction: DecodeDirection<'_>,
        history: Option<(
            &crate::protocol::DecodedRequest,
            &crate::protocol::DecodedResponse,
        )>,
    ) -> Result<Request, CodecError> {
        let mut value = crate::protocol::json::decode(bytes)?;
        if self.protocol == Profile::Responses
            && self.adaptation.rules.responses_developer_is_user
            && let Some(items) = value.get_mut("input").and_then(Value::as_array_mut)
        {
            // The trusted source dialect, never a requested target, defines this
            // spelling as user content. The standard decoder still validates it.
            for item in items {
                if item.get("role").and_then(Value::as_str) == Some("developer") {
                    if let Some(parts) = item.get("content").and_then(Value::as_array)
                        && parts.iter().any(|part| {
                            !matches!(
                                part.get("type").and_then(Value::as_str),
                                Some("input_text" | "output_text" | "input_image")
                            )
                        })
                    {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                    item["role"] = serde_json::json!("user");
                }
            }
        }
        if self.protocol == Profile::Chat
            && direction.client()
            && !self.adaptation.rules.thinking_controls
            && let Some(o) = value.as_object()
            && let Some(_key) = ["enable_thinking", "thinking_budget", "top_k"]
                .into_iter()
                .find(|key| o.contains_key(*key))
        {
            return Err(CodecError::Unsupported(CarrierReason::UnknownField));
        }
        let mut native_controls = serde_json::Map::new();
        if self.adaptation.rules.thinking_controls {
            openai::dialect::bailian::source(
                &value,
                self.protocol,
                self.adaptation.rules.bailian_requests,
            )?;
            if let Some(o) = value.as_object_mut() {
                for name in openai::dialect::bailian::CONTROL_FIELDS {
                    if let Some(value) = o.shift_remove(*name) {
                        native_controls.insert((*name).into(), value);
                    }
                }
            }
        }
        let cache_session = if let Some(session) = value
            .as_object_mut()
            .and_then(|o| o.shift_remove("session_id"))
        {
            if !self.adaptation.cache.session_id {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            Some(crate::protocol::cache::CacheSession::new(
                session.as_str().ok_or(CodecError::invalid())?,
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
                let decoded = envelope::decode_request_with_history(&value, direction, history)?;
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
        if !native_controls.is_empty() || self.adaptation.rules.bailian_requests {
            let mut settings = request.task.semantic.settings().clone();
            openai::dialect::bailian::read_controls(
                &native_controls,
                &mut settings,
                self.protocol,
            )?;
            if self.adaptation.rules.bailian_requests
                && let Some(mapping) =
                    openai::dialect::bailian::effort_mapping(&request.model, self.protocol)
                && let Some(control) = settings.effort.value().copied()
            {
                let level = mapping.encode(control.level)?;
                if level != control.level {
                    settings.effort =
                        Presence::Value(crate::semantic::task::generation::EffortControl {
                            scope: control.scope,
                            level,
                        });
                    request.task.evidence.record_effort_mapping(
                        mapping.rule,
                        control,
                        crate::semantic::task::generation::EffortControl {
                            scope: control.scope,
                            level,
                        },
                        crate::lowering::generation::ConversionBoundary::RequestDecode,
                        crate::lowering::generation::Owner::Root,
                    );
                }
            }
            if self.adaptation.rules.bailian_requests && self.protocol == Profile::Responses {
                for tool in settings.tools.iter_mut().flatten() {
                    if let crate::semantic::task::generation::ToolDefinition::Function(tool) = tool
                        && tool.strict
                            == crate::semantic::task::generation::FunctionStrictness::Omitted(
                                crate::semantic::task::generation::StrictDefault::NormalizeSchema,
                            )
                    {
                        tool.strict =
                            crate::semantic::task::generation::FunctionStrictness::Omitted(
                                crate::semantic::task::generation::StrictDefault::NonStrict,
                            );
                    }
                }
            }
            request.task.semantic = request.task.semantic.with_settings(settings)?;
        }
        if self.adaptation.rules.bailian_requests {
            openai::dialect::bailian::check(&request, self.protocol, &request.model)?;
        }
        request.delivery.validate()?;
        let retained = history.map_or(0, |(input, output)| {
            input.semantic.items().len() + output.semantic.items().len()
        });
        if let Some(scope) = &self.adaptation.scope {
            request
                .task
                .fidelity
                .bind_replay_origin_for_items(scope, &request.task.semantic.items()[retained..])?;
        }
        for (_, item) in &request.task.semantic.items()[retained..] {
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
    ) -> Result<Value, ConversionFailure> {
        self.encode_request_with_evidence(request, model, contract)
            .map(|projection| projection.value)
    }
    /// The body plus the names of the degradable rules applied to it.
    pub fn encode_request_with_rules(
        &self,
        request: &Request,
        model: &str,
        contract: &TargetCaps,
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
        contract: &TargetCaps,
    ) -> Result<crate::lowering::generation::Conversion<serde_json::Value>, ConversionFailure> {
        let result = (|| {
            use crate::lowering::generation::Effect;
            if self.adaptation.rules.bailian_requests
                && self.adaptation.effort_mapping.is_none()
                && let Some(mapping) =
                    openai::dialect::bailian::effort_mapping(model, self.protocol)
            {
                let mut bound = self.clone();
                bound.adaptation.effort_mapping = Some(mapping);
                return bound.encode_request_with_evidence(request, model, contract);
            }
            // Compatibility and carrier support are separate: a same-issuer value
            // without a target carrier must fail lowering, not disappear silently.
            let issuer = contract
                .replay_origin
                .as_ref()
                .or(self.adaptation.scope.as_ref());
            let stripped = request.without_foreign_reasoning(issuer)?;

            let mut controls = vec![];
            if stripped.is_some() {
                let chat = self.protocol == Profile::Chat;
                controls.push(crate::lowering::generation::Effect::new(
                    if chat {
                        "chat.transfer-visible-history.v1"
                    } else {
                        "responses.transfer-visible-history.v1"
                    },
                    crate::lowering::generation::Field::Replay,
                    crate::lowering::generation::ConversionBoundary::RequestProject,
                    crate::lowering::generation::Change::Omit,
                ));
            }
            let mut projected = stripped.map_or(Cow::Borrowed(request), Cow::Owned);
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
                                self.protocol == Profile::Chat
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
                let input = crate::lowering::generation::ReasoningControls::request(
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
                        rule: crate::lowering::generation::RuleId::new(mapping.rule),
                        input,
                        output: crate::lowering::generation::ReasoningControls::request(
                            &projected.task.semantic,
                        ),
                        preset_tokens: None,
                        owner: crate::lowering::generation::Owner::Root,
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
                controls.push(crate::lowering::generation::Effect::new(
                    "openai.default-schema-name.v1",
                    crate::lowering::generation::Field::SchemaName,
                    crate::lowering::generation::ConversionBoundary::RequestProject,
                    crate::lowering::generation::Change::Default(
                        crate::lowering::generation::DefaultBasis::SelectedPolicy,
                    ),
                ));
            }
            // Without a native flag, preserve the failure distinction in model-visible text.
            // Keep source values, execution facts and every native candidate unchanged.
            let (projected, changed) = with_tool_error_status(projected)?;
            if changed {
                controls.push(crate::lowering::generation::Effect::Field {
                    rule: crate::lowering::generation::RuleId::new(
                        "openai.wrap-tool-result-errors.v1",
                    ),
                    field: ("generation.tool_results.is_error").into(),
                    direction: ("request.encode").into(),
                    action: ("wrap").into(),
                    owner: crate::lowering::generation::Owner::Root,
                });
            }
            let request = projected.as_ref();
            request.check_context(contract.standard_context)?;
            let contract = self.contract(contract);
            if self.adaptation.rules.responses_siwc {
                if self.protocol != Profile::Responses {
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
            if self.protocol == Profile::Chat
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
                    .losses(&request.context)
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
            if self.protocol == Profile::Chat && !context.max_tool_calls.is_absent() {
                return Err(RepresentationError::UnmigratedSemantic.into());
            }
            let target = lower_request(
                &request.task.semantic,
                &request.task.fidelity,
                self.protocol,
                contract,
            )
            .map_err(|error| {
                crate::adapter::ConversionFailure::from(error)
                    .at(crate::lowering::generation::ConversionBoundary::RequestProject)
            })?;
            for evidence in target.projection().iter().cloned() {
                controls.push(evidence);
            }
            let mut value = match self.protocol {
                Profile::Chat => {
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
            Ok(crate::lowering::generation::Conversion {
                value,
                evidence: controls.into_iter().collect(),
            })
        })();
        result.map_err(|error: crate::adapter::ConversionFailure| {
            error.at(crate::lowering::generation::ConversionBoundary::RequestEncode)
        })
    }
}

impl Request {
    pub fn check(
        &self,
        contract: &TargetCaps,
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
        let wire = Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_request(&source, "m", &TargetCaps::full())
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
        for profile in [Profile::Chat, Profile::Responses] {
            let target = Adapter::new(profile, Dialect::Standard, None);
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
                    .encode_request_with_evidence(&source, "m", &TargetCaps::full())
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
                    Profile::Chat => {
                        assert_eq!(
                            projected.value["response_format"]["json_schema"]["name"],
                            "response"
                        );
                        assert_eq!(
                            projected.value["messages"][1],
                            json!({"role":"tool","tool_call_id":"call","content":content})
                        );
                    }
                    Profile::Responses => {
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
                let mut caps = TargetCaps::full();
                caps.standard_context = false;
                assert!(matches!(
                    target.encode_request(&invalid, "m", &caps),
                    Err(ConversionFailure {
                        cause: crate::adapter::ConversionCause::Representation(
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
                if e.rule() == "cache.omit-day-retention.v1" && e.semantic_field() == crate::lowering::generation::Field::CacheRetention
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
