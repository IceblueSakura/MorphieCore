//! Independent native wires and guarded Messages terminal projection at real EOF.
use super::*;
use crate::{
    protocol::{
        failure::CarrierReason,
        openai::{Dialect, GenerationApi, GenerationCodec, TargetContract},
    },
    semantic::{task::generation::*, value::Presence},
    topology::{Endpoint, catalog::google_bindings},
};
use serde_json::{Value, json};

fn fixture(
    calls: bool,
) -> (
    Endpoint,
    crate::protocol::GenerationRequestEnvelope,
    Vec<Value>,
) {
    let binding = &google_bindings()[0];
    let endpoint = binding.endpoint();
    let request = ClientAdapter::Messages.decode_client_request(json!({
        "model":binding.model,"max_tokens":256,"messages":[{"role":"user","content":"Read."}]
    }).to_string().as_bytes()).unwrap();
    let mut frames =
        vec![json!({"event_type":"interaction.created","interaction":{"model":binding.model}})];
    if calls {
        for index in 0..2 {
            let id = format!("c{index}");
            frames.extend([
                json!({"event_type":"step.start","index":index,"step":{"type":"function_call",
                    "id":id,"name":"read","arguments":{}}}),
                json!({"event_type":"step.delta","index":index,"delta":{"type":"arguments_delta",
                    "arguments":format!("{{\"path\":\"{index}\"}}")}}),
                json!({"event_type":"step.stop","index":index}),
            ]);
        }
    } else {
        frames.extend([
            json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"text","text":"answer"}}),
            json!({"event_type":"step.stop","index":0}),
        ]);
    }
    frames.push(json!({"event_type":"interaction.completed","interaction":{
        "status":if calls {"requires_action"} else {"completed"},
        "usage":{"total_input_tokens":10,"total_output_tokens":3,"total_cached_tokens":0,
            "total_thought_tokens":2,"total_tool_use_tokens":0,"total_tokens":15}}}));
    (endpoint, request, frames)
}
fn delivery(
    endpoint: &Endpoint,
    request: &crate::protocol::GenerationRequestEnvelope,
    stream: bool,
) -> ResponseDelivery {
    ResponseDelivery::new(
        ClientAdapter::Messages,
        "public",
        Default::default(),
        Default::default(),
        Obfuscation::Disabled,
        stream,
    )
    .with_request(endpoint, request, request)
}
fn feed(
    attempt: &mut Attempt,
    delivery: &mut ResponseDelivery,
    frames: &[Value],
    stream: bool,
) -> Vec<u8> {
    let mut wire = frames
        .iter()
        .map(|f| {
            format!(
                "event: {}\ndata: {f}\n\n",
                f["event_type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    wire.push_str("event: done\ndata: [DONE]\n\n");
    let mut output = vec![];
    for chunk in wire.as_bytes().chunks(13) {
        let mut rest = chunk;
        while !rest.is_empty() {
            let (used, events) = attempt.push(rest).unwrap();
            assert!(used > 0);
            assert!(
                events
                    .iter()
                    .all(|e| !matches!(e, StreamEvent::Terminal { .. }))
            );
            rest = &rest[used..];
            if stream {
                output.extend(
                    delivery
                        .encode_events(attempt, &events)
                        .map(|batch| batch.into_bytes())
                        .unwrap()
                        .concat(),
                );
            }
        }
    }
    output
}

#[test]
fn stateless_clients_reject_hidden_signatures_without_fabricated_reasoning() {
    use crate::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
    for client in [
        ClientAdapter::Messages,
        ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None),
            TargetContract::full(),
        ),
        ClientAdapter::openai(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
            TargetContract::full(),
        ),
    ] {
        for stream in [false, true] {
            let (endpoint, request, mut frames) = fixture(true);
            frames[1]["step"]["signature"] = json!("synthetic-call");
            let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = ResponseDelivery::new(
                client.clone(),
                "public",
                Default::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: Some(false),
                    ..Default::default()
                },
                Obfuscation::Disabled,
                stream,
            )
            .with_request(&endpoint, &request, &request);
            let mut all_events = vec![];
            for frame in &frames {
                let wire = format!(
                    "event: {}\ndata: {frame}\n\n",
                    frame["event_type"].as_str().unwrap()
                );
                let (n, events) = attempt.push(wire.as_bytes()).unwrap();
                assert_eq!(n, wire.len());
                all_events.extend(events);
            }
            attempt.finish().unwrap();
            let original = attempt.response().unwrap().clone();
            let error = if stream {
                delivery
                    .encode_events(&attempt, &all_events)
                    .map(|batch| batch.into_bytes())
                    .unwrap_err()
            } else {
                delivery.encode_json(&attempt).unwrap_err()
            };
            assert!(
                matches!(
                    error.codec(),
                    Some(crate::protocol::CodecError::Unsupported(
                        CarrierReason::Replay
                    ))
                ),
                "{error:?}"
            );
            assert_eq!(attempt.response().unwrap(), &original);
            assert!(delivery.encode_json(&attempt).is_err());
            assert!(delivery.commit().is_err());
        }
    }
    for content in [
        json!({"type":"thinking","thinking":"","signature":"mca1.synthetic"}),
        json!({"type":"redacted_thinking","data":"mca1.synthetic"}),
    ] {
        let mut request = ClientAdapter::Messages
            .decode_client_request(
                json!({"model":"public","max_tokens":32,"messages":[
                {"role":"user","content":"visible"},{"role":"assistant","content":[content]}]})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        super::super::replay::open_from_client(&mut request).unwrap();
        let (endpoint, _, _) = fixture(false);
        let encoded = endpoint.encode_request(&request).unwrap();
        assert!(encoded.to_string().contains("visible"));
        assert!(!encoded.to_string().contains("mca1."));
    }
}

#[test]
fn google_messages_terminals_keep_source_facts_and_ordered_calls() {
    for calls in [false, true] {
        for stream in [false, true] {
            let (endpoint, request, frames) = fixture(calls);
            let mut delivery = delivery(&endpoint, &request, stream);
            let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut wire = feed(&mut attempt, &mut delivery, &frames, stream);
            assert!(!String::from_utf8_lossy(&wire).contains("message_stop"));
            attempt.finish().unwrap();
            let original = attempt.response().unwrap().clone();
            let original_terminal = attempt.terminal_event().unwrap().clone();
            assert_eq!(original.semantic.details().completion, None);
            let (stop, ids) = if stream {
                wire.extend(
                    delivery
                        .finish_stream(&attempt)
                        .map(|batch| batch.into_bytes())
                        .unwrap()
                        .concat(),
                );
                let wire = String::from_utf8(wire).unwrap();
                let events: Vec<Value> = wire
                    .lines()
                    .filter_map(|l| l.strip_prefix("data: "))
                    .map(|s| serde_json::from_str(s).unwrap())
                    .collect();
                assert_eq!(
                    events
                        .iter()
                        .filter(|v| v["type"] == "message_stop")
                        .count(),
                    1
                );
                let finals: Vec<_> = events
                    .iter()
                    .filter(|v| v["type"] == "message_delta")
                    .collect();
                assert_eq!(finals.len(), 1);
                assert_eq!(finals[0]["usage"]["input_tokens"], 10);
                assert_eq!(finals[0]["usage"]["output_tokens"], 5);
                (
                    finals[0]["delta"]["stop_reason"].clone(),
                    events
                        .iter()
                        .filter(|v| {
                            v["type"] == "content_block_start"
                                && v["content_block"]["type"] == "tool_use"
                        })
                        .map(|v| v["content_block"]["id"].clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                let (retained, bytes) = delivery
                    .encode_json_snapshot(&attempt)
                    .map(|(response, batch)| (response, batch.into_bytes().concat()))
                    .unwrap();
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    retained.semantic.details().completion,
                    if calls {
                        None
                    } else {
                        Some(CompletionCause::Natural)
                    }
                );
                (
                    value["stop_reason"].clone(),
                    value["content"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|p| p["type"] == "tool_use")
                        .map(|p| p["id"].clone())
                        .collect::<Vec<_>>(),
                )
            };
            assert_eq!(stop, if calls { "tool_use" } else { "end_turn" });
            assert_eq!(
                ids,
                if calls {
                    vec![json!("c0"), json!("c1")]
                } else {
                    vec![]
                }
            );
            let losses: Vec<_> = delivery
                .effects()
                .into_iter()
                .filter(|l| l.field() == "generation.completion_cause")
                .collect();
            assert!(losses.is_empty());
            assert_eq!(attempt.response().unwrap(), &original);
            assert_eq!(attempt.terminal_event().unwrap(), &original_terminal);
        }
    }
}

#[test]
fn google_service_reports_keep_processing_meaning_and_omit_uncarried_capacity_reports() {
    for profile in [GenerationApi::Chat, GenerationApi::Responses] {
        for stream in [false, true] {
            let (endpoint, request, mut frames) = fixture(false);
            frames[0]["interaction"]["service_tier"] = json!("priority");
            frames.last_mut().unwrap()["interaction"]["service_tier"] = json!("priority");
            let mut source = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            source.begin(200, "text/event-stream").unwrap();
            let mut client = ResponseDelivery::new(
                ClientAdapter::openai(
                    GenerationCodec::new(profile, Dialect::MorphieCore, None),
                    TargetContract::full(),
                ),
                "public",
                Default::default(),
                StreamOptions {
                    include_usage: Some(true),
                    include_obfuscation: Some(false),
                },
                Obfuscation::Disabled,
                stream,
            )
            .with_request(&endpoint, &request, &request);
            let mut body = feed(&mut source, &mut client, &frames, stream);
            source.finish().unwrap();
            let original = source.response().unwrap().clone();
            if stream {
                body.extend(
                    client
                        .finish_stream(&source)
                        .map(|batch| batch.into_bytes())
                        .unwrap()
                        .concat(),
                );
                assert!(String::from_utf8_lossy(&body).contains("\"service_tier\":\"priority\""));
            } else {
                body = client.encode_json(&source).unwrap();
                let value: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(value["service_tier"], "priority");
            }
            assert_eq!(
                original.metadata.context.service_class(),
                &Some(crate::semantic::context::ServiceClass::PriorityProcessing)
            );
            let mut incompatible = delivery(&endpoint, &request, false);
            let wire: Value =
                serde_json::from_slice(&incompatible.encode_json(&source).unwrap()).unwrap();
            assert!(wire["usage"].get("service_tier").is_none());
            assert!(
                incompatible
                    .effects()
                    .iter()
                    .any(|effect| effect.rule() == "messages.omit-reported-service-class.v1")
            );
            let mut incompatible = delivery(&endpoint, &request, true);
            let mut message_source =
                Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            message_source.begin(200, "text/event-stream").unwrap();
            let mut body = feed(&mut message_source, &mut incompatible, &frames, true);
            message_source.finish().unwrap();
            body.extend(
                incompatible
                    .finish_stream(&message_source)
                    .unwrap()
                    .into_bytes()
                    .concat(),
            );
            let wire = String::from_utf8(body).unwrap();
            assert_eq!(wire.matches("event: message_stop\n").count(), 1);
            assert!(!wire.contains("\"service_tier\":\"priority\""));
            assert!(
                incompatible
                    .effects()
                    .iter()
                    .any(|effect| effect.rule() == "messages.omit-reported-service-class.v1")
            );
            assert!(
                incompatible
                    .finish_stream(&source)
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert_eq!(source.response().unwrap(), &original);
        }
    }
}

#[test]
fn google_messages_projection_requires_context_and_never_repairs_failure() {
    let (endpoint, request, frames) = fixture(false);
    let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    feed(
        &mut attempt,
        &mut delivery(&endpoint, &request, false),
        &frames,
        false,
    );
    attempt.finish().unwrap();
    for guard in ["unbound", "stops"] {
        let mut request = request.clone();
        if guard == "stops" {
            let mut settings = request.task.semantic.settings().clone();
            settings
                .controls
                .stop_sequences
                .push(crate::semantic::value::Text::new("END", "stop", 16).unwrap());
            request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
        }
        let mut d = if guard == "unbound" {
            ResponseDelivery::new(
                ClientAdapter::Messages,
                "public",
                Default::default(),
                Default::default(),
                Obfuscation::Disabled,
                false,
            )
        } else {
            delivery(&endpoint, &request, false)
        };
        if guard == "stops" {
            // Generic call-free derivation uses admitted request context.
            let wire: serde_json::Value =
                serde_json::from_slice(&d.encode_json(&attempt).unwrap()).unwrap();
            assert_eq!(wire["stop_reason"], "end_turn");
            assert!(
                d.effects()
                    .iter()
                    .any(|effect| effect.rule() == "messages.ambiguous-stop-as-end-turn.v1")
            );
            continue;
        }
        let error = d.encode_json(&attempt).unwrap_err();
        assert!(
            matches!(
                error.codec(),
                Some(crate::protocol::CodecError::Unsupported(
                    CarrierReason::Terminal
                ))
            ),
            "{error:?}"
        );
    }
    let (endpoint, request, frames) = fixture(false);
    for premature in [true, false] {
        let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut d = delivery(&endpoint, &request, true);
        feed(&mut attempt, &mut d, &frames, true);
        if !premature {
            attempt.finish().unwrap();
            d.cancel();
        }
        assert!(
            d.finish_stream(&attempt)
                .map(|batch| batch.into_bytes())
                .is_err()
        );
    }
    let (endpoint, request, mut frames) = fixture(false);
    frames.last_mut().unwrap()["interaction"]["usage"]
        .as_object_mut()
        .unwrap()
        .remove("total_cached_tokens");
    let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut d = delivery(&endpoint, &request, false);
    feed(&mut attempt, &mut d, &frames, false);
    attempt.finish().unwrap();
    let source = attempt.response().unwrap().clone();
    let value: Value = serde_json::from_slice(&d.encode_json(&attempt).unwrap()).unwrap();
    assert_eq!(value["usage"]["input_tokens"], 10);
    assert_eq!(value["usage"]["output_tokens"], 5);
    assert!(value["usage"].get("cache_read_input_tokens").is_none());
    assert_eq!(attempt.response().unwrap(), &source);
    assert!(d.effects().iter().any(|effect| effect.semantic_field()
        == crate::protocol::conversion::generation::Field::InputTokens
        && effect.change() == crate::protocol::conversion::generation::Change::Estimate));
}

fn stateless_google_responses_setup(stream: bool) -> (Attempt, ResponseDelivery) {
    let target =
        crate::protocol::native::ReplayTarget::new("synthetic-google", "synthetic-model").unwrap();
    let mut attempt = Attempt::new(
        crate::adapter::UpstreamAdapter::GoogleInteractions { target },
        2 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let codec = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(crate::protocol::replay_envelope::CLIENT_ORIGIN.clone()),
    );
    let delivery = ResponseDelivery::new(
        ClientAdapter::openai(codec.clone(), TargetContract::full()),
        "public",
        SseLimits::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
        stream,
    );
    let request = codec
        .decode_client_request(br#"{"model":"public","input":"go","max_output_tokens":32}"#)
        .unwrap();
    let delivery = delivery.with_request(&google_bindings()[0].endpoint(), &request, &request);
    (attempt, delivery)
}

#[test]
fn missing_and_late_google_ids_keep_delivery_identity_in_json_sse_and_retained_views() {
    for late in [false, true] {
        for stream in [false, true] {
            let (mut attempt, mut delivery) = stateless_google_responses_setup(stream);
            let mut frames = vec![
                json!({"event_type":"interaction.created","interaction":{"model":"synthetic-model"}}),
                json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
                json!({"event_type":"step.delta","index":0,"delta":{"type":"text","text":"OK"}}),
                json!({"event_type":"step.stop","index":0}),
                json!({"event_type":"interaction.completed","interaction":{"status":"completed"}}),
            ];
            if late {
                frames.last_mut().unwrap()["interaction"]["id"] = json!("late-upstream");
            }
            let mut output = vec![];
            let mut id = None;
            for frame in frames {
                let wire = format!(
                    "event: {}\ndata: {frame}\n\n",
                    frame["event_type"].as_str().unwrap()
                );
                let (_, events) = attempt.push(wire.as_bytes()).unwrap();
                if stream {
                    output.extend(
                        delivery
                            .encode_events(&attempt, &events)
                            .unwrap()
                            .into_bytes()
                            .concat(),
                    );
                } else {
                    delivery.observe_events(&attempt, &events).unwrap();
                }
                id.get_or_insert_with(|| delivery.envelope.generated_id.clone().unwrap());
            }
            let (_, done) = attempt.push(b"event: done\ndata: [DONE]\n\n").unwrap();
            assert!(done.is_empty());
            let id = id.unwrap();
            assert_ne!(id, "late-upstream");
            assert!(!String::from_utf8_lossy(&output).contains("response.completed"));
            attempt.finish().unwrap();
            assert_eq!(
                attempt.response().unwrap().metadata.id.as_deref(),
                late.then_some("late-upstream")
            );
            let delivered: Value = if stream {
                output.extend(
                    delivery
                        .finish_stream(&attempt)
                        .unwrap()
                        .into_bytes()
                        .concat(),
                );
                let events: Vec<Value> = String::from_utf8(output)
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                for event in &events {
                    if let Some(response) = event.get("response") {
                        assert_eq!(response["id"], id);
                    }
                }
                assert_eq!(events.last().unwrap()["type"], "response.completed");
                events.last().unwrap()["response"].clone()
            } else {
                serde_json::from_slice(&delivery.encode_json(&attempt).unwrap()).unwrap()
            };
            assert_eq!(delivered["id"], id);
            assert_eq!(delivered["output"][0]["content"][0]["text"], "OK");
            let (retained, bytes) = delivery.snapshot(&attempt).unwrap();
            assert_eq!(retained.metadata.id.as_deref(), Some(id.as_str()));
            let retained: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(retained["id"], delivered["id"]);
            assert_eq!(retained["output"], delivered["output"]);
        }
    }
}

#[test]
fn stateless_responses_reject_hidden_signatures_and_poison_delivery() {
    let signed_wire = [
        json!({"event_type":"interaction.created","interaction":{"id":"r","model":"synthetic-model"}}),
        json!({"event_type":"step.start","index":0,"step":{"type":"thought","signature":"UjE=","summary":[{"type":"text","text":"plan"}]}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"step.start","index":1,"step":{"type":"model_output"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"text","text":"OK"}}),
        json!({"event_type":"step.stop","index":1}),
        json!({"event_type":"step.start","index":2,"step":{"type":"function_call","id":"c","name":"lookup","arguments":{"n":1},"signature":"UzM="}}),
        json!({"event_type":"step.stop","index":2}),
        json!({"event_type":"interaction.completed","interaction":{"status":"requires_action","usage":{"total_input_tokens":10,"total_output_tokens":5,"total_cached_tokens":0,"total_thought_tokens":1,"total_tokens":16}}}),
    ]
    .into_iter()
    .map(|frame| format!("event: {}\ndata: {frame}\n\n", frame["event_type"].as_str().unwrap()))
    .collect::<String>()
        + "event: done\ndata: [DONE]\n\n";
    let (mut attempt, _) = stateless_google_responses_setup(false);
    let mut rest = signed_wire.as_bytes();
    while !rest.is_empty() {
        let (n, _) = attempt.push(rest).unwrap();
        assert!(n > 0);
        rest = &rest[n..];
    }
    attempt.finish().unwrap();
    let mut source = attempt.response().unwrap().clone();
    super::super::replay::seal_for_client(
        &mut source,
        &crate::protocol::replay_envelope::CLIENT_ORIGIN,
    )
    .unwrap();
    source.metadata.storage = Some(crate::protocol::ResponseStorage {
        store: Presence::Value(true),
        ..Default::default()
    });
    let client_codec = GenerationCodec::new(
        GenerationApi::Responses,
        Dialect::Standard,
        Some(crate::protocol::replay_envelope::CLIENT_ORIGIN.clone()),
    );
    let client = ClientAdapter::openai(client_codec, TargetContract::full());
    assert!(
        matches!(client.snapshot(source),
        Err(crate::protocol::ConversionFailure { cause: crate::protocol::ConversionCause::Codec(CodecError::Unsupported(reason)), .. })
            if matches!(reason, CarrierReason::Replay)),
        "reported storage does not authorize execution-owned retention"
    );
    for stream in [false, true] {
        let (mut attempt, mut delivery) = stateless_google_responses_setup(stream);
        let mut events = vec![];
        let mut rest = signed_wire.as_bytes();
        while !rest.is_empty() {
            let (n, batch) = attempt.push(rest).unwrap();
            assert!(n > 0);
            events.extend(batch);
            rest = &rest[n..];
        }
        let result = if stream {
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .map(|_| ())
        } else {
            attempt.finish().unwrap();
            delivery.encode_json(&attempt).map(|_| ())
        };
        assert!(
            matches!(result, Err(AttemptError::Codec(CodecError::Unsupported(reason)))
                | Err(AttemptError::Conversion(crate::protocol::ConversionFailure { cause: crate::protocol::ConversionCause::Codec(CodecError::Unsupported(reason)), .. }))
                if matches!(reason, CarrierReason::Replay)),
            "{result:?}"
        );
        assert!(delivery.commit().is_err());
        assert!(delivery.encode_json(&attempt).is_err());
    }
}
