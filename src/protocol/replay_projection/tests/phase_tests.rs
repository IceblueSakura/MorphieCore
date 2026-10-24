use super::*;

fn source() -> DecodedResponse {
    let items = [
        (None, "prefix"),
        (None, "another"),
        (Some(Phase::Commentary), "checking"),
        (Some(Phase::FinalAnswer), "done"),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, (phase, text))| {
        let id = n as u64 + 1;
        (
            ItemId::new(id),
            Item::Message(Message {
                participant: None,
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase,
                parts: vec![Part::new(
                    PartId::new(id),
                    ContentPart::Text(Text::new(text, "fixture", 64).unwrap().into()),
                )],
            }),
        )
    })
    .collect();
    DecodedResponse {
        semantic: GenerationResponse::new(items, Outcome::Incomplete)
            .unwrap()
            .with_details(TerminalDetails {
                incomplete: Some(IncompleteReason::MaxOutputTokens),
                ..Default::default()
            })
            .unwrap()
            .with_usage(Some(Usage {
                cached_input_tokens: Some(0),
                input_cache_write_tokens: Some(0),
                ..Usage::operation(10, 3, 13)
            }))
            .unwrap(),
        fidelity: Default::default(),
        metadata: crate::protocol::ResponseMetadata {
            delivery: None,
            id: Some("phase-response".into()),
            model: "public".into(),
            created: Some(0.into()),
            storage: None,
            context: Default::default(),
            instruction_fidelity: Default::default(),
        },
    }
}

fn returned(protocol: ClientProtocol, body: &Value) -> Value {
    match protocol {
        ClientProtocol::Chat => json!({"model":"public","messages":[
            {"role":"user","content":"go"},body["choices"][0]["message"],
            {"role":"user","content":"continue"}]}),
        ClientProtocol::Messages => json!({"model":"public","max_tokens":64,"messages":[
            {"role":"user","content":"go"},{"role":"assistant","content":body["content"]},
            {"role":"user","content":"continue"}]}),
        ClientProtocol::Responses => json!({"model":"public","input":body["output"]}),
    }
}

#[test]
fn phase_delivery_preserves_content_without_fabricated_owners() {
    let source = source();
    let before = source.clone();
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Messages,
        ClientProtocol::Responses,
    ] {
        let client = client(protocol);
        let (retained, body, losses) = client.snapshot_with_evidence(source.clone()).unwrap();
        assert!(!body.to_string().contains("mca1."));
        if protocol == ClientProtocol::Responses {
            assert_eq!(retained.semantic.items(), source.semantic.items());
            assert_eq!(body["output"][2]["phase"], "commentary");
            assert_eq!(body["output"][3]["phase"], "final_answer");
            assert_eq!(body["output"].as_array().unwrap().len(), 4);
            assert!(!losses.iter().any(|e| e.field == "generation.message.phase"));
        } else {
            assert!(!crate::protocol::phase::has_phase(
                retained.semantic.items()
            ));
            assert!(losses.iter().any(
                |e| e.rule == "generation.omit-delivery-message-phase.v1" && e.action == "omit"
            ));
            if protocol == ClientProtocol::Chat {
                assert_eq!(
                    body["choices"][0]["message"],
                    json!({"role":"assistant","content":"prefixanothercheckingdone"})
                );
            } else {
                assert_eq!(
                    body["content"],
                    json!([
                        {"type":"text","text":"prefix"}, {"type":"text","text":"another"},
                        {"type":"text","text":"checking"}, {"type":"text","text":"done"}
                    ])
                );
            }
        }
        let decoded = client
            .decode_client_request(returned(protocol, &body).to_string().as_bytes())
            .unwrap();
        assert_eq!(
            crate::protocol::phase::has_phase(decoded.task.semantic.items()),
            protocol == ClientProtocol::Responses
        );
    }
    assert_eq!(source, before);
}

/// Hand-written Responses event expectations, independent of the encoder under test.
pub(super) fn frames() -> Vec<Value> {
    let envelope = |status: &str, output: Vec<Value>| {
        json!({
            "id":"phase-response","object":"response","model":"public","created_at":0,
            "status":status,"output":output
        })
    };
    let mut events =
        vec![json!({"type":"response.created","response":envelope("in_progress",vec![])})];
    let mut output = vec![];
    for (index, (phase, text)) in [
        (Value::Null, "prefix"),
        (Value::Null, "another"),
        (json!("commentary"), "checking"),
        (json!("final_answer"), "done"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("m{index}");
        let part = json!({"type":"output_text","text":text,"annotations":[]});
        let item = json!({"id":id,"type":"message","role":"assistant","status":"completed","phase":phase,"content":[part.clone()]});
        events.extend([
            json!({"type":"response.output_item.added","output_index":index,"item":{"id":id,"type":"message","role":"assistant","status":"in_progress","phase":phase,"content":[]}}),
            json!({"type":"response.content_part.added","output_index":index,"item_id":id,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":index,"item_id":id,"content_index":0,"delta":text,"logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":index,"item_id":id,"content_index":0,"text":text,"logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":index,"item_id":id,"content_index":0,"part":part}),
            json!({"type":"response.output_item.done","output_index":index,"item":item.clone()}),
        ]);
        output.push(item);
    }
    let mut terminal = envelope("incomplete", output);
    terminal["incomplete_details"] = json!({"reason":"max_output_tokens"});
    terminal["usage"] = json!({"input_tokens":10,"output_tokens":3,"total_tokens":13,
        "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}});
    events.push(json!({"type":"response.incomplete","response":terminal}));
    events
}

#[test]
fn phase_stream_preserves_return_carrier_at_eof_without_changing_terminal_policy() {
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Messages,
        ClientProtocol::Responses,
    ] {
        for truncated in [false, true] {
            let mut attempt = Attempt::new(
                Adapter::new(Profile::Responses, Dialect::Standard, None),
                1 << 20,
                SseLimits::default(),
            );
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = ResponseDelivery::new(
                client(protocol),
                "public",
                SseLimits::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: crate::semantic::value::Presence::Value(false),
                    ..Default::default()
                },
                Obfuscation::Disabled,
            );
            let mut output = vec![];
            for (sequence, mut frame) in frames().into_iter().enumerate() {
                frame["sequence_number"] = json!(sequence);
                let wire = format!(
                    "event: {}\ndata: {frame}\n\n",
                    frame["type"].as_str().unwrap()
                );
                let (n, events) = attempt.push(wire.as_bytes()).unwrap();
                assert_eq!(n, wire.len());
                output.extend(
                    delivery
                        .encode_events(&attempt, &events)
                        .unwrap_or_else(|error| {
                            panic!("{protocol:?} {} {events:?}: {error:?}", frame["type"])
                        })
                        .concat(),
                );
            }
            assert!(!String::from_utf8_lossy(&output).contains("mca1."));
            if truncated {
                attempt.push(b"data: {").unwrap();
                assert!(attempt.finish().is_err());
                assert!(delivery.finish_stream(&attempt).is_err());
                continue;
            }
            attempt.finish().unwrap();
            output.extend(delivery.finish_stream(&attempt).unwrap().concat());
            let text = String::from_utf8(output).unwrap();
            if protocol == ClientProtocol::Responses {
                assert!(!text.contains("mca1."));
                assert!(text.contains("\"phase\":\"commentary\""));
                assert!(text.contains("response.incomplete"));
            } else if protocol == ClientProtocol::Messages {
                assert!(!text.contains("mca1."));
                assert!(!text.contains("\"type\":\"thinking\""));
                assert!(text.contains("\"stop_reason\":\"max_tokens\""));
                assert!(
                    delivery
                        .losses()
                        .iter()
                        .any(|e| e.rule == "generation.omit-delivery-message-phase.v1"
                            && e.action == "omit")
                );
            } else {
                assert!(!text.contains("mca1."));
                assert!(!text.contains("reasoning_details"));
                assert!(
                    delivery
                        .losses()
                        .iter()
                        .any(|e| e.rule == "generation.omit-delivery-message-phase.v1")
                );
                assert!(text.contains(if protocol == ClientProtocol::Chat {
                    "\"finish_reason\":\"length\""
                } else {
                    "\"stop_reason\":\"max_tokens\""
                }));
            }
        }
    }
}

#[test]
fn unphased_streams_cannot_use_phase_grouping_to_bypass_original_chat_admission() {
    for multiple in [false, true] {
        let mut source = source();
        source = strip(&source, true).unwrap();
        if !multiple {
            source.semantic = source
                .semantic
                .clone()
                .with_items(source.semantic.items()[..1].to_vec())
                .unwrap();
        }
        let client = client(ClientProtocol::Chat);
        assert!(client.snapshot(source.clone()).is_err());
        let mut events = frames();
        if !multiple {
            events.retain(|v| v.get("output_index").is_none_or(|n| n == 0));
            events.last_mut().unwrap()["response"]["output"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
        }
        for event in &mut events {
            if let Some(item) = event.get_mut("item") {
                item["phase"] = Value::Null;
            }
            if let Some(output) = event
                .get_mut("response")
                .and_then(|r| r.get_mut("output"))
                .and_then(Value::as_array_mut)
            {
                for item in output {
                    item["phase"] = Value::Null;
                }
            }
        }
        let mut attempt = Attempt::new(
            Adapter::new(Profile::Responses, Dialect::Standard, None),
            1 << 20,
            Default::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            client,
            "public",
            Default::default(),
            crate::semantic::context::StreamOptions {
                include_obfuscation: crate::semantic::value::Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        );
        for (sequence, mut event) in events.into_iter().enumerate() {
            event["sequence_number"] = json!(sequence);
            let wire = format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            );
            let (_, events) = attempt.push(wire.as_bytes()).unwrap();
            delivery.encode_events(&attempt, &events).unwrap();
        }
        attempt.finish().unwrap();
        assert!(delivery.finish_stream(&attempt).is_err());
    }
}
