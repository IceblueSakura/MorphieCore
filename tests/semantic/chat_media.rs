//! Independent standard Chat media/citation oracles; never use a live credential or catalog.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"synthetic-response","object":"chat.completion.chunk","created":1,"model":"synthetic-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
#[test]
fn audio_stream_closes_only_complete_artifact_and_preserves_bytes() {
    let mut decoder = standard(Profile::Chat).event_decoder();
    let frames = [
        chunk(
            json!({"role":"assistant","audio":{"id":"synthetic-audio"}}),
            Value::Null,
        ),
        chunk(
            json!({"audio":{"data":"AQ","transcript":"hel"}}),
            Value::Null,
        ),
        chunk(
            json!({"audio":{"data":"ID","transcript":"lo"}}),
            Value::Null,
        ),
        chunk(json!({"audio":{"expires_at":2000}}), Value::Null),
        chunk(json!({}), json!("stop")),
    ];
    for frame in &frames {
        decoder.push(frame).unwrap();
    }
    assert!(decoder.finish().is_err());
    decoder.done().unwrap();
    let decoded = decoder.materialize().unwrap();
    let wire = standard(Profile::Chat)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        wire["choices"][0]["message"]["audio"],
        json!({"id":"synthetic-audio","data":"AQID","transcript":"hello","expires_at":2000})
    );
    for omitted in [0, 1, 3, 4] {
        let mut bad = standard(Profile::Chat).event_decoder();
        let result = (|| {
            for (n, frame) in frames.iter().enumerate() {
                if n != omitted {
                    bad.push(frame)?;
                }
            }
            bad.done()
        })();
        assert!(result.is_err());
    }
    let mut late = standard(Profile::Chat).event_decoder();
    for frame in &frames[..4] {
        late.push(frame).unwrap();
    }
    assert!(
        late.push(&chunk(json!({"audio":{"data":"BA=="}}), Value::Null))
            .is_err()
    );
}
#[test]
fn reported_audio_usage_is_typed_and_never_erased_for_another_profile() {
    let mut wire = completion(json!({"role":"assistant","content":"hello"}));
    wire["usage"] = json!({"prompt_tokens":10,"completion_tokens":6,"total_tokens":16,"prompt_tokens_details":{"audio_tokens":3},"completion_tokens_details":{"audio_tokens":4}});
    let decoded = standard(Profile::Chat)
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let encoded = standard(Profile::Chat)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(encoded["usage"], wire["usage"]);
    assert!(
        standard(Profile::Responses)
            .encode_response(&decoded, &Contract::full())
            .is_err()
    );
}
#[test]
fn audio_history_requires_source_clock_and_final_owner_not_just_an_id() {
    use morphiecore::semantic::value::ReplayOrigin;
    let origin = ReplayOrigin::new("synthetic-audio-source").unwrap();
    let source = Adapter::new(Profile::Chat, Dialect::Standard, Some(origin.clone()));
    let wire = json!({"model":"synthetic-model","messages":[{"role":"assistant","audio":{"id":"synthetic-audio","data":"AQID","transcript":"hello","expires_at":2000}},{"role":"user","content":"Repeat it."}]});
    let request = source
        .decode_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let mut contract = Contract::full();
    contract.replay_origin = Some(origin.clone());
    contract.resource_time = Some(1000);
    let sent = source
        .encode_request(&request, "synthetic-model", &contract)
        .unwrap();
    assert_eq!(
        sent["messages"][0]["audio"],
        json!({"id":"synthetic-audio"})
    );
    let mut wrong = contract.clone();
    wrong.replay_origin = Some(ReplayOrigin::new("different").unwrap());
    assert!(
        source
            .encode_request(&request, "synthetic-model", &wrong)
            .is_err()
    );
    wrong = contract.clone();
    wrong.resource_time = None;
    assert!(
        source
            .encode_request(&request, "synthetic-model", &wrong)
            .is_err()
    );
    wrong = contract.clone();
    wrong.resource_time = Some(2000);
    assert!(
        source
            .encode_request(&request, "synthetic-model", &wrong)
            .is_err()
    );
    let mut unbound = request.clone();
    unbound.task.fidelity = Default::default();
    assert!(
        source
            .encode_request(&unbound, "synthetic-model", &contract)
            .is_err()
    );
    let mut changed = request.clone();
    let mut items = changed.task.semantic.items().to_vec();
    let Item::Message(m) = &mut items[0].1 else {
        panic!()
    };
    let ContentPart::Audio(audio) = &m.parts[0].content else {
        panic!()
    };
    let text = |s: &str| morphiecore::semantic::value::Text::new(s, "synthetic", 1024).unwrap();
    m.parts[0].content = ContentPart::Audio(
        GeneratedAudio::new(audio.reference().clone(), text("BAUG"), text("different")).unwrap(),
    );
    changed.task.semantic = changed.task.semantic.with_items(items).unwrap();
    assert!(
        source
            .encode_request(&changed, "synthetic-model", &contract)
            .is_err()
    );
    let id_only = json!({"model":"synthetic-model","messages":[{"role":"assistant","audio":{"id":"synthetic-audio"}},{"role":"user","content":"Repeat it."}]});
    let decoded = source
        .decode_request(&serde_json::to_vec(&id_only).unwrap())
        .unwrap();
    assert!(
        source
            .encode_request(&decoded, "synthetic-model", &contract)
            .is_ok()
    );
    assert!(
        standard(Profile::Chat)
            .encode_request(
                &standard(Profile::Chat)
                    .decode_request(&serde_json::to_vec(&id_only).unwrap())
                    .unwrap(),
                "synthetic-model",
                &contract
            )
            .is_err()
    );
}
#[test]
fn annotations_edits_and_unsupported_streams_do_not_lose_citations() {
    use morphiecore::{protocol::openai::events::EventEncoder, semantic::value::Text};
    let wire = completion(
        json!({"role":"assistant","content":"a😀b","annotations":[{"type":"url_citation","url_citation":{"start_index":1,"end_index":2,"title":"Example","url":"https://example.invalid/"}}]}),
    );
    let decoded = standard(Profile::Chat)
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let mut items = decoded.semantic.items().to_vec();
    let (item, Item::Message(m)) = &mut items[0] else {
        panic!()
    };
    let part = m.parts[0].id;
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    let annotation = t.annotations()[0].clone();
    m.parts[0].content = ContentPart::Text(
        t.clone()
            .replace_text(Text::new("changed", "text", 1024).unwrap()),
    );
    let item = *item;
    let mut edited = decoded.clone();
    edited.semantic = edited.semantic.with_items(items).unwrap();
    let encoded = standard(Profile::Chat)
        .encode_response(&edited, &Contract::full())
        .unwrap();
    assert!(
        encoded["choices"][0]["message"]
            .get("annotations")
            .is_none()
    );
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata).unwrap();
    for event in [
        StreamEvent::Started,
        StreamEvent::ItemStarted {
            item,
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        StreamEvent::PartStarted {
            item,
            part,
            kind: PartKind::Text,
        },
    ] {
        encoder.encode(&event, &Default::default()).unwrap();
    }
    assert!(
        encoder
            .encode(
                &StreamEvent::AnnotationAdded {
                    item,
                    part,
                    annotation
                },
                &Default::default()
            )
            .is_err()
    );
    for (pointer, value) in [
        (
            "/choices/0/message/annotations/0/url_citation/end_index",
            json!(4),
        ),
        (
            "/choices/0/message/annotations/0/url_citation/start_index",
            json!(-1),
        ),
        (
            "/choices/0/message/annotations/0/type",
            json!("file_citation"),
        ),
        ("/choices/0/message/content", Value::Null),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            standard(Profile::Chat)
                .decode_response(&serde_json::to_vec(&bad).unwrap())
                .is_err(),
            "{pointer}"
        );
    }
}
#[test]
fn audio_shapes_budgets_and_controls_fail_without_repair() {
    let wire = completion(
        json!({"role":"assistant","content":null,"audio":{"id":"synthetic-audio","data":"AQID","transcript":"","expires_at":2000}}),
    );
    for field in ["id", "data", "transcript", "expires_at"] {
        let mut bad = wire.clone();
        bad["choices"][0]["message"]["audio"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            standard(Profile::Chat)
                .decode_response(&serde_json::to_vec(&bad).unwrap())
                .is_err()
        );
    }
    for (field, value) in [
        ("data", json!("invalid base64")),
        ("expires_at", json!(-1)),
        ("expires_at", json!(1.5)),
        ("id", json!("")),
        ("transcript", Value::Null),
    ] {
        let mut bad = wire.clone();
        bad["choices"][0]["message"]["audio"][field] = value;
        assert!(
            standard(Profile::Chat)
                .decode_response(&serde_json::to_vec(&bad).unwrap())
                .is_err()
        );
    }
    let mut request = json!({"model":"synthetic-model","messages":[{"role":"user","content":"hello"}],"modalities":["audio"],"audio":{"format":"wav","voice":"alloy"}});
    request["stream"] = json!(true);
    let decoded = standard(Profile::Chat)
        .decode_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    assert!(
        standard(Profile::Chat)
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .is_err()
    );
    request["audio"]["format"] = json!("pcm16");
    let decoded = standard(Profile::Chat)
        .decode_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    assert!(
        standard(Profile::Chat)
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .is_ok()
    );
    let mut restricted = Contract::full();
    restricted.semantics.audio_output = false;
    assert!(
        standard(Profile::Chat)
            .encode_request(&decoded, "synthetic-model", &restricted)
            .is_err()
    );
    assert!(
        standard(Profile::Responses)
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .is_err()
    );
    for modalities in [
        json!([]),
        json!(["audio", "audio"]),
        json!(["video"]),
        Value::Null,
    ] {
        request["modalities"] = modalities;
        assert!(
            standard(Profile::Chat)
                .decode_request(&serde_json::to_vec(&request).unwrap())
                .is_err()
        );
    }
    let mut buffer = AudioBuffer::default();
    assert!(
        buffer
            .push(&AudioUpdate {
                data: Some(vec![0; MAX_AUDIO_DECODED_BYTES + 1]),
                ..Default::default()
            })
            .is_err()
    );
}
#[test]
fn audio_empty_text_refusal_padding_and_identity_boundaries() {
    for extra in [
        chunk(json!({"refusal":"no"}), Value::Null),
        chunk(json!({"audio":{"id":"different"}}), Value::Null),
        chunk(json!({"content":"late text"}), Value::Null),
    ] {
        let mut decoder = standard(Profile::Chat).event_decoder();
        decoder
            .push(&chunk(
                json!({"role":"assistant","audio":{"id":"a"}}),
                Value::Null,
            ))
            .unwrap();
        assert!(decoder.push(&extra).is_err());
    }
    let mut bad = standard(Profile::Chat).event_decoder();
    bad.push(&chunk(
        json!({"audio":{"id":"a","data":"AQ==","transcript":""}}),
        Value::Null,
    ))
    .unwrap();
    assert!(
        bad.push(&chunk(json!({"audio":{"data":"Ag=="}}), Value::Null))
            .is_err()
    );
    let mut good = standard(Profile::Chat).event_decoder();
    for value in [
        chunk(
            json!({"role":"assistant","content":"","audio":{"id":"a"}}),
            Value::Null,
        ),
        chunk(
            json!({"content":"hello","audio":{"data":"AQID","transcript":"hello"}}),
            Value::Null,
        ),
        chunk(json!({"audio":{"expires_at":2000}}), Value::Null),
        chunk(json!({}), json!("stop")),
    ] {
        good.push(&value).unwrap();
    }
    good.done().unwrap();
    let output = standard(Profile::Chat)
        .encode_response(&good.materialize().unwrap(), &Contract::full())
        .unwrap();
    assert_eq!(output["choices"][0]["message"]["content"], "hello");
    assert_eq!(
        output["choices"][0]["message"]["audio"]["transcript"],
        "hello"
    );
}
// The low-level typed events need an independent encoder oracle, not only decode round trips.
#[test]
fn byte_fragments_encode_as_one_base64_value() {
    use morphiecore::{protocol::openai::events::EventEncoder, semantic::value::Text};
    let t = |s: &str| Text::new(s, "synthetic", 1024).unwrap();
    let meta = morphiecore::protocol::ResponseMetadata {
        id: "r".into(),
        model: "synthetic".into(),
        created: 1.into(),
        context: Default::default(),
        instruction_fidelity: Default::default(),
    };
    let mut e = EventEncoder::new(Profile::Chat, meta).unwrap();
    let item = ItemId::new(1);
    let part = PartId::new(1);
    let mut data = String::new();
    for event in [
        StreamEvent::Started,
        StreamEvent::ItemStarted {
            item,
            kind: ItemKind::Message { phase: None },
            replay: None,
        },
        StreamEvent::PartStarted {
            item,
            part,
            kind: PartKind::Audio,
        },
        StreamEvent::AudioDelta {
            item,
            part,
            update: AudioUpdate {
                id: Some(t("a")),
                data: Some(vec![1]),
                transcript: Some(t("hello")),
                ..Default::default()
            },
        },
        StreamEvent::AudioDelta {
            item,
            part,
            update: AudioUpdate {
                data: Some(vec![2, 3, 4]),
                ..Default::default()
            },
        },
        StreamEvent::AudioDelta {
            item,
            part,
            update: AudioUpdate {
                expires_at: Some(2000),
                ..Default::default()
            },
        },
        StreamEvent::ValueFinished { item, part },
        StreamEvent::PartFinished { item, part },
        StreamEvent::ItemFinished {
            item,
            status: ItemLifecycle::Completed,
            replay: None,
        },
        StreamEvent::Terminal {
            terminal: StreamTerminal::Completed,
            details: Default::default(),
        },
    ] {
        for v in e.encode(&event, &Default::default()).unwrap() {
            if let Some(s) = v["choices"][0]["delta"]["audio"]["data"].as_str() {
                data.push_str(s)
            }
        }
    }
    assert_eq!(data, "AQIDBA==");
}

fn standard(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn completion(message: Value) -> Value {
    json!({"id":"synthetic-response","object":"chat.completion","created":1,"model":"synthetic-model","choices":[{"index":0,"message":message,"finish_reason":"stop"}]})
}
#[test]
fn chat_url_citations_are_standard_typed_and_cross_profile() {
    let wire = completion(
        json!({"role":"assistant","content":"hello","annotations":[{"type":"url_citation","url_citation":{"start_index":0,"end_index":5,"title":"Synthetic","url":"https://example.invalid/source"}}]}),
    );
    let decoded = standard(Profile::Chat)
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let (_, item) = &decoded.semantic.items()[0];
    let Item::Message(message) = item else {
        panic!()
    };
    let ContentPart::Text(text) = &message.parts[0].content else {
        panic!()
    };
    assert_eq!(
        text.annotations(),
        &[Annotation::UrlCitation {
            start_index: 0,
            end_index: 5,
            title: "Synthetic".into(),
            url: "https://example.invalid/source".into()
        }]
    );
    let encoded = standard(Profile::Chat)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["choices"][0]["message"]["annotations"],
        wire["choices"][0]["message"]["annotations"]
    );
    let responses = standard(Profile::Responses)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        responses["output"][0]["content"][0]["annotations"][0],
        json!({"type":"url_citation","start_index":0,"end_index":5,"title":"Synthetic","url":"https://example.invalid/source"})
    );
}
#[test]
fn chat_audio_complete_values_and_generation_controls_are_standard() {
    let request = json!({"model":"synthetic-model","messages":[{"role":"user","content":"Say hello."}],"modalities":["text","audio"],"audio":{"format":"wav","voice":"alloy"}});
    let decoded = standard(Profile::Chat)
        .decode_request(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let encoded = standard(Profile::Chat)
        .encode_request(&decoded, "synthetic-model", &Contract::full())
        .unwrap();
    assert_eq!(encoded["modalities"], json!(["text", "audio"]));
    assert_eq!(encoded["audio"], json!({"format":"wav","voice":"alloy"}));
    let wire = completion(
        json!({"role":"assistant","content":null,"audio":{"id":"synthetic-audio","data":"AQID","transcript":"hello","expires_at":2000}}),
    );
    let audio = standard(Profile::Chat)
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let encoded = standard(Profile::Chat)
        .encode_response(&audio, &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["choices"][0]["message"]["audio"],
        wire["choices"][0]["message"]["audio"]
    );
    assert!(
        standard(Profile::Responses)
            .encode_response(&audio, &Contract::full())
            .is_err()
    );
}
#[test]
fn chat_inactive_standard_fields_do_not_need_a_provider_rule() {
    let wire =
        completion(json!({"role":"assistant","content":"hello","annotations":null,"audio":null}));
    let decoded = standard(Profile::Chat)
        .decode_response(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    assert_eq!(decoded.semantic.items().len(), 1);
}
