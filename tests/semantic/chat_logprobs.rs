//! Independent Chat probability and context oracles against the pinned SDK contract.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{Profile, events::EventEncoder},
    semantic::{task::generation::*, value::Presence},
};
use serde_json::{Value, json};

fn adapter(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn probability(token: &str) -> Value {
    json!({"token":token,"logprob":-0.1234567890123456,"bytes":null,
        "top_logprobs":[{"token":token,"logprob":-0.1234567890123456,"bytes":token.as_bytes()}]})
}
fn response(refusal: bool, probabilities: Value) -> Value {
    let message = if refusal {
        json!({"role":"assistant","content":null,"refusal":"no"})
    } else {
        json!({"role":"assistant","content":"ok"})
    };
    json!({"id":"c","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":message,"finish_reason":"stop","logprobs":probabilities}]})
}
fn chunk(delta: Value, probabilities: Value, finish: Value) -> Value {
    json!({"id":"c","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":delta,"logprobs":probabilities,"finish_reason":finish}]})
}
fn part(response: &GenerationResponse) -> &ContentPart {
    let Item::Message(message) = &response.items()[0].1 else {
        panic!("message")
    };
    &message.parts[0].content
}

#[test]
fn chat_context_projects_final_hints_without_inventing_reported_facts() {
    let wire = json!({"model":"m","messages":[{"role":"user","content":"hi"}],
        "metadata":{"tag":"original"},"service_tier":"fast"});
    let mut request = adapter(Profile::Chat)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(request.context.metadata.value().unwrap()["tag"], "original");
    assert_eq!(
        adapter(Profile::Responses)
            .encode_request(&request, "upstream", &Contract::full())
            .unwrap()["metadata"],
        wire["metadata"]
    );
    request.context.metadata = Presence::Value([("tag".into(), "changed".into())].into());
    request.context.service_tier = Presence::Absent;
    let encoded = adapter(Profile::Chat)
        .encode_request(&request, "upstream", &Contract::full())
        .unwrap();
    assert_eq!(encoded["metadata"], json!({"tag":"changed"}));
    assert!(encoded.get("service_tier").is_none());
    let mut limited = Contract::full();
    limited.standard_context = false;
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "upstream", &limited)
            .is_err()
    );
    request.context.metadata = Presence::Absent;
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "upstream", &limited)
            .unwrap()
            .get("metadata")
            .is_none()
    );
    for value in [Value::Null, json!({})] {
        let mut wire = wire.clone();
        wire["metadata"] = value.clone();
        wire["service_tier"] = Value::Null;
        let request = adapter(Profile::Chat)
            .decode_request(wire.to_string().as_bytes())
            .unwrap();
        let encoded = adapter(Profile::Chat)
            .encode_request(&request, "m", &Contract::full())
            .unwrap();
        assert_eq!(encoded["metadata"], value);
        assert!(encoded["service_tier"].is_null());
        let decoded = adapter(Profile::Chat)
            .decode_response(response(false, Value::Null).to_string().as_bytes())
            .unwrap();
        assert!(decoded.metadata.context.execution.metadata.is_absent());
        assert!(decoded.metadata.context.execution.service_tier.is_absent());
    }
    for (key, value) in [
        ("metadata", json!({"tag":1})),
        ("metadata", json!({"x":"x".repeat(513)})),
        ("service_tier", json!("ultrafast")),
        ("service_tier", json!("unknown")),
    ] {
        let mut wire = wire.clone();
        wire[key] = value;
        assert!(
            adapter(Profile::Chat)
                .decode_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
}

#[test]
fn chat_probability_request_controls_project_and_reject_invalid_combinations() {
    let wire = json!({"model":"m","messages":[{"role":"user","content":"hi"}],"logprobs":true,"top_logprobs":2});
    let mut request = adapter(Profile::Chat)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    let encoded = adapter(Profile::Chat)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(encoded["logprobs"], true);
    assert_eq!(encoded["top_logprobs"], 2);
    let responses = adapter(Profile::Responses)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(
        responses["include"],
        json!(["message.output_text.logprobs"])
    );
    assert_eq!(responses["top_logprobs"], 2);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.top_logprobs = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "m", &Contract::full())
            .unwrap()
            .get("top_logprobs")
            .is_none()
    );
    let mut contract = Contract::full();
    contract.semantics.logprobs = false;
    assert!(
        adapter(Profile::Chat)
            .encode_request(&request, "m", &contract)
            .is_err()
    );
    for value in [Value::Null, json!(false)] {
        let mut wire = wire.clone();
        wire["logprobs"] = value.clone();
        wire.as_object_mut().unwrap().shift_remove("top_logprobs");
        let request = adapter(Profile::Chat)
            .decode_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            adapter(Profile::Chat)
                .encode_request(&request, "m", &Contract::full())
                .unwrap()["logprobs"],
            value
        );
    }
    for (enabled, top) in [
        (json!(false), json!(1)),
        (Value::Null, json!(0)),
        (json!(true), json!(21)),
        (json!(true), json!(-1)),
        (json!(true), json!(0.5)),
    ] {
        let mut wire = wire.clone();
        wire["logprobs"] = enabled;
        wire["top_logprobs"] = top;
        assert!(
            adapter(Profile::Chat)
                .decode_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
}

#[test]
fn chat_content_and_refusal_probabilities_close_static_and_event_paths() {
    for refusal in [false, true] {
        let key = if refusal { "refusal" } else { "content" };
        let word = if refusal { "no" } else { "ok" };
        let probabilities = json!({key:[probability(word)]});
        let wire = response(refusal, probabilities.clone());
        let decoded = adapter(Profile::Chat)
            .decode_response(wire.to_string().as_bytes())
            .unwrap();
        let encoded = adapter(Profile::Chat)
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        // Canonical optional bytes are null on Chat, not a guessed UTF-8 value.
        assert_eq!(encoded["choices"][0]["logprobs"][key], probabilities[key]);
        let mut decoder = adapter(Profile::Chat).event_decoder();
        let mut events = Vec::new();
        for payload in [
            chunk(json!({"role":"assistant"}), Value::Null, Value::Null),
            chunk(json!({key:word}), probabilities.clone(), Value::Null),
            chunk(json!({}), Value::Null, json!("stop")),
        ] {
            events.extend(decoder.push(&payload).unwrap());
        }
        events.extend(decoder.done().unwrap());
        assert_eq!(decoder.materialize().unwrap().semantic, decoded.semantic);
        let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone()).unwrap();
        let mut observed = Vec::new();
        for event in &events {
            observed.extend(encoder.encode(event, &decoded.fidelity).unwrap());
        }
        encoder.finish().unwrap();
        let reported: Vec<_> = observed
            .iter()
            .filter_map(|v| {
                v.pointer(&format!("/choices/0/logprobs/{key}"))
                    .and_then(Value::as_array)
            })
            .flatten()
            .cloned()
            .collect();
        assert_eq!(
            reported,
            vec![probability(word)],
            "final metadata must not duplicate streamed probabilities"
        );
        let mut limited = Contract::full();
        limited.semantics.logprobs = false;
        assert!(
            adapter(Profile::Chat)
                .encode_response(&decoded, &limited)
                .is_err()
        );
        assert!(
            adapter(Profile::Responses)
                .encode_response(&decoded, &Contract::full())
                .is_err(),
            "refusal has no Responses probability position; text needs reported byte details"
        );
        let mut changed = decoded.clone();
        let mut items = changed.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            unreachable!()
        };
        // Replacement through a fresh semantic part cannot restore source probabilities.
        message.parts[0].content = if refusal {
            ContentPart::Refusal(
                morphiecore::semantic::value::Text::new("changed", "test", 128)
                    .unwrap()
                    .into(),
            )
        } else {
            ContentPart::Text(
                morphiecore::semantic::value::Text::new("changed", "test", 128)
                    .unwrap()
                    .into(),
            )
        };
        changed.semantic = changed.semantic.with_items(items).unwrap();
        let encoded = adapter(Profile::Chat)
            .encode_response(&changed, &Contract::full())
            .unwrap();
        assert!(encoded["choices"][0].get("logprobs").is_none());
        assert!(matches!(part(&decoded.semantic), ContentPart::Refusal(_)) == refusal);
    }
}

#[test]
fn chat_probability_arrays_are_reported_even_when_empty_and_bad_owners_fail() {
    for refusal in [false, true] {
        let key = if refusal { "refusal" } else { "content" };
        let decoded = adapter(Profile::Chat)
            .decode_response(response(refusal, json!({key:[]})).to_string().as_bytes())
            .unwrap();
        assert_eq!(
            adapter(Profile::Chat)
                .encode_response(&decoded, &Contract::full())
                .unwrap()["choices"][0]["logprobs"][key],
            json!([])
        );
        let other = if refusal { "content" } else { "refusal" };
        assert!(
            adapter(Profile::Chat)
                .decode_response(response(refusal, json!({other:[]})).to_string().as_bytes())
                .is_err()
        );
    }
    for probability in [
        json!({"token":"ok","logprob":-1}),
        json!({"token":"ok","logprob":1,"top_logprobs":[]}),
        json!({"token":"ok","logprob":-1,"top_logprobs":[{"token":"other"}]}),
        json!({"token":"ok","logprob":-1,"top_logprobs":[],"bytes":[256]}),
    ] {
        assert!(
            adapter(Profile::Chat)
                .decode_response(
                    response(false, json!({"content":[probability]}))
                        .to_string()
                        .as_bytes()
                )
                .is_err()
        );
    }
    let mut decoder = adapter(Profile::Chat).event_decoder();
    assert!(
        decoder
            .push(&chunk(
                json!({"content":"ok"}),
                json!({"refusal":[probability("no")]}),
                Value::Null
            ))
            .is_err()
    );
    assert!(
        decoder
            .push(&chunk(json!({}), Value::Null, json!("stop")))
            .is_err()
    );
    assert!(decoder.done().is_err());
}

#[test]
fn independent_probability_owners_encode_and_text_edits_invalidate_only_their_facts() {
    use morphiecore::{protocol::openai::ResponseMetadata, semantic::value::Text};
    let probs: Vec<Logprob> = serde_json::from_value(json!([probability("no")])).unwrap();
    let refusal = RefusalContent::new(
        Text::new("no", "test", 128).unwrap(),
        Presence::Value(probs.clone()),
    )
    .unwrap();
    assert!(
        refusal
            .clone()
            .replace_text(Text::new("no", "test", 128).unwrap())
            .logprobs()
            .value()
            .is_some()
    );
    assert!(
        refusal
            .clone()
            .replace_text(Text::new("changed", "test", 128).unwrap())
            .logprobs()
            .is_absent()
    );
    let plain = TextContent::new(
        Text::new("no", "test", 128).unwrap(),
        vec![],
        Presence::Value(probs.clone()),
    )
    .unwrap();
    assert!(
        plain
            .clone()
            .replace_text(Text::new("changed", "test", 128).unwrap())
            .logprobs()
            .is_absent()
    );
    let metadata = ResponseMetadata {
        id: "c".into(),
        model: "m".into(),
        created: 1.into(),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
    for content in [ContentPart::Text(plain), ContentPart::Refusal(refusal)] {
        let kind = if matches!(content, ContentPart::Text(_)) {
            PartKind::Text
        } else {
            PartKind::Refusal
        };
        let key = if kind == PartKind::Text {
            "content"
        } else {
            "refusal"
        };
        let message = Item::Message(Message {
            role: MessageRole::Assistant,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![Part {
                id: PartId::new(9),
                content,
            }],
        });
        let response =
            GenerationResponse::new(vec![(ItemId::new(7), message)], Outcome::Completed).unwrap();
        let lowered = morphiecore::lowering::generation::lower_response(
            &response,
            &fidelity,
            &metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        let wire = morphiecore::protocol::openai::chat_envelope::encode_response(&lowered).unwrap();
        assert_eq!(
            wire["choices"][0]["logprobs"][key],
            json!([probability("no")])
        );
        let events = [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item: ItemId::new(7),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            StreamEvent::PartStarted {
                item: ItemId::new(7),
                part: PartId::new(9),
                kind,
            },
            StreamEvent::Delta {
                item: ItemId::new(7),
                part: PartId::new(9),
                fragment: "no".into(),
                logprobs: probs.clone(),
            },
            StreamEvent::ValueFinished {
                item: ItemId::new(7),
                part: PartId::new(9),
            },
            StreamEvent::TextMetadata {
                item: ItemId::new(7),
                part: PartId::new(9),
                annotations: vec![],
                logprobs: Presence::Value(probs.clone()),
            },
            StreamEvent::PartFinished {
                item: ItemId::new(7),
                part: PartId::new(9),
            },
            StreamEvent::ItemFinished {
                item: ItemId::new(7),
                status: ItemLifecycle::Completed,
                replay: None,
            },
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        ];
        let mut encoder = EventEncoder::new(Profile::Chat, metadata.clone()).unwrap();
        let mut reported = Vec::new();
        for event in events {
            for chunk in encoder.encode(&event, &fidelity).unwrap() {
                if let Some(values) = chunk
                    .pointer(&format!("/choices/0/logprobs/{key}"))
                    .and_then(Value::as_array)
                {
                    reported.extend(values.clone());
                }
            }
        }
        assert_eq!(reported, vec![probability("no")]);
        encoder.finish().unwrap();
        let mut items = response.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            unreachable!()
        };
        message.parts[0].content = match &message.parts[0].content {
            ContentPart::Text(text) => {
                ContentPart::Text(text.clone().with_logprobs(Presence::Absent).unwrap())
            }
            ContentPart::Refusal(text) => {
                ContentPart::Refusal(text.clone().with_logprobs(Presence::Absent).unwrap())
            }
            _ => unreachable!(),
        };
        let edited = response.with_items(items).unwrap();
        let lowered = morphiecore::lowering::generation::lower_response(
            &edited,
            &fidelity,
            &metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        assert!(morphiecore::protocol::openai::chat_envelope::encode_response(&lowered).unwrap()["choices"][0].get("logprobs").is_none());
    }
}

#[test]
fn chat_stream_metadata_and_probability_retractions_fail_before_success() {
    use morphiecore::semantic::value::Text;
    let decoded = adapter(Profile::Chat)
        .decode_response(
            response(false, json!({"content":[probability("ok")]}))
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let mut metadata = decoded.metadata.clone();
    metadata.context.execution.metadata =
        Presence::Value([("tag".into(), "reported".into())].into());
    assert!(EventEncoder::new(Profile::Chat, metadata.clone()).is_err());
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone()).unwrap();
    assert!(encoder.update_metadata(metadata).is_err());
    assert!(encoder.finish().is_err());
    for next in [Presence::Absent, Presence::Value(vec![])] {
        let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone()).unwrap();
        for event in [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Text,
            },
            StreamEvent::Delta {
                item: ItemId::new(1),
                part: PartId::new(1),
                fragment: "ok".into(),
                logprobs: serde_json::from_value(json!([probability("ok")])).unwrap(),
            },
            StreamEvent::ValueFinished {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
        ] {
            encoder.encode(&event, &decoded.fidelity).unwrap();
        }
        assert!(
            encoder
                .encode(
                    &StreamEvent::TextMetadata {
                        item: ItemId::new(1),
                        part: PartId::new(1),
                        annotations: vec![],
                        logprobs: next
                    },
                    &decoded.fidelity
                )
                .is_err()
        );
        assert!(
            encoder
                .encode(
                    &StreamEvent::Terminal {
                        terminal: StreamTerminal::Completed,
                        details: TerminalDetails::default()
                    },
                    &decoded.fidelity
                )
                .is_err()
        );
    }
    let oversized = RefusalContent::new(
        Text::new("no", "test", 128).unwrap(),
        Presence::Value(
            serde_json::from_value(json!(vec![probability("no"); MAX_ITEMS + 1])).unwrap(),
        ),
    );
    assert!(oversized.is_err());
}

#[test]
fn complete_text_probability_facts_have_an_independent_responses_projection() {
    let mut probability = probability("ok");
    probability["bytes"] = json!([111, 107]);
    let decoded = adapter(Profile::Chat)
        .decode_response(
            response(false, json!({"content":[probability.clone()]}))
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let projected = adapter(Profile::Responses)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        projected["output"][0]["content"][0],
        json!({"type":"output_text","text":"ok","annotations":[],"logprobs":[probability]})
    );
    let decoded = adapter(Profile::Responses)
        .decode_response(projected.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        adapter(Profile::Chat)
            .encode_response(&decoded, &Contract::full())
            .unwrap()["choices"][0]["logprobs"]["content"],
        json!([probability])
    );
}
