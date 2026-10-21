//! Independent native wires and guarded Messages terminal projection at real EOF.
use super::*;
use crate::{
    protocol::{
        CodecError,
        anthropic::messages::client::GoogleTerminalPolicy,
        openai::{Adapter, Dialect, Profile, TargetCaps},
    },
    semantic::{task::generation::*, value::Presence},
    topology::{Endpoint, catalog::GOOGLE_BINDINGS},
};
use serde_json::{Value, json};

fn fixture(interactions: bool, calls: bool) -> (Endpoint, crate::adapter::Request, Vec<Value>) {
    let binding = &GOOGLE_BINDINGS[0];
    let endpoint = if interactions {
        binding.interactions_endpoint()
    } else {
        binding.endpoint()
    };
    let request = ClientAdapter::Messages.decode_client_request(json!({
        "model":binding.model,"max_tokens":256,"messages":[{"role":"user","content":"Read."}]
    }).to_string().as_bytes()).unwrap();
    let mut frames = vec![];
    if interactions {
        frames.push(
            json!({"event_type":"interaction.created","interaction":{"model":binding.model}}),
        );
        if calls {
            for index in 0..2 {
                let id = format!("c{index}");
                frames.extend([
                    json!({"event_type":"step.start","index":index,"step":{"type":"function_call",
                        "id":id,"name":"read","arguments":{},"signature":format!("synthetic-call-{index}")}}),
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
    } else {
        let parts = if calls {
            json!([
                {"functionCall":{"id":"c0","name":"read","args":{"path":"0"}},"thoughtSignature":"U0lHLTE="},
                {"functionCall":{"id":"c1","name":"read","args":{"path":"1"}},"thoughtSignature":"U0lHLTI="}
            ])
        } else {
            json!([{"text":"answer","thoughtSignature":"U0lH"}])
        };
        frames.push(json!({"responseId":"r","modelVersion":binding.model,
            "candidates":[{"index":0,"content":{"role":"model","parts":parts},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":3,"thoughtsTokenCount":2,"totalTokenCount":15}}));
    }
    (endpoint, request, frames)
}
fn delivery(endpoint: &Endpoint, request: &crate::adapter::Request) -> ResponseDelivery {
    ResponseDelivery::new(
        ClientAdapter::Messages,
        "public",
        Default::default(),
        Default::default(),
        Obfuscation::Disabled,
    )
    .with_request(endpoint, request)
}
fn feed(
    attempt: &mut Attempt,
    delivery: &mut ResponseDelivery,
    frames: &[Value],
    stream: bool,
    interactions: bool,
) -> Vec<u8> {
    let mut wire = frames
        .iter()
        .map(|f| {
            if interactions {
                format!(
                    "event: {}\ndata: {f}\n\n",
                    f["event_type"].as_str().unwrap()
                )
            } else {
                format!("data: {f}\n\n")
            }
        })
        .collect::<String>();
    if interactions {
        wire.push_str("event: done\ndata: [DONE]\n\n");
    }
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
                output.extend(delivery.encode_events(attempt, &events).unwrap().concat());
            }
        }
    }
    output
}

#[test]
fn google_messages_terminals_keep_source_facts_and_ordered_calls() {
    for interactions in [false, true] {
        for calls in [false, true] {
            for stream in [false, true] {
                let (endpoint, request, frames) = fixture(interactions, calls);
                let mut delivery = delivery(&endpoint, &request);
                let mut attempt =
                    Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
                attempt.begin(200, "text/event-stream").unwrap();
                let mut wire = feed(&mut attempt, &mut delivery, &frames, stream, interactions);
                assert!(!String::from_utf8_lossy(&wire).contains("message_stop"));
                attempt.finish().unwrap();
                let original = attempt.response().unwrap().clone();
                let original_terminal = attempt.terminal_event().unwrap().clone();
                assert_eq!(
                    original.semantic.details().completion,
                    if interactions {
                        None
                    } else {
                        Some(CompletionCause::NaturalOrStopSequence)
                    }
                );
                let (stop, ids) = if stream {
                    wire.extend(delivery.finish_stream(&attempt).unwrap().concat());
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
                    let (retained, bytes) = delivery.encode_json_snapshot(&attempt).unwrap();
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
                let rule = if interactions {
                    (!calls).then_some("messages.interactions-completed-as-end-turn.v1")
                } else {
                    Some(if calls {
                        "messages.google-tool-stop.v1"
                    } else {
                        "messages.google-stop-without-sequences.v1"
                    })
                };
                let losses: Vec<_> = delivery
                    .losses()
                    .iter()
                    .filter(|l| l.field == "generation.completion_cause")
                    .collect();
                assert_eq!(losses.len(), usize::from(rule.is_some()));
                if let Some(rule) = rule {
                    assert_eq!(losses[0].rule, rule);
                    assert_eq!(
                        losses[0].direction,
                        if stream {
                            "event.encode"
                        } else {
                            "response.encode"
                        }
                    );
                }
                assert_eq!(attempt.response().unwrap(), &original);
                assert_eq!(attempt.terminal_event().unwrap(), &original_terminal);
            }
        }
    }
}

#[test]
fn google_finish_explanation_omission_keeps_incomplete_source_and_requires_receipts() {
    for client in [
        ClientAdapter::Messages,
        ClientAdapter::openai(
            Adapter::new(Profile::Chat, Dialect::MorphieCore, None),
            TargetCaps::full(),
        ),
        ClientAdapter::openai(
            Adapter::new(Profile::Responses, Dialect::MorphieCore, None),
            TargetCaps::full(),
        ),
    ] {
        for stream in [false, true] {
            let (endpoint, request, mut frames) = fixture(false, false);
            frames[0]["candidates"][0]["finishReason"] = json!("MAX_TOKENS");
            frames[0]["candidates"][0]["finishMessage"] = json!("synthetic explanation");
            let mut delivery = ResponseDelivery::new(
                client.clone(),
                "public",
                Default::default(),
                StreamOptions {
                    include_usage: Presence::Value(true),
                    include_obfuscation: Presence::Value(false),
                },
                Obfuscation::Disabled,
            )
            .with_request(&endpoint, &request);
            let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut output = feed(&mut attempt, &mut delivery, &frames, stream, false);
            assert!(!String::from_utf8_lossy(&output).contains("message_stop"));
            assert!(!String::from_utf8_lossy(&output).contains("response.incomplete"));
            attempt.finish().unwrap();
            let original = attempt.response().unwrap().clone();
            assert_eq!(original.semantic.outcome(), Outcome::Incomplete);
            assert_eq!(
                original.semantic.details().incomplete,
                Some(IncompleteReason::MaxOutputTokens)
            );
            assert!(matches!(
                original.semantic.details().explanation,
                Presence::Value(_)
            ));
            if stream {
                output.extend(delivery.finish_stream(&attempt).unwrap().concat());
                let body = String::from_utf8_lossy(&output);
                assert!(body.contains(match client.protocol() {
                    ClientProtocol::Messages => "\"stop_reason\":\"max_tokens\"",
                    ClientProtocol::Chat => "\"finish_reason\":\"length\"",
                    ClientProtocol::Responses => "event: response.incomplete",
                }));
                assert!(!body.contains("event: response.completed"));
            } else {
                let (_, bytes) = delivery.encode_json_snapshot(&attempt).unwrap();
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                match client.protocol() {
                    ClientProtocol::Messages => assert_eq!(value["stop_reason"], "max_tokens"),
                    ClientProtocol::Chat => {
                        assert_eq!(value["choices"][0]["finish_reason"], "length")
                    }
                    ClientProtocol::Responses => assert_eq!(value["status"], "incomplete"),
                }
                output.extend(bytes);
            }
            assert!(!String::from_utf8_lossy(&output).contains("synthetic explanation"));
            let rule = if client.protocol() == ClientProtocol::Messages {
                "messages.omit-terminal-explanation.v1"
            } else {
                "openai.omit-terminal-explanation.v1"
            };
            let reports: Vec<_> = delivery
                .losses()
                .iter()
                .filter(|l| l.rule == rule)
                .collect();
            assert_eq!(reports.len(), 1);
            assert_eq!(
                reports[0].direction,
                if stream {
                    "event.encode"
                } else {
                    "response.encode"
                }
            );
            assert_eq!(reports[0].action, "omit");
            assert_eq!(attempt.response().unwrap(), &original);
        }
    }
}

#[test]
fn google_messages_projection_requires_context_and_never_repairs_failure() {
    for interactions in [false, true] {
        let (endpoint, request, frames) = fixture(interactions, false);
        let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        feed(
            &mut attempt,
            &mut delivery(&endpoint, &request),
            &frames,
            false,
            interactions,
        );
        attempt.finish().unwrap();
        let chat = ResponseDelivery::new(
            ClientAdapter::openai(
                crate::protocol::openai::Adapter::new(
                    crate::protocol::openai::Profile::Chat,
                    crate::protocol::openai::Dialect::MorphieCore,
                    None,
                ),
                crate::protocol::openai::TargetCaps::full(),
            ),
            "public",
            Default::default(),
            Default::default(),
            Obfuscation::Disabled,
        )
        .with_request(&endpoint, &request);
        assert!(chat.messages_terminal.is_none());
        let (other, _, _) = fixture(!interactions, false);
        assert!(matches!(
            delivery(&other, &request).encode_json(&attempt),
            Err(AttemptError::Delivery("terminal projection source changed"))
        ));
        for guard in ["unbound", "stops", "disabled"] {
            let mut selected = endpoint.clone();
            let mut request = request.clone();
            if guard == "stops" {
                let mut settings = request.task.semantic.settings().clone();
                settings
                    .controls
                    .stop_sequences
                    .push(crate::semantic::value::Text::new("END", "stop", 16).unwrap());
                request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
            }
            if guard == "disabled" {
                match &mut selected.representation {
                    crate::lowering::target::TargetCaps::GoogleGenerateContent(c) => {
                        c.messages_terminal_projection = false
                    }
                    crate::lowering::target::TargetCaps::GoogleInteractions(c) => {
                        c.messages_terminal_projection = false
                    }
                    _ => unreachable!(),
                }
            }
            let mut d = if guard == "unbound" {
                ResponseDelivery::new(
                    ClientAdapter::Messages,
                    "public",
                    Default::default(),
                    Default::default(),
                    Obfuscation::Disabled,
                )
            } else {
                delivery(&selected, &request)
            };
            assert!(d.messages_terminal.is_none());
            let error = d.encode_json(&attempt).unwrap_err().to_string();
            assert!(error.contains(if interactions {
                "Messages terminal"
            } else {
                "Messages completion cause"
            }));
        }
        // Knowing an outcome, or having calls, never turns an unspecified failure into success.
        let mut incomplete = attempt
            .response()
            .unwrap()
            .semantic
            .clone()
            .with_details(TerminalDetails::default())
            .unwrap()
            .with_progress(InteractionProgress::Unreported)
            .unwrap();
        incomplete = GenerationResponse::new(incomplete.items().to_vec(), Outcome::Incomplete)
            .unwrap()
            .with_details(TerminalDetails {
                incomplete: Some(IncompleteReason::Unspecified),
                ..Default::default()
            })
            .unwrap();
        let policy = if interactions {
            GoogleTerminalPolicy::InteractionsWithoutStops
        } else {
            GoogleTerminalPolicy::GenerateContentWithoutStops
        };
        assert!(
            policy
                .project(&incomplete, "event.encode")
                .unwrap()
                .is_none()
        );
        for progress in [
            InteractionProgress::Unreported,
            InteractionProgress::NeedsContinuation,
        ] {
            let unknown = attempt
                .response()
                .unwrap()
                .semantic
                .clone()
                .with_progress(progress)
                .unwrap();
            assert!(policy.project(&unknown, "event.encode").unwrap().is_none());
        }
        // A real native MAX_TOKENS call turn keeps its incomplete terminal.
        if !interactions {
            let (endpoint, request, mut frames) = fixture(false, true);
            frames[0]["candidates"][0]["finishReason"] = json!("MAX_TOKENS");
            let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut d = delivery(&endpoint, &request);
            feed(&mut attempt, &mut d, &frames, false, false);
            attempt.finish().unwrap();
            let value: Value = serde_json::from_slice(&d.encode_json(&attempt).unwrap()).unwrap();
            assert_eq!(value["stop_reason"], "max_tokens");
            assert!(
                !d.losses()
                    .iter()
                    .any(|l| l.field == "generation.completion_cause")
            );
        }
    }
    let (endpoint, request, frames) = fixture(true, false);
    for premature in [true, false] {
        let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut d = delivery(&endpoint, &request);
        feed(&mut attempt, &mut d, &frames, true, true);
        if !premature {
            attempt.finish().unwrap();
            d.cancel();
        }
        assert!(d.finish_stream(&attempt).is_err());
    }
    let (endpoint, request, mut frames) = fixture(true, false);
    frames.last_mut().unwrap()["interaction"]["usage"]
        .as_object_mut()
        .unwrap()
        .remove("total_cached_tokens");
    let mut attempt = Attempt::new(endpoint.adapter().unwrap(), 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut d = delivery(&endpoint, &request);
    feed(&mut attempt, &mut d, &frames, false, true);
    attempt.finish().unwrap();
    assert!(
        matches!(d.encode_json(&attempt),Err(AttemptError::Adapter(crate::adapter::AdapterError::Codec(CodecError::Unsupported(reason)))) if reason=="Messages usage")
    );
}
