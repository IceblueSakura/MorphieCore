//! Independent semantic mapping and append-only native continuation expectations.
use morphiecore::{
    protocol::{DecodedRequest, anthropic::messages::Profile, native::ReplayTarget},
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
}
fn request() -> &'static [u8] {
    br#"{"model":"synthetic-model","max_tokens":512,"system":"Use lookup.","tools":[{"name":"lookup","strict":false,"input_schema":{"type":"object","properties":{"key":{"type":"string"}}}}],"messages":[{"role":"user","content":"Look up A."}]}"#
}
fn response() -> &'static [u8] {
    br#"{"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":7}}"#
}
fn feedback(
    source: &DecodedRequest,
    observed: &morphiecore::protocol::DecodedResponse,
    execution: ToolExecution,
) -> GenerationRequest {
    let id = ItemId::scoped(LocalScope::new(3), 1);
    let mut request = ClientManaged::new(source.semantic.settings().clone())
        .unwrap()
        .select_request(&source.semantic)
        .unwrap()
        .select_response(&observed.semantic)
        .unwrap()
        .append_items(vec![(
            id,
            Item::ToolResult(ToolResult {
                call_id: Some(Text::new("c", "call", 64).unwrap()),
                cache: None,
                name: None,
                output: "actual feedback".into(),
                is_error: if matches!(&execution, ToolExecution::Succeeded) {
                    None
                } else {
                    Some(true)
                },
                execution: Some(execution),
                status: None,
                context: Default::default(),
            }),
        )])
        .unwrap()
        .build()
        .unwrap();
    let mut groups = request.message_envelopes().to_vec();
    groups.push(
        MessageEnvelope::new(
            GroupId::new(LocalScope::new(3), 1),
            MessageEnvelopeRole::User,
            vec![id],
        )
        .unwrap(),
    );
    request = request.with_message_envelopes(groups).unwrap();
    request
}
#[test]
fn native_intake_keeps_independent_error_execution_progress_and_signature_owners() {
    let p = Profile::AdaptiveTextTools;
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    let observed = p
        .decode_generation_response(response(), LocalScope::new(2), &target())
        .unwrap();
    assert_eq!(observed.metadata.created, None);
    assert_eq!(
        observed.semantic.progress(),
        InteractionProgress::AwaitingToolResults
    );
    assert_eq!(observed.semantic.outcome(), Outcome::Completed);
    assert!(
        matches!(&observed.semantic.items()[0].1, Item::Reasoning(r) if r.parts.is_empty() && r.replay.as_ref().unwrap().format() == ReplayFormat::AnthropicMessagesThinking)
    );
    assert!(
        matches!(&observed.semantic.items()[1].1, Item::ToolCall(c) if c.arguments.as_structured().unwrap().value() == &json!({"key":"A"}))
    );
    assert_eq!(observed.semantic.usage().unwrap().total_tokens, None);
    assert_eq!(
        observed.semantic.usage().unwrap().input_relation,
        InputTokenRelation::ExcludesCacheReadAndWrite
    );
    assert_eq!(
        p.encode_generation_response(&observed, &source, &target())
            .unwrap(),
        serde_json::from_slice::<Value>(response()).unwrap()
    );
    use morphiecore::semantic::context::ServiceClass;
    assert_eq!(
        observed.metadata.context.service_class().clone(),
        Presence::Absent
    );
    let mut reported: Value = serde_json::from_slice(response()).unwrap();
    reported["usage"]["service_tier"] = json!("priority");
    let decoded = p
        .decode_generation_response(
            reported.to_string().as_bytes(),
            LocalScope::new(2),
            &target(),
        )
        .unwrap();
    assert_eq!(
        decoded.metadata.context.service_class().clone(),
        Presence::Value(ServiceClass::PriorityCapacity)
    );
    let mut manual = observed.clone();
    manual
        .metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::PriorityCapacity);
    assert_eq!(
        p.encode_generation_response(&manual, &source, &target())
            .unwrap(),
        reported
    );
    manual
        .metadata
        .context
        .reports
        .latest_mut()
        .expect("report")
        .service_class = Presence::Value(ServiceClass::PriorityProcessing);
    assert!(
        p.encode_generation_response(&manual, &source, &target())
            .is_err()
    );
}
#[test]
fn native_protected_dispatch_and_fallback_reports_reject_before_projection() {
    use morphiecore::protocol::CodecError;
    let p = Profile::AdaptiveTextTools;
    let original: Value = serde_json::from_slice(response()).unwrap();
    for namespace in [json!("set"), json!(""), Value::Null] {
        let mut wire = original.clone();
        wire["content"][1]["toolset_name"] = namespace;
        assert_eq!(
            p.decode_generation_response(wire.to_string().as_bytes(), LocalScope::ROOT, &target())
                .unwrap_err(),
            CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Tools)
        );
    }
    let mut refusal = json!({"id":"msg_refusal","type":"message","role":"assistant","model":"synthetic-model",
        "content":[{"type":"text","text":"partial"}],"stop_reason":"refusal","stop_sequence":null,
        "usage":{"input_tokens":11,"output_tokens":3}});
    let plain = p
        .decode_generation_response(refusal.to_string().as_bytes(), LocalScope::ROOT, &target())
        .unwrap();
    assert_eq!(plain.semantic.outcome(), Outcome::Incomplete);
    for (field, value) in [
        ("fallback_credit_token", json!("synthetic-credit")),
        ("recommended_model", json!("synthetic-fallback")),
        ("fallback_has_prefill_claim", json!(false)),
        ("fallback_has_prefill_claim", json!(true)),
    ] {
        let mut details = json!({"type":"refusal","category":"cyber","explanation":"synthetic"});
        details[field] = value;
        if field == "fallback_has_prefill_claim" {
            details["fallback_credit_token"] = json!("synthetic-credit");
        }
        refusal["stop_details"] = details;
        assert_eq!(
            p.decode_generation_response(
                refusal.to_string().as_bytes(),
                LocalScope::ROOT,
                &target()
            )
            .unwrap_err(),
            CodecError::Unsupported(if field == "fallback_has_prefill_claim" {
                morphiecore::protocol::failure::CarrierReason::Unavailable
            } else {
                morphiecore::protocol::failure::CarrierReason::UnknownField
            })
        );
    }
    // The pinned beta source explicitly uses null for unavailable facts.
    refusal["stop_details"] = json!({"type":"refusal","fallback_credit_token":null,
        "fallback_has_prefill_claim":null,"recommended_model":null,"future_report":false});
    let absent = p
        .decode_generation_response(refusal.to_string().as_bytes(), LocalScope::ROOT, &target())
        .unwrap();
    assert_eq!(absent.semantic.outcome(), plain.semantic.outcome());
    assert_eq!(
        absent.semantic.details().refusal,
        Presence::Value(RefusalDetails {
            category: Presence::Absent,
            explanation: Presence::Absent,
        })
    );
    for (field, value) in [
        ("fallback_credit_token", json!(false)),
        ("recommended_model", json!({})),
        ("fallback_has_prefill_claim", json!("false")),
    ] {
        refusal["stop_details"] = json!({"type":"refusal"});
        refusal["stop_details"][field] = value;
        assert_eq!(
            p.decode_response(refusal.to_string().as_bytes())
                .unwrap_err(),
            CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
        );
    }
}

#[test]
fn stable_refusal_details_have_independent_native_directions_and_guarded_delivery_loss() {
    use morphiecore::protocol::{CodecError, DecodedResponse, ResponseMetadata};
    let p = Profile::AdaptiveTextTools;
    let text = |s: &str| Text::allowing_empty(s, "fixture", MAX_TERMINAL_REPORT_BYTES).unwrap();
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    for (wire_details, expected) in [
        (None, Presence::Absent),
        (Some(Value::Null), Presence::Null),
        (
            Some(json!({"type":"refusal"})),
            Presence::Value(RefusalDetails {
                category: Presence::Absent,
                explanation: Presence::Absent,
            }),
        ),
        (
            Some(json!({"type":"refusal","category":null,"explanation":null})),
            Presence::Value(RefusalDetails {
                category: Presence::Null,
                explanation: Presence::Null,
            }),
        ),
        (
            Some(
                json!({"type":"refusal","category":"cyber","explanation":"synthetic explanation"}),
            ),
            Presence::Value(RefusalDetails {
                category: Presence::Value(RefusalCategory::Cyber),
                explanation: Presence::Value(text("synthetic explanation")),
            }),
        ),
        (
            Some(json!({"type":"refusal","category":null,"explanation":""})),
            Presence::Value(RefusalDetails {
                category: Presence::Null,
                explanation: Presence::Value(text("")),
            }),
        ),
    ] {
        let mut wire = json!({"id":"msg_refusal","type":"message","role":"assistant","model":"synthetic-model",
            "content":[{"type":"text","text":"partial"}],"stop_reason":"refusal","stop_sequence":null,
            "usage":{"input_tokens":11,"output_tokens":3}});
        if let Some(value) = wire_details {
            wire["stop_details"] = value;
        }
        let decoded = p
            .decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target())
            .unwrap();
        assert_eq!(decoded.semantic.details().refusal, expected);
        assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
        // The reverse oracle is constructed without decoding the report under test.
        let id = ItemId::scoped(LocalScope::new(3), 1);
        let mut usage = Usage::operation(11, 3, 14);
        usage.total_tokens = None;
        usage.total_relation = TotalTokenRelation::Unreported;
        usage.input_relation = InputTokenRelation::ExcludesCacheReadAndWrite;
        let semantic = GenerationResponse::new(
            vec![(
                id,
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part::new(
                        PartId::scoped(LocalScope::new(3), 1),
                        ContentPart::Text(text("partial").into()),
                    )],
                }),
            )],
            Outcome::Incomplete,
        )
        .unwrap()
        .with_message_envelopes(vec![
            MessageEnvelope::new(
                GroupId::new(LocalScope::new(3), 1),
                MessageEnvelopeRole::Assistant,
                vec![id],
            )
            .unwrap(),
        ])
        .unwrap()
        .with_usage(Some(usage))
        .unwrap()
        .with_details(TerminalDetails {
            incomplete: Some(IncompleteReason::ContentFilter),
            refusal: expected.clone(),
            ..Default::default()
        })
        .unwrap();
        let manual = DecodedResponse {
            evidence: Default::default(),
            semantic,
            fidelity: Default::default(),
            metadata: ResponseMetadata {
                delivery: None,
                id: Some("msg_refusal".into()),
                model: "synthetic-model".into(),
                storage: None,
                created: None,
                context: Default::default(),
                instruction_fidelity: Default::default(),
            },
        };
        assert_eq!(
            p.encode_generation_response(&manual, &source, &target())
                .unwrap(),
            wire
        );
        assert_eq!(
            morphiecore::protocol::anthropic::messages::client::encode_client_response(
                &manual,
                "synthetic-model"
            )
            .unwrap(),
            wire
        );
        for profile in [
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::Profile::Responses,
        ] {
            // Isolate reports with a target-representable incomplete text item;
            // native lifecycle/usage projection has separate checks.
            let mut items = manual.semantic.items().to_vec();
            let Item::Message(message) = &mut items[0].1 else {
                unreachable!()
            };
            message.status = ItemLifecycle::Incomplete;
            let report_only = GenerationResponse::new(items, Outcome::Incomplete)
                .unwrap()
                .with_details(manual.semantic.details().clone())
                .unwrap();
            let metadata = crate::events_support::metadata();
            let result = morphiecore::protocol::openai::lowering::lower_response(
                &report_only,
                &manual.fidelity,
                &metadata,
                profile,
                morphiecore::protocol::openai::TargetCaps::full(),
            );
            if expected
                .value()
                .is_some_and(|v| v.category.value().is_some())
            {
                assert_eq!(
                    result.err(),
                    Some(morphiecore::lowering::generation::RepresentationError::Terminal)
                );
            } else {
                let lowered = result.unwrap();
                assert!(lowered.semantic().details().refusal.is_absent());
                assert_eq!(report_only.details().refusal, expected);
                assert_eq!(
                    lowered
                        .projection()
                        .iter()
                        .any(|v| v.rule() == "openai.omit-explanatory-refusal-details.v1"),
                    !expected.is_absent()
                );
            }
        }
        if let Some(report) = expected.value() {
            assert!(!format!("{report:?}").contains("synthetic explanation"));
        }
    }
    for (details, invalid_shape) in [
        (json!({"type":"other"}), false),
        (json!({"type":"refusal","category":"unknown"}), true),
        (json!({"type":"refusal","explanation":false}), true),
    ] {
        let wire = json!({"id":"msg_refusal","type":"message","role":"assistant","model":"synthetic-model",
            "content":[],"stop_reason":"refusal","stop_sequence":null,"stop_details":details,
            "usage":{"input_tokens":11,"output_tokens":3}});
        let error = p.decode_response(wire.to_string().as_bytes()).unwrap_err();
        assert!(if invalid_shape {
            matches!(
                error,
                CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue)
            )
        } else {
            matches!(
                error,
                CodecError::Unsupported(morphiecore::protocol::failure::CarrierReason::Unavailable)
            )
        });
    }
    let details = TerminalDetails {
        refusal: Presence::Value(RefusalDetails {
            category: Presence::Null,
            explanation: Presence::Value(text("synthetic explanation")),
        }),
        ..Default::default()
    };
    assert_eq!(
        details.validate(Outcome::Completed),
        Err(GenerationError::InvalidResponse)
    );
    let mut large = details;
    large.incomplete = Some(IncompleteReason::ContentFilter);
    large.refusal = Presence::Value(RefusalDetails {
        category: Presence::Absent,
        explanation: Presence::Value(text(&"x".repeat(MAX_TERMINAL_REPORT_BYTES))),
    });
    assert_eq!(
        large.validate(Outcome::Incomplete),
        Err(GenerationError::Limit)
    );
}

#[test]
fn native_stop_reasons_have_independent_terminal_owners() {
    let p = Profile::AdaptiveTextTools;
    let decode = |stop: &str, sequence: Value| {
        let mut wire = json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
                "content":[{"type":"text","text":"partial"}],"stop_reason":stop,"stop_sequence":sequence,
                "usage":{"input_tokens":11,"output_tokens":7}});
        if stop == "refusal" {
            wire["stop_details"] =
                json!({"type":"refusal","category":"cyber","explanation":"synthetic"});
        }
        p.decode_generation_response(wire.to_string().as_bytes(), LocalScope::new(2), &target())
    };
    for (stop, sequence, outcome, progress, incomplete) in [
        (
            "stop_sequence",
            json!("END"),
            Outcome::Completed,
            InteractionProgress::TurnFinished,
            None,
        ),
        (
            "model_context_window_exceeded",
            Value::Null,
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            Some(IncompleteReason::ContextWindowExceeded),
        ),
        (
            "refusal",
            Value::Null,
            Outcome::Incomplete,
            InteractionProgress::Unreported,
            Some(IncompleteReason::ContentFilter),
        ),
    ] {
        let observed = decode(stop, sequence).unwrap();
        assert_eq!(observed.semantic.outcome(), outcome, "{stop}");
        assert_eq!(observed.semantic.progress(), progress, "{stop}");
        assert_eq!(observed.semantic.details().incomplete, incomplete, "{stop}");
        if stop == "stop_sequence" {
            let source = p
                .decode_generation_request(request(), LocalScope::new(1), &target())
                .unwrap();
            let wire = p
                .encode_generation_response(&observed, &source, &target())
                .unwrap();
            assert_eq!(wire["stop_reason"], "stop_sequence");
            assert_eq!(wire["stop_sequence"], "END");
        }
    }
    assert!(decode("end_turn", json!("END")).is_err());
    assert!(decode("stop_sequence", Value::Null).is_err());
    assert_eq!(
        decode("end_turn", Value::Null)
            .unwrap()
            .semantic
            .details()
            .completion,
        Some(CompletionCause::Natural)
    );
    let mut unreported = decode("end_turn", Value::Null).unwrap();
    unreported.semantic = unreported
        .semantic
        .with_details(TerminalDetails::default())
        .unwrap();
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    assert!(
        p.encode_generation_response(&unreported, &source, &target())
            .is_err()
    );
    let expected = decode("stop_sequence", json!("END")).unwrap();
    assert_eq!(
        expected.semantic.details().completion,
        Some(CompletionCause::StopSequence(
            Text::new("END", "fixture", 64).unwrap()
        ))
    );
    // Responses has no completion-cause carrier: the cause is omitted under a named rule.
    // Native progress is removed by the separate native client projection.
    let delivered = GenerationResponse::new(
        expected.semantic.items().to_vec(),
        expected.semantic.outcome(),
    )
    .unwrap()
    .with_details(expected.semantic.details().clone())
    .unwrap();
    let (fidelity, metadata) = (Default::default(), crate::events_support::metadata());
    let projected = morphiecore::protocol::openai::lowering::lower_response(
        &delivered,
        &fidelity,
        &metadata,
        morphiecore::protocol::openai::Profile::Responses,
        morphiecore::protocol::openai::TargetCaps::full(),
    )
    .unwrap();
    assert!(
        projected
            .projection()
            .iter()
            .any(|stage| { stage.rule() == "responses.omit-completion-cause.v1" })
    );
    let responses = morphiecore::protocol::openai::responses::encode_response(&projected).unwrap();
    assert_eq!(responses["status"], "completed");
    assert!(responses["incomplete_details"].is_null());
    let chat = morphiecore::protocol::openai::lowering::lower_response(
        &delivered,
        &fidelity,
        &metadata,
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::TargetCaps::full(),
    )
    .unwrap();
    assert!(
        chat.projection()
            .iter()
            .any(|stage| { stage.rule() == "chat.coarsen-completion-cause.v1" })
    );
    assert_eq!(
        morphiecore::protocol::openai::chat::encode_response(&chat).unwrap()["choices"][0]["finish_reason"],
        "stop"
    );
    // The reported thinking subset of output tokens is kept, never derived.
    let thinking = json!({"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model",
            "content":[{"type":"text","text":"partial"}],"stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":11,"output_tokens":7,"output_tokens_details":{"thinking_tokens":5}}});
    let thinking = p
        .decode_generation_response(
            thinking.to_string().as_bytes(),
            LocalScope::new(2),
            &target(),
        )
        .unwrap();
    assert_eq!(thinking.semantic.usage().unwrap().reasoning_tokens, Some(5));
    let window = decode("model_context_window_exceeded", Value::Null).unwrap();
    let window_wire =
        morphiecore::protocol::anthropic::messages::client::encode_client_response(&window, "m")
            .unwrap();
    assert_eq!(window_wire["stop_reason"], "model_context_window_exceeded");
    // OpenAI has no context-window reason: Chat reports `length` and Responses an
    // unspecified incomplete reason, each under a named rule; neither becomes success.
    use morphiecore::protocol::openai::Profile as OpenAi;
    // As after the native client projection: no native usage, items follow the outcome.
    let mut items = window.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        unreachable!()
    };
    message.status = ItemLifecycle::Incomplete;
    let stopped = GenerationResponse::new(items, window.semantic.outcome())
        .unwrap()
        .with_details(window.semantic.details().clone())
        .unwrap();
    for (profile, rule) in [
        (OpenAi::Chat, "chat.context-window-as-length.v1"),
        (OpenAi::Responses, "responses.omit-context-window-reason.v1"),
    ] {
        let projected = morphiecore::protocol::openai::lowering::lower_response(
            &stopped,
            &fidelity,
            &metadata,
            profile,
            morphiecore::protocol::openai::TargetCaps::full(),
        )
        .unwrap();
        let named = |stages: &[morphiecore::lowering::generation::Effect]| {
            stages.iter().any(|stage| stage.rule() == rule)
        };
        assert!(named(projected.projection()), "{rule}");
        let events = [
            vec![
                StreamEvent::Started,
                crate::events_support::start(1, ItemKind::Message { phase: None }),
            ],
            crate::events_support::part(1, 1, PartKind::Text, "partial"),
            vec![
                crate::events_support::close(1, ItemLifecycle::Incomplete),
                StreamEvent::Terminal {
                    terminal: StreamTerminal::Incomplete,
                    details: stopped.details().clone(),
                },
            ],
        ]
        .concat();
        let mut encoder =
            morphiecore::protocol::openai::events::EventEncoder::new(profile, metadata.clone())
                .unwrap();
        let frames: Vec<_> = events
            .iter()
            .flat_map(|event| encoder.encode(event, &fidelity).unwrap())
            .collect();
        encoder.finish().unwrap();
        assert!(named(encoder.projection()), "{rule}");
        let last = frames.last().unwrap();
        if profile == OpenAi::Chat {
            let json = morphiecore::protocol::openai::chat::encode_response(&projected).unwrap();
            assert_eq!(json["choices"][0]["finish_reason"], "length");
            assert_eq!(last["choices"][0]["finish_reason"], "length");
        } else {
            let json =
                morphiecore::protocol::openai::responses::encode_response(&projected).unwrap();
            for response in [&json, &last["response"]] {
                assert_eq!(response["status"], "incomplete");
                assert_eq!(response["incomplete_details"], json!({"reason": null}));
            }
            assert_eq!(last["type"], "response.incomplete");
        }
    }
    let frames = vec![
        json!({"type":"message_start","message":{"id":"msg_synthetic","type":"message","role":"assistant","model":"synthetic-model","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"partial"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"END"},"usage":{"output_tokens":7}}),
        json!({"type":"message_stop"}),
    ];
    let bytes = frames
        .iter()
        .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
        .collect::<String>();
    let mut decoder = morphiecore::protocol::anthropic::messages::GenerationSseDecoder::new(
        200,
        "text/event-stream",
        Default::default(),
        LocalScope::new(2),
        target(),
    )
    .unwrap();
    let mut state = StreamState::new();
    let mut client =
        morphiecore::protocol::anthropic::messages::client::sse::ClientEventEncoder::new("r", "m");
    let mut output = Vec::new();
    let mut rest = bytes.as_bytes();
    while !rest.is_empty() {
        let (used, events) = decoder.consume(rest).unwrap();
        assert!(used > 0);
        rest = &rest[used..];
        for event in events {
            state = reduce(state, event.clone()).unwrap();
            output.extend(client.encode(&event).unwrap().into_iter().flatten());
        }
    }
    decoder.finish().unwrap();
    client.finish().unwrap();
    assert_eq!(materialize(&state).unwrap(), expected.semantic);
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\"stop_reason\":\"stop_sequence\""));
    assert!(output.contains("\"stop_sequence\":\"END\""));
    let static_wire =
        morphiecore::protocol::anthropic::messages::client::encode_client_response(&expected, "m")
            .unwrap();
    assert_eq!(static_wire["stop_sequence"], "END");
    let mut conflict = frames;
    conflict.insert(4, json!({"type":"message_delta","delta":{"stop_reason":"stop_sequence","stop_sequence":"DIFFERENT"},"usage":{"output_tokens":7}}));
    let bytes = conflict
        .iter()
        .map(|v| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap()))
        .collect::<String>();
    let mut decoder = morphiecore::protocol::anthropic::messages::MessageSseDecoder::new(
        200,
        "text/event-stream",
        Default::default(),
    )
    .unwrap();
    let mut rest = bytes.as_bytes();
    let rejected = loop {
        match decoder.consume(rest) {
            Ok(used) if used > 0 => {
                rest = &rest[used..];
                if rest.is_empty() {
                    break false;
                }
            }
            _ => break true,
        }
    };
    assert!(rejected);
    assert!(decoder.finish().is_err());
}
#[test]
fn saved_observation_and_appended_error_encode_the_independent_native_history() {
    let p = Profile::AdaptiveTextTools;
    let source = p
        .decode_generation_request(request(), LocalScope::new(1), &target())
        .unwrap();
    let observed = p
        .decode_generation_response(response(), LocalScope::new(2), &target())
        .unwrap();
    for (execution, marker) in [
        (ToolExecution::NotExecuted, Some(true)),
        (ToolExecution::Failed { code: None }, Some(true)),
        (ToolExecution::Succeeded, None),
    ] {
        let mut next = feedback(&source, &observed, execution);
        let mut items = next.items().to_vec();
        let Item::ToolResult(r) = &mut items.last_mut().unwrap().1 else {
            panic!()
        };
        r.is_error = marker;
        next = next.with_items(items).unwrap();
        let encoded = p
            .encode_generation_request(&next, &observed.fidelity, &target())
            .unwrap();
        let mut result =
            json!({"type":"tool_result","tool_use_id":"c","content":"actual feedback"});
        if let Some(marker) = marker {
            result["is_error"] = json!(marker);
        }
        assert_eq!(
            encoded.value["messages"],
            json!([
                {"role":"user","content":[{"type":"text","text":"Look up A."}]},
                {"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"key":"A"}}]},
                {"role":"user","content":[result]}
            ])
        );
        assert_eq!(
            encoded
                .evidence
                .iter()
                .filter(|e| e.rule() == "messages.omit-tool-execution.v1")
                .map(|e| e.owner())
                .collect::<Vec<_>>(),
            vec![morphiecore::lowering::generation::Owner::item(
                ItemId::scoped(LocalScope::new(3), 1)
            )]
        );
        let intake = p
            .decode_generation_request(
                &serde_json::to_vec(&encoded.value).unwrap(),
                LocalScope::new(4),
                &target(),
            )
            .unwrap();
        let Item::ToolResult(result) = &intake.semantic.items().last().unwrap().1 else {
            panic!()
        };
        assert_eq!(result.is_error, marker);
        assert_eq!(result.execution, None);
    }
}
#[test]
fn automatic_cache_keeps_examples_context_and_deferred_definitions() {
    use morphiecore::protocol::CodecError;
    let p = Profile::AdaptiveTextTools;
    let body = json!({"model":"synthetic-model","max_tokens":32,
    "tools":[
        {"name":"visible","input_schema":{"type":"object"},
            "input_examples":[{"n":9007199254740993_u64}],
            "cache_control":{"type":"ephemeral","ttl":"1h"}},
        {"name":"later","input_schema":{"type":"object"},"defer_loading":true}
    ],
    "cache_control":{"type":"ephemeral","ttl":"1h"},
    "messages":[{"role":"user","content":[
        {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"},
            "context":"not citable","cache_control":{"type":"ephemeral","ttl":"1h"}},
        {"type":"text","text":""}
    ]}]});
    let exact = p
        .decode_generation_request(body.to_string().as_bytes(), LocalScope::ROOT, &target())
        .unwrap();
    assert_eq!(
        p.encode_generation_request(&exact.semantic, &exact.fidelity, &target())
            .unwrap()
            .value["tools"],
        body["tools"]
    );
    let mut conflict = body.clone();
    conflict["cache_control"]["ttl"] = json!("5m");
    let error = CodecError::Invalid(morphiecore::protocol::failure::ShapeReason::InvalidValue);
    assert_eq!(
        p.decode_request(conflict.to_string().as_bytes()).err(),
        Some(error.clone())
    );
    assert_eq!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            conflict.to_string().as_bytes()
        )
        .err(),
        Some(error.clone())
    );
    let mut settings = exact.semantic.settings().clone();
    settings.automatic_prefix_cache = Some(PrefixCacheTtl::FiveMinutes);
    let edited = exact.semantic.clone().with_settings(settings).unwrap();
    assert_eq!(
        p.encode_generation_request(&edited, &exact.fidelity, &target())
            .err(),
        Some(error)
    );
    let mut extended = body.clone();
    extended["messages"].as_array_mut().unwrap().push(json!({
        "role":"system","content":[{"type":"tool_addition","tool":{"type":"tool_definition",
            "definition":{"name":"inline","input_schema":{"type":"object"},
                "input_examples":[{}],"cache_control":{"type":"ephemeral","ttl":"1h"}}}}]
    }));
    let decoded = p
        .decode_generation_request(extended.to_string().as_bytes(), LocalScope::ROOT, &target())
        .unwrap();
    let output = p
        .encode_generation_request(&decoded.semantic, &decoded.fidelity, &target())
        .unwrap();
    assert_eq!(output.value["tools"], body["tools"]);
    assert_eq!(output.value["messages"], extended["messages"]);
}

#[test]
fn ordered_system_blocks_and_disabled_controls_have_independent_semantic_owners() {
    let p = Profile::AdaptiveTextTools;
    let content_wire = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},{"type":"text","text":"changing"}],"cache_control":{"type":"ephemeral"},"messages":[{"role":"user","content":[{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]}]}"#;
    let content = p
        .decode_generation_request(content_wire, LocalScope::new(1), &target())
        .unwrap();
    let content_client =
        morphiecore::protocol::anthropic::messages::client::decode_client_request(content_wire)
            .unwrap();
    assert_eq!(
        content.semantic.settings(),
        content_client.task.semantic.settings()
    );
    let encoded = p
        .encode_generation_request(&content.semantic, &content.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["system"],
        json!([
            {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"type":"text","text":"changing"}
        ])
    );
    assert_eq!(
        encoded.value["messages"],
        json!([
            {"role":"user","content":[{"type":"text","text":"go","cache_control":{"type":"ephemeral"}}]}
        ])
    );
    assert_eq!(encoded.value["cache_control"], json!({"type":"ephemeral"}));
    let Item::Instruction(instruction) = &content.semantic.items()[0].1 else {
        panic!("system owner")
    };
    assert_eq!(
        instruction.parts[0].cache,
        Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour))
    );
    assert_eq!(instruction.parts[1].cache, None);
    assert_eq!(
        content.semantic.settings().automatic_prefix_cache,
        Some(PrefixCacheTtl::FiveMinutes)
    );
    let text = |value| Text::new(value, "fixture", 64).unwrap();
    let mut settings = GenerationSettings {
        automatic_prefix_cache: Some(PrefixCacheTtl::FiveMinutes),
        ..Default::default()
    };
    settings.controls.max_output_tokens = Some(32);
    let manual = GenerationRequest::from_settings(
        vec![
            (
                ItemId::new(1),
                Item::Instruction(Instruction {
                    participant: None,
                    authority: InstructionAuthority::System,
                    status: None,
                    parts: vec![
                        Part {
                            replay: None,
                            id: PartId::new(1),
                            content: text("stable"),
                            cache: Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour)),
                        },
                        Part::new(PartId::new(2), text("changing")),
                    ],
                }),
            ),
            (
                ItemId::new(2),
                Item::Message(Message {
                    participant: None,
                    role: MessageRole::User,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: vec![Part {
                        replay: None,
                        id: PartId::new(3),
                        content: ContentPart::Text(text("go").into()),
                        cache: Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::FiveMinutes)),
                    }],
                }),
            ),
        ],
        settings,
    )
    .unwrap()
    .with_message_envelopes(vec![
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 1),
            MessageEnvelopeRole::User,
            vec![ItemId::new(2)],
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(
        p.encode_generation_request(&manual, &Default::default(), &target())
            .unwrap()
            .value,
        serde_json::from_slice::<Value>(content_wire).unwrap()
    );
    assert!(
        morphiecore::protocol::openai::lowering::lower_request(
            &manual,
            &Default::default(),
            morphiecore::protocol::openai::Profile::Chat,
            morphiecore::protocol::openai::TargetCaps::full()
        )
        .is_err()
    );
    let mut conflict: Value = serde_json::from_slice(content_wire).unwrap();
    conflict["cache_control"]["ttl"] = json!("1h");
    assert_eq!(
        p.decode_request(conflict.to_string().as_bytes())
            .err()
            .unwrap(),
        morphiecore::protocol::CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        )
    );
    let mut full: Value = serde_json::from_slice(content_wire).unwrap();
    full["tools"] = json!([
        {"name":"first","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}},
        {"name":"second","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}
    ]);
    assert_eq!(
        p.decode_request(full.to_string().as_bytes()).err(),
        Some(morphiecore::protocol::CodecError::Limit)
    );
    // A single 1h tail must not conflict with an unrequested 5m policy.
    let long = p.decode_generation_request(
            br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}}]}]}"#,
            LocalScope::new(1), &target(),
        ).unwrap();
    let Item::Message(message) = &long.semantic.items()[0].1 else {
        panic!("message")
    };
    assert_eq!(
        message.parts[0].cache,
        Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour))
    );
    assert_eq!(
        p.encode_generation_request(&long.semantic, &long.fidelity, &target())
            .unwrap()
            .value,
        json!({"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[
            {"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}}
        ]}]})
    );
    let mut items = content.semantic.items().to_vec();
    let Item::Instruction(instruction) = &mut items[0].1 else {
        unreachable!()
    };
    instruction.parts[0].cache = None;
    let edited = content.semantic.clone().with_items(items).unwrap();
    let encoded = p
        .encode_generation_request(&edited, &content.fidelity, &target())
        .unwrap();
    assert!(encoded.value["system"][0].get("cache_control").is_none());
    assert!(GenerationRequirements::derive(&edited).explicit_cache);
    let nested_wire = br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"lookup"},{"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},"cache_control":{"type":"ephemeral","ttl":"1h"}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"c","cache_control":{"type":"ephemeral"},"content":[{"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},{"type":"text","text":"tail"}]}]}]}"#;
    let nested = p
        .decode_generation_request(nested_wire, LocalScope::new(1), &target())
        .unwrap();
    let Item::ToolCall(call) = &nested.semantic.items()[1].1 else {
        panic!("call owner")
    };
    assert_eq!(call.cache, Some(PrefixCacheTtl::OneHour));
    let Item::ToolResult(result) = &nested.semantic.items()[2].1 else {
        panic!("result owner")
    };
    assert_eq!(result.cache, Some(PrefixCacheTtl::FiveMinutes));
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("inner owners")
    };
    assert_eq!(
        parts[0].cache,
        Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::OneHour))
    );
    assert_eq!(parts[1].cache, None);
    let encoded = p
        .encode_generation_request(&nested.semantic, &nested.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["messages"],
        json!([
            {"role":"user","content":[{"type":"text","text":"lookup"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"c","name":"lookup","input":{},"cache_control":{"type":"ephemeral","ttl":"1h"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","cache_control":{"type":"ephemeral"},"content":[
                {"type":"text","text":"stable","cache_control":{"type":"ephemeral","ttl":"1h"}},
                {"type":"text","text":"tail"}
            ]}]}
        ])
    );
    assert!(p.decode_request(br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":[{"type":"text","text":"","cache_control":{"type":"ephemeral"}}]}]}"#).is_err());
    let empty_wire = json!({
        "model":"synthetic-model","max_tokens":32,
        "system":[{"type":"text","text":"stable"},{"type":"text","text":""}],
        "cache_control":{"type":"ephemeral","ttl":"1h"},
        "messages":[{"role":"user","content":[
            {"type":"text","text":"go","cache_control":{"type":"ephemeral","ttl":"1h"}},
            {"type":"text","text":""}
        ]}]
    });
    let empty = p
        .decode_generation_request(
            empty_wire.to_string().as_bytes(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            empty_wire.to_string().as_bytes()
        )
        .is_ok()
    );
    let encoded = p
        .encode_generation_request(&empty.semantic, &empty.fidelity, &target())
        .unwrap();
    assert_eq!(encoded.value, empty_wire);
    let reject_empty_cache = |wire: &Value| {
        let bytes = wire.to_string();
        let error = morphiecore::protocol::CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue,
        );
        assert_eq!(
            p.decode_request(bytes.as_bytes()).err(),
            Some(error.clone())
        );
        assert_eq!(
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                bytes.as_bytes()
            )
            .err(),
            Some(error)
        );
    };
    let mut cached_empty = empty_wire.clone();
    cached_empty["messages"][0]["content"][1]["cache_control"] =
        json!({"type":"ephemeral","ttl":"1h"});
    reject_empty_cache(&cached_empty);
    let mut cached_system = empty_wire.clone();
    cached_system["system"][1]["cache_control"] = json!({"type":"ephemeral","ttl":"1h"});
    reject_empty_cache(&cached_system);
    let mut cached_nested: Value = serde_json::from_slice(nested_wire).unwrap();
    cached_nested["messages"][2]["content"][0]["content"][0]["text"] = json!("");
    reject_empty_cache(&cached_nested);
    cached_nested["messages"][2]["content"][0]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("cache_control");
    assert!(
        p.decode_generation_request(
            cached_nested.to_string().as_bytes(),
            LocalScope::new(1),
            &target()
        )
        .is_ok()
    );
    let mut items = manual.items().to_vec();
    let Item::Message(message) = &mut items[1].1 else {
        panic!("message owner")
    };
    message.parts.push(Part {
        replay: None,
        id: PartId::new(4),
        content: ContentPart::Text(Text::allowing_empty("", "fixture", 64).unwrap().into()),
        cache: Some(PrefixCacheBoundary::Ttl(PrefixCacheTtl::FiveMinutes)),
    });
    let cached_edit = manual.clone().with_items(items).unwrap();
    assert_eq!(
        p.encode_generation_request(&cached_edit, &Default::default(), &target())
            .err(),
        Some(morphiecore::protocol::CodecError::Invalid(
            morphiecore::protocol::failure::ShapeReason::InvalidValue
        ))
    );
    let mut automatic_conflict = empty_wire.clone();
    automatic_conflict["cache_control"]["ttl"] = json!("5m");
    let conflict_error = morphiecore::protocol::CodecError::Invalid(
        morphiecore::protocol::failure::ShapeReason::InvalidValue,
    );
    assert_eq!(
        p.decode_request(automatic_conflict.to_string().as_bytes())
            .err(),
        Some(conflict_error.clone())
    );
    assert_eq!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            automatic_conflict.to_string().as_bytes()
        )
        .err(),
        Some(conflict_error.clone())
    );
    let mut settings = empty.semantic.settings().clone();
    settings.automatic_prefix_cache = Some(PrefixCacheTtl::FiveMinutes);
    let conflict_edit = empty.semantic.clone().with_settings(settings).unwrap();
    assert_eq!(
        p.encode_generation_request(&conflict_edit, &empty.fidelity, &target())
            .err(),
        Some(conflict_error)
    );
    let cached_wire = br#"{"model":"synthetic-model","max_tokens":32,"tools":[{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}],"messages":[{"role":"user","content":"go"}]}"#;
    let cached = p
        .decode_generation_request(cached_wire, LocalScope::new(1), &target())
        .unwrap();
    assert!(
        matches!(&cached.semantic.tools()[0], ToolDefinition::Function(tool)
            if tool.cache == Some(PrefixCacheTtl::OneHour)
                && tool.strict == FunctionStrictness::Omitted(StrictDefault::NonStrict)
                && tool.parameters.as_ref().unwrap().root() == &json!({"type":"object"}))
    );
    let client =
        morphiecore::protocol::anthropic::messages::client::decode_client_request(cached_wire)
            .unwrap();
    assert_eq!(client.task.semantic.settings(), cached.semantic.settings());
    assert!(GenerationRequirements::derive(&client.task.semantic).explicit_cache);
    let encoded = p
        .encode_generation_request(&cached.semantic, &cached.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["tools"],
        json!([{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral","ttl":"1h"}}])
    );
    let mut settings = cached.semantic.settings().clone();
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!("function tool")
    };
    tool.cache = Some(PrefixCacheTtl::FiveMinutes);
    let short = cached
        .semantic
        .clone()
        .with_settings(settings.clone())
        .unwrap();
    let encoded = p
        .encode_generation_request(&short, &cached.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["tools"],
        json!([{"name":"lookup","input_schema":{"type":"object"},"cache_control":{"type":"ephemeral"}}])
    );
    let ToolDefinition::Function(tool) = &mut settings.tools.as_mut().unwrap()[0] else {
        panic!("function tool")
    };
    tool.cache = None;
    let removed = cached.semantic.clone().with_settings(settings).unwrap();
    let encoded = p
        .encode_generation_request(&removed, &cached.fidelity, &target())
        .unwrap();
    assert!(encoded.value["tools"][0].get("cache_control").is_none());
    assert!(!GenerationRequirements::derive(&removed).explicit_cache);
    for profile in [
        morphiecore::protocol::openai::Profile::Chat,
        morphiecore::protocol::openai::Profile::Responses,
    ] {
        assert_eq!(
            morphiecore::protocol::openai::lowering::lower_request(
                &client.task.semantic,
                &client.task.fidelity,
                profile,
                morphiecore::protocol::openai::TargetCaps::full(),
            )
            .err()
            .unwrap(),
            morphiecore::lowering::generation::RepresentationError::Controls
        );
    }
    assert!(
        morphiecore::protocol::google::generate_content::encode_generation_request(
            &client.task.semantic,
            &client.task.fidelity,
            &target(),
            None,
        )
        .is_err()
    );
    assert!(
        morphiecore::protocol::google::interactions::encode_generation_request(
            &client.task.semantic,
            &client.task.fidelity,
            &target()
        )
        .is_err()
    );
    let invalid = std::str::from_utf8(cached_wire)
        .unwrap()
        .replace("\"1h\"", "\"1d\"");
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            invalid.as_bytes()
        )
        .is_err()
    );
    let mut bounded: Value = serde_json::from_slice(cached_wire).unwrap();
    bounded["tools"] = json!(
        (0..4)
            .map(|index| json!({
                "name":format!("lookup_{index}"),"input_schema":{"type":"object"},
                "cache_control":{"type":"ephemeral","ttl":"1h"}
            }))
            .collect::<Vec<_>>()
    );
    let four = p
        .decode_generation_request(
            bounded.to_string().as_bytes(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    assert_eq!(four.semantic.settings().automatic_prefix_cache, None);
    let requirements = GenerationRequirements::derive(&four.semantic);
    assert!(requirements.explicit_cache && !requirements.automatic_prefix_cache);
    let encoded = p
        .encode_generation_request(&four.semantic, &four.fidelity, &target())
        .unwrap();
    assert!(encoded.value.get("cache_control").is_none());
    assert_eq!(encoded.value["tools"].as_array().unwrap().len(), 4);
    bounded["tools"].as_array_mut().unwrap().push(json!({
        "name":"extra","input_schema":{"type":"object"},
        "cache_control":{"type":"ephemeral"}
    }));
    assert!(
        p.decode_generation_request(
            bounded.to_string().as_bytes(),
            LocalScope::new(1),
            &target()
        )
        .is_err()
    );
    bounded["tools"].as_array_mut().unwrap().truncate(2);
    bounded["tools"][0]["cache_control"]["ttl"] = json!("5m");
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            bounded.to_string().as_bytes()
        )
        .is_err()
    );
    let raw = br#"{"model":"synthetic-model","max_tokens":32,"system":[{"type":"text","text":"one"},{"type":"text","text":"two"}],"thinking":{"type":"disabled"},"output_config":{"effort":"high"},"messages":[{"role":"user","content":"go"}]}"#;
    let decoded = p
        .decode_generation_request(raw, LocalScope::new(1), &target())
        .unwrap();
    assert!(decoded.semantic.instructions().is_absent());
    let Item::Instruction(i) = &decoded.semantic.items()[0].1 else {
        panic!()
    };
    assert_eq!(i.authority, InstructionAuthority::System);
    assert_eq!(
        i.texts().map(|(_, t)| t).collect::<Vec<_>>(),
        ["one", "two"]
    );
    let encoded = p
        .encode_generation_request(&decoded.semantic, &decoded.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["system"],
        json!([{"type":"text","text":"one"},{"type":"text","text":"two"}])
    );
    assert_eq!(encoded.value["thinking"], json!({"type":"disabled"}));
    assert_eq!(encoded.value["output_config"], json!({"effort":"high"}));
    assert!(encoded.value.get("cache_control").is_none());
    assert_eq!(encoded.value["max_tokens"], 32);
    let mut stopped: Value = serde_json::from_slice(raw).unwrap();
    stopped["stop_sequences"] = json!(["END"]);
    let stopped = p
        .decode_generation_request(
            stopped.to_string().as_bytes(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    assert_eq!(
        stopped
            .semantic
            .controls()
            .stop_sequences
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        ["END"]
    );
    let encoded = p
        .encode_generation_request(&stopped.semantic, &stopped.fidelity, &target())
        .unwrap();
    assert_eq!(encoded.value["stop_sequences"], json!(["END"]));
    assert!(
        morphiecore::protocol::openai::lowering::lower_request(
            &stopped.semantic,
            &stopped.fidelity,
            morphiecore::protocol::openai::Profile::Responses,
            morphiecore::protocol::openai::TargetCaps::full()
        )
        .is_err()
    );
    let mut automatic: Value = serde_json::from_slice(raw).unwrap();
    automatic["cache_control"] = json!({"type":"ephemeral"});
    assert!(
        p.decode_generation_request(
            automatic.to_string().as_bytes(),
            LocalScope::new(1),
            &target()
        )
        .is_ok()
    );
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            automatic.to_string().as_bytes()
        )
        .is_ok()
    );
    let client =
        morphiecore::protocol::anthropic::messages::client::decode_client_request(raw).unwrap();
    assert_eq!(
        client.task.semantic.reasoning().activation,
        Presence::Value(ReasoningActivation::Disabled)
    );
    let invalid = std::str::from_utf8(raw)
        .unwrap()
        .replace("\"disabled\"", "7");
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            invalid.as_bytes()
        )
        .is_err()
    );
    // Pure codecs require a resolved ceiling; only trusted target caps insert defaults.
    let mut settings = decoded.semantic.settings().clone();
    settings.controls.max_output_tokens = None;
    let uncapped = decoded.semantic.clone().with_settings(settings).unwrap();
    assert!(
        matches!(p.encode_generation_request(&uncapped, &decoded.fidelity, &target()),
        Err(morphiecore::protocol::CodecError::Unsupported(value)) if value == morphiecore::protocol::failure::CarrierReason::Unavailable)
    );
    for max_tokens in [Value::Null, json!(0)] {
        let mut invalid: Value = serde_json::from_slice(raw).unwrap();
        invalid["max_tokens"] = max_tokens;
        assert!(
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                invalid.to_string().as_bytes()
            )
            .is_err()
        );
    }
    let mut missing: Value = serde_json::from_slice(raw).unwrap();
    missing.as_object_mut().unwrap().remove("max_tokens");
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            missing.to_string().as_bytes()
        )
        .is_err()
    );
    automatic["cache_control"]["ttl"] = json!("1h");
    let long = p
        .decode_generation_request(
            automatic.to_string().as_bytes(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    assert_eq!(
        long.semantic.settings().automatic_prefix_cache,
        Some(PrefixCacheTtl::OneHour)
    );
    let encoded = p
        .encode_generation_request(&long.semantic, &long.fidelity, &target())
        .unwrap();
    assert_eq!(
        encoded.value["cache_control"],
        json!({"type":"ephemeral","ttl":"1h"})
    );
    automatic["cache_control"]["ttl"] = json!("1d");
    assert!(
        p.decode_generation_request(
            automatic.to_string().as_bytes(),
            LocalScope::new(1),
            &target()
        )
        .is_err()
    );
}
