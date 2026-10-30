//! Delivery snapshots, coalescing barriers and bounded wire output.
use super::*;
use crate::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    semantic::value::Presence,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn coalesce_events(
    delivery: &mut ResponseDelivery,
    attempt: &Attempt,
    events: &[StreamEvent],
    now: Instant,
) -> Result<Vec<Bytes>, AttemptError> {
    let mut frames = vec![];
    for batch in delivery.queue_events(attempt, events, now)? {
        frames.extend(
            delivery
                .encode_prepared(batch)
                .map(|batch| batch.into_bytes())?,
        );
    }
    Ok(frames)
}

#[test]
fn native_tool_argument_strings_match_stream_terminal_and_retained_history() {
    use crate::semantic::task::generation::{Item, ToolArguments};
    let endpoint = crate::topology::catalog::MESSAGES_BINDINGS[0].endpoint();
    let arguments = " \n\t{\"serial\":9007199254740993, \"n\":1e0}\n";
    let frames = [
        json!({"type":"message_start","message":{"id":"r","type":"message","role":"assistant",
            "model":endpoint.upstream_model,"content":[],"stop_reason":null,"stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"c","name":"lookup","input":{}}}),
        json!({"type":"content_block_delta","index":0,
            "delta":{"type":"input_json_delta","partial_json":arguments}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},
            "usage":{"output_tokens":2}}),
        json!({"type":"message_stop"}),
    ];
    for stream in [false, true] {
        let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(
                GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
                TargetContract::full(),
            ),
            "public",
            Default::default(),
            Default::default(),
            Obfuscation::Disabled,
        );
        let mut output = vec![];
        for frame in &frames {
            let (_, events) = attempt
                .push(
                    format!(
                        "event: {}\ndata: {frame}\n\n",
                        frame["type"].as_str().unwrap()
                    )
                    .as_bytes(),
                )
                .unwrap();
            if stream {
                output.extend(
                    delivery
                        .encode_events(&attempt, &events)
                        .unwrap()
                        .into_bytes()
                        .concat(),
                );
            }
        }
        attempt.finish().unwrap();
        let source = attempt.response().unwrap().clone();
        assert!(matches!(&source.semantic.items()[0].1,
            Item::ToolCall(call) if matches!(call.arguments, ToolArguments::Structured(_))));
        let delivered: Value = if stream {
            output.extend(
                delivery
                    .finish_stream(&attempt)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
            let text = String::from_utf8(output).unwrap();
            let terminal: Value = text
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .next_back()
                .unwrap();
            assert_eq!(terminal["response"]["output"][0]["arguments"], arguments);
            terminal["response"].clone()
        } else {
            serde_json::from_slice(&delivery.encode_json(&attempt).unwrap()).unwrap()
        };
        let (retained, bytes) = delivery.snapshot(&attempt).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["output"], delivered["output"]);
        let Item::ToolCall(call) = &retained.semantic.items()[0].1 else {
            panic!("tool call")
        };
        assert_eq!(
            call.arguments.as_raw().unwrap(),
            saved["output"][0]["arguments"].as_str().unwrap()
        );
        assert_eq!(attempt.response().unwrap(), &source);
    }
}

#[test]
fn finalized_json_still_needs_handoff_and_cannot_resume_after_rejection() {
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 4096, Default::default());
    attempt.begin(200, "application/json").unwrap();
    attempt.push(br#"{"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[]}"#).unwrap();
    attempt.finish().unwrap();
    for reject in [false, true] {
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(adapter.clone(), TargetContract::full()),
            "public",
            Default::default(),
            StreamOptions::default(),
            Obfuscation::Disabled,
        );
        let body = delivery.encode_json(&attempt).unwrap();
        assert_eq!(
            delivery.lifecycle().state(),
            crate::execution::DeliveryState::Uncommitted
        );
        assert_eq!(delivery.snapshot(&attempt).unwrap().1, body);
        assert!(matches!(
            delivery.complete(&attempt),
            Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::DeliveryNotComplete
            ))
        ));
        if reject {
            assert!(matches!(
                delivery.encode_json(&attempt),
                Err(AttemptError::Protocol(
                    crate::execution::attempt::ProtocolReason::DeliveryClosed
                ))
            ));
            assert!(matches!(
                delivery.commit(),
                Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::NoDeliverableOutput
                ))
            ));
            assert!(matches!(
                delivery.complete(&attempt),
                Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::DeliveryNotComplete
                ))
            ));
        } else {
            delivery.commit().unwrap();
            delivery.complete(&attempt).unwrap();
            assert_eq!(
                delivery.lifecycle().state(),
                crate::execution::DeliveryState::Terminal
            );
        }
    }
}

#[test]
fn messages_end_turn_derivation_uses_original_request_without_mutating_reports() {
    use crate::semantic::task::generation::{
        CompletionCause, InteractionProgress, Item, TerminalDetails,
    };
    let binding = crate::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "openrouter")
        .unwrap();
    for api in [GenerationApi::Chat, GenerationApi::Responses] {
        let codec = GenerationCodec::new(api, Dialect::Standard, None);
        let endpoint = binding
            .endpoint(if api == GenerationApi::Chat {
                crate::topology::GenerationProtocol::OpenAiChat
            } else {
                crate::topology::GenerationProtocol::OpenAiResponses
            })
            .unwrap();
        let wire = if api == GenerationApi::Chat {
            json!({"id":"r","object":"chat.completion","created":1,"model":"m","choices":[
                {"index":0,"message":{"role":"assistant","content":"answer"},"finish_reason":"stop","logprobs":null}
            ],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2,
                "prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}})
        } else {
            json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed","metadata":null,
                "output":[{"id":"msg","type":"message","role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]}],
                "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2,
                    "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}})
        };
        let mut attempt = Attempt::new(codec, 65536, Default::default());
        attempt.begin(200, "application/json").unwrap();
        attempt.push(wire.to_string().as_bytes()).unwrap();
        assert!(attempt.response().is_err());
        attempt.finish().unwrap();
        let source = attempt.response().unwrap().clone();
        assert_eq!(source.semantic.progress(), InteractionProgress::Unreported);
        assert_eq!(
            source.semantic.details().completion,
            (api == GenerationApi::Chat).then_some(CompletionCause::NaturalOrStopSequence)
        );
        for (bound, original_stops, effective_stops) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let mut original=GenerationCodec::new(GenerationApi::Chat,Dialect::Standard,None)
                .decode_client_request(br#"{"model":"m","messages":[{"role":"user","content":"go"}],"max_completion_tokens":32}"#).unwrap();
            let mut settings = original.task.semantic.settings().clone();
            if original_stops {
                settings
                    .controls
                    .stop_sequences
                    .push(crate::semantic::value::Text::new("END", "fixture", 16).unwrap());
            }
            original.task.semantic = original.task.semantic.with_settings(settings).unwrap();
            let mut effective = original.clone();
            let mut settings = effective.task.semantic.settings().clone();
            settings.controls.stop_sequences.clear();
            if effective_stops {
                settings
                    .controls
                    .stop_sequences
                    .push(crate::semantic::value::Text::new("END", "fixture", 16).unwrap());
            }
            effective.task.semantic = effective.task.semantic.with_settings(settings).unwrap();
            let mut delivery = ResponseDelivery::new(
                ClientAdapter::Messages,
                "public",
                Default::default(),
                Default::default(),
                Obfuscation::Disabled,
            );
            if bound {
                delivery = delivery.with_request(&endpoint, &effective, &original);
            }
            let result = delivery.encode_json(&attempt);
            if bound && !original_stops && !effective_stops {
                let output: Value = serde_json::from_slice(&result.unwrap()).unwrap();
                assert_eq!(output["stop_reason"], "end_turn");
                assert_eq!(output["stop_sequence"], Value::Null);
                assert_eq!(output["content"], json!([{"type":"text","text":"answer"}]));
                delivery.commit().unwrap();
                delivery.complete(&attempt).unwrap();
                let (retained, _) = delivery.snapshot(&attempt).unwrap();
                assert_eq!(
                    retained.semantic.progress(),
                    InteractionProgress::TurnFinished
                );
                assert_eq!(
                    retained.semantic.details().completion,
                    Some(CompletionCause::Natural)
                );
                assert!(
                    delivery
                        .report
                        .attempt()
                        .evidence
                        .entries
                        .iter()
                        .any(|entry| entry.effect.rule()
                            == "messages.infer-end-turn-from-request.v1")
                );
            } else {
                assert!(matches!(
                    result.unwrap_err().codec(),
                    Some(CodecError::Unsupported(CarrierReason::Terminal))
                ));
            }
            assert_eq!(attempt.response().unwrap(), &source);
        }
        let continued = source
            .semantic
            .clone()
            .with_progress(InteractionProgress::NeedsContinuation)
            .unwrap();
        assert!(
            crate::protocol::anthropic::messages::client::completed_without_stops(
                &continued,
                "response.project"
            )
            .unwrap()
            .is_none()
        );
        let stopped = source
            .semantic
            .clone()
            .with_details(TerminalDetails {
                completion: Some(CompletionCause::StopSequence(
                    crate::semantic::value::Text::new("END", "fixture", 16).unwrap(),
                )),
                ..Default::default()
            })
            .unwrap();
        assert!(
            crate::protocol::anthropic::messages::client::completed_without_stops(
                &stopped,
                "response.project"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            source
                .semantic
                .items()
                .iter()
                .all(|(_, item)| matches!(item, Item::Message(_)))
        );
    }
}

#[test]
fn messages_end_turn_stream_waits_for_eof_and_final_metadata() {
    use crate::semantic::task::generation::{CompletionCause, InteractionProgress};
    let endpoint = crate::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| b.endpoint_prefix == "openrouter")
        .unwrap()
        .endpoint(crate::topology::GenerationProtocol::OpenAiResponses)
        .unwrap();
    let request = ClientAdapter::Messages
        .decode_client_request(
            br#"{"model":"m","max_tokens":32,"messages":[{"role":"user","content":"go"}]}"#,
        )
        .unwrap();
    for mode in ["complete", "before-eof", "cancel", "late-metadata"] {
        let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let mut attempt = Attempt::new(codec, 65536, Default::default());
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::Messages,
            "public",
            Default::default(),
            Default::default(),
            Obfuscation::Disabled,
        )
        .with_request(&endpoint, &request, &request);
        attempt.begin(200, "text/event-stream").unwrap();
        let part = json!({"type":"output_text","text":"answer","annotations":[],"logprobs":[]});
        let item = json!({"id":"msg","type":"message","role":"assistant","status":"completed","content":[part]});
        let response = json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[item],
            "metadata":if mode=="late-metadata" {json!({"must_keep":"value"})}else{Value::Null},
            "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}});
        let frames = [
            json!({"type":"response.created","response":{"id":"r","object":"response","created_at":1,"model":"m","status":"in_progress","output":[],"metadata":null}}),
            json!({"type":"response.output_item.added","output_index":0,"item":{"id":"msg","type":"message","role":"assistant","status":"in_progress","content":[]}}),
            json!({"type":"response.content_part.added","output_index":0,"item_id":"msg","content_index":0,"part":{"type":"output_text","text":"","annotations":[],"logprobs":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"msg","content_index":0,"delta":"answer","logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":"msg","content_index":0,"text":"answer","logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":"msg","content_index":0,"part":part}),
            json!({"type":"response.output_item.done","output_index":0,"item":item}),
            json!({"type":"response.completed","response":response}),
        ];
        let mut output = vec![];
        let mut encoding_error = None;
        for (sequence, mut frame) in frames.into_iter().enumerate() {
            frame["sequence_number"] = sequence.into();
            let wire = format!("data: {frame}\n\n");
            let (used, events) = attempt.push(wire.as_bytes()).unwrap();
            assert_eq!(used, wire.len());
            if encoding_error.is_none() {
                match delivery.encode_events(&attempt, &events) {
                    Ok(batch) => {
                        output.extend(batch.into_bytes().into_iter().flatten());
                        if !output.is_empty() && delivery.lifecycle().may_retry_or_fallback() {
                            delivery.commit().unwrap();
                        }
                    }
                    Err(error) => encoding_error = Some(error),
                }
            }
        }
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("answer"));
        assert!(!text.contains("message_stop"));
        if mode == "before-eof" {
            assert!(matches!(
                delivery.finish_stream(&attempt),
                Err(AttemptError::Protocol(
                    crate::execution::attempt::ProtocolReason::AttemptNotFinished
                ))
            ));
            continue;
        }
        attempt.finish().unwrap();
        let source = attempt.response().unwrap().clone();
        let terminal = attempt.terminal_event().unwrap().clone();
        if mode == "cancel" {
            delivery.cancel();
        }
        let result = delivery.finish_stream(&attempt);
        if mode == "complete" {
            assert!(encoding_error.is_none());
            output.extend(result.unwrap().into_bytes().into_iter().flatten());
            let text = String::from_utf8_lossy(&output);
            assert_eq!(text.matches("event: message_stop\n").count(), 1);
            assert!(text.contains("\"stop_reason\":\"end_turn\""));
            delivery.complete(&attempt).unwrap();
            let (snapshot, _) = delivery.snapshot(&attempt).unwrap();
            assert_eq!(
                snapshot.semantic.progress(),
                InteractionProgress::TurnFinished
            );
            assert_eq!(
                snapshot.semantic.details().completion,
                Some(CompletionCause::Natural)
            );
        } else if mode == "late-metadata" {
            assert!(result.is_err());
            let error = encoding_error.unwrap_or_else(|| result.unwrap_err());
            assert!(matches!(
                error.codec(),
                Some(CodecError::Unsupported(CarrierReason::Unavailable))
            ));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(attempt.response().unwrap(), &source);
        assert_eq!(attempt.terminal_event().unwrap(), &terminal);
        if mode != "complete" {
            assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
        }
    }
}

#[test]
fn messages_empty_reports_match_json_stream_and_retained_views_without_loss() {
    let part = json!({"type":"output_text","text":"partial","annotations":[],"logprobs":[]});
    let item = json!({"id":"msg","type":"message","role":"assistant","status":"incomplete","content":[part]});
    for (hints, allowed) in [
        (json!({}), true),
        (json!({"metadata":null}), true),
        (json!({"metadata":{}}), false),
        (json!({"metadata":{"ticket":"reported"}}), false),
        (json!({"metadata":null,"background":false}), false),
    ] {
        for (stream, late) in [(false, false), (true, false), (true, true)] {
            let mut response = json!({"id":"r","object":"response","created_at":1,"model":"m",
                "status":"incomplete","output":[item],"incomplete_details":{"reason":"max_output_tokens"},
                "usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3,"input_tokens_details":{"cached_tokens":0}}});
            response
                .as_object_mut()
                .unwrap()
                .extend(hints.as_object().unwrap().clone());
            let codec = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
            let mut attempt = Attempt::new(codec, 65536, Default::default());
            let mut delivery = ResponseDelivery::new(
                ClientAdapter::Messages,
                "public",
                Default::default(),
                Default::default(),
                Obfuscation::Disabled,
            );
            let mut output = vec![];
            let mut error = None;
            if stream {
                attempt.begin(200, "text/event-stream").unwrap();
                let mut opening = json!({"id":"r","object":"response","created_at":1,"model":"m",
                    "status":"in_progress","output":[]});
                if !late {
                    opening
                        .as_object_mut()
                        .unwrap()
                        .extend(hints.as_object().unwrap().clone());
                }
                let frames = [
                    json!({"type":"response.created","response":opening}),
                    json!({"type":"response.output_item.added","output_index":0,"item":{
                        "id":"msg","type":"message","role":"assistant","status":"in_progress","content":[]}}),
                    json!({"type":"response.content_part.added","output_index":0,"item_id":"msg","content_index":0,
                        "part":{"type":"output_text","text":"","annotations":[]}}),
                    json!({"type":"response.output_text.delta","output_index":0,"item_id":"msg","content_index":0,"delta":"partial","logprobs":[]}),
                    json!({"type":"response.output_text.done","output_index":0,"item_id":"msg","content_index":0,"text":"partial","logprobs":[]}),
                    json!({"type":"response.content_part.done","output_index":0,"item_id":"msg","content_index":0,"part":part}),
                    json!({"type":"response.output_item.done","output_index":0,"item":item}),
                    json!({"type":"response.incomplete","response":response}),
                ];
                for (sequence, mut frame) in frames.into_iter().enumerate() {
                    frame["sequence_number"] = sequence.into();
                    let wire = format!(
                        "event: {}\ndata: {frame}\n\n",
                        frame["type"].as_str().unwrap()
                    );
                    for fragment in wire.as_bytes().chunks(7) {
                        let mut rest = fragment;
                        while !rest.is_empty() {
                            let (used, events) = attempt.push(rest).unwrap();
                            assert!(used > 0);
                            rest = &rest[used..];
                            if error.is_none() {
                                match delivery.encode_events(&attempt, &events) {
                                    Ok(batch) => {
                                        output.extend(batch.into_bytes().into_iter().flatten());
                                        if !output.is_empty()
                                            && delivery.lifecycle().may_retry_or_fallback()
                                        {
                                            delivery.commit().unwrap();
                                        }
                                    }
                                    Err(e) => error = Some(e),
                                }
                            }
                        }
                    }
                }
                assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
                if late {
                    assert!(String::from_utf8_lossy(&output).contains("partial"));
                    assert!(!delivery.lifecycle().may_retry_or_fallback());
                }
            } else {
                attempt.begin(200, "application/json").unwrap();
                attempt
                    .push(&serde_json::to_vec(&response).unwrap())
                    .unwrap();
            }
            attempt.finish().unwrap();
            let observed = attempt.response().unwrap().clone();
            if hints.get("background").is_some() {
                assert_eq!(
                    observed.metadata.context.execution.background,
                    Presence::Value(false)
                );
            }
            let result = if stream {
                delivery
                    .finish_stream(&attempt)
                    .map(|batch| batch.into_bytes().concat())
            } else {
                delivery.encode_json(&attempt)
            };
            if !allowed {
                assert!(
                    result.is_err(),
                    "rejection must prevent terminal finalization"
                );
                let error = error.unwrap_or_else(|| result.unwrap_err());
                assert!(
                    matches!(
                        error.codec(),
                        Some(CodecError::Unsupported(CarrierReason::Unavailable))
                    ),
                    "{stream} {late} {hints}: {error:?}"
                );
                assert!(delivery.complete(&attempt).is_err());
                assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
                assert!(
                    ClientAdapter::Messages
                        .stream_encoder(
                            observed.metadata.clone(),
                            Default::default(),
                            Default::default(),
                            Obfuscation::Disabled
                        )
                        .is_err()
                );
                assert!(
                    crate::protocol::anthropic::messages::client::encode_client_response(
                        &observed, "m"
                    )
                    .is_err()
                );
                continue;
            }
            assert!(error.is_none(), "{stream} {late} {hints}: {error:?}");
            output.extend(result.unwrap());
            delivery.commit().unwrap();
            delivery.complete(&attempt).unwrap();
            let (retained, wire) = delivery.snapshot(&attempt).unwrap();
            let wire: Value = serde_json::from_slice(&wire).unwrap();
            assert_eq!(wire["content"], json!([{"type":"text","text":"partial"}]));
            assert_eq!(wire["stop_reason"], "max_tokens");
            assert!(wire.get("metadata").is_none());
            assert!(retained.metadata.context.execution.metadata.is_absent());
            assert_eq!(attempt.response().unwrap(), &observed);
            if stream {
                let wire = String::from_utf8(output).unwrap();
                assert!(wire.contains("\"stop_reason\":\"max_tokens\""));
                assert_eq!(wire.matches("event: message_stop").count(), 1);
                assert!(!wire.contains("\"metadata\""));
            } else {
                assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), wire);
            }
            let omitted = hints.get("metadata") == Some(&Value::Null);
            for view in [
                if stream {
                    View::DownstreamSse
                } else {
                    View::DownstreamJson
                },
                View::RetainedJson,
            ] {
                assert_eq!(
                    delivery
                        .report
                        .attempt()
                        .evidence
                        .entries
                        .iter()
                        .filter(
                            |e| e.view == view && e.effect.field() == "response.context.metadata"
                        )
                        .count(),
                    0
                );
            }
            let mut encoder = ClientAdapter::Messages
                .stream_encoder(
                    observed.metadata.clone(),
                    Default::default(),
                    Default::default(),
                    Obfuscation::Disabled,
                )
                .unwrap();
            assert_eq!(
                encoder
                    .effects()
                    .iter()
                    .filter(|e| e.field() == "response.context.metadata")
                    .count(),
                0
            );
            let mut unsupported = observed.metadata.clone();
            unsupported.context.execution.metadata = Presence::Value(Default::default());
            assert!(encoder.update_metadata(unsupported).is_err());
            assert!(encoder.update_metadata(observed.metadata.clone()).is_err());
            assert!(encoder.finish().is_err());
            // Raw rendering requires a projected view so it cannot silently omit a fact.
            assert_eq!(
                crate::protocol::anthropic::messages::client::encode_client_response(
                    &observed, "m"
                )
                .is_err(),
                omitted
            );
        }
    }
}

#[test]
fn original_request_metadata_is_attempt_local_and_matches_responses_snapshots() {
    let chat = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let responses = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let original = responses
        .decode_client_request(
            br#"{"model":"public","input":"go","metadata":{"ticket":"original"}}"#,
        )
        .unwrap();
    let endpoint = crate::topology::catalog::API_KEY_BINDINGS
        .iter()
        .find(|b| {
            b.protocols
                .contains(&crate::topology::GenerationProtocol::OpenAiChat)
        })
        .unwrap()
        .endpoint(crate::topology::GenerationProtocol::OpenAiChat)
        .unwrap();
    // Candidate-local preparation is deliberately different. Independent
    // attempts must use the same admitted association, not the last candidate.
    for (stream, prepared_metadata) in [
        (
            false,
            Presence::Value(std::collections::BTreeMap::from([(
                "ticket".into(),
                "prepared".into(),
            )])),
        ),
        (true, Presence::Absent),
    ] {
        let mut request = original.clone();
        request.context.metadata = prepared_metadata;
        let mut attempt = Attempt::new(chat.clone(), 4096, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(responses.clone(), TargetContract::full()),
            "public",
            Default::default(),
            StreamOptions {
                include_obfuscation: Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        )
        .with_request(&endpoint, &request, &original);
        let chunk = json!({"id":"r","object":"chat.completion.chunk","model":"m","created":1,
            "choices":[{"index":0,"delta":{"content":"A."},"finish_reason":null}]});
        let (_, events) = attempt
            .push(format!("data: {chunk}\n\n").as_bytes())
            .unwrap();
        let mut output = vec![];
        if stream {
            let prepared = delivery.prepare_events(&attempt, &events).unwrap().unwrap();
            let bytes = prepared.context.retained_bytes().unwrap();
            let mut metadata = prepared.context.metadata.clone();
            let amount = metadata.delivery.as_ref().unwrap().request_metadata_bytes();
            let mut context = (*prepared.context).clone();
            Arc::make_mut(metadata.delivery.as_mut().unwrap()).request_metadata = Presence::Absent;
            context.metadata = metadata;
            context.accounted_bytes = Default::default();
            let absent_bytes = context.retained_bytes().unwrap();
            assert!(bytes > absent_bytes && bytes >= amount);
            output.extend(
                delivery
                    .encode_prepared(prepared)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
            let text = String::from_utf8_lossy(&output);
            let opening: Value =
                serde_json::from_str(text.lines().find_map(|l| l.strip_prefix("data: ")).unwrap())
                    .unwrap();
            assert_eq!(
                opening["response"]["metadata"],
                json!({"ticket":"original"})
            );
        }
        let finish = json!({"id":"r","object":"chat.completion.chunk","model":"m","created":1,
            "choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
        for wire in [format!("data: {finish}\n\n"), "data: [DONE]\n\n".into()] {
            let (_, events) = attempt.push(wire.as_bytes()).unwrap();
            if stream {
                output.extend(
                    delivery
                        .encode_events(&attempt, &events)
                        .unwrap()
                        .into_bytes()
                        .concat(),
                );
            }
        }
        attempt.finish().unwrap();
        let observation = attempt.response().unwrap().clone();
        let delivered = if stream {
            output.extend(
                delivery
                    .finish_stream(&attempt)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
            let text = String::from_utf8(output).unwrap();
            let terminal: Value = text
                .lines()
                .filter_map(|l| l.strip_prefix("data: "))
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .next_back()
                .unwrap();
            terminal["response"].clone()
        } else {
            serde_json::from_slice(&delivery.encode_json(&attempt).unwrap()).unwrap()
        };
        let (retained, body) = delivery.snapshot(&attempt).unwrap();
        let saved: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(saved["metadata"], json!({"ticket":"original"}));
        assert_eq!(delivered["metadata"], saved["metadata"]);
        assert!(retained.metadata.context.execution.metadata.is_absent());
        assert_eq!(
            retained
                .metadata
                .delivery
                .as_ref()
                .unwrap()
                .request_metadata,
            original.context.metadata
        );
        assert_eq!(attempt.response().unwrap(), &observation);
        assert!(observation.metadata.delivery.is_none());
        assert!(observation.metadata.context.execution.metadata.is_absent());
        assert!(delivery.effects().iter().any(|e| e.rule()
            == "responses.echo-request-metadata.v1"
            && e.change() == crate::protocol::conversion::generation::Change::InferFromRequest));
        let (_, again) = ClientAdapter::openai(responses.clone(), TargetContract::full())
            .snapshot(retained.clone())
            .unwrap();
        assert_eq!(again["metadata"], saved["metadata"]);
        let mut no_echo = retained.clone();
        Arc::make_mut(no_echo.metadata.delivery.as_mut().unwrap()).request_metadata =
            Presence::Absent;
        let stored = crate::protocol::openai::responses::managed::retained_size(
            &original.task,
            &retained,
            body.len(),
        )
        .unwrap();
        let without = crate::protocol::openai::responses::managed::retained_size(
            &original.task,
            &no_echo,
            body.len(),
        )
        .unwrap();
        assert!(stored > without);
        let (next, _) = crate::protocol::openai::responses::managed::decode(
            &responses,
            br#"{"model":"public","input":"next"}"#,
            Some((&original.task, &retained)),
        )
        .unwrap();
        assert!(
            next.context.metadata.is_absent(),
            "history does not inherit request association"
        );
    }
}

#[test]
fn prepared_batches_keep_the_metadata_snapshot_from_their_intake() {
    // A later report must not become visible in an earlier pending batch.
    let adapter = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 4096, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::openai(adapter, TargetContract::full()),
        "public",
        Default::default(),
        StreamOptions {
            include_obfuscation: Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    let mut batches = vec![];
    for (fragment, fingerprint) in [("first", None), ("second", Some("later"))] {
        let mut chunk = json!({
            "id":"reported", "object":"chat.completion.chunk", "created":1, "model":"model",
            "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
        });
        if let Some(fingerprint) = fingerprint {
            chunk["system_fingerprint"] = fingerprint.into();
        }
        let (_, events) = attempt
            .push(format!("data: {chunk}\n\n").as_bytes())
            .unwrap();
        batches.push(delivery.prepare_events(&attempt, &events).unwrap().unwrap());
    }
    for (index, batch) in batches.into_iter().enumerate() {
        let frames = delivery
            .encode_prepared(batch)
            .map(|batch| batch.into_bytes())
            .unwrap();
        let wire = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
        let values: Vec<Value> = wire
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(!values.is_empty());
        if index == 0 {
            assert!(values.iter().all(|v| v.get("system_fingerprint").is_none()));
            assert!(
                values
                    .iter()
                    .any(|v| v["choices"][0]["delta"]["content"] == "first")
            );
        } else {
            assert!(values.iter().all(|v| v["system_fingerprint"] == "later"));
            assert!(
                values
                    .iter()
                    .any(|v| v["choices"][0]["delta"]["content"] == "second")
            );
        }
    }
    delivery.cancel();
}

#[test]
fn lifecycle_reports_freeze_opening_batches_and_charge_both_observations() {
    use crate::semantic::context::{ReportPhase, ServiceClass};
    let mut adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::OpenRouter, None);
    adapter.adaptation.rules.responses_lifecycle_reports = true;
    let mut attempt = Attempt::new(adapter, 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None),
            TargetContract::full(),
        ),
        "public",
        Default::default(),
        StreamOptions {
            include_obfuscation: Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    let opening = json!({"type":"response.created","sequence_number":0,"response":{"id":"reported","object":"response",
        "created_at":1,"model":"model","status":"in_progress","output":[],
        "reasoning":{"summary":"auto"},"service_tier":"auto"}});
    let (_, events) = attempt
        .push(format!("data: {opening}\n\n").as_bytes())
        .unwrap();
    let prepared = delivery.prepare_events(&attempt, &events).unwrap().unwrap();
    let opening_bytes = prepared.context.retained_bytes().unwrap();
    let terminal = json!({"type":"response.completed","sequence_number":1,"response":{"id":"reported","object":"response",
        "created_at":1,"model":"model","status":"completed","output":[],
        "reasoning":{"summary":"detailed","context":"all_turns","mode":"standard"},"service_tier":"default"}});
    let (_, events) = attempt
        .push(format!("data: {terminal}\n\n").as_bytes())
        .unwrap();
    assert!(events.is_empty(), "terminal remains EOF-gated");
    attempt.finish().unwrap();
    assert_eq!(
        prepared
            .context
            .metadata
            .context
            .reports
            .at(ReportPhase::Terminal),
        None
    );
    assert_eq!(
        prepared.context.metadata.context.service_class(),
        &Presence::Value(ServiceClass::ConfiguredProcessing)
    );
    let wire: Vec<_> = delivery
        .encode_prepared(prepared)
        .map(|batch| batch.into_bytes())
        .unwrap()
        .into_iter()
        .flatten()
        .collect();
    let opening: Value = serde_json::from_str(
        std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(opening["response"]["service_tier"], "auto");
    let frames = delivery
        .finish_stream(&attempt)
        .map(|batch| batch.into_bytes())
        .unwrap();
    let wire: Vec<_> = frames.into_iter().flatten().collect();
    let terminal: Value = serde_json::from_str(
        std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["response"]["service_tier"], "default");
    let retained = delivery
        .prepared_source
        .as_ref()
        .unwrap()
        .context
        .retained_bytes()
        .unwrap();
    assert!(
        retained > opening_bytes,
        "both retained reports count against coalescing"
    );
}

#[test]
fn classified_failure_after_opening_has_no_invented_terminal_reports() {
    // A classified failure after a native Responses opening has no upstream
    // terminal report. A standard error must not invent a failed Response.
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut failure_delivery = ResponseDelivery::new(
        ClientAdapter::openai(adapter, TargetContract::full()),
        "public",
        Default::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    let opening = json!({"type":"response.created","sequence_number":0,"response":{
        "id":"failed-report","object":"response","created_at":1,"model":"m",
        "status":"in_progress","output":[],"reasoning":{"summary":"auto"},"service_tier":"auto"}});
    let (_, events) = attempt
        .push(format!("data: {opening}\n\n").as_bytes())
        .unwrap();
    failure_delivery
        .encode_events(&attempt, &events)
        .map(|batch| batch.into_bytes())
        .unwrap();
    let frames = failure_delivery.fail_stream(&json!({"error":{
        "code":"context_length_exceeded","type":"invalid_request_error","message":"Context limit"}})).map(|batch|batch.into_bytes()).unwrap();
    let bytes: Vec<_> = frames.into_iter().flatten().collect();
    let value: Value = serde_json::from_str(
        std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["sequence_number"], 2);
    assert_eq!(value["code"], "context_length_exceeded");
    assert_eq!(value["message"], "Context limit");
    assert!(value.get("response").is_none());
    assert!(
        failure_delivery
            .fail_stream(&json!({"error":{}}))
            .map(|batch| batch.into_bytes())
            .is_err()
    );
}

#[test]
fn classified_response_errors_obey_remaining_budgets_and_poison_failed_delivery() {
    let error = json!({"error":{
        "code":"server_is_overloaded","message":"x".repeat(4096),"param":null
    }});
    let make = |limits, started| {
        let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
        let mut attempt = Attempt::new(adapter.clone(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(adapter, TargetContract::full()),
            "public",
            limits,
            StreamOptions::default(),
            Obfuscation::Disabled,
        );
        let prefix = if started {
            let (_, events) = attempt.push(br#"data: {"type":"response.created","sequence_number":0,"response":{"id":"r","object":"response","created_at":1,"model":"m","status":"in_progress","output":[]}}

"#).unwrap();
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .unwrap()
        } else {
            vec![]
        };
        (
            attempt,
            delivery,
            prefix.iter().map(Bytes::len).sum::<usize>(),
        )
    };
    for started in [false, true] {
        let (_, mut reference, prefix_bytes) = make(SseLimits::default(), started);
        let error_bytes: usize = reference
            .fail_stream(&error)
            .map(|batch| batch.into_bytes())
            .unwrap()
            .iter()
            .map(Bytes::len)
            .sum();
        let event_count = if started { 3 } else { 1 };
        for limits in [
            SseLimits {
                max_events: event_count,
                ..Default::default()
            },
            SseLimits {
                max_wire_bytes: prefix_bytes + error_bytes,
                ..Default::default()
            },
            SseLimits {
                max_event_bytes: error_bytes,
                ..Default::default()
            },
        ] {
            let (_, mut delivery, _) = make(limits, started);
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_ok(),
                "exact fit"
            );
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_err(),
                "closed"
            );
        }
        let mut exceeded = vec![
            SseLimits {
                max_wire_bytes: prefix_bytes + error_bytes - 1,
                ..Default::default()
            },
            SseLimits {
                max_event_bytes: error_bytes - 1,
                ..Default::default()
            },
        ];
        if started {
            exceeded.push(SseLimits {
                max_events: event_count - 1,
                ..Default::default()
            });
        }
        for limits in exceeded {
            let (attempt, mut delivery, _) = make(limits, started);
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_err(),
                "budget must reject"
            );
            assert!(
                delivery
                    .fail_stream(&json!({"error":{"message":"small"}}))
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(
                delivery
                    .encode_events(&attempt, &[StreamEvent::Started])
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(
                delivery
                    .finish_stream(&attempt)
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(delivery.commit().is_err());
        }
    }
}

#[test]
fn coalescing_preserves_first_content_barriers_and_attempt_isolation() {
    let policy = DeltaCoalescing {
        max_delay_ms: 10,
        max_pending_bytes: 16384,
    };
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        let source = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
        let client = match protocol {
            ClientProtocol::Messages => ClientAdapter::Messages,
            _ => ClientAdapter::openai(
                GenerationCodec::new(
                    if protocol == ClientProtocol::Chat {
                        GenerationApi::Chat
                    } else {
                        GenerationApi::Responses
                    },
                    Dialect::Standard,
                    None,
                ),
                TargetContract::full(),
            ),
        };
        let mut attempt = Attempt::new(source, 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            client,
            "public",
            Default::default(),
            StreamOptions {
                include_obfuscation: Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        )
        .with_coalescing(Some(policy));
        let now = Instant::now();
        let mut output = vec![];
        for (index, text) in ["first", "\"", "\\", "\n雪"].into_iter().enumerate() {
            let chunk = json!({
                "id":"reported","object":"chat.completion.chunk","created":1,"model":"model",
                "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]
            });
            let (_, events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            let frames = coalesce_events(
                &mut delivery,
                &attempt,
                &events,
                now + Duration::from_millis(index as u64),
            )
            .unwrap();
            if index == 0 {
                assert!(!frames.is_empty(), "first content is never held");
            } else {
                assert!(frames.is_empty());
                assert_eq!(
                    delivery.flush_deadline(),
                    Some(now + Duration::from_millis(11))
                );
            }
            output.extend(frames);
        }
        assert!(delivery.coalescing_peak_bytes() <= policy.max_pending_bytes);
        output.extend(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        assert!(delivery.flush_deadline().is_none());
        let wire = String::from_utf8(output.into_iter().flatten().collect()).unwrap();
        let fragments: Vec<String> = wire
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| {
                value
                    .pointer("/choices/0/delta/content")
                    .or_else(|| {
                        (value["type"] == "response.output_text.delta").then(|| &value["delta"])
                    })
                    .or_else(|| {
                        (value["delta"]["type"] == "text_delta").then(|| &value["delta"]["text"])
                    })
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
            .filter(|text| !text.is_empty())
            .collect();
        assert_eq!(fragments, ["first", "\"\\\n雪"]);
        let now = now + Duration::from_millis(20);
        let mut last_events = vec![];
        for (text, fingerprint) in [
            ("pending", None),
            ("", None),
            ("before", None),
            ("after", Some("later")),
        ] {
            let mut chunk = json!({
                "id":"reported","object":"chat.completion.chunk","created":1,"model":"model",
                "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]
            });
            if let Some(fingerprint) = fingerprint {
                chunk["system_fingerprint"] = fingerprint.into();
            }
            let (_, mut events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            if text.is_empty() {
                // Empty semantic deltas are barriers even when a source codec
                // normally normalizes away an empty native content field.
                let StreamEvent::Delta { item, part, .. } = &last_events[0] else {
                    panic!()
                };
                events = vec![StreamEvent::Delta {
                    item: *item,
                    part: *part,
                    fragment: String::new(),
                    logprobs: vec![],
                }];
            } else {
                last_events = events.clone();
            }
            let frames = coalesce_events(&mut delivery, &attempt, &events, now).unwrap();
            if text.is_empty() || fingerprint.is_some() {
                assert!(!frames.is_empty(), "barriers drain the preceding run");
                if protocol == ClientProtocol::Chat && fingerprint.is_some() {
                    assert!(
                        !String::from_utf8(frames.into_iter().flatten().collect())
                            .unwrap()
                            .contains("later")
                    );
                }
            } else {
                assert!(frames.is_empty());
            }
        }
        assert!(
            delivery.flush_deadline().is_some(),
            "new context starts a distinct run"
        );
        if protocol == ClientProtocol::Responses {
            let mut other = Attempt::new(
                GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
                4096,
                Default::default(),
            );
            other.begin(200, "text/event-stream").unwrap();
            let (_, events) = other.push(
                br#"data: {"id":"reported","object":"chat.completion.chunk","created":1,"model":"model","choices":[{"index":0,"delta":{"content":"other"},"finish_reason":null}]}

"#,
            ).unwrap();
            assert!(coalesce_events(&mut delivery, &other, &events, now).is_err());
            assert!(
                delivery
                    .flush_pending()
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
        }
        delivery.cancel();
        assert!(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .is_err()
        );
    }
}

#[test]
fn paced_text_windows_preserve_content_and_bound_intentional_delay() {
    for cadence_ms in [5, 20] {
        for window_ms in [10, 50, 100] {
            let codec = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None);
            let mut attempt = Attempt::new(codec.clone(), 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = ResponseDelivery::new(
                ClientAdapter::openai(codec, TargetContract::full()),
                "public",
                Default::default(),
                StreamOptions {
                    include_obfuscation: Presence::Value(false),
                    ..Default::default()
                },
                Obfuscation::Disabled,
            )
            .with_coalescing(Some(DeltaCoalescing {
                max_delay_ms: window_ms,
                max_pending_bytes: 16384,
            }));
            let start = Instant::now();
            let mut output = vec![];
            let mut expected = String::new();
            for index in 0..101 {
                let now = start + Duration::from_millis(index * cadence_ms);
                // Drive the fixed timer independently of the next source event.
                if let Some(deadline) = delivery.flush_deadline().filter(|t| *t <= now) {
                    output.extend(
                        delivery
                            .flush_pending()
                            .unwrap()
                            .into_bytes()
                            .into_iter()
                            .map(|bytes| (deadline, bytes)),
                    );
                }
                let fragment = format!("{index:03}|");
                expected.push_str(&fragment);
                let chunk = json!({
                    "id":"paced","object":"chat.completion.chunk","created":1,"model":"model",
                    "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
                });
                let (_, events) = attempt
                    .push(format!("data: {chunk}\n\n").as_bytes())
                    .unwrap();
                output.extend(
                    coalesce_events(&mut delivery, &attempt, &events, now)
                        .unwrap()
                        .into_iter()
                        .map(|bytes| (now, bytes)),
                );
            }
            if let Some(deadline) = delivery.flush_deadline() {
                output.extend(
                    delivery
                        .flush_pending()
                        .unwrap()
                        .into_bytes()
                        .into_iter()
                        .map(|bytes| (deadline, bytes)),
                );
            }
            let bytes: usize = output.iter().map(|(_, bytes)| bytes.len()).sum();
            let mut actual = String::new();
            let mut count = 0;
            let mut delay_sum = 0;
            let mut max_delay = 0;
            for (at, bytes) in output {
                for line in std::str::from_utf8(&bytes).unwrap().lines() {
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    let value: Value = serde_json::from_str(data).unwrap();
                    let Some(fragment) = value
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                    else {
                        continue;
                    };
                    count += 1;
                    for offset in 0..fragment.len() / 4 {
                        let emitted = (actual.len() / 4 + offset) as u64;
                        let source = start + Duration::from_millis(emitted * cadence_ms);
                        let delay = at.duration_since(source).as_millis();
                        assert!(delay <= u128::from(window_ms));
                        if emitted == 0 {
                            assert_eq!(delay, 0, "first content stays immediate");
                        }
                        delay_sum += delay;
                        max_delay = max_delay.max(delay);
                    }
                    actual.push_str(fragment);
                }
            }
            assert_eq!(actual, expected);
            assert_eq!(count, 1 + 100_u64.div_ceil(window_ms.div_ceil(cadence_ms)));
            println!(
                "cadence_ms={cadence_ms} window_ms={window_ms} text_events={count} encoded_bytes={bytes} mean_delay_ms={:.2} max_delay_ms={max_delay}",
                delay_sum as f64 / 101.0
            );
        }
    }
}

#[test]
fn coalescing_respects_escaped_frame_and_pending_memory_limits() {
    let policy = DeltaCoalescing {
        max_delay_ms: 10,
        max_pending_bytes: 16384,
    };
    // Escape expansion, maximum envelope labels and padding must not turn
    // independently valid chunks into a newly oversized merged frame.
    for max_pending_bytes in [16384, 1024] {
        let policy = DeltaCoalescing {
            max_pending_bytes,
            ..policy
        };
        let mut attempt = Attempt::new(
            GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
            65536,
            Default::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(
                GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
                TargetContract::full(),
            ),
            &"\\".repeat(256),
            SseLimits {
                max_event_bytes: 8192,
                ..Default::default()
            },
            Default::default(),
            Obfuscation::Seeded([7; 32]),
        )
        .with_coalescing(Some(policy));
        let mut output = vec![];
        let now = Instant::now();
        for fragment in
            std::iter::once("first".to_owned()).chain(std::iter::repeat_n("\"".repeat(400), 10))
        {
            let chunk = json!({
                "id":"r".repeat(256),"object":"chat.completion.chunk","created":1,"model":"model",
                "system_fingerprint":"\"".repeat(256),
                "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
            });
            let (_, events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            output.extend(coalesce_events(&mut delivery, &attempt, &events, now).unwrap());
        }
        output.extend(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        assert!(output.iter().all(|frame| frame.len() <= 8192));
        assert!(delivery.coalescing_peak_bytes() <= policy.max_pending_bytes);
        if max_pending_bytes == 1024 {
            assert_eq!(
                delivery.coalescing_peak_bytes(),
                0,
                "oversized contexts bypass buffering"
            );
        }
        let text: String = output
            .iter()
            .flat_map(|frame| std::str::from_utf8(frame).unwrap().lines())
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter_map(|v| {
                v["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(text, format!("first{}", "\"".repeat(4000)));
        delivery.cancel();
    }
}

#[test]
fn assessment_only_safety_reports_are_omitted_and_blocking_reports_reject() {
    use crate::{
        adapter::UpstreamAdapter,
        protocol::{ClientProtocol, native::ReplayTarget},
    };
    let wire = |blocked: bool| {
        let body = json!({"responseId":"r","modelVersion":"fixture-model","candidates":[{
            "content":{"role":"model","parts":[{"text":"OK"}]},"finishReason":"MAX_TOKENS",
            "safetyRatings":[{"category":"HARM_CATEGORY_HARASSMENT","probability":"NEGLIGIBLE",
                "blocked":blocked}]}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1,
            "totalTokenCount":4}});
        format!("data: {body}\n\n")
    };
    let run = |protocol: ClientProtocol, blocked: bool, stream: bool| {
        let mut attempt = Attempt::new(
            UpstreamAdapter::GoogleGenerateContent {
                target: ReplayTarget::new("fixture-google", "fixture-model").unwrap(),
            },
            1 << 20,
            Default::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let client = match protocol {
            ClientProtocol::Messages => ClientAdapter::Messages,
            _ => ClientAdapter::openai(
                GenerationCodec::new(
                    if protocol == ClientProtocol::Chat {
                        GenerationApi::Chat
                    } else {
                        GenerationApi::Responses
                    },
                    Dialect::Standard,
                    None,
                ),
                TargetContract::full(),
            ),
        };
        let mut delivery = ResponseDelivery::new(
            client,
            "public",
            Default::default(),
            StreamOptions {
                include_obfuscation: Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        );
        let (_, events) = attempt.push(wire(blocked).as_bytes()).unwrap();
        let body = if stream {
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .and_then(|frames| {
                    delivery.commit()?;
                    attempt.finish()?;
                    Ok(frames
                        .into_iter()
                        .chain(
                            delivery
                                .finish_stream(&attempt)
                                .map(|batch| batch.into_bytes())?,
                        )
                        .collect::<Vec<_>>()
                        .concat())
                })
        } else {
            attempt.finish().unwrap();
            delivery.encode_json(&attempt)
        };
        (body, delivery.effects().to_vec())
    };
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        for stream in [false, true] {
            let (body, losses) = run(protocol, false, stream);
            let body = String::from_utf8(body.unwrap()).unwrap();
            assert!(
                body.contains("OK") && !body.contains("HARASSMENT"),
                "{protocol:?}"
            );
            assert!(
                losses
                    .iter()
                    .any(|loss| loss.rule() == "generation.omit-safety-assessment.v1"
                        && loss.field() == "response.safety_reports"),
                "{protocol:?} {stream}"
            );
            // Only the block flag differs from the delivered case above.
            let error = format!("{:?}", run(protocol, true, stream).0.unwrap_err());
            assert!(
                error.contains("Safety")
                    || error.contains("safety report")
                    || error.contains("event target representation"),
                "{protocol:?} {stream}: {error}"
            );
        }
    }
}
