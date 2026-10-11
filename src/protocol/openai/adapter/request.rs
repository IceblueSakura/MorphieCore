//! OpenAI request envelopes, controls and header projection.
use super::{Adapter, AdapterError, Request};
use crate::{
    lowering::generation::RepresentationError,
    protocol::{
        extensions::CustomSections,
        openai::{
            self, CodecError, Profile, TargetCaps, chat_envelope, envelope, lowering::lower_request,
        },
    },
    semantic::{
        context::{DeliveryIntent, ExecutionHints, StreamOptions},
        value::Presence,
    },
};
use serde_json::Value;
impl Adapter {
    pub fn decode_request(&self, bytes: &[u8]) -> Result<Request, CodecError> {
        let mut value = openai::json::decode(bytes)?;
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
                let decoded = chat_envelope::decode_request_with(&value, &self.adaptation)?;
                Request {
                    task: decoded.task,
                    model: decoded.context.model,
                    context: ExecutionHints {
                        cache: decoded.context.cache,
                        identity: decoded.context.identity,
                        metadata: decoded.context.metadata,
                        service_tier: decoded.context.service_tier,
                        ..Default::default()
                    },
                    delivery: DeliveryIntent {
                        stream: decoded.context.stream,
                        options: decoded.context.stream_options,
                    },
                    extensions: CustomSections::default(),
                    source: Some(self.protocol),
                    conversation: None,
                    cache_session,
                    inferred_affinity: None,
                    n: decoded.context.n,
                }
            }
            Profile::Responses => {
                let decoded = envelope::decode_request(&value)?;
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
                    delivery: DeliveryIntent {
                        stream: decoded.context.delivery.stream,
                        options,
                    },
                    extensions: decoded.context.extensions,
                    source: Some(self.protocol),
                    conversation: None,
                    cache_session,
                    inferred_affinity: None,
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
        // Compatibility and carrier support are separate: a same-issuer value
        // without a target carrier must fail lowering, not disappear silently.
        let issuer = contract
            .replay_origin
            .as_ref()
            .or(self.adaptation.scope.as_ref());
        let stripped = request.without_foreign_replay(issuer)?;
        let request = stripped.as_ref().unwrap_or(request);
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
        let mut context = request.context.clone();
        contract.cache.project_request(
            &mut context,
            request.conversation.as_ref(),
            request.inferred_affinity,
        )?;
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
                || request.task.semantic.reasoning().encrypted_output())
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
        let mut value = match self.protocol {
            Profile::Chat => {
                let mut options = request.delivery.options.clone();
                // Cross-protocol delivery still needs actual upstream usage; request
                // its wire report rather than synthesizing any counters.
                if request.source != Some(self.protocol) && request.delivery.streaming() {
                    let mut common = options.value().cloned().unwrap_or_default();
                    common.include_usage = Presence::Value(true);
                    options = Presence::Value(common);
                }
                chat_envelope::encode_request(
                    &target,
                    &chat_envelope::RequestContext {
                        model: model.into(),
                        n: if request.source == Some(self.protocol) {
                            request.n.clone()
                        } else {
                            Presence::Absent
                        },
                        stream: request.delivery.stream.clone(),
                        stream_options: options,
                        cache: context.cache,
                        identity: context.identity,
                        metadata: context.metadata,
                        service_tier: context.service_tier,
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
                        delivery: envelope::Delivery {
                            stream: request.delivery.stream.clone(),
                            options,
                        },
                        extensions: CustomSections::default(),
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
                openai::adapter_shapes::encode_message(message);
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
        Ok(value)
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
