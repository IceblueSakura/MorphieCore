//! Chat media, probabilities, citations, files and client carriers.
mod chat_media {
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
                    ResourceDeclaration {
                        body: ResourceBody::Media(ResourceLocation::Inline {
                            media_type: morphiecore::semantic::value::Text::new(mime, "mime", 64)
                                .unwrap(),
                            data_base64: morphiecore::semantic::value::Text::new(
                                "BAUG", "audio", 100,
                            )
                            .unwrap(),
                        }),
                        conditions: Default::default(),
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
            GeneratedAudio::new(audio.reference().clone(), text("BAUG"), text("different"))
                .unwrap(),
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
    // The low-level typed events need an independent encoder oracle, not only decode round trips.
    #[test]
    fn byte_fragments_encode_as_one_base64_value() {
        use morphiecore::{protocol::openai::events::EventEncoder, semantic::value::Text};
        let t = |s: &str| Text::new(s, "synthetic", 1024).unwrap();
        let meta = morphiecore::protocol::ResponseMetadata {
            id: "r".into(),
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
            matches!(&decoded.semantic.resources().get(citation.source).unwrap().body,
        ResourceBody::Media(ResourceLocation::Url(url)) if url.as_str() == "https://example.invalid/source")
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
}
mod chat_logprobs {
    //! Independent Chat probability and context oracles against the pinned SDK contract.
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract};
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
}
mod citation_bindings {
    use morphiecore::semantic::{
        task::generation::*,
        value::{Presence, Text},
    };
    fn text(s: &str) -> Text {
        Text::allowing_empty(s, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn sources(s: &str) -> ResourceTable {
        ResourceTable::new(vec![(
            ResourceId::new(1),
            ResourceDeclaration {
                body: ResourceBody::Text(text(s)),
                conditions: Default::default(),
            },
        )])
        .unwrap()
    }
    fn cite(table: &ResourceTable, coordinates: SourceCoordinates) -> Annotation {
        Annotation::new(
            Citation {
                kind: CitationKind::Citation,
                claim: ClaimAnchor::Whole,
                source: ResourceId::new(1),
                coordinates,
                label: None,
                reported_ordinal: None,
            },
            table,
        )
        .unwrap()
    }
    fn message(id: u64, annotations: Vec<Annotation>) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::Message(Message {
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part {
                    id: PartId::new(id),
                    replay: None,
                    content: ContentPart::Text(
                        TextContent::new(text("claim"), annotations, Presence::Absent).unwrap(),
                    ),
                }],
            }),
        )
    }
    #[test]
    fn source_coordinate_shapes_do_not_invent_document_extents_or_wire_carriers() {
        use morphiecore::protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps, lowering::lower_response},
        };
        let table = ResourceTable::new(vec![(
            ResourceId::new(1),
            ResourceDeclaration {
                body: ResourceBody::Media(ResourceLocation::OpaqueReference(text(
                    "synthetic-document",
                ))),
                conditions: Default::default(),
            },
        )])
        .unwrap();
        let native = Citation {
            kind: CitationKind::Citation,
            claim: ClaimAnchor::Unreported,
            source: ResourceId::new(1),
            coordinates: SourceCoordinates::Unreported,
            label: Some(text("document")),
            reported_ordinal: Some(0),
        };
        let metadata =
            morphiecore::protocol::openai::envelope::decode_response(&crate::wire::response(2))
                .unwrap()
                .metadata;
        let positive = GenerationResponse::from_resources(
            vec![message(
                1,
                vec![Annotation::new(native.clone(), &table).unwrap()],
            )],
            Outcome::Completed,
            table.clone(),
        )
        .unwrap();
        assert!(
            lower_response(
                &positive,
                &FidelityRecords::default(),
                &metadata,
                Profile::Responses,
                TargetCaps::full()
            )
            .is_ok()
        );
        for coordinates in [
            SourceCoordinates::Pages { start: 1, end: 3 },
            SourceCoordinates::Blocks { start: 0, end: 2 },
            SourceCoordinates::Text(TextRange {
                unit: TextUnit::UnicodeScalars,
                start: 0,
                end: 999,
            }),
        ] {
            let annotation = Annotation::new(
                Citation {
                    coordinates,
                    ..native.clone()
                },
                &table,
            )
            .unwrap();
            assert_eq!(
                annotation.source_bounds(&table).unwrap(),
                SourceBounds::Unreported
            );
            let response = GenerationResponse::from_resources(
                vec![message(1, vec![annotation])],
                Outcome::Completed,
                table.clone(),
            )
            .unwrap();
            for profile in [Profile::Responses, Profile::Chat] {
                assert!(
                    lower_response(
                        &response,
                        &FidelityRecords::default(),
                        &morphiecore::protocol::openai::envelope::decode_response(
                            &crate::wire::response(2)
                        )
                        .unwrap()
                        .metadata,
                        profile,
                        TargetCaps::full()
                    )
                    .is_err()
                );
            }
        }
        for coordinates in [
            SourceCoordinates::Pages { start: 0, end: 2 },
            SourceCoordinates::Blocks { start: 2, end: 2 },
            SourceCoordinates::Text(TextRange {
                unit: TextUnit::Utf8Bytes,
                start: 5,
                end: 4,
            }),
        ] {
            let mut value = cite(&table, SourceCoordinates::Unreported).value().clone();
            value.coordinates = coordinates;
            assert!(Annotation::new(value, &table).is_err());
        }
        assert!(
            TextRange {
                unit: TextUnit::Utf8Bytes,
                start: 1,
                end: 5
            }
            .validate("A😀B")
            .is_ok()
        );
        assert!(
            TextRange {
                unit: TextUnit::UnicodeScalars,
                start: 1,
                end: 5
            }
            .validate("A😀B")
            .is_err()
        );
    }
    #[test]
    fn source_selection_edits_replay_cache_and_original_usage_share_one_dependency_owner() {
        use morphiecore::{
            protocol::fidelity::FidelityRecords,
            semantic::{
                cache::{CachePrefixContext, CachePrefixIntent, CachePrefixScope},
                context::ExecutionHints,
                value::ReplayOrigin,
            },
        };
        let table = sources("A😀B");
        let annotation = cite(
            &table,
            SourceCoordinates::Text(TextRange {
                unit: TextUnit::UnicodeScalars,
                start: 0,
                end: 3,
            }),
        );
        let mut owner = message(1, vec![annotation]);
        let Item::Message(m) = &mut owner.1 else {
            panic!()
        };
        m.parts[0].replay = Some(ReplayValue::final_value(
            ReplayFormat::GoogleGenerateContentPart,
            text("synthetic-token"),
        ));
        let unused = ResourceDeclaration {
            body: ResourceBody::Text(text("unused")),
            conditions: Default::default(),
        };
        let response = GenerationResponse::from_resources(
            vec![owner],
            Outcome::Completed,
            table
                .clone()
                .insert(ResourceId::new(2), unused.clone())
                .unwrap(),
        )
        .unwrap()
        .with_usage(Usage::operation(3, 5, 8))
        .unwrap();
        let request = ClientManaged::new(Default::default())
            .unwrap()
            .select_response(&response)
            .unwrap()
            .build(&[])
            .unwrap();
        assert_eq!(request.resources().len(), 1);
        let replay_owner = ReplayOwner::Part {
            item: ItemId::new(1),
            part: PartId::new(1),
        };
        let origin = ReplayOrigin::new("synthetic-source").unwrap();
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_attachment(replay_owner, &request, origin.clone())
            .unwrap();
        let dependency = RequestDependencyProof::capture(
            &request,
            HistoryDependency::Owners(vec![ItemId::new(1)]),
            SettingsDependency::All,
        )
        .unwrap();
        fidelity
            .bind_attachment_dependency(replay_owner, dependency, &request)
            .unwrap();
        let hints = ExecutionHints::default();
        let context = CachePrefixContext {
            model: "synthetic",
            hints: &hints,
            grouping: None,
            conversation: None,
        };
        let scope = CachePrefixScope::new("synthetic").unwrap();
        let cache = CachePrefixIntent::through(ItemId::new(1))
            .capture(&request, context, &scope)
            .unwrap();
        let unrelated = request
            .clone()
            .with_resources(table.clone().insert(ResourceId::new(2), unused).unwrap())
            .unwrap();
        assert!(cache.check(&unrelated, context, &scope).is_ok());
        assert!(fidelity.attachment_matches(replay_owner, &unrelated, Some(&origin)));
        assert!(
            request
                .clone()
                .with_resources(sources("replacement"))
                .is_err()
        );
        assert!(
            request
                .clone()
                .with_resources(ResourceTable::default())
                .is_err()
        );
        for next_table in [table.clone(), sources("replacement")] {
            let mut items = request.items().to_vec();
            let Item::Message(m) = &mut items[0].1 else {
                panic!()
            };
            let ContentPart::Text(t) = &m.parts[0].content else {
                panic!()
            };
            let mut value = t.annotations()[0].value().clone();
            value.label = Some(text("changed label"));
            m.parts[0].content = ContentPart::Text(
                TextContent::new(
                    text(t.as_str()),
                    vec![Annotation::new(value, &next_table).unwrap()],
                    Presence::Absent,
                )
                .unwrap(),
            );
            let rebound =
                GenerationRequest::from_resources(items, request.settings().clone(), next_table)
                    .unwrap();
            assert!(cache.check(&rebound, context, &scope).is_err());
            assert!(!fidelity.attachment_matches(replay_owner, &rebound, Some(&origin)));
        }
        let mut changed_conditions = table.get(ResourceId::new(1)).unwrap().clone();
        changed_conditions.conditions.expires_at = Some(99);
        assert!(
            request
                .clone()
                .with_resources(
                    ResourceTable::new(vec![(ResourceId::new(1), changed_conditions)]).unwrap()
                )
                .is_err()
        );
        let mut items = response.items().to_vec();
        let Item::Message(m) = &mut items[0].1 else {
            panic!()
        };
        let ContentPart::Text(t) = &m.parts[0].content else {
            panic!()
        };
        m.parts[0].content = ContentPart::Text(t.clone().replace_text(text("edited claim")));
        let edited = response.clone().with_items(items).unwrap();
        assert_eq!(edited.usage_reports(), response.usage_reports());
        let branch = ClientManaged::new(Default::default())
            .unwrap()
            .select_response(&edited)
            .unwrap()
            .build(&[])
            .unwrap();
        assert!(branch.resources().is_empty());
        assert!(cache.check(&branch, context, &scope).is_err());
        assert!(!fidelity.attachment_matches(replay_owner, &branch, Some(&origin)));
        // A second branch changes configuration, not either citation anchor.
        let mut settings = request.settings().clone();
        settings.instructions = Presence::Value(text("branch configuration"));
        let configured = request
            .clone()
            .with_configuration(
                ConfigurationSnapshot::new(ConfigurationId::new(LocalScope::new(9), 2), settings)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(configured.items(), request.items());
        assert_eq!(configured.resources(), request.resources());
        assert!(cache.check(&configured, context, &scope).is_err());
        assert!(!fidelity.attachment_matches(replay_owner, &configured, Some(&origin)));
        let recaptured = RequestDependencyProof::capture(
            &configured,
            HistoryDependency::Owners(vec![ItemId::new(1)]),
            SettingsDependency::All,
        )
        .unwrap();
        assert!(
            fidelity
                .bind_attachment_dependency(replay_owner, recaptured, &configured)
                .is_err()
        );
        assert!(fidelity.attachment_matches(replay_owner, &request, Some(&origin)));
        for candidate in [&request, &configured] {
            for profile in [
                morphiecore::protocol::openai::Profile::Chat,
                morphiecore::protocol::openai::Profile::Responses,
            ] {
                let mut contract = morphiecore::protocol::openai::TargetCaps::full();
                contract.replay_origin = Some(origin.clone());
                assert!(
                    morphiecore::protocol::openai::lowering::lower_request(
                        candidate, &fidelity, profile, contract
                    )
                    .is_err()
                );
            }
        }
        assert_eq!(response.usage().unwrap().output_tokens, Some(5));
    }
    #[test]
    fn native_file_reports_preserve_list_ordinals_and_namespaced_identity_without_fake_claims() {
        use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
        use serde_json::json;
        let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let mut wire = crate::wire::response(2);
        let annotations = json!([
            {"type":"file_citation","file_id":"synthetic-file","filename":"","index":999},
            {"type":"container_file_citation","container_id":"synthetic-container","file_id":"synthetic-file","filename":"report","start_index":0,"end_index":1},
            {"type":"file_path","file_id":"synthetic-artifact","index":7}
        ]);
        wire["output"][0]["content"][0]["annotations"] = annotations.clone();
        let decoded = adapter
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(decoded.semantic.resources().len(), 3);
        let Item::Message(m) = &decoded.semantic.items()[0].1 else {
            panic!()
        };
        let ContentPart::Text(t) = &m.parts[0].content else {
            panic!()
        };
        assert_eq!(t.annotations()[0].value().claim, ClaimAnchor::Unreported);
        assert_eq!(t.annotations()[0].value().reported_ordinal, Some(999));
        assert_eq!(
            t.annotations()[0].value().label.as_ref().unwrap().as_str(),
            ""
        );
        assert_eq!(
            t.annotations()[2].value().kind,
            CitationKind::ArtifactReference
        );
        assert!(
            matches!(&decoded.semantic.resources().get(t.annotations()[1].value().source).unwrap().body,
        ResourceBody::Media(ResourceLocation::NamespacedReference { namespace, id })
        if namespace.as_str()=="synthetic-container" && id.as_str()=="synthetic-file")
        );
        let encoded = adapter
            .encode_response(&decoded, &TargetCaps::full())
            .unwrap();
        assert_eq!(
            encoded["output"][0]["content"][0]["annotations"],
            annotations
        );
        let request = ClientManaged::new(Default::default())
            .unwrap()
            .select_response(&decoded.semantic)
            .unwrap()
            .build(&[])
            .unwrap();
        let needs = GenerationRequirements::derive(&request);
        assert_eq!(needs.file_inputs, 0);
        assert_eq!(needs.image_inputs, 0);
        assert!(needs.text_metadata);
    }
}
mod files {
    //! Independent inline-file wire oracles; no file parser, credentials or network.
    use crate::resources_support as resources;
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn adapter() -> Adapter {
        Adapter::new(Profile::Responses, Dialect::Standard, None)
    }
    fn wire(parts: Value) -> Value {
        json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
    }
    fn file() -> Value {
        json!({"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf","detail":"low"})
    }
    fn decode(
        parts: Value,
    ) -> Result<morphiecore::adapter::Request, morphiecore::protocol::CodecError> {
        adapter().decode_request(&serde_json::to_vec(&wire(parts)).unwrap())
    }

    fn text(value: &str) -> morphiecore::semantic::value::Text {
        morphiecore::semantic::value::Text::allowing_empty(value, "synthetic", MAX_TOTAL_BYTES)
            .unwrap()
    }
    fn typed_file(name: Option<&str>) -> Resource {
        Resource {
            location: ResourceLocation::Inline {
                media_type: text("application/pdf"),
                data_base64: text("AQID"),
            },
            description: ResourceDescription::File(FileDescription {
                filename: name.map(text),
                detail: None,
            }),
        }
    }
    fn typed_request(values: Vec<Resource>) -> Result<GenerationRequest, GenerationError> {
        let parts = values
            .iter()
            .enumerate()
            .map(|(i, value)| Part {
                replay: None,
                id: PartId::new(i as u64 + 1),
                content: ContentPart::Resource(ResourceUse {
                    description: value.description.clone(),
                    ..resources::input(i as u64 + 1)
                }),
            })
            .collect();
        let table = ResourceTable::new(
            values
                .into_iter()
                .enumerate()
                .map(|(i, value)| {
                    (
                        ResourceId::new(i as u64 + 1),
                        ResourceDeclaration {
                            body: ResourceBody::Media(value.location),
                            conditions: Default::default(),
                        },
                    )
                })
                .collect(),
        )?;
        GenerationRequest::from_resources(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::User,
                    parts,
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            )],
            GenerationSettings::default(),
            table,
        )
    }
    fn encode_typed(
        request: &GenerationRequest,
        contract: Contract,
    ) -> Result<Value, morphiecore::lowering::generation::RepresentationError> {
        let fidelity = morphiecore::protocol::fidelity::FidelityRecords::default();
        let target = morphiecore::protocol::openai::lowering::lower_request(
            request,
            &fidelity,
            Profile::Responses,
            contract,
        )?;
        Ok(morphiecore::protocol::openai::responses::encode_generation(&target).unwrap())
    }

    #[test]
    fn file_url_limits_intersections_and_edits_do_not_claim_remote_bytes() {
        let mut remote = typed_file(None);
        remote.location =
            ResourceLocation::Url(text("https://example.invalid/not-a-pdf?sig=synthetic"));
        let request = typed_request(vec![remote.clone()]).unwrap();
        assert_eq!(remote.inline_decoded_bytes().unwrap(), None);
        let mut contract = Contract::full();
        contract.files.max_inline_bytes = 0;
        contract.files.max_total_inline_bytes = 0;
        contract.files.inline_media_types = Some(vec![]);
        assert!(encode_typed(&request, contract.clone()).is_ok());
        for (a, b) in [(true, false), (false, true), (false, false)] {
            let mut profile = adapter();
            profile.adaptation.files.urls = a;
            contract.files.urls = b;
            assert!(!profile.contract(&contract).files.urls);
            assert!(encode_typed(&request, profile.contract(&contract)).is_err());
        }
        contract.files.urls = true;
        contract.files.max_files = 1;
        assert!(
            encode_typed(
                &typed_request(vec![remote.clone(), typed_file(None)]).unwrap(),
                contract
            )
            .is_err()
        );
        let mut decoded =
            decode(json!([{"type":"input_file","file_url":"https://example.invalid/old"}]))
                .unwrap();
        let source = decoded.task.semantic.clone();
        let proof = RequestDependencyProof::capture(
            &source,
            HistoryDependency::Owners(vec![ItemId::new(1)]),
            SettingsDependency::None,
        )
        .unwrap();
        let mut items = source.items().to_vec();
        let Item::Message(m) = &mut items[0].1 else {
            panic!()
        };
        let id = m.parts[0].id;
        let ContentPart::Resource(reference) = &mut m.parts[0].content else {
            panic!()
        };
        reference.description = typed_file(Some("edited.pdf")).description;
        let table = resources::replace(
            source.resources(),
            reference.id,
            typed_file(Some("edited.pdf")),
        );
        decoded.task.semantic = source
            .clone()
            .with_resources(table)
            .unwrap()
            .with_items(items)
            .unwrap();
        assert!(proof.check(&decoded.task.semantic).is_err());
        let wire = adapter()
            .encode_request(&decoded, "synthetic", &Contract::full())
            .unwrap();
        assert_eq!(
            wire["input"][0]["content"],
            json!([{"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"edited.pdf"}])
        );
        let Item::Message(m) = &decoded.task.semantic.items()[0].1 else {
            panic!()
        };
        assert_eq!(m.parts[0].id, id);
        assert!(proof.check(&source).is_ok());
    }

    #[test]
    fn malformed_file_urls_are_rejected_by_wire_and_typed_boundaries() {
        for raw in [
            "file:///tmp/x",
            "ftp://example.test/x",
            "https://user:secret@example.test/x",
            "https://@example.test/x",
            "https://example.test/ x",
            "https://example.test/\\\\x",
            "https://example.test/\n",
            "//example.test/x",
            "https:///",
        ] {
            assert!(decode(json!([{"type":"input_file","file_url":raw}])).is_err());
            let mut r = typed_file(None);
            r.location = ResourceLocation::Url(text(raw));
            assert!(typed_request(vec![r]).is_err());
        }
        let raw = format!(
            "https://example.test/{}",
            "a".repeat(MAX_RESOURCE_URL_BYTES)
        );
        assert!(decode(json!([{"type":"input_file","file_url":raw}])).is_err());
        for other in [json!(null), json!("data:application/pdf;base64,AQID")] {
            assert!(
            decode(
                json!([{"type":"input_file","file_url":"https://example.test/x","file_data":other}])
            )
            .is_err()
        );
        }
    }

    #[test]
    fn malformed_typed_files_and_aggregate_budgets_are_revalidated_after_edits() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        for mime in ["", "not-a-type", "*/*", "text/*", "application/pdf\n"] {
            let mut resource = typed_file(None);
            resource.location = ResourceLocation::Inline {
                media_type: text(mime),
                data_base64: text("AQID"),
            };
            assert!(typed_request(vec![resource]).is_err());
        }
        let mut resource = typed_file(Some(&"x".repeat(MAX_FILE_NAME_BYTES + 1)));
        assert_eq!(resource.validate(), Err(GenerationError::Limit));
        resource = typed_file(None);
        resource.location = ResourceLocation::Inline {
            media_type: text("application/pdf"),
            data_base64: text(&STANDARD.encode(vec![0; MAX_FILE_DECODED_BYTES + 1])),
        };
        assert_eq!(resource.validate(), Err(GenerationError::Limit));
        let mut large = typed_file(None);
        large.location = ResourceLocation::Inline {
            media_type: text("application/pdf"),
            data_base64: text(&STANDARD.encode(vec![0; 720 * 1024])),
        };
        assert!(typed_request(vec![large.clone(), large.clone()]).is_ok());
        assert_eq!(
            typed_request(vec![large.clone(), large.clone(), large]),
            Err(GenerationError::Limit)
        );

        let mut resource = typed_file(None);
        resource.location = ResourceLocation::Inline {
            media_type: text("application/pdf"),
            data_base64: text(&"A".repeat(MAX_TEXT_BYTES + 4)),
        };
        assert_eq!(resource.validate(), Err(GenerationError::Limit));
        assert!(
            ResourceTable::new(vec![(
                ResourceId::new(1),
                ResourceDeclaration {
                    body: ResourceBody::Media(resource.location),
                    conditions: Default::default(),
                }
            )])
            .is_err()
        );
        // Generic IDs remain typed but have no issuer-bound file carrier.
        let mut resource = typed_file(None);
        resource.location = ResourceLocation::OpaqueReference(text("file-synthetic"));
        let request = typed_request(vec![resource]).unwrap();
        assert!(encode_typed(&request, Contract::full()).is_err());
    }

    #[test]
    fn file_byte_boundary_rejects_duplicate_keys_and_does_not_echo_private_values() {
        let duplicate = br#"{"model":"synthetic","input":[{"role":"user","content":[{"type":"input_file","file_data":"data:application/pdf;base64,AQID","file_data":"data:application/pdf;base64,BAUG"}]}]}"#;
        assert!(adapter().decode_request(duplicate).is_err());
        let mut part = file();
        part["file_data"] = json!("data:private-type;base64,private-body");
        part["filename"] = json!("private-name.pdf");
        let error = decode(json!([part])).unwrap_err();
        for value in ["private-type", "private-body", "private-name.pdf"] {
            assert!(!format!("{error:?} {error}").contains(value));
        }
    }
}
mod images {
    //! Independent image admission and projection oracles; no network or media decoder.
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps};
    use serde_json::{Value, json};

    fn adapter(profile: Profile) -> Adapter {
        Adapter::new(profile, Dialect::Standard, None)
    }
    fn wire(profile: Profile, parts: Value) -> Value {
        match profile {
            Profile::Chat => {
                json!({"model":"synthetic","messages":[{"role":"user","content":parts}]})
            }
            Profile::Responses => {
                json!({"model":"synthetic","input":[{"role":"user","content":parts}]})
            }
        }
    }
    fn parts(profile: Profile) -> Value {
        match profile {
            Profile::Chat => json!([
                {"type":"text","text":"before"},
                {"type":"image_url","image_url":{"url":"https://example.test/a.png","detail":"low"}},
                {"type":"text","text":"between"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AQID"}}
            ]),
            Profile::Responses => json!([
                {"type":"input_text","text":"before"},
                {"type":"input_image","image_url":"https://example.test/a.png","detail":"low"},
                {"type":"input_text","text":"between"},
                {"type":"input_image","image_url":"data:image/png;base64,AQID"}
            ]),
        }
    }

    #[test]
    fn lowering_rejects_disabled_images_and_undeclared_detail_without_mutating_ir() {
        let request = adapter(Profile::Responses)
            .decode_request(
                &serde_json::to_vec(&wire(Profile::Responses, parts(Profile::Responses))).unwrap(),
            )
            .unwrap();
        let before = request.clone();
        let mut contract = TargetCaps::full();
        contract.semantics.image_input = false;
        assert!(
            adapter(Profile::Responses)
                .encode_request(&request, "synthetic", &contract)
                .is_err()
        );
        let xiaomi = Adapter::new(Profile::Responses, Dialect::Xiaomi, None);
        assert!(
            xiaomi
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_err()
        );
        assert_eq!(request, before);
        let original = wire(
            Profile::Responses,
            json!([{"type":"input_image","image_url":"https://example.test/x","detail":"original"}]),
        );
        let request = adapter(Profile::Responses)
            .decode_request(&serde_json::to_vec(&original).unwrap())
            .unwrap();
        assert!(
            adapter(Profile::Chat)
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_err()
        );
        assert!(
            Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
                .encode_request(&request, "synthetic", &TargetCaps::full())
                .is_ok()
        );
        let bmp = wire(
            Profile::Responses,
            json!([{"type":"input_image","image_url":"data:image/bmp;base64,AQID"}]),
        );
        let request = Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
            .decode_request(&serde_json::to_vec(&bmp).unwrap())
            .unwrap();
        for profile in [Profile::Chat, Profile::Responses] {
            assert!(
                adapter(profile)
                    .encode_request(&request, "synthetic", &TargetCaps::full())
                    .is_err()
            );
            assert!(
                Adapter::new(profile, Dialect::DeepSeek, None)
                    .encode_request(&request, "synthetic", &TargetCaps::full())
                    .is_err()
            );
            assert!(
                Adapter::new(profile, Dialect::Xiaomi, None)
                    .encode_request(&request, "synthetic", &TargetCaps::full())
                    .is_ok()
            );
        }
    }
}
mod client_carrier {
    //! Public protocol boundaries reject private attachments without weakening typed IR.
    use crate::{events_support as event, wire};
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::{task::generation::*, value::ReplayOrigin},
    };
    use serde_json::{Value, json};
    fn client(dialect: Dialect) -> Adapter {
        Adapter::new(
            Profile::Responses,
            dialect,
            Some(ReplayOrigin::new("synthetic-client").unwrap()),
        )
    }
    fn contract() -> Contract {
        let mut contract = Contract::full();
        contract.replay_origin = Some(ReplayOrigin::new("synthetic-client").unwrap());
        contract
    }
    fn no_attachment(value: &Value) {
        match value {
            Value::Object(object) => {
                assert!(!object.contains_key("_openbridge"));
                object.values().for_each(no_attachment);
            }
            Value::Array(values) => values.iter().for_each(no_attachment),
            _ => {}
        }
    }
    #[test]
    fn unsupported_typed_values_remain_rejected_instead_of_becoming_text_or_extras() {
        let adapter = client(Dialect::MorphieCore);
        let base = adapter
            .decode_response(&serde_json::to_vec(&wire::response(2)).unwrap())
            .unwrap();
        let call = |arguments| {
            Item::ToolCall(ToolCall {
                call_id: event::text("c"),
                name: event::text("lookup"),
                arguments,
                status: ItemLifecycle::Completed,
                context: Default::default(),
            })
        };
        let mut usage = Usage::operation(3, 5, 8);
        usage.scope = UsageScope::Session;
        let mut prediction = Usage::operation(3, 5, 8);
        prediction.accepted_prediction_tokens = Some(1);
        let response = |item| {
            GenerationResponse::new(vec![(ItemId::new(1), item)], Outcome::Completed).unwrap()
        };
        let mut grouped = base.semantic.items().to_vec();
        grouped.push((ItemId::new(2), call(ToolArguments::Raw("{}".into()))));
        for semantic in [
            base.semantic
                .clone()
                .with_progress(InteractionProgress::TurnFinished)
                .unwrap(),
            base.semantic.clone().with_usage(usage).unwrap(),
            response(call(ToolArguments::Structured(
                StructuredValue::new(json!({"n":1})).unwrap(),
            ))),
            response(Item::Reasoning(ReasoningItem {
                parts: vec![],
                status: ItemLifecycle::Completed,
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::GoogleInteractionsV1Thought,
                    event::text("synthetic"),
                )),
            })),
        ] {
            let mut decoded = base.clone();
            decoded.semantic = semantic.clone();
            assert!(adapter.encode_response(&decoded, &contract()).is_err());
            assert_eq!(decoded.semantic, semantic);
        }
        let mut decoded = base.clone();
        decoded.semantic = GenerationResponse::from_resources(
            grouped,
            Outcome::Completed,
            base.semantic.resources().clone(),
        )
        .unwrap()
        .with_message_owners(vec![(ItemId::new(2), ItemId::new(1))])
        .unwrap();
        let encoded = adapter.encode_response(&decoded, &contract()).unwrap();
        no_attachment(&encoded);
        assert_eq!(encoded["output"][1]["call_id"], "c");
        assert_eq!(decoded.semantic.message_owners().len(), 1);
        let mut counted = base.clone();
        counted.semantic = counted.semantic.with_usage(prediction).unwrap();
        let projected = adapter.encode_response(&counted, &contract()).unwrap();
        no_attachment(&projected);
        assert_eq!(projected["usage"]["total_tokens"], 8);
        assert!(
            projected["usage"]["output_tokens_details"]
                .get("accepted_prediction_tokens")
                .is_none()
        );
        assert_eq!(
            counted.semantic.usage().unwrap().accepted_prediction_tokens,
            Some(1)
        );
        let request = adapter
            .decode_request(
                &serde_json::to_vec(&json!({"model":"fixture-model","input":[
                    {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
                    {"type":"function_call_output","call_id":"c","output":"{}"}
                ]}))
                .unwrap(),
            )
            .unwrap();
        for structured in [false, true] {
            let mut request = request.clone();
            let mut items = request.task.semantic.items().to_vec();
            let Item::ToolResult(result) = &mut items[1].1 else {
                unreachable!()
            };
            if structured {
                result.output =
                    ToolOutput::Structured(StructuredValue::new(json!({"ok":true})).unwrap());
            } else {
                result.execution = Some(ToolExecution::Succeeded);
            }
            request.task.semantic = request.task.semantic.with_items(items).unwrap();
            assert!(
                adapter
                    .encode_request(&request, "fixture-model", &contract())
                    .is_err()
            );
        }
    }
}
