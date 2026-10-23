//! Independent standard Chat media/citation oracles; never use a live credential or catalog.
use morphiecore::{
    protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"synthetic-response","object":"chat.completion.chunk","created":1,"model":"synthetic-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}
#[test]
fn audit_responses_rejects_credential_owned_locators_even_with_a_public_source() {
    use morphiecore::protocol::CodecError;
    let adapter = standard(Profile::Responses);
    for part in [
        json!({"type":"input_image","image_url":"https://example.test/a.png","detail":"auto"}),
        json!({"type":"input_file","file_url":"https://example.test/a.pdf"}),
    ] {
        let body = json!({"model":"m","input":[{"role":"user","content":[part]}]});
        assert!(
            adapter
                .decode_client_request(body.to_string().as_bytes())
                .is_ok()
        );
        let mut nullable = body.clone();
        nullable["input"][0]["content"][0]["file_id"] = Value::Null;
        assert!(
            adapter
                .decode_client_request(nullable.to_string().as_bytes())
                .is_ok()
        );
        assert!(
            adapter
                .decode_request(nullable.to_string().as_bytes())
                .is_ok()
        );
        for locator in [json!("file-other"), json!(7)] {
            let mut mixed = body.clone();
            mixed["input"][0]["content"][0]["file_id"] = locator;
            assert!(matches!(
                adapter.decode_client_request(mixed.to_string().as_bytes()),
                Err(CodecError::Unsupported(field)) if field == "Responses file_id"
            ));
        }
    }
}
#[test]
fn input_audio_is_an_ordered_inline_resource_not_a_transcript_or_assistant_artifact() {
    let adapter = standard(Profile::Chat);
    for (format, mime) in [("wav", "audio/wav"), ("mp3", "audio/mpeg")] {
        let wire = json!({"model":"synthetic-model","messages":[{"role":"user","content":[
            {"type":"text","text":"Listen."},
            {"type":"input_audio","input_audio":{"data":"AQID","format":format}},
            {"type":"text","text":"Answer with text."}]}]});
        let decoded = adapter
            .decode_request(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(
            GenerationRequirements::derive(&decoded.task.semantic).audio_inputs,
            1
        );
        let Item::Message(message) = &decoded.task.semantic.items()[0].1 else {
            panic!("message")
        };
        let ContentPart::Resource(audio) = &message.parts[1].content else {
            panic!("audio resource")
        };
        let view = audio.media(decoded.task.semantic.resources()).unwrap();
        assert_eq!(view.kind(), ResourceKind::Audio);
        assert!(
            matches!(view.location, ResourceLocation::Inline{media_type,data_base64}
            if media_type.as_str()==mime && data_base64.as_str()=="AQID")
        );
        let output = adapter
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .unwrap();
        assert_eq!(output["messages"], wire["messages"]);
        let mut edited = decoded.clone();
        let replacement = ResourceTable::default()
            .insert(
                audio.id,
                ResourceLocation::Inline {
                    media_type: morphiecore::semantic::value::Text::new(mime, "mime", 64).unwrap(),
                    data_base64: morphiecore::semantic::value::Text::new("BAUG", "audio", 100)
                        .unwrap(),
                },
            )
            .unwrap();
        edited.task.semantic = edited
            .task
            .semantic
            .clone()
            .with_resources(replacement)
            .unwrap();
        let updated = adapter
            .encode_request(&edited, "synthetic-model", &Contract::full())
            .unwrap();
        assert_eq!(
            updated["messages"][0]["content"][1]["input_audio"]["data"],
            "BAUG"
        );
        let mut items = decoded.task.semantic.items().to_vec();
        let Item::Message(message) = &mut items[0].1 else {
            panic!("message")
        };
        message.parts.remove(1);
        message.parts.reverse();
        edited.task.semantic = decoded.task.semantic.clone().with_items(items).unwrap();
        let updated = adapter
            .encode_request(&edited, "synthetic-model", &Contract::full())
            .unwrap();
        assert_eq!(
            updated["messages"][0]["content"],
            json!([
            {"type":"text","text":"Answer with text."},{"type":"text","text":"Listen."}])
        );
        assert_eq!(
            GenerationRequirements::derive(&edited.task.semantic).audio_inputs,
            0
        );
        assert!(
            standard(Profile::Responses)
                .encode_request(&decoded, "synthetic-model", &Contract::full())
                .is_err()
        );
        let mut unsupported = Contract::full();
        unsupported.semantics.audio_input = false;
        assert!(
            adapter
                .encode_request(&decoded, "synthetic-model", &unsupported)
                .is_err()
        );
        for role in ["assistant", "system", "developer", "tool"] {
            let mut bad = wire.clone();
            bad["messages"][0]["role"] = json!(role);
            assert!(
                adapter
                    .decode_request(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
        for value in [
            json!({"data":"AQID","format":"flac"}),
            json!({"data":"https://example.invalid/a.wav","format":"wav"}),
            json!({"data":"","format":"wav"}),
            json!({"data":"AQID","format":"wav","extra":1}),
            json!({"data":"AQID","format":null}),
        ] {
            let mut bad = wire.clone();
            bad["messages"][0]["content"][1]["input_audio"] = value;
            assert!(
                adapter
                    .decode_request(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
    }
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
#[test]
fn chat_message_parts_allow_only_text_then_audio() {
    use morphiecore::protocol::openai::lowering::{RepresentationError, events::check_event};
    let contract = Contract::full();
    let item = ItemId::new(1);
    for first in [PartKind::Text, PartKind::Audio, PartKind::Refusal] {
        let mut state = StreamState::new();
        for event in [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            StreamEvent::PartStarted {
                item,
                part: PartId::new(1),
                kind: first,
            },
        ] {
            check_event(&state, &event, Profile::Chat, &contract).unwrap();
            state = reduce(state, event).unwrap();
        }
        for second in [PartKind::Text, PartKind::Audio, PartKind::Refusal] {
            let event = StreamEvent::PartStarted {
                item,
                part: PartId::new(2),
                kind: second,
            };
            let expected = if (first, second) == (PartKind::Text, PartKind::Audio) {
                Ok(())
            } else {
                Err(RepresentationError::MessageGrouping)
            };
            assert_eq!(
                check_event(&state, &event, Profile::Chat, &contract),
                expected,
                "{first:?} -> {second:?}"
            );
        }
    }
}
// The low-level typed events need an independent encoder oracle, not only decode round trips.
#[test]
fn byte_fragments_encode_as_one_base64_value() {
    use morphiecore::{protocol::openai::events::EventEncoder, semantic::value::Text};
    let t = |s: &str| Text::new(s, "synthetic", 1024).unwrap();
    let meta = morphiecore::protocol::ResponseMetadata {
        delivery: None,
        storage: None,
        id: Some("r".into()),
        model: "synthetic".into(),
        created: Some(1.into()),
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
#[test]
fn video_url_is_a_video_resource_only_for_declaring_chat_dialects() {
    let wire = json!({"model":"synthetic-model","messages":[{"role":"user","content":[
            {"type":"video_url","video_url":{"url":"https://example.invalid/clip.mp4"}},
            {"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}},
            {"type":"text","text":"Describe both."}]}]});
    let decoded = standard(Profile::Chat)
        .decode_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let semantic = &decoded.task.semantic;
    assert_eq!(GenerationRequirements::derive(semantic).video_inputs, 2);
    let Item::Message(message) = &semantic.items()[0].1 else {
        panic!("message")
    };
    let views: Vec<_> = message.parts[..2]
        .iter()
        .map(|p| {
            let ContentPart::Resource(r) = &p.content else {
                panic!("resource")
            };
            r.media(semantic.resources()).unwrap()
        })
        .collect();
    assert!(views.iter().all(|v| v.kind() == ResourceKind::Video));
    assert!(
        matches!(views[0].location, ResourceLocation::Url(u) if u.as_str()=="https://example.invalid/clip.mp4")
    );
    assert!(
        matches!(views[1].location, ResourceLocation::Inline{media_type,data_base64}
                if media_type.as_str()=="video/mp4" && data_base64.as_str()=="AQID")
    );
    let modelbest = Adapter::new(Profile::Chat, Dialect::ModelBest, None);
    let output = modelbest
        .encode_request(&decoded, "synthetic-model", &Contract::full())
        .unwrap();
    assert_eq!(output["messages"], wire["messages"]);
    for adapter in [standard(Profile::Chat), standard(Profile::Responses)] {
        assert!(
            adapter
                .encode_request(&decoded, "synthetic-model", &Contract::full())
                .is_err()
        );
    }
    let mut mislabeled = wire.clone();
    mislabeled["messages"][0]["content"][1]["video_url"]["url"] =
        json!("data:image/png;base64,AQID");
    assert!(
        standard(Profile::Chat)
            .decode_request(&serde_json::to_vec(&mislabeled).unwrap())
            .is_err()
    );
}
#[test]
fn input_video_reaches_only_declaring_responses_dialects() {
    let content = json!([
            {"type":"input_video","video_url":"data:video/mp4;base64,AQID"},
            {"type":"input_text","text":"Describe it."}]);
    let wire = json!({"model":"synthetic-model","input":[
            {"type":"message","role":"user","content":content}]});
    let decoded = standard(Profile::Responses)
        .decode_request(&serde_json::to_vec(&wire).unwrap())
        .unwrap();
    let semantic = &decoded.task.semantic;
    assert_eq!(GenerationRequirements::derive(semantic).video_inputs, 1);
    let Item::Message(message) = &semantic.items()[0].1 else {
        panic!("message")
    };
    let ContentPart::Resource(video) = &message.parts[0].content else {
        panic!("video")
    };
    let view = video.media(semantic.resources()).unwrap();
    assert_eq!(view.kind(), ResourceKind::Video);
    assert!(
        matches!(view.location, ResourceLocation::Inline { media_type, data_base64 }
            if media_type.as_str() == "video/mp4" && data_base64.as_str() == "AQID")
    );
    let openrouter = |profile| Adapter::new(profile, Dialect::OpenRouter, None);
    let output = openrouter(Profile::Responses)
        .encode_request(&decoded, "synthetic-model", &Contract::full())
        .unwrap();
    assert_eq!(output["input"][0]["content"], content);
    let output = openrouter(Profile::Chat)
        .encode_request(&decoded, "synthetic-model", &Contract::full())
        .unwrap();
    assert_eq!(
        output["messages"][0]["content"][0],
        json!({"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}})
    );
    assert!(
        standard(Profile::Responses)
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .is_err()
    );
    // Tool-result video decodes, but OpenRouter's function_call_output has no video part.
    let tool = json!({"model":"synthetic-model","input":[
            {"type":"function_call","call_id":"call_1","name":"record","arguments":"{}"},
            {"type":"function_call_output","call_id":"call_1","output":[
                {"type":"input_video","video_url":"https://example.invalid/clip.mp4"}]}],
            "tools":[{"type":"function","name":"record","parameters":{"type":"object"}}]});
    let decoded = standard(Profile::Responses)
        .decode_request(&serde_json::to_vec(&tool).unwrap())
        .unwrap();
    let needs = GenerationRequirements::derive(&decoded.task.semantic);
    assert_eq!((needs.tool_result_media, needs.video_inputs), (1, 1));
    assert!(
        openrouter(Profile::Responses)
            .encode_request(&decoded, "synthetic-model", &Contract::full())
            .is_err()
    );
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
    assert_eq!(text.annotations().len(), 1);
    let citation = text.annotations()[0].value();
    assert_eq!(
        citation.claim,
        ClaimAnchor::Range(TextRange {
            unit: TextUnit::UnicodeScalars,
            start: 0,
            end: 5
        })
    );
    assert_eq!(citation.kind, CitationKind::Citation);
    assert_eq!(citation.coordinates, SourceCoordinates::Unreported);
    assert_eq!(citation.label.as_ref().unwrap().as_str(), "Synthetic");
    assert_eq!(citation.reported_ordinal, None);
    assert!(
        matches!(decoded.semantic.resources().get(citation.source).unwrap(),
        ResourceLocation::Url(url) if url.as_str() == "https://example.invalid/source")
    );
    let encoded = standard(Profile::Chat)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["choices"][0]["message"]["annotations"],
        wire["choices"][0]["message"]["annotations"]
    );
    // Responses omits the ambiguous stop cause by rule; the citation keeps its carrier.
    let responses = standard(Profile::Responses)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(responses["status"], "completed");
    assert_eq!(
        responses["output"][0]["content"][0]["annotations"],
        json!([{"type":"url_citation","start_index":0,"end_index":5,
                "title":"Synthetic","url":"https://example.invalid/source"}])
    );
    assert_eq!(
        decoded.semantic.details().completion,
        Some(CompletionCause::NaturalOrStopSequence)
    );
    // Returned history carries the citation independently of the terminal.
    let history = GenerationRequest::from_resources(
        decoded.semantic.items().to_vec(),
        GenerationSettings::default(),
        decoded.semantic.resources().clone(),
    )
    .unwrap();
    let target = morphiecore::protocol::openai::lowering::lower_request(
        &history,
        &decoded.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let responses = morphiecore::protocol::openai::responses::encode_generation(&target).unwrap();
    assert_eq!(
        responses["input"][0]["content"][0]["annotations"][0],
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
