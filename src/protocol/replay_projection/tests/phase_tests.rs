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
                status: ItemLifecycle::Incomplete,
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
        evidence: Default::default(),
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
            assert!(
                !losses
                    .iter()
                    .any(|e| e.field() == "generation.message.phase")
            );
        } else {
            assert!(!crate::protocol::phase::has_phase(
                retained.semantic.items()
            ));
            assert!(
                losses
                    .iter()
                    .any(|e| e.rule() == "generation.omit-delivery-message-phase.v1"
                        && e.change().as_str() == "omit")
            );
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
        let item = json!({"id":id,"type":"message","role":"assistant","status":"incomplete","phase":phase,"content":[part.clone()]});
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
                GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
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
                        .map(|batch| batch.into_bytes())
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
                assert!(
                    delivery
                        .finish_stream(&attempt)
                        .map(|batch| batch.into_bytes())
                        .is_err()
                );
                continue;
            }
            attempt.finish().unwrap();
            output.extend(
                delivery
                    .finish_stream(&attempt)
                    .map(|batch| batch.into_bytes())
                    .unwrap()
                    .concat(),
            );
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
                        .effects()
                        .iter()
                        .any(|e| e.rule() == "generation.omit-delivery-message-phase.v1"
                            && e.change().as_str() == "omit")
                );
            } else {
                assert!(!text.contains("mca1."));
                assert!(!text.contains("reasoning_details"));
                assert!(
                    delivery
                        .effects()
                        .iter()
                        .any(|e| e.rule() == "generation.omit-delivery-message-phase.v1")
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
fn unphased_plain_text_uses_the_same_selected_chat_grouping() {
    // Only max-output-token text is selected, not another incomplete cause.
    let mut incompatible = source();
    let mut items = incompatible.semantic.items().to_vec();
    for (_, item) in &mut items {
        if let Item::Message(message) = item {
            message.status = ItemLifecycle::Completed;
        }
    }
    incompatible.semantic = incompatible
        .semantic
        .with_items(items)
        .unwrap()
        .with_details(TerminalDetails {
            incomplete: Some(IncompleteReason::ContentFilter),
            ..Default::default()
        })
        .unwrap();
    assert!(client(ClientProtocol::Chat).snapshot(incompatible).is_err());
    for (multiple, completed) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut source = source();
        source = strip(&source, true).unwrap();
        if !multiple {
            source.semantic = source
                .semantic
                .clone()
                .with_items(source.semantic.items()[..1].to_vec())
                .unwrap();
        }
        if completed {
            let mut items = source.semantic.items().to_vec();
            let Item::Message(message) = &mut items[0].1 else {
                unreachable!()
            };
            message.status = ItemLifecycle::Completed;
            source.semantic = source.semantic.with_items(items).unwrap();
        }
        let mut items = source.semantic.items().to_vec();
        items.push((
            ItemId::new(100),
            Item::Reasoning(ReasoningItem {
                status: ItemLifecycle::Incomplete,
                replay: None,
                parts: vec![
                    (
                        PartId::new(100),
                        ReasoningContent::Summary(
                            Text::new("Hidden summary", "fixture", 64).unwrap(),
                        ),
                    ),
                    (
                        PartId::new(101),
                        ReasoningContent::Text(
                            Text::new("Hidden reasoning", "fixture", 64).unwrap(),
                        ),
                    ),
                ],
            }),
        ));
        source.semantic = source.semantic.with_items(items).unwrap();
        let before = source.clone();
        let client = client(ClientProtocol::Chat);
        let (retained, snapshot, effects) = client.snapshot_with_evidence(source.clone()).unwrap();
        let status_rule = "chat.omit-text-item-status.v1";
        let reasoning_rule = "chat.omit-readable-reasoning.v1";
        let omitted = effects
            .iter()
            .find(|effect| effect.rule() == reasoning_rule)
            .unwrap();
        assert_eq!(
            omitted.owner(),
            crate::protocol::conversion::generation::Owner::item(ItemId::new(100))
        );
        assert!(
            !retained
                .semantic
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::Reasoning(_)))
        );
        assert_eq!(snapshot["usage"]["total_tokens"], 13);
        assert_eq!(
            effects.iter().filter(|e| e.rule() == status_rule).count(),
            usize::from(completed)
        );
        if completed {
            let effect = effects.iter().find(|e| e.rule() == status_rule).unwrap();
            assert_eq!(effect.field(), "generation.message.status");
            assert_eq!(
                effect.owner(),
                crate::protocol::conversion::generation::Owner::item(source.semantic.items()[0].0)
            );
            assert!(effect.change().is_loss());
        }
        assert_eq!(
            retained.semantic.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Incomplete)
        );
        assert_eq!(snapshot["choices"][0]["finish_reason"], "length");
        assert_eq!(source, before);
        assert_eq!(
            snapshot["choices"][0]["message"]["content"],
            if multiple {
                "prefixanothercheckingdone"
            } else {
                "prefix"
            }
        );
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
                if completed && item["id"] == "m0" && item["status"] == "incomplete" {
                    item["status"] = json!("completed");
                }
            }
            if let Some(output) = event
                .get_mut("response")
                .and_then(|r| r.get_mut("output"))
                .and_then(Value::as_array_mut)
            {
                for item in output {
                    item["phase"] = Value::Null;
                    if completed && item["id"] == "m0" {
                        item["status"] = json!("completed");
                    }
                }
            }
        }
        let mut terminal = events.pop().unwrap();
        let output = terminal["response"]["output"].as_array_mut().unwrap();
        let index = output.len();
        let reasoning = json!({"id":"reason","type":"reasoning","status":"incomplete",
            "summary":[{"type":"summary_text","text":"Hidden summary"}],
            "content":[{"type":"reasoning_text","text":"Hidden reasoning"}]});
        output.push(reasoning.clone());
        events.extend([
            json!({"type":"response.output_item.added","output_index":index,"item":{
                "id":"reason","type":"reasoning","status":"in_progress","summary":[],"content":[]}}),
            json!({"type":"response.reasoning_summary_part.added","output_index":index,"item_id":"reason",
                "summary_index":0,"part":{"type":"summary_text","text":""}}),
            json!({"type":"response.reasoning_summary_text.delta","output_index":index,"item_id":"reason",
                "summary_index":0,"delta":"Hidden summary"}),
            json!({"type":"response.reasoning_summary_text.done","output_index":index,"item_id":"reason",
                "summary_index":0,"text":"Hidden summary"}),
            json!({"type":"response.reasoning_summary_part.done","output_index":index,"item_id":"reason",
                "summary_index":0,"part":reasoning["summary"][0]}),
            json!({"type":"response.content_part.added","output_index":index,"item_id":"reason",
                "content_index":0,"part":{"type":"reasoning_text","text":""}}),
            json!({"type":"response.reasoning_text.delta","output_index":index,"item_id":"reason",
                "content_index":0,"delta":"Hidden reasoning"}),
            json!({"type":"response.reasoning_text.done","output_index":index,"item_id":"reason",
                "content_index":0,"text":"Hidden reasoning"}),
            json!({"type":"response.content_part.done","output_index":index,"item_id":"reason",
                "content_index":0,"part":reasoning["content"][0]}),
            json!({"type":"response.output_item.done","output_index":index,"item":reasoning}),
        ]);
        events.push(terminal);
        let mut attempt = Attempt::new(
            GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
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
        let mut output = vec![];
        for (sequence, mut event) in events.into_iter().enumerate() {
            event["sequence_number"] = json!(sequence);
            let wire = format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            );
            let (_, events) = attempt.push(wire.as_bytes()).unwrap();
            output.extend(
                delivery
                    .encode_events(&attempt, &events)
                    .map(|batch| batch.into_bytes())
                    .unwrap()
                    .concat(),
            );
        }
        attempt.finish().unwrap();
        let observation = attempt.response().unwrap().clone();
        let reasoning_owner = observation
            .semantic
            .items()
            .iter()
            .find_map(|(id, item)| matches!(item, Item::Reasoning(_)).then_some(*id))
            .unwrap();
        assert_eq!(
            observation.semantic.items()[0].1.lifecycle(),
            Some(if completed {
                ItemLifecycle::Completed
            } else {
                ItemLifecycle::Incomplete
            })
        );
        let end = delivery.finish_stream(&attempt).unwrap().into_bytes();
        assert!(
            end.iter()
                .any(|frame| String::from_utf8_lossy(frame).contains("[DONE]"))
        );
        output.extend(end.concat());
        assert!(!String::from_utf8_lossy(&output).contains("Hidden"));
        assert_eq!(
            delivery
                .effects()
                .iter()
                .filter(|effect| effect.rule() == reasoning_rule)
                .count(),
            1
        );
        assert_eq!(
            delivery
                .effects()
                .iter()
                .find(|effect| effect.rule() == reasoning_rule)
                .unwrap()
                .owner(),
            crate::protocol::conversion::generation::Owner::item(reasoning_owner)
        );
        assert_eq!(
            delivery
                .effects()
                .iter()
                .filter(|e| e.rule() == status_rule)
                .count(),
            usize::from(completed)
        );
        if completed {
            let effects = delivery.effects();
            let effect = effects.iter().find(|e| e.rule() == status_rule).unwrap();
            assert_eq!(
                effect.owner(),
                crate::protocol::conversion::generation::Owner::item(
                    observation.semantic.items()[0].0
                )
            );
            assert_eq!(
                effect.boundary(),
                crate::protocol::conversion::generation::ConversionBoundary::EventProject
            );
        }
        let (saved, body) = delivery.snapshot(&attempt).unwrap();
        assert!(
            !saved
                .semantic
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::Reasoning(_)))
        );
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["choices"], snapshot["choices"]);
        assert_eq!(
            saved.semantic.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Incomplete)
        );
        assert_eq!(attempt.response().unwrap(), &observation);
        let mut consumer = crate::protocol::openai::chat::sse::ChatSseDecoder::new(
            200,
            "text/event-stream",
            Default::default(),
        )
        .unwrap();
        let mut remaining = output.as_slice();
        while !remaining.is_empty() {
            let (used, _) = consumer.consume(remaining).unwrap();
            assert!(used > 0);
            remaining = &remaining[used..];
        }
        consumer.finish().unwrap();
        let streamed = consumer.materialize().unwrap();
        assert_eq!(streamed.semantic.outcome(), Outcome::Incomplete);
        assert_eq!(
            streamed.semantic.details().incomplete,
            Some(IncompleteReason::MaxOutputTokens)
        );
        assert_eq!(
            streamed.semantic.items()[0].1.lifecycle(),
            Some(ItemLifecycle::Incomplete)
        );
        let Item::Message(message) = &streamed.semantic.items()[0].1 else {
            unreachable!()
        };
        let ContentPart::Text(text) = &message.parts[0].content else {
            unreachable!()
        };
        assert_eq!(
            text.as_str(),
            snapshot["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
        );
    }
}

#[test]
fn unselected_item_status_loss_rejects_before_chat_stream_terminal() {
    let mut events = frames();
    events.retain(|v| v.get("output_index").is_none_or(|n| n == 0));
    let terminal = events.last_mut().unwrap();
    terminal["response"]["output"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    terminal["response"]["output"][0]["status"] = json!("completed");
    terminal["response"]["incomplete_details"]["reason"] = json!("content_filter");
    for event in &mut events {
        if event["type"] == "response.output_item.done" {
            event["item"]["status"] = json!("completed");
        }
    }
    let mut attempt = Attempt::new(
        GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None),
        1 << 20,
        Default::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        client(ClientProtocol::Chat),
        "public",
        Default::default(),
        crate::semantic::context::StreamOptions {
            include_obfuscation: crate::semantic::value::Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    let mut output = vec![];
    for (sequence, mut event) in events.into_iter().enumerate() {
        event["sequence_number"] = json!(sequence);
        let (_, decoded) = attempt
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap();
        output.extend(
            delivery
                .encode_events(&attempt, &decoded)
                .unwrap()
                .into_bytes()
                .concat(),
        );
    }
    attempt.finish().unwrap();
    assert_eq!(
        delivery
            .finish_stream(&attempt)
            .unwrap_err()
            .representation(),
        Some(&crate::protocol::conversion::generation::RepresentationError::Terminal)
    );
    assert!(!String::from_utf8_lossy(&output).contains("[DONE]"));
    assert!(delivery.finish_stream(&attempt).is_err());
}
