//! OpenAI request envelopes, controls and header projection.
use crate::{
    protocol::{
        ConversionFailure, GenerationRequestEnvelope as Request,
        openai::{
            self, CodecError, GenerationApi, TargetContract, chat::envelope as chat_envelope,
            codec::GenerationCodec, lowering::render_request, responses::envelope,
        },
    },
    semantic::value::Presence,
};
use serde_json::Value;
use std::borrow::Cow;

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
            if self.adaptation.rules.bailian_requests
                && self.adaptation.effort_mapping.is_none()
                && let Some(mapping) =
                    openai::dialect::bailian::effort_mapping(model, self.protocol)
            {
                let mut bound = self.clone();
                bound.adaptation.effort_mapping = Some(mapping);
                return bound.encode_request_with_evidence(request, model, contract);
            }
            let contract = self.contract(contract);
            let request_projection = crate::lowering::request::project(
                request,
                crate::lowering::request::Target::OpenAi(self.protocol, &contract),
                model,
                None,
            )?;
            let request = request_projection.value.as_ref();
            let mut evidence = request_projection.evidence;
            if self.adaptation.rules.responses_siwc && self.protocol != GenerationApi::Responses {
                return Err(CodecError::ProfileMismatch.into());
            }
            let context = request.context.clone();
            // Candidate preflight uses this same encoder, so invalid header carriers cannot reach I/O.
            self.request_headers(request, contract.cache)?;
            let target = render_request(
                Cow::Borrowed(&request.task.semantic),
                &request.task.fidelity,
                self.protocol,
                contract,
            )
            .map_err(|error| {
                crate::protocol::ConversionFailure::from(error)
                    .at(crate::protocol::conversion::generation::ConversionBoundary::RequestProject)
            })?;
            for effect in target.projection().iter().cloned() {
                evidence.push(effect);
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
                            service_policy: request.service_policy,
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
                            service_policy: request.service_policy,
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
            Ok(crate::protocol::conversion::generation::Conversion { value, evidence })
        })();
        result.map_err(|error: crate::protocol::ConversionFailure| {
            error.at(crate::protocol::conversion::generation::ConversionBoundary::RequestEncode)
        })
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
        let contract = TargetContract::full();
        let projected = crate::lowering::request::project_semantic(
            &source.task.semantic,
            crate::lowering::request::Target::OpenAi(GenerationApi::Responses, &contract),
            "m",
        )
        .unwrap();
        assert!(
            projected
                .evidence
                .contains_rule("openai.wrap-tool-result-errors.v1")
        );
        let Item::ToolResult(result) = &projected.value.items()[1].1 else {
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
        assert!(
            crate::lowering::request::project_semantic(
                &source.task.semantic,
                crate::lowering::request::Target::OpenAi(GenerationApi::Responses, &contract),
                "m"
            )
            .is_err()
        );
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
                    !projected
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
                caps.dispositions
                    .set_native(crate::semantic::task::generation::Feature::Metadata, false);
                caps.dispositions.set_native(
                    crate::semantic::task::generation::Feature::ServicePolicy,
                    false,
                );
                caps.dispositions.set_native(
                    crate::semantic::task::generation::Feature::ToolBudget,
                    false,
                );
                let projected = target
                    .encode_request_with_evidence(&invalid, "m", &caps)
                    .unwrap();
                assert!(projected.value.get("metadata").is_none());
                assert!(
                    projected
                        .evidence
                        .contains_rule("generation.omit-metadata.v1")
                );
                assert_eq!(invalid, before);
            }
        }
    }

    #[test]
    fn siwc_projects_advisory_controls_without_weakening_tool_or_storage_guarantees() {
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
            let output = target
                .encode_request(&request, "selected-slug", &contract)
                .unwrap();
            assert!(output.get(field).is_none(), "{field}");
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
        let output = target
            .encode_request(&edited, "selected-slug", &contract)
            .unwrap();
        assert!(output.get("max_output_tokens").is_none());
        let mut flat = body;
        let mut budgeted = flat.clone();
        budgeted["max_tool_calls"] = serde_json::json!(1);
        let budgeted = client
            .decode_request(budgeted.to_string().as_bytes())
            .unwrap();
        assert!(matches!(
            target.encode_request(&budgeted, "selected-slug", &contract),
            Err(crate::protocol::ConversionFailure {
                cause: crate::protocol::ConversionCause::Representation(
                    crate::protocol::conversion::generation::RepresentationError::Admission(
                        crate::semantic::task::generation::Feature::ToolBudget
                    )
                ),
                ..
            })
        ));
        flat["tools"] = serde_json::json!([{"type":"function","name":"status","strict":false}]);
        let flat = client.decode_request(flat.to_string().as_bytes()).unwrap();
        assert!(
            target
                .encode_request(&flat, "selected-slug", &contract)
                .is_err()
        );
        let approved = contract.clone();
        let mut extended = request.clone();
        extended.context.cache.prompt_cache_retention =
            Some(crate::semantic::context::CacheRetention::Day);
        let before = extended.clone();
        let projected = target
            .encode_request_with_evidence(&extended, "selected-slug", &approved)
            .unwrap();
        assert!(projected.value.get("prompt_cache_retention").is_none());
        assert!(projected.evidence.iter().any(|loss| matches!(loss,
            e
                if e.rule() == "generation.omit-cache-retention.v1" && e.semantic_field() == crate::protocol::conversion::generation::Field::CacheRetention
        )));
        assert_eq!(extended, before);
        extended.context.cache.prompt_cache_options = Presence::Value(Default::default());
        let output = target
            .encode_request(&extended, "selected-slug", &approved)
            .unwrap();
        assert!(output.get("prompt_cache_options").is_none());
        extended.context.cache.prompt_cache_options = Presence::Absent;
        extended.context.cache.prompt_cache_retention =
            Some(crate::semantic::context::CacheRetention::InMemory);
        assert!(matches!(
            target.encode_request(&extended, "selected-slug", &approved),
            Err(crate::protocol::ConversionFailure {
                cause: crate::protocol::ConversionCause::Representation(
                    crate::protocol::conversion::generation::RepresentationError::Admission(
                        crate::semantic::task::generation::Feature::CacheRetention
                    )
                ),
                ..
            })
        ));
    }
}
