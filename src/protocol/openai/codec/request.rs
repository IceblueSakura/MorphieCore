//! OpenAI request envelopes, controls and header projection.
use crate::{
    protocol::{
        GenerationRequestEnvelope as Request,
        failure::CarrierReason,
        openai::{
            self, CodecError, DecodeDirection, GenerationApi, IgnoredFields,
            chat::envelope as chat_envelope, codec::GenerationCodec, responses::envelope,
        },
    },
    semantic::{
        context::{DeliveryIntent, ExecutionHints, StreamOptions},
        value::Presence,
    },
};
use serde_json::Value;

impl GenerationCodec {
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
        if history.is_some() && self.protocol != GenerationApi::Responses {
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
        if self.protocol == GenerationApi::Responses
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
        if self.protocol == GenerationApi::Chat
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
            GenerationApi::Chat => {
                let decoded =
                    chat_envelope::decode_request_with(&value, &self.adaptation, direction)?;
                Request {
                    task: decoded.task,
                    model: decoded.context.model,
                    context: ExecutionHints {
                        store: decoded.context.store,
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
            GenerationApi::Responses => {
                let decoded = envelope::decode_request_with_history(
                    &value,
                    direction,
                    history,
                    self.adaptation.rules.responses_input_video,
                )?;
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
                        crate::protocol::conversion::generation::ConversionBoundary::RequestDecode,
                        crate::protocol::conversion::generation::Owner::Root,
                    );
                }
            }
            if self.adaptation.rules.bailian_requests && self.protocol == GenerationApi::Responses {
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
