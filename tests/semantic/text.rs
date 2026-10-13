//! Text responses, admission, events, terminals and open upstream decoding.
mod text_profile {
    //! Independent field and transformation contracts for stateless Responses text.
    use crate::wire;
    use morphiecore::{
        protocol::openai::{
            DecodedRequest, Profile, TargetCaps as Contract, chat,
            events::EventDecoder,
            lowering::{lower_request, lower_response},
            responses::{self, envelope},
        },
        semantic::{
            task::generation::*,
            value::{Presence, ReplayOrigin, Text},
        },
    };
    use serde_json::{Value, json};
    fn origin() -> ReplayOrigin {
        ReplayOrigin::new("synthetic-loopback").unwrap()
    }
    fn contract() -> Contract {
        let mut c = Contract::full();
        c.replay_origin = Some(origin());
        c
    }
    fn text(s: &str) -> Text {
        Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn request_wire(d: &DecodedRequest) -> Value {
        responses::encode_generation(
            &lower_request(&d.semantic, &d.fidelity, Profile::Responses, contract()).unwrap(),
        )
        .unwrap()
    }
    fn chat_wire(d: &DecodedRequest) -> Value {
        chat::encode_generation(
            &lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn response_format_shells_map_to_one_owner_with_independent_shapes() {
        let closed = json!({"type":"object","properties":{"zeta":{"type":"string"}},"required":["zeta"],"additionalProperties":false});
        let full = OutputConstraint::JsonSchema {
            name: Some(text("answer")),
            description: Some(text("pick")),
            schema: closed.clone().into(),
            strict: Some(true),
        };
        let bare = OutputConstraint::JsonSchema {
            name: Some(text("answer")),
            description: None,
            schema: closed.clone().into(),
            strict: None,
        };
        for (format, chat_format, responses_text, structured) in [
            (None, None, None, false),
            (
                Some(Presence::Null),
                Some(Value::Null),
                Some(json!({"format":null})),
                false,
            ),
            (
                Some(Presence::Value(OutputConstraint::Text)),
                Some(json!({"type":"text"})),
                Some(json!({"format":{"type":"text"}})),
                false,
            ),
            (
                Some(Presence::Value(OutputConstraint::JsonObject)),
                Some(json!({"type":"json_object"})),
                Some(json!({"format":{"type":"json_object"}})),
                true,
            ),
            (
                Some(Presence::Value(full.clone())),
                Some(
                    json!({"type":"json_schema","json_schema":{"name":"answer","description":"pick","schema":closed,"strict":true}}),
                ),
                Some(
                    json!({"format":{"type":"json_schema","name":"answer","description":"pick","schema":closed,"strict":true}}),
                ),
                true,
            ),
            (
                Some(Presence::Value(bare.clone())),
                Some(json!({"type":"json_schema","json_schema":{"name":"answer","schema":closed}})),
                Some(json!({"format":{"type":"json_schema","name":"answer","schema":closed}})),
                true,
            ),
        ] {
            let options = TextOptions {
                presence: format.is_some(),
                format: format.unwrap_or(Presence::Absent),
                verbosity: Presence::Absent,
            };
            let mut source = json!({"messages":[{"role":"user","content":"hello"}]});
            if let Some(value) = &chat_format {
                source["response_format"] = value.clone();
            }
            let d = chat::decode_generation(&source).unwrap();
            assert_eq!(d.semantic.text_options(), &options);
            assert_eq!(chat_wire(&d).get("response_format"), chat_format.as_ref());
            let mut source = json!({"input":"hello"});
            if let Some(value) = &responses_text {
                source["text"] = value.clone();
            }
            let r = responses::decode_generation(&source).unwrap();
            assert_eq!(r.semantic.text_options(), &options);
            assert_eq!(request_wire(&r).get("text"), responses_text.as_ref());
            assert_eq!(
                GenerationRequirements::derive(&d.semantic).structured_output,
                structured
            );
            let mut limited = contract();
            limited.semantics.structured_output = false;
            for profile in [Profile::Chat, Profile::Responses] {
                assert_eq!(
                    lower_request(&d.semantic, &d.fidelity, profile, limited.clone()).is_err(),
                    structured
                );
            }
        }
    }

    #[test]
    fn instruction_status_is_checked_in_history_and_reported_echoes() {
        for role in ["system", "developer"] {
            for status in [
                None,
                Some(json!("completed")),
                Some(Value::Null),
                Some(json!(false)),
                Some(json!("unknown")),
                Some(json!("in_progress")),
                Some(json!("incomplete")),
            ] {
                let lifecycle = matches!(&status, Some(value) if value == &json!("in_progress") || value == &json!("incomplete"));
                let accepted = status.is_none() || status == Some(json!("completed")) || lifecycle;
                let mut item = json!({"type":"message","role":role,"content":[{"type":"input_text","text":"Be precise"}]});
                if let Some(status) = status.clone() {
                    item["status"] = status;
                }
                let decoded = responses::decode_generation(&json!({"input":[item.clone()]}));
                assert_eq!(decoded.is_ok(), accepted, "{item}");
                if let Ok(d) = decoded {
                    let encoded = request_wire(&d)["input"][0].clone();
                    if lifecycle {
                        assert_eq!(encoded["status"], item["status"]);
                        assert_eq!(encoded["type"], json!("message"));
                    } else {
                        assert!(encoded.get("status").is_none());
                        assert_eq!(encoded["content"], json!("Be precise"));
                    }
                }
                let mut response = wire::response(2);
                response["instructions"] = json!([item]);
                let decoded = envelope::decode_response(&response);
                assert_eq!(decoded.is_ok(), accepted);
                if let Ok(d) = decoded {
                    let output = envelope::encode_response(
                        &lower_response(
                            &d.semantic,
                            &d.fidelity,
                            &d.metadata,
                            Profile::Responses,
                            contract(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    let encoded = &output["instructions"][0];
                    if lifecycle {
                        assert_eq!(encoded["status"], item["status"]);
                    } else {
                        assert!(encoded.get("status").is_none());
                        assert_eq!(encoded["content"], json!("Be precise"));
                    }
                }
            }
        }
    }

    #[test]
    fn custom_tools_and_text_array_results_are_not_function_arguments() {
        let mut d=responses::decode_generation(&json!({"input":[{"type":"custom_tool_call","call_id":"c","name":"sql","input":"SELECT 1"},{"type":"custom_tool_call_output","call_id":"c","output":[{"type":"input_text","text":"1"},{"type":"input_text","text":""}]}],"tools":[{"type":"custom","name":"sql","format":{"type":"grammar","syntax":"regex","definition":"SELECT [0-9]+"}}],"tool_choice":{"type":"custom","name":"sql"}})).unwrap();
        assert!(matches!(&d.semantic.items()[0].1,Item::CustomCall(c) if c.input=="SELECT 1"));
        assert!(GenerationRequirements::derive(&d.semantic).custom_tools);
        let mut items = d.semantic.items().to_vec();
        items[0].0 = ItemId::new(99);
        items[1].0 = ItemId::new(100);
        let Item::CustomCall(c) = &mut items[0].1 else {
            panic!()
        };
        c.input = "not JSON, not executed".into();
        c.call_id = text("edited-custom-call");
        let Item::CustomResult(r) = &mut items[1].1 else {
            panic!()
        };
        // Explicit synthetic report for the new call, never automatic result relinking.
        r.call_id = text("edited-custom-call");
        r.output = ToolOutput::Text("changed".into());
        d.semantic = d.semantic.with_items(items).unwrap();
        let out = request_wire(&d);
        assert_eq!(out["input"][0]["input"], "not JSON, not executed");
        assert!(out["input"][0].get("arguments").is_none());
        assert_eq!(out["input"][1]["output"], "changed");
        assert!(lower_request(&d.semantic, &d.fidelity, Profile::Chat, contract()).is_err());
        let mut mismatch = out;
        mismatch["input"][1]["type"] = json!("function_call_output");
        assert!(responses::decode_generation(&mismatch).is_err());
    }
    #[test]
    fn null_empty_false_and_default_controls_are_deliberate_not_unknown_passthrough() {
        let d=responses::decode_generation(&json!({"input":"","instructions":null,"reasoning":{"effort":null,"summary":null},"tools":[],"tool_choice":"none","parallel_tool_calls":false,"text":{"format":{"type":"text"},"verbosity":null}})).unwrap();
        let out = request_wire(&d);
        assert_eq!(out["instructions"], Value::Null);
        assert_eq!(out["reasoning"], json!({"effort":null,"summary":null}));
        assert_eq!(out["parallel_tool_calls"], false);
        assert_eq!(out["tools"], json!([]));
        for bad in [
            json!({"top_logprobs":21}),
            json!({"top_p":1.1}),
            json!({"temperature":-1}),
            json!({"text":{"format":{"type":"json_schema","name":"bad name","schema":{}}}}),
            json!({"tools":[{"type":"custom","name":"x","format":{"type":"grammar","syntax":"unknown","definition":"x"}}]}),
            json!({"tools":[{"type":"function","name":"x","async":"yes"}]}),
            json!({"reasoning":{"unknown":true}}),
            json!({"text":null}),
            json!({"tools":null}),
            json!({"tool_choice":null}),
            json!({"reasoning":{"mode":null}}),
        ] {
            let mut value = json!({"input":"hello"});
            value
                .as_object_mut()
                .unwrap()
                .extend(bad.as_object().unwrap().clone());
            assert!(responses::decode_generation(&value).is_err());
        }
    }

    #[test]
    fn probabilities_annotations_and_snapshots_cannot_rewrite_observed_values() {
        for field in ["probability", "annotation", "terminal"] {
            let mut values = wire::events(1);
            match field {
                "probability" => {
                    let v = values
                        .iter_mut()
                        .find(|v| v["type"] == "response.content_part.done")
                        .unwrap();
                    v["part"]["logprobs"][0]["logprob"] = json!(-0.75);
                }
                "annotation" => {
                    let v = values
                        .iter_mut()
                        .find(|v| v["type"] == "response.output_text.annotation.added")
                        .unwrap();
                    v["annotation_index"] = json!(1);
                }
                _ => {
                    values.last_mut().unwrap()["response"]["output"][3]["content"][0]["text"] =
                        json!("resurrected");
                }
            }
            let mut d = EventDecoder::new(Profile::Responses).with_replay_origin(origin());
            assert!(values.iter().any(|v| d.push(v).is_err()), "{field}");
            assert!(d.finish().is_err());
        }
    }
    fn responses_round_trip(request: &Value) -> Value {
        let d = envelope::decode_request(request).unwrap();
        envelope::encode_request(
            &lower_request(
                &d.task.semantic,
                &d.task.fidelity,
                Profile::Responses,
                contract(),
            )
            .unwrap(),
            &d.context,
        )
        .unwrap()
    }
    fn response_round_trip(response: &Value) -> Value {
        let d = envelope::decode_response(response).unwrap();
        envelope::encode_response(
            &lower_response(
                &d.semantic,
                &d.fidelity,
                &d.metadata,
                Profile::Responses,
                contract(),
            )
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn reported_cache_context_is_an_independent_response_fact() {
        let mut response = wire::response(2);
        response["prompt_cache_options"] = json!({"mode":"explicit","ttl":"30m","prewarm":true});
        response["prompt_cache_diagnostics"] = json!({"type":"cache_miss","reason":"input_changed","cache_missed_tokens":3,"comparison_reusable_tokens":2});
        let out = response_round_trip(&response);
        assert_eq!(
            out["prompt_cache_options"],
            json!({"mode":"explicit","ttl":"30m","prewarm":true})
        );
        assert_eq!(
            out["prompt_cache_diagnostics"],
            response["prompt_cache_diagnostics"]
        );
        // The effective tier is a reported fact and stays independent of the request hint.
        assert_eq!(out["service_tier"], json!("default"));
        assert_eq!(
            responses_round_trip(&wire::request(false))["service_tier"],
            json!("auto")
        );
        // Nothing is invented for absent echoes.
        let mut bare = wire::response(2);
        for key in [
            "prompt_cache_key",
            "prompt_cache_retention",
            "prompt_cache_options",
            "prompt_cache_diagnostics",
            "safety_identifier",
            "user",
            "metadata",
        ] {
            bare.as_object_mut().unwrap().remove(key);
        }
        let out = response_round_trip(&bare);
        for key in [
            "prompt_cache_key",
            "prompt_cache_retention",
            "prompt_cache_options",
            "prompt_cache_diagnostics",
            "safety_identifier",
            "user",
            "metadata",
        ] {
            assert!(out.get(key).is_none(), "{key}");
        }
    }
}
mod text_admission {
    //! Admission regressions for the existing text subset, not the whole SDK union.
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile};
    use serde_json::json;

    #[test]
    fn nullable_reasoning_effort_and_optional_function_headers_follow_their_own_schema() {
        let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
        let wire = json!({"model":"m","messages":[{"role":"user","content":"hi"}],"reasoning_effort":null});
        let decoded = adapter.decode_request(wire.to_string().as_bytes()).unwrap();
        let contract = morphiecore::protocol::openai::TargetCaps::full();
        assert!(
            adapter
                .encode_request(&decoded, "m", &contract)
                .unwrap()
                .get("reasoning_effort")
                .unwrap()
                .is_null()
        );
        let wire = json!({"id":"r","object":"response","created_at":0,"model":"m","status":"completed","output":[{"id":null,"type":"function_call","call_id":"call","name":"lookup","arguments":"{}","status":null}]});
        let decoded = Adapter::new(Profile::Responses, Dialect::Standard, None)
            .decode_response(wire.to_string().as_bytes())
            .unwrap();
        assert!(
            decoded
                .fidelity
                .response_item_id(morphiecore::semantic::task::generation::ItemId::new(1))
                .is_none()
        );
        assert_eq!(
            decoded.semantic.continuation(),
            morphiecore::semantic::task::generation::Continuation::ToolResults(vec![
                morphiecore::semantic::task::generation::CallReference {
                    item: morphiecore::semantic::task::generation::ItemId::new(1),
                    call_id: "call",
                },
            ])
        );
    }

    #[test]
    fn known_unrepresentable_event_fields_and_changed_bound_metadata_fail_early() {
        use morphiecore::{
            protocol::{
                fidelity::FidelityRecords,
                openai::{ResponseMetadata, events::EventEncoder},
            },
            semantic::{
                context::ServiceTier,
                task::generation::*,
                value::{Presence, Text},
            },
        };
        let metadata = ResponseMetadata {
            id: Some("c".into()),
            model: "m".into(),
            created: Some(1.into()),
            context: Default::default(),
            instruction_fidelity: Default::default(),
        };
        let fidelity = FidelityRecords::default();
        for kind in [
            ItemKind::Message {
                phase: Some(Phase::FinalAnswer),
            },
            ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: Text::new("call", "test", 128).unwrap(),
                name: Text::new("lookup", "test", 128).unwrap(),
                context: CallContext {
                    namespace: Some(Text::new("scope", "test", 128).unwrap()),
                    ..Default::default()
                },
            },
        ] {
            let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
            encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
            assert!(
                encoder
                    .encode(
                        &StreamEvent::ItemStarted {
                            item: ItemId::new(1),
                            kind,
                            replay: None
                        },
                        &fidelity
                    )
                    .is_err()
            );
            assert!(encoder.finish().is_err());
        }
        let mut metadata = metadata;
        metadata.context.execution.service_tier = Presence::Value(ServiceTier::Default);
        let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
        encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        metadata.context.execution.service_tier = Presence::Value(ServiceTier::Priority);
        assert!(encoder.update_metadata(metadata).is_err());
        assert!(encoder.finish().is_err());
    }
}
mod text_events {
    //! Text/refusal parts preserve boundaries, empty values and independent item lifecycle.
    use crate::events_support::*;
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{
                Profile,
                events::{EventDecoder, EventEncoder},
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::json;

    #[test]
    fn independently_constructed_text_ir_encodes_expected_boundaries_and_refusal() {
        let mut e = vec![
            StreamEvent::Started,
            start(9, ItemKind::Message { phase: None }),
        ];
        e.extend(part(9, 40, PartKind::Text, ""));
        e.extend(part(9, 70, PartKind::Refusal, "Cannot comply"));
        e.push(close(9, ItemLifecycle::Completed));
        e.push(terminal(StreamTerminal::Completed));
        let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
        assert_eq!(
            wire.last().unwrap()["response"]["output"][0]["content"],
            json!([{"type":"output_text","text":"","annotations":[]},{"type":"refusal","refusal":"Cannot comply"}])
        );
        assert!(
            wire.iter()
                .any(|v| v["type"] == "response.refusal.delta" && v["delta"] == "Cannot comply")
        );
    }
    #[test]
    fn empty_failed_and_incomplete_static_event_closure_keeps_details() {
        for (terminal, status) in [
            (StreamTerminal::Failed, "failed"),
            (StreamTerminal::Incomplete, "incomplete"),
        ] {
            let details = match terminal {
                StreamTerminal::Failed => TerminalDetails {
                    error: Some(ResponseError {
                        code: Some(text("server_error")),
                        message: text("synthetic error"),
                        param: None,
                    }),
                    incomplete: None,
                },
                StreamTerminal::Incomplete => TerminalDetails {
                    error: None,
                    incomplete: Some(IncompleteReason::MaxOutputTokens),
                },
                _ => TerminalDetails::default(),
            };
            let e = vec![
                StreamEvent::Started,
                StreamEvent::Terminal {
                    terminal,
                    details: details.clone(),
                },
            ];
            let wire = encode(&e, Profile::Responses, &FidelityRecords::default());
            assert_eq!(wire.last().unwrap()["response"]["status"], status);
            assert_eq!(wire.last().unwrap()["response"]["output"], json!([]));
            let mut d = EventDecoder::new(Profile::Responses);
            for v in wire {
                d.push(&v).unwrap();
            }
            assert_eq!(d.materialize().unwrap().semantic.details(), &details);
        }
    }
    #[test]
    fn queued_lifecycle_has_no_chat_projection() {
        let mut chat = EventEncoder::new(Profile::Chat, metadata()).unwrap();
        assert!(
            chat.encode(&StreamEvent::Queued, &FidelityRecords::default())
                .is_err()
        );
        let mut responses = EventEncoder::new(Profile::Responses, metadata()).unwrap();
        let v = responses
            .encode(&StreamEvent::Queued, &FidelityRecords::default())
            .unwrap();
        assert_eq!(v[0]["type"], json!("response.queued"));
        assert_eq!(v[0]["response"]["status"], json!("queued"));
    }
}
mod response {
    //! Refusal and non-success terminals for the text and function-call path.
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::openai::{
            Profile, ResponseMetadata, TargetCaps as Contract, chat, lowering::lower_response,
            responses,
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn metadata() -> ResponseMetadata {
        ResponseMetadata {
            id: Some("response_1".into()),
            model: "fixture-model".into(),
            created: Some(10.into()),
            context: Default::default(),
            instruction_fidelity: Default::default(),
        }
    }

    fn chat_refusal() -> Value {
        json!({"id":"response_1","object":"chat.completion","created":10,"model":"fixture-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":null,"refusal":"Cannot do that."},"finish_reason":"stop"}],
        "usage":null})
    }
    fn responses_refusal() -> Value {
        json!({"id":"response_1","object":"response","created_at":10,"model":"fixture-model","status":"completed",
        "output":[{"id":"item_1","type":"message","role":"assistant","status":"completed","content":[{"type":"refusal","refusal":"Cannot do that."}]}],
        "usage":null})
    }

    #[test]
    fn incomplete_and_failed_responses_do_not_encode_as_success() {
        let mut chat_wire = chat_refusal();
        chat_wire["choices"][0]["message"] = json!({"role":"assistant","content":"Partial"});
        chat_wire["choices"][0]["finish_reason"] = json!("length");
        let decoded = chat::decode_response(&chat_wire).unwrap();
        assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
        let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
        let metadata = metadata();
        let encoded = lower_response(
            &decoded.semantic,
            &fidelity,
            &metadata,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        assert_eq!(
            responses::encode_response(&encoded).unwrap()["status"],
            json!("incomplete")
        );
        let mut failed = responses_refusal();
        failed["status"] = json!("failed");
        failed["output"][0]["status"] = json!("incomplete");
        let decoded = responses::decode_response(&failed).unwrap();
        assert_eq!(decoded.semantic.outcome(), Outcome::Failed);
        let mut fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
        fidelity
            .record_response_item_id(decoded.semantic.items()[0].0, "item_1")
            .unwrap();
        assert!(matches!(
            lower_response(
                &decoded.semantic,
                &fidelity,
                &metadata,
                Profile::Chat,
                Contract::full()
            ),
            Err(RepresentationError::Terminal)
        ));
        assert_eq!(
            responses::encode_response(
                &lower_response(
                    &decoded.semantic,
                    &fidelity,
                    &metadata,
                    Profile::Responses,
                    Contract::full()
                )
                .unwrap()
            )
            .unwrap()["status"],
            json!("failed")
        );
    }

    #[test]
    fn content_filter_is_incomplete_not_length_or_refusal_success() {
        for message in [
            json!({"role":"assistant","content":null}),
            json!({"role":"assistant","content":"Partial"}),
            json!({"role":"assistant","content":null,"refusal":"Cannot do that."}),
            json!({"role":"assistant","content":null,"tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{"}}]}),
        ] {
            let mut wire = chat_refusal();
            wire["choices"][0]["message"] = message.clone();
            wire["choices"][0]["finish_reason"] = json!("content_filter");
            let decoded = chat::decode_response(&wire).unwrap();
            assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
            assert_eq!(
                decoded.semantic.details().incomplete,
                Some(IncompleteReason::ContentFilter)
            );
            assert!(
                decoded
                    .semantic
                    .items()
                    .iter()
                    .all(|(_, i)| i.lifecycle() == Some(ItemLifecycle::Incomplete))
            );
            for profile in [Profile::Chat, Profile::Responses] {
                let target = lower_response(
                    &decoded.semantic,
                    &decoded.fidelity,
                    &decoded.metadata,
                    profile,
                    Contract::full(),
                );
                let target = target.unwrap();
                if profile == Profile::Chat {
                    let encoded = chat::encode_response(&target).unwrap();
                    assert_eq!(encoded["choices"][0]["finish_reason"], "content_filter");
                    assert_eq!(encoded["choices"][0]["message"], message);
                } else {
                    let encoded = responses::encode_response(&target).unwrap();
                    assert_eq!(encoded["status"], "incomplete");
                    assert_eq!(encoded["incomplete_details"]["reason"], "content_filter");
                    let other = responses::decode_response(&encoded).unwrap();
                    let back = lower_response(
                        &other.semantic,
                        &other.fidelity,
                        &other.metadata,
                        Profile::Chat,
                        Contract::full(),
                    )
                    .unwrap();
                    assert_eq!(
                        chat::encode_response(&back).unwrap()["choices"][0]["finish_reason"],
                        "content_filter"
                    );
                }
            }
        }
    }
}
mod timestamps {
    //! Independent timestamp admission and typed edits, without Provider or Gateway bindings.
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::openai::{
            Profile, TargetCaps as Contract, events::EventEncoder, lowering::lower_response,
            responses::envelope,
        },
        semantic::value::Presence,
    };
    use serde_json::Number;

    fn number(source: &str) -> Number {
        source.parse().unwrap()
    }
    fn response(created: &str, completed: Option<&str>) -> String {
        let completed = completed.map_or(String::new(), |v| format!(",\"completed_at\":{v}"));
        format!(
            r#"{{"id":"r","object":"response","created_at":{created}{completed},"model":"synthetic","status":"completed","output":[]}}"#
        )
    }

    #[test]
    fn timestamp_typed_edits_cannot_bypass_context_lowering_or_event_encoding() {
        let decoded = envelope::decode_response_bytes(response("0", None).as_bytes()).unwrap();
        for completion in [false, true] {
            let mut metadata = decoded.metadata.clone();
            if completion {
                metadata.context.completed_at = Presence::Value(number("-1e-9999"));
                assert!(metadata.context.validate().is_err());
            } else {
                metadata.created = Some(number("-1e-9999"));
            }
            assert!(matches!(
                lower_response(
                    &decoded.semantic,
                    &decoded.fidelity,
                    &metadata,
                    Profile::Responses,
                    Contract::full()
                ),
                Err(RepresentationError::Metadata)
            ));
            assert!(EventEncoder::new(Profile::Responses, metadata).is_err());
        }
    }
}
mod open_decoding {
    //! Upstream responses ignore unknown keys while known keys stay strict; requests stay closed.
    use crate::wire;
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile};
    use serde_json::{Value, json};

    use crate::chat_wire;

    fn adapter(profile: Profile) -> Adapter {
        Adapter::new(profile, Dialect::Standard, None)
    }
    fn future() -> Value {
        json!({"synthetic":["future",1,null]})
    }
    fn add(value: &mut Value, pointer: &str) {
        value
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .unwrap_or_else(|| panic!("{pointer}"))
            .insert("future_field".into(), future());
    }

    #[test]
    fn responses_static_response_ignores_unknown_keys_at_every_level() {
        let a = adapter(Profile::Responses);
        let plain = wire::response(1);
        let mut extended = plain.clone();
        for pointer in [
            "",
            "/usage",
            "/usage/input_tokens_details",
            "/usage/output_tokens_details",
            "/output/0",
            "/output/0/summary/0",
            "/output/2",
            "/output/3",
            "/output/3/content/0",
            "/output/3/content/0/annotations/0",
        ] {
            add(&mut extended, pointer);
        }
        assert_eq!(
            a.decode_response(extended.to_string().as_bytes()).unwrap(),
            a.decode_response(plain.to_string().as_bytes()).unwrap()
        );
    }

    #[test]
    fn chat_static_response_ignores_unknown_keys_at_every_level() {
        let a = adapter(Profile::Chat);
        for turn in [1, 2] {
            let plain = chat_wire::response(turn);
            let mut extended = plain.clone();
            for pointer in ["", "/usage", "/choices/0", "/choices/0/message"] {
                add(&mut extended, pointer);
            }
            if turn == 1 {
                add(&mut extended, "/choices/0/message/tool_calls/0");
                add(&mut extended, "/choices/0/message/tool_calls/0/function");
            }
            assert_eq!(
                a.decode_response(extended.to_string().as_bytes()).unwrap(),
                a.decode_response(plain.to_string().as_bytes()).unwrap()
            );
        }
    }

    fn stream(
        a: &Adapter,
        chat: bool,
        frames: &[Value],
    ) -> Vec<morphiecore::semantic::task::generation::StreamEvent> {
        let mut decoder = a.event_decoder();
        let mut events = vec![];
        for frame in frames {
            events.extend(decoder.push(frame).unwrap());
        }
        if chat {
            // Chat closes on the `[DONE]` sentinel, not on the last chunk.
            events.extend(decoder.done().unwrap());
        }
        decoder.finish().unwrap();
        events
    }

    #[test]
    fn responses_events_ignore_unknown_keys() {
        let a = adapter(Profile::Responses);
        let plain = wire::events(1);
        let mut extended = plain.clone();
        for frame in &mut extended {
            add(frame, "");
            for key in ["item", "part", "response"] {
                if frame.get(key).is_some_and(Value::is_object) {
                    add(frame, &format!("/{key}"));
                }
            }
        }
        assert_eq!(stream(&a, false, &extended), stream(&a, false, &plain));
    }

    #[test]
    fn chat_chunks_ignore_unknown_keys() {
        let a = adapter(Profile::Chat);
        for turn in [1, 2] {
            let plain = chat_wire::events(turn);
            let mut extended = plain.clone();
            for frame in &mut extended {
                add(frame, "");
                if frame["choices"].as_array().is_some_and(|c| !c.is_empty()) {
                    add(frame, "/choices/0");
                    add(frame, "/choices/0/delta");
                }
            }
            assert_eq!(stream(&a, true, &extended), stream(&a, true, &plain));
        }
    }

    #[test]
    fn known_upstream_fields_stay_strict() {
        let a = adapter(Profile::Responses);
        for (pointer, value) in [
            ("/usage/input_tokens", json!("3")),
            ("/status", json!("synthetic_status")),
            ("/output/3/content/0/text", json!(7)),
        ] {
            let mut body = wire::response(1);
            *body.pointer_mut(pointer).unwrap() = value;
            assert!(
                a.decode_response(body.to_string().as_bytes()).is_err(),
                "{pointer}"
            );
        }
        let mut body = wire::response(1);
        body["output"][1]["type"] = json!("synthetic_future_item");
        assert!(a.decode_response(body.to_string().as_bytes()).is_err());
    }

    #[test]
    fn downstream_requests_still_reject_unmodeled_fields() {
        let mut body = wire::request(false);
        body["future_control"] = json!(true);
        assert!(
            adapter(Profile::Responses)
                .decode_request(body.to_string().as_bytes())
                .is_err()
        );
        let body =
            json!({"model":"m","messages":[{"role":"user","content":"hi"}],"future_control":true});
        assert!(
            adapter(Profile::Chat)
                .decode_request(body.to_string().as_bytes())
                .is_err()
        );
    }
}
