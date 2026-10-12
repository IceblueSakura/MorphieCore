//! Reasoning controls and opaque replay ownership, formats and client carriers.
mod reasoning {
    //! Reasoning ownership, opaque replay, mixed tool continuation and resource failures.
    use crate::events_support::*;
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{
                Profile, TargetCaps as Contract,
                events::{EventDecoder, EventEncoder},
                lowering::{lower_request, lower_response},
                responses,
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};
    fn reasoning(status: &str) -> Value {
        json!({"id":"rs","type":"reasoning","status":status,"summary":[{"type":"summary_text","text":"summary"}],"content":[{"type":"reasoning_text","text":"reasoning"}],"encrypted_content":"final-synthetic"})
    }

    #[test]
    fn typed_disabled_summary_is_valid_semantics_without_a_responses_carrier() {
        let settings = GenerationSettings {
            instructions: morphiecore::semantic::value::Presence::Value(text("hello")),
            reasoning: ReasoningRequest::present(
                Some(ReasoningEffort::None),
                Some(ReasoningSummary::Disabled),
            ),
            ..Default::default()
        };
        let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
        assert_eq!(
            request.reasoning().summary(),
            Some(ReasoningSummary::Disabled)
        );
        let fidelity = FidelityRecords::default();
        for profile in [Profile::Responses, Profile::Chat] {
            assert!(matches!(
                lower_request(&request, &fidelity, profile, Contract::full()),
                Err(morphiecore::lowering::generation::RepresentationError::Reasoning)
            ));
        }
        let mut reported = metadata();
        reported.context.settings = Some(settings);
        let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
        assert!(
            lower_response(
                &response,
                &fidelity,
                &reported,
                Profile::Responses,
                Contract::full()
            )
            .is_err()
        );
        let mut encoder = EventEncoder::new(Profile::Responses, reported).unwrap();
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
        assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
    }

    #[test]
    fn standard_summary_presence_and_deprecated_alias_have_independent_wire_expectations() {
        use morphiecore::semantic::value::Presence;
        for (summary, wire) in [
            (Presence::Absent, json!({})),
            (Presence::Null, json!({"summary":null})),
            (
                Presence::Value(ReasoningSummary::Auto),
                json!({"summary":"auto"}),
            ),
            (
                Presence::Value(ReasoningSummary::Concise),
                json!({"summary":"concise"}),
            ),
            (
                Presence::Value(ReasoningSummary::Detailed),
                json!({"summary":"detailed"}),
            ),
        ] {
            let decoded =
                responses::decode_generation(&json!({"input":"hello","reasoning":wire})).unwrap();
            assert_eq!(decoded.semantic.reasoning().summary, summary);
            let mut reasoning = ReasoningRequest::present(None, None);
            reasoning.summary = summary;
            let request = GenerationRequest::from_settings(
                vec![],
                GenerationSettings {
                    instructions: Presence::Value(text("hello")),
                    reasoning,
                    ..Default::default()
                },
            )
            .unwrap();
            let fidelity = FidelityRecords::default();
            let target =
                lower_request(&request, &fidelity, Profile::Responses, Contract::full()).unwrap();
            assert_eq!(
                responses::encode_generation(&target).unwrap()["reasoning"],
                wire
            );
        }
        for value in [
            Value::Null,
            json!("auto"),
            json!("concise"),
            json!("detailed"),
        ] {
            let decoded = responses::decode_generation(&json!({
                "input":"hello","reasoning":{"generate_summary":value}
            }))
            .unwrap();
            let target = lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap();
            let wire = responses::encode_generation(&target).unwrap();
            assert_eq!(wire["reasoning"], json!({"summary":value}));
        }
        assert!(
            responses::decode_generation(&json!({
                "input":"hello","reasoning":{"summary":"auto","generate_summary":"detailed"}
            }))
            .is_err()
        );
    }

    #[test]
    fn unsupported_snapshot_content_is_not_silently_dropped() {
        let mut d = EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":{"id":"rs","type":"reasoning","status":"in_progress","summary":[]}})).unwrap();
        assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":reasoning("completed")})).is_err());
    }
    #[test]
    fn encoder_rejects_unbound_replay_before_emitting_it() {
        let mut encoder = EventEncoder::new(Profile::Responses, metadata()).unwrap();
        let f = FidelityRecords::default();
        encoder.encode(&StreamEvent::Started, &f).unwrap();
        assert!(
            encoder
                .encode(
                    &StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind: ItemKind::Reasoning,
                        replay: Some(token("partial", false))
                    },
                    &f
                )
                .is_err()
        );
    }
    #[test]
    fn configuration_update_is_an_ordered_effort_item_not_a_settings_patch() {
        let source = json!({"input":[
            {"type":"configuration_update","reasoning":{"effort":"low"}},
            {"role":"user","content":"next"}
        ]});
        let decoded = responses::decode_generation(&source).unwrap();
        assert!(decoded.semantic.reasoning().effort().is_none());
        let encoded = responses::encode_generation(
            &lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                Profile::Responses,
                contract(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            encoded["input"][0],
            json!({"type":"configuration_update","reasoning":{"effort":"low"}})
        );
        let mut items = decoded.semantic.items().to_vec();
        let Item::ConfigurationUpdate(update) = &mut items[0].1 else {
            panic!("configuration update")
        };
        update.effort = Some(ReasoningEffort::High);
        let replaced = decoded.semantic.clone().with_items(items).unwrap();
        let replaced_wire = responses::encode_generation(
            &lower_request(&replaced, &decoded.fidelity, Profile::Responses, contract()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            replaced_wire["input"][0]["reasoning"]["effort"],
            json!("high")
        );
        let deleted = replaced
            .retain_items(|_, item| !matches!(item, Item::ConfigurationUpdate(_)))
            .unwrap();
        let deleted_wire = responses::encode_generation(
            &lower_request(&deleted, &decoded.fidelity, Profile::Responses, contract()).unwrap(),
        )
        .unwrap();
        assert_eq!(deleted_wire["input"].as_array().unwrap().len(), 1);
        for bad in [
            json!({"type":"configuration_update","reasoning":null}),
            json!({"type":"configuration_update","summary":"auto"}),
            json!({"type":"configuration_update","reasoning":{"effort":"unregistered"}}),
            json!({"type":"configuration_update","reasoning":{"mode":"pro"}}),
        ] {
            assert!(
                responses::decode_generation(
                    &json!({"input":[bad,{"role":"user","content":"next"}]})
                )
                .is_err()
            );
        }
        assert!(
            lower_request(
                &decoded.semantic,
                &decoded.fidelity,
                Profile::Chat,
                contract()
            )
            .is_err()
        );
        let mut idless = json!({"type":"configuration_update","reasoning":{"effort":"max"}});
        assert!(
            responses::decode_response(&envelope("completed", json!([idless.clone()]))).is_err()
        );
        idless["id"] = json!("cu");
        let response = responses::decode_response(&envelope("completed", json!([idless]))).unwrap();
        let Item::ConfigurationUpdate(update) = &response.semantic.items()[0].1 else {
            panic!("response update")
        };
        assert_eq!(update.effort, Some(ReasoningEffort::Max));
        let mut d = EventDecoder::new(Profile::Responses);
        d.push(&created()).unwrap();
        let item = json!({"id":"cu","type":"configuration_update","reasoning":{"effort":"low"}});
        d.push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
            .unwrap();
        assert!(d.push(&json!({"type":"response.output_item.done","output_index":0,"item":{"id":"cu","type":"configuration_update","reasoning":{"effort":"high"}}})).is_err());
        let mut ok = EventDecoder::new(Profile::Responses);
        ok.push(&created()).unwrap();
        ok.push(&json!({"type":"response.output_item.added","output_index":0,"item":item.clone()}))
            .unwrap();
        ok.push(&json!({"type":"response.output_item.done","output_index":0,"item":item.clone()}))
            .unwrap();
        ok.push(
            &json!({"type":"response.completed","response":envelope("completed", json!([item]))}),
        )
        .unwrap();
        let materialized = ok.materialize().unwrap();
        let Item::ConfigurationUpdate(update) = &materialized.semantic.items()[0].1 else {
            panic!("stream update")
        };
        assert_eq!(update.effort, Some(ReasoningEffort::Low));
    }
}
mod reasoning_controls {
    //! Independent control intents, not generated facts or approximate wire mappings.
    use morphiecore::{
        lowering::generation::RepresentationError,
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps, lowering::lower_request},
        },
        semantic::{
            task::generation::*,
            value::{Presence, Text},
        },
    };

    #[test]
    fn each_new_control_independently_rejects_request_echo_and_stream_projection() {
        use morphiecore::protocol::openai::{events::EventEncoder, lowering::lower_response};
        for dimension in 0..5 {
            let mut control = ReasoningRequest::present(Some(ReasoningEffort::High), None);
            match dimension {
                0 => {
                    control.budget = Some(ReasoningBudget {
                        hard_limit: Some(1024),
                        soft_target: None,
                    })
                }
                1 => {
                    control.budget = Some(ReasoningBudget {
                        hard_limit: None,
                        soft_target: Some(1024),
                    })
                }
                2 => control.display = Presence::Value(ReasoningDisplay::Omitted),
                3 => control.display = Presence::Value(ReasoningDisplay::Summary),
                _ => control.mode = Presence::Value(ReasoningMode::Adaptive),
            }
            let settings = GenerationSettings {
                instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
                reasoning: control,
                ..Default::default()
            };
            let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
            let mut metadata = crate::events_support::metadata();
            metadata.context.settings = Some(settings);
            let response = GenerationResponse::new(vec![], Outcome::Completed).unwrap();
            let fidelity = FidelityRecords::default();
            for profile in [Profile::Responses, Profile::Chat] {
                assert_eq!(
                    lower_request(&request, &fidelity, profile, TargetCaps::full()).err(),
                    Some(RepresentationError::Reasoning)
                );
                assert_eq!(
                    lower_response(&response, &fidelity, &metadata, profile, TargetCaps::full())
                        .err(),
                    Some(RepresentationError::Reasoning)
                );
            }
            let mut encoder = EventEncoder::new(Profile::Responses, metadata).unwrap();
            assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
            assert!(encoder.encode(&StreamEvent::Started, &fidelity).is_err());
        }
    }
}
mod replay_attachments {
    //! Owner-local replay values and trusted bindings, without native Google/Anthropic codecs.
    use crate::resources_support as resources;
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps, lowering::lower_request},
        },
        semantic::{
            task::generation::*,
            value::{ReplayOrigin, Text},
        },
    };
    fn text(s: &str) -> Text {
        Text::new(s, "synthetic", MAX_TEXT_BYTES).unwrap()
    }
    fn token(format: ReplayFormat) -> ReplayValue {
        ReplayValue::final_value(format, text("synthetic-private-token"))
    }
    fn call() -> Item {
        Item::ToolCall(ToolCall {
            call_id: text("C"),
            name: text("lookup"),
            arguments: "{}".into(),
            status: ItemLifecycle::Completed,
            context: CallContext {
                replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
                ..Default::default()
            },
        })
    }
    fn message() -> Item {
        Item::Message(Message {
            role: MessageRole::Assistant,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![
                Part {
                    id: PartId::new(1),
                    content: ContentPart::Text(text("visible").into()),
                    replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
                },
                Part {
                    id: PartId::new(2),
                    content: ContentPart::Resource(resources::output(1)),
                    replay: Some(token(ReplayFormat::GoogleGenerateContentPart)),
                },
                Part {
                    id: PartId::new(3),
                    content: ContentPart::Text(text("outside").into()),
                    replay: None,
                },
            ],
        })
    }
    fn request() -> GenerationRequest {
        GenerationRequest::from_resources(
            vec![(ItemId::new(1), message()), (ItemId::new(2), call())],
            GenerationSettings::default(),
            resources::table([(
                1,
                Resource {
                    location: ResourceLocation::Url(text("https://example.test/image.png")),
                    description: ResourceDescription::Image { detail: None },
                },
            )]),
        )
        .unwrap()
    }
    #[test]
    fn local_attachments_have_one_owner_and_no_existing_public_carrier() {
        let source = request();
        let requirements = GenerationRequirements::derive(&source);
        assert!(requirements.assistant_media);
        assert_eq!(requirements.image_inputs, 0);
        assert_eq!(requirements.resource_count, 1);
        let owners = [
            ReplayOwner::Part {
                item: ItemId::new(1),
                part: PartId::new(1),
            },
            ReplayOwner::Part {
                item: ItemId::new(1),
                part: PartId::new(2),
            },
            ReplayOwner::Item(ItemId::new(2)),
        ];
        let origin = ReplayOrigin::new("synthetic:model-a:version-1").unwrap();
        for owner in owners {
            assert_eq!(
                owner.value(source.items()).unwrap().replay_token(),
                Some("synthetic-private-token")
            );
            let mut fidelity = FidelityRecords::default();
            fidelity
                .record_attachment(owner, &source, origin.clone())
                .unwrap();
            assert!(fidelity.attachment_matches(owner, &source, Some(&origin)));
            assert!(!fidelity.attachment_matches(owner, &source, None));
            assert!(!fidelity.attachment_matches(
                owner,
                &source,
                Some(&ReplayOrigin::new("synthetic:model-a:version-2").unwrap())
            ));
            assert!(!format!("{source:?}{fidelity:?}").contains("synthetic-private-token"));
            for profile in [Profile::Chat, Profile::Responses] {
                assert!(lower_request(&source, &fidelity, profile, TargetCaps::full()).is_err());
            }
            assert!(GenerationSemanticContract::full().check(&source).is_err());
        }
    }
    #[test]
    fn selected_part_binding_ignores_siblings_but_rejects_own_edits_and_resurrection() {
        let source = request();
        let owner = ReplayOwner::Part {
            item: ItemId::new(1),
            part: PartId::new(2),
        };
        let origin = ReplayOrigin::new("synthetic:model-a:version-1").unwrap();
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_attachment(owner, &source, origin.clone())
            .unwrap();
        let mut outside = source.items().to_vec();
        let Item::Message(m) = &mut outside[0].1 else {
            panic!()
        };
        m.parts[2].content = ContentPart::Text(text("changed sibling").into());
        let outside = source.clone().with_items(outside).unwrap();
        assert!(fidelity.attachment_matches(owner, &outside, Some(&origin)));
        for edit in 0..5 {
            let mut items = source.items().to_vec();
            let mut declarations = source.resources().clone();
            let Item::Message(m) = &mut items[0].1 else {
                panic!()
            };
            match edit {
                0 => {
                    let ContentPart::Resource(r) = &mut m.parts[1].content else {
                        panic!()
                    };
                    let mut value = r.media(source.resources()).unwrap().to_owned();
                    value.location =
                        ResourceLocation::Url(text("https://example.test/changed.png"));
                    declarations = resources::replace(&declarations, r.id, value);
                }
                1 => {
                    m.parts[1].replay = Some(ReplayValue::final_value(
                        ReplayFormat::GoogleGenerateContentPart,
                        text("changed-token"),
                    ))
                }
                2 => m.parts[1].replay = None,
                3 => m.parts[1].id = PartId::scoped(LocalScope::new(8), 2),
                _ => {
                    m.parts.remove(1);
                }
            }
            let candidate = source
                .clone()
                .with_resources(declarations)
                .unwrap()
                .with_items(items)
                .unwrap();
            assert!(!fidelity.attachment_matches(owner, &candidate, Some(&origin)));
            assert!(
                fidelity
                    .clone()
                    .record_attachment(owner, &candidate, origin.clone())
                    .is_err()
            );
            if edit >= 2 {
                assert!(owner.value(candidate.items()).is_none());
            }
        }
        let proof = RequestDependencyProof::capture(
            &source,
            HistoryDependency::PrefixThrough(ItemId::new(2)),
            SettingsDependency::All,
        )
        .unwrap();
        fidelity
            .bind_attachment_dependency(owner, proof, &source)
            .unwrap();
        assert!(!fidelity.attachment_matches(owner, &outside, Some(&origin)));
    }

    #[test]
    fn call_and_provider_step_events_preserve_their_own_attachments() {
        for format in [
            ReplayFormat::GoogleGenerateContentPart,
            ReplayFormat::GoogleInteractionsV1Step,
        ] {
            let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
            state = reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(2),
                    kind: ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id: text("C"),
                        name: text("lookup"),
                        message: None,
                        context: Default::default(),
                    },
                    replay: None,
                },
            )
            .unwrap();
            let attachment = StreamEvent::ReplayFinalized {
                owner: ReplayOwner::Item(ItemId::new(2)),
                value: token(format),
            };
            assert!(reduce(state.clone(), attachment.clone()).is_err());
            for event in [
                StreamEvent::PartStarted {
                    item: ItemId::new(2),
                    part: PartId::new(1),
                    kind: PartKind::Arguments,
                },
                StreamEvent::Delta {
                    item: ItemId::new(2),
                    part: PartId::new(1),
                    fragment: "{}".into(),
                    logprobs: vec![],
                },
                StreamEvent::ValueFinished {
                    item: ItemId::new(2),
                    part: PartId::new(1),
                },
                StreamEvent::PartFinished {
                    item: ItemId::new(2),
                    part: PartId::new(1),
                },
            ] {
                state = reduce(state, event).unwrap();
            }
            assert!(
                reduce(
                    state.clone(),
                    StreamEvent::ReplayFinalized {
                        owner: ReplayOwner::Item(ItemId::new(2)),
                        value: ReplayValue::partial(format, text("partial")),
                    }
                )
                .is_err()
            );
            assert!(
                reduce(
                    state.clone(),
                    StreamEvent::ReplayFinalized {
                        owner: ReplayOwner::Item(ItemId::new(2)),
                        value: token(ReplayFormat::ResponsesEncrypted),
                    }
                )
                .is_err()
            );
            state = reduce(state, attachment).unwrap();
            state = reduce(
                state,
                StreamEvent::ItemFinished {
                    item: ItemId::new(2),
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: TerminalDetails::default(),
                },
            )
            .unwrap();
            let mut expected = call();
            let Item::ToolCall(c) = &mut expected else {
                panic!()
            };
            c.context.replay = Some(token(format));
            assert_eq!(
                materialize(&state).unwrap(),
                GenerationResponse::new(vec![(ItemId::new(2), expected)], Outcome::Completed)
                    .unwrap()
            );
        }
        let mut observed = ProviderToolObservation {
            replay: None,
            source: NativeAliasDomain {
                source: text("interactions-v1"),
                scope: LocalScope::new(8),
            },
            operation: ProviderOperation::Reported {
                tool: text("search"),
                alias: Some(text("S")),
                requester: ProviderRequester::Model,
                action: Some(ProviderAction::Search {
                    query: None,
                    queries: Some(vec![text("query")]),
                }),
            },
            progress: None,
            execution: None,
            output: None,
            artifact_status: None,
        };
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::ProviderTool(observed.clone()),
                replay: None,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::ReplayFinalized {
                owner: ReplayOwner::Item(ItemId::new(1)),
                value: token(ReplayFormat::GoogleInteractionsV1Step),
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::ItemFinished {
                item: ItemId::new(1),
                status: ItemLifecycle::Completed,
                replay: None,
            },
        )
        .unwrap();
        state = reduce(
            state,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        )
        .unwrap();
        observed.replay = Some(token(ReplayFormat::GoogleInteractionsV1Step));
        let expected = GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed))],
            Outcome::Completed,
        )
        .unwrap();
        assert_eq!(materialize(&state).unwrap(), expected);
        let source =
            GenerationRequest::new(expected.items().to_vec(), GenerationControls::default())
                .unwrap();
        let origin = ReplayOrigin::new("synthetic:step:v1").unwrap();
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_attachment(ReplayOwner::Item(ItemId::new(1)), &source, origin.clone())
            .unwrap();
        assert!(fidelity.attachment_matches(
            ReplayOwner::Item(ItemId::new(1)),
            &source,
            Some(&origin)
        ));
        let mut changed = source.items().to_vec();
        let Item::ProviderTool(p) = &mut changed[0].1 else {
            panic!()
        };
        p.replay = Some(ReplayValue::final_value(
            ReplayFormat::GoogleInteractionsV1Step,
            text("changed"),
        ));
        let changed = source.with_items(changed).unwrap();
        assert!(!fidelity.attachment_matches(
            ReplayOwner::Item(ItemId::new(1)),
            &changed,
            Some(&origin)
        ));
    }

    #[test]
    fn attachment_budgets_count_each_value_and_the_aggregate_in_static_and_events() {
        for size in [0, MAX_TEXT_BYTES - 1, MAX_TEXT_BYTES, MAX_TEXT_BYTES + 1] {
            let value = ReplayValue::final_value(
                ReplayFormat::GoogleGenerateContentPart,
                Text::allowing_empty("x".repeat(size), "synthetic", MAX_TEXT_BYTES + 1).unwrap(),
            );
            let mut call = call();
            let Item::ToolCall(c) = &mut call else {
                panic!()
            };
            c.context.replay = Some(value);
            assert_eq!(
                GenerationRequest::new(vec![(ItemId::new(1), call)], GenerationControls::default())
                    .is_ok(),
                size > 0 && size <= MAX_TEXT_BYTES
            );
        }
        for last in [9, 10, 11] {
            let sizes = [
                MAX_TEXT_BYTES,
                MAX_TEXT_BYTES,
                MAX_TEXT_BYTES,
                MAX_TEXT_BYTES - 10,
                last,
            ];
            let mut parts = vec![];
            let mut state = Ok(reduce(StreamState::new(), StreamEvent::Started).unwrap());
            state = state.and_then(|s| {
                reduce(
                    s,
                    StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind: ItemKind::Message { phase: None },
                        replay: None,
                    },
                )
            });
            for (i, size) in sizes.into_iter().enumerate() {
                let id = PartId::new(i as u64);
                let value = ReplayValue::final_value(
                    ReplayFormat::GoogleGenerateContentPart,
                    text(&"x".repeat(size)),
                );
                parts.push(Part {
                    id,
                    content: ContentPart::Text(
                        Text::allowing_empty("", "synthetic", MAX_TEXT_BYTES)
                            .unwrap()
                            .into(),
                    ),
                    replay: Some(value.clone()),
                });
                for event in [
                    StreamEvent::PartStarted {
                        item: ItemId::new(1),
                        part: id,
                        kind: PartKind::Text,
                    },
                    StreamEvent::ValueFinished {
                        item: ItemId::new(1),
                        part: id,
                    },
                    StreamEvent::PartFinished {
                        item: ItemId::new(1),
                        part: id,
                    },
                    StreamEvent::ReplayFinalized {
                        owner: ReplayOwner::Part {
                            item: ItemId::new(1),
                            part: id,
                        },
                        value,
                    },
                ] {
                    state = state.and_then(|s| reduce(s, event));
                }
            }
            let response = GenerationResponse::new(
                vec![(
                    ItemId::new(1),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts,
                    }),
                )],
                Outcome::Completed,
            );
            assert_eq!(response.is_ok(), last <= 10);
            assert_eq!(state.is_ok(), last <= 10);
        }
    }
}
mod replay_carrier {
    //! Opaque reasoning crosses the client boundary in a tagged envelope and returns
    //! only to the Provider/model that issued it.
    use crate::wire;
    use morphiecore::{
        execution::replay::{CLIENT_ORIGIN, open_from_client, seal_event, seal_for_client},
        protocol::{
            openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
            replay_envelope,
        },
        semantic::{
            task::generation::{Item, ReplayFormat, StreamEvent},
            value::ReplayOrigin,
        },
    };
    use serde_json::{Value, json};

    fn origin(label: &str) -> ReplayOrigin {
        ReplayOrigin::new(label).unwrap()
    }
    fn upstream(label: &str) -> Adapter {
        Adapter::new(Profile::Responses, Dialect::Standard, Some(origin(label)))
    }
    fn contract(label: &str) -> Contract {
        Contract {
            replay_origin: Some(origin(label)),
            ..Contract::full()
        }
    }
    fn client() -> Adapter {
        Adapter::new(
            Profile::Responses,
            Dialect::Standard,
            Some(CLIENT_ORIGIN.clone()),
        )
    }
    fn reasoning(items: &[(morphiecore::semantic::task::generation::ItemId, Item)]) -> Vec<&Item> {
        items
            .iter()
            .map(|(_, item)| item)
            .filter(|item| matches!(item, Item::Reasoning(_)))
            .collect()
    }

    /// Upstream response → sealed client JSON → the client's next request.
    fn client_round_trip(issuer: &str) -> Value {
        let mut decoded = upstream(issuer)
            .decode_response(wire::response(1).to_string().as_bytes())
            .unwrap();
        seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
        let output = client()
            .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
            .unwrap();
        let sealed = output["output"][0]["encrypted_content"].as_str().unwrap();
        let (o, format, payload) = replay_envelope::open(sealed).unwrap();
        assert_eq!(o, origin(issuer));
        assert_eq!(format, ReplayFormat::ResponsesEncrypted);
        assert_eq!(payload.as_str(), "synthetic-final-token");
        let mut item = output["output"][0].clone();
        item.as_object_mut().unwrap().remove("content");
        json!({"model":"m","input":[
            {"role":"user","content":"first"},
            item,
            {"role":"user","content":"next"},
        ]})
    }

    #[test]
    fn same_issuer_receives_its_original_value() {
        let body = client_round_trip("openai/model-a");
        let mut request = client()
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut request).unwrap();
        assert!(
            upstream("openai/model-a")
                .encode_request(&request, "model-a", &Contract::full())
                .is_err()
        );
        let wire = upstream("openai/model-a")
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert_eq!(
            wire["input"][1]["encrypted_content"],
            "synthetic-final-token"
        );
    }

    #[test]
    fn another_target_gets_a_copy_without_the_opaque_value() {
        let body = client_round_trip("openai/model-a");
        let mut request = client()
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut request).unwrap();
        let before = request.clone();
        let (wire, rules) = upstream("other/model-b")
            .encode_request_with_rules(&request, "model-b", &contract("other/model-b"))
            .unwrap();
        // The readable summary reaches the new target as assistant text without the
        // issuer's item ID; the shared request is unchanged.
        assert_eq!(
            wire["input"],
            json!([
                {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Plan 🧪","annotations":[]}]},
                {"type":"message","role":"user","content":[{"type":"input_text","text":"next"}]},
            ])
        );
        assert_eq!(
            rules,
            [
                "responses.omit-foreign-replay.v1",
                "responses.reasoning-as-text.v1"
            ]
        );
        assert_eq!(request, before);
    }

    #[test]
    fn values_without_a_known_origin_are_dropped() {
        let mut body = client_round_trip("openai/model-a");
        body["input"][1]["encrypted_content"] = json!("raw-token-from-elsewhere");
        let mut request = client()
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut request).unwrap();
        assert!(
            reasoning(request.task.semantic.items())
                .iter()
                .all(|item| matches!(item, Item::Reasoning(r) if r.replay.is_none()))
        );
        let wire = upstream("openai/model-a")
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert!(!wire.to_string().contains("raw-token-from-elsewhere"));
    }

    #[test]
    fn streamed_replay_is_sealed_before_client_encoding() {
        let issuer = upstream("openai/model-a");
        let mut decoder = issuer.event_decoder();
        let mut sealed_tokens = vec![];
        for frame in wire::events(1) {
            for event in decoder.push(&frame).unwrap() {
                let event = seal_event(event, &CLIENT_ORIGIN).unwrap();
                if let StreamEvent::ItemStarted {
                    replay: Some(replay),
                    ..
                }
                | StreamEvent::ItemFinished {
                    replay: Some(replay),
                    ..
                } = &event
                {
                    assert_eq!(replay.origin.as_ref(), Some(&*CLIENT_ORIGIN));
                    sealed_tokens.push(replay.value.as_str().to_owned());
                }
            }
        }
        decoder.finish().unwrap();
        assert!(!sealed_tokens.is_empty());
        for token in sealed_tokens {
            let (o, _, _) = replay_envelope::open(&token).unwrap();
            assert_eq!(o, origin("openai/model-a"));
        }
    }

    #[test]
    fn foreign_and_malformed_opaque_only_items_are_removed() {
        for token in [
            replay_envelope::seal(
                &origin("foreign/model"),
                ReplayFormat::ResponsesEncrypted,
                "foreign-token",
            )
            .unwrap()
            .as_str()
            .to_owned(),
            "bare-token".into(),
            "mc1.invalid".into(),
            "mc2.future".into(),
        ] {
            let body = json!({"model":"m","input":[
                {"type":"reasoning","id":"rs","summary":[],"encrypted_content":token},
                {"role":"user","content":"next"}
            ]});
            let mut request = client()
                .decode_request(body.to_string().as_bytes())
                .unwrap();
            open_from_client(&mut request).unwrap();
            let saved = request.clone();
            let wire = upstream("openai/model-a")
                .encode_request(&request, "model-a", &contract("openai/model-a"))
                .unwrap();
            assert_eq!(wire["input"].as_array().unwrap().len(), 1);
            assert_eq!(wire["input"][0]["role"], "user");
            assert_eq!(request, saved);
        }
    }

    fn chat_client() -> Adapter {
        Adapter::new(
            Profile::Chat,
            Dialect::MorphieCore,
            Some(CLIENT_ORIGIN.clone()),
        )
    }
    /// Two reasoning items, each closed by its own encrypted value, ahead of one message.
    fn two_reasoning_items() -> (Value, Vec<Value>) {
        let r1 = json!({"id":"r1","type":"reasoning","status":"completed","summary":[{"type":"summary_text","text":"Aa"}],"encrypted_content":"tok-1"});
        let r2 = json!({"id":"r2","type":"reasoning","status":"completed","summary":[],"content":[{"type":"reasoning_text","text":"B"}],"encrypted_content":"tok-2"});
        let m = json!({"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[]}]});
        let response = json!({"id":"resp","object":"response","created_at":1,"status":"completed","model":"model-a",
            "error":null,"incomplete_details":null,"output":[r1,r2,m],
            "usage":{"input_tokens":3,"output_tokens":5,"total_tokens":8,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":2}}});
        let mut created = response.clone();
        created["status"] = json!("in_progress");
        created["output"] = json!([]);
        created["usage"] = Value::Null;
        let added = |n: usize, item: &Value| {
            let mut item = item.clone();
            item["status"] = json!("in_progress");
            for key in ["summary", "content"] {
                if item.get(key).is_some() {
                    item[key] = json!([]);
                }
            }
            item.as_object_mut().unwrap().remove("encrypted_content");
            json!({"type":"response.output_item.added","output_index":n,"item":item})
        };
        let done = |n: usize, item: &Value| json!({"type":"response.output_item.done","output_index":n,"item":item});
        let at = |n: usize, id: &str, mut event: Value| {
            event["output_index"] = json!(n);
            event["item_id"] = json!(id);
            event
        };
        let mut events = vec![
            json!({"type":"response.created","response":created}),
            added(0, &r1),
            at(
                0,
                "r1",
                json!({"type":"response.reasoning_summary_part.added","summary_index":0,"part":{"type":"summary_text","text":""}}),
            ),
            at(
                0,
                "r1",
                json!({"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"A"}),
            ),
            at(
                0,
                "r1",
                json!({"type":"response.reasoning_summary_text.delta","summary_index":0,"delta":"a"}),
            ),
            at(
                0,
                "r1",
                json!({"type":"response.reasoning_summary_text.done","summary_index":0,"text":"Aa"}),
            ),
            at(
                0,
                "r1",
                json!({"type":"response.reasoning_summary_part.done","summary_index":0,"part":{"type":"summary_text","text":"Aa"}}),
            ),
            done(0, &r1),
            added(1, &r2),
            at(
                1,
                "r2",
                json!({"type":"response.reasoning_text.delta","content_index":0,"delta":"B"}),
            ),
            at(
                1,
                "r2",
                json!({"type":"response.reasoning_text.done","content_index":0,"text":"B"}),
            ),
            done(1, &r2),
            added(2, &m),
            at(
                2,
                "m",
                json!({"type":"response.content_part.added","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            ),
            at(
                2,
                "m",
                json!({"type":"response.output_text.delta","content_index":0,"delta":"ok","logprobs":[]}),
            ),
            at(
                2,
                "m",
                json!({"type":"response.output_text.done","content_index":0,"text":"ok","logprobs":[]}),
            ),
            at(
                2,
                "m",
                json!({"type":"response.content_part.done","content_index":0,"part":{"type":"output_text","text":"ok","annotations":[]}}),
            ),
            done(2, &m),
            json!({"type":"response.completed","response":response}),
        ];
        for (n, event) in events.iter_mut().enumerate() {
            event["sequence_number"] = json!(n);
        }
        (response, events)
    }
    /// Replaces each sealed value with the payload it carries.
    fn opened(mut details: Value) -> Value {
        for detail in details.as_array_mut().unwrap() {
            if let Some(data) = detail.get_mut("data") {
                let (o, _, payload) = replay_envelope::open(data.as_str().unwrap()).unwrap();
                assert_eq!(o, origin("openai/model-a"));
                *data = json!(payload.as_str());
            }
        }
        details
    }

    #[test]
    fn chat_details_keep_item_boundaries_and_survive_a_pi_merged_replay() {
        use morphiecore::protocol::openai::events::EventEncoder;
        let issuer = upstream("openai/model-a");
        let client_contract = chat_client().contract(&contract(CLIENT_ORIGIN.as_str()));
        let (response, events) = two_reasoning_items();
        let details = json!([
            {"type":"reasoning.summary","summary":"Aa","format":"openai-responses-v1","index":0},
            {"type":"reasoning.encrypted","data":"tok-1","format":"openai-responses-v1","index":1,"id":"r1"},
            {"type":"reasoning.text","text":"B","format":"openai-responses-v1","index":2},
            {"type":"reasoning.encrypted","data":"tok-2","format":"openai-responses-v1","index":3,"id":"r2"},
        ]);

        let mut decoded = issuer
            .decode_response(response.to_string().as_bytes())
            .unwrap();
        seal_for_client(&mut decoded, &CLIENT_ORIGIN).unwrap();
        let mut message = chat_client()
            .encode_response(&decoded, &contract(CLIENT_ORIGIN.as_str()))
            .unwrap()["choices"][0]["message"]
            .take();
        message["reasoning_details"] = opened(message["reasoning_details"].take());
        assert_eq!(
            message,
            json!({"role":"assistant","content":"ok","reasoning_content":"AaB","reasoning_details":details})
        );

        let mut decoder = issuer.event_decoder();
        let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
            .unwrap()
            .with_contract(client_contract);
        let mut deltas = vec![];
        for frame in &events {
            for event in decoder.push(frame).unwrap() {
                let event = seal_event(event, &CLIENT_ORIGIN).unwrap();
                for chunk in encoder.encode(&event, &decoded.fidelity).unwrap() {
                    deltas.push(chunk["choices"][0]["delta"].clone());
                }
            }
        }
        let summary = |s: &str| json!({"reasoning_content":s,"reasoning_details":[{"type":"reasoning.summary","summary":s,"format":"openai-responses-v1","index":0}]});
        let sealed = |n: usize| json!({"reasoning_details":[{"type":"reasoning.encrypted","format":"openai-responses-v1","index":n,"id":format!("r{}", n / 2 + 1)}]});
        let without_data: Vec<_> = deltas
            .iter()
            .cloned()
            .map(|mut delta| {
                if let Some(detail) = delta.pointer_mut("/reasoning_details/0")
                    && detail.get("data").is_some()
                {
                    detail.as_object_mut().unwrap().remove("data");
                }
                delta
            })
            .collect();
        // No empty reasoning opener; every readable delta has both fields.
        assert_eq!(
            without_data[..8],
            [
                json!({"role":"assistant"}),
                summary("A"),
                summary("a"),
                sealed(1),
                json!({"reasoning_content":"B","reasoning_details":[{"type":"reasoning.text","text":"B","format":"openai-responses-v1","index":2}]}),
                sealed(3),
                json!({"content":""}),
                json!({"content":"ok"}),
            ]
        );

        // pi merges adjacent readable deltas of one type, keeps the first index,
        // and replays only the details.
        let mut merged: Vec<Value> = vec![];
        for detail in deltas
            .iter()
            .filter_map(|d| d.get("reasoning_details"))
            .flat_map(|d| d.as_array().unwrap().clone())
        {
            match merged.last_mut() {
                Some(last)
                    if last["type"] == "reasoning.summary"
                        && detail["type"] == "reasoning.summary" =>
                {
                    last["summary"] = json!(format!(
                        "{}{}",
                        last["summary"].as_str().unwrap(),
                        detail["summary"].as_str().unwrap()
                    ));
                }
                _ => merged.push(detail),
            }
        }
        let history = json!({"model":"m","messages":[
            {"role":"user","content":"q"},
            {"role":"assistant","content":"ok","reasoning_details":merged},
            {"role":"user","content":"next"},
        ]});
        let mut request = chat_client()
            .decode_client_request(history.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut request).unwrap();
        let wire = issuer
            .encode_request(&request, "model-a", &contract("openai/model-a"))
            .unwrap();
        assert_eq!(
            wire["input"].as_array().unwrap()[1..3],
            [
                json!({"type":"reasoning","id":"r1","summary":[{"type":"summary_text","text":"Aa"}],"encrypted_content":"tok-1"}),
                json!({"type":"reasoning","id":"r2","summary":[],"content":[{"type":"reasoning_text","text":"B"}],"encrypted_content":"tok-2"}),
            ]
        );
    }

    #[test]
    fn openrouter_signed_text_details_become_items_and_views_must_agree() {
        use morphiecore::semantic::{
            task::generation::{
                ItemLifecycle, PartId, ReasoningContent, ReasoningItem, ReplayValue,
            },
            value::Text,
        };
        let history = |view: &str| {
            json!({"model":"m","messages":[
                {"role":"user","content":"q"},
                {"role":"assistant","content":"ok","reasoning":view,"reasoning_details":[
                    {"type":"reasoning.text","text":"A","signature":"sig-1","format":"anthropic-claude-v1","index":0},
                    {"type":"reasoning.summary","summary":"B","format":"openai-responses-v1","index":0},
                    {"type":"reasoning.encrypted","id":"r2","data":"tok-2","format":"openai-responses-v1","index":0},
                ]},
            ]})
        };
        let request = chat_client()
            .decode_client_request(history("AB").to_string().as_bytes())
            .unwrap();
        let text = |s: &str| Text::new(s, "synthetic", 64).unwrap();
        let item = |content, token: &str| {
            Item::Reasoning(ReasoningItem {
                parts: vec![(PartId::new(0), content)],
                status: ItemLifecycle::Completed,
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text(token),
                )),
            })
        };
        let items: Vec<_> = request.task.semantic.items()[1..3]
            .iter()
            .map(|(_, item)| {
                let mut item = item.clone();
                if let Item::Reasoning(r) = &mut item {
                    r.parts[0].0 = PartId::new(0);
                }
                item
            })
            .collect();
        assert_eq!(
            items,
            [
                item(ReasoningContent::Text(text("A")), "sig-1"),
                item(ReasoningContent::Summary(text("B")), "tok-2"),
            ]
        );
        assert!(
            chat_client()
                .decode_client_request(history("AX").to_string().as_bytes())
                .is_err()
        );
    }
}
mod replay_formats {
    //! Format identity and selected dependencies never prove issuer authenticity.
    use crate::events_support::text;
    use morphiecore::semantic::task::generation::*;
    fn message(id: u64, value: &str) -> (ItemId, Item) {
        (
            ItemId::new(id),
            Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    replay: None,
                    id: PartId::new(id),
                    content: ContentPart::Text(text(value).into()),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )
    }
    fn request(items: Vec<(ItemId, Item)>) -> GenerationRequest {
        GenerationRequest::new(items, GenerationControls::default()).unwrap()
    }
    #[test]
    fn selected_owners_bind_values_order_and_only_selected_settings() {
        let source = request(vec![
            message(1, "a"),
            message(2, "unrelated"),
            message(3, "b"),
        ]);
        let scope = HistoryDependency::Owners(vec![ItemId::new(1), ItemId::new(3)]);
        let proof = RequestDependencyProof::capture(
            &source,
            scope.clone(),
            SettingsDependency::only(SettingsField::Tools)
                .union(SettingsDependency::only(SettingsField::Text)),
        )
        .unwrap();
        proof.check(&source).unwrap();
        let mut settings = source.settings().clone();
        settings.controls.max_output_tokens = Some(100);
        proof
            .check(&source.clone().with_settings(settings).unwrap())
            .unwrap();
        proof
            .check(&request(vec![
                message(1, "a"),
                message(2, "changed"),
                message(3, "b"),
                message(4, "new"),
            ]))
            .unwrap();
        for items in [
            vec![message(3, "b"), message(1, "a")],
            vec![message(1, "changed"), message(3, "b")],
            vec![message(1, "a")],
        ] {
            assert!(proof.check(&request(items)).is_err());
        }
        for owners in [
            vec![],
            vec![ItemId::new(1), ItemId::new(1)],
            vec![ItemId::new(9)],
        ] {
            assert!(
                RequestDependencyProof::capture(
                    &source,
                    HistoryDependency::Owners(owners),
                    SettingsDependency::None
                )
                .is_err()
            );
        }
        let mut settings = source.settings().clone();
        settings.tools = Some(vec![]);
        assert!(
            proof
                .check(&source.with_settings(settings).unwrap())
                .is_err()
        );
    }
}
mod replay_groups {
    //! A07/T06: overlapping groups borrow ordered members; no wire carrier is invented.
    use morphiecore::{
        protocol::openai::{Profile, TargetCaps as Contract},
        semantic::{task::generation::*, value::Text},
    };
    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", 256).unwrap()
    }
    fn group(value: u64, owners: &[u64]) -> ReplayGroup {
        ReplayGroup::new(
            GroupId::new(LocalScope::new(7), value),
            owners.iter().map(|n| ItemId::new(*n)).collect(),
        )
        .unwrap()
    }
    fn items() -> Vec<(ItemId, Item)> {
        vec![
            (
                ItemId::new(1),
                Item::Reasoning(ReasoningItem {
                    parts: vec![(
                        PartId::new(1),
                        ReasoningContent::Summary(text("reported summary")),
                    )],
                    status: ItemLifecycle::Completed,
                    replay: None,
                }),
            ),
            (
                ItemId::new(2),
                Item::ToolCall(ToolCall {
                    call_id: text("C"),
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                }),
            ),
            (
                ItemId::new(3),
                Item::ToolCall(ToolCall {
                    call_id: text("other"),
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                }),
            ),
        ]
    }
    fn request() -> GenerationRequest {
        GenerationRequest::new(items(), GenerationControls::default())
            .unwrap()
            .with_replay_groups(vec![group(1, &[1, 2]), group(2, &[2, 3])])
            .unwrap()
    }
    #[test]
    fn shared_member_has_two_derived_views_and_no_second_body() {
        let source = request();
        assert_eq!(source.replay_groups_for(ItemId::new(2)).count(), 2);
        let groups: Vec<_> = source.replay_group_views().collect();
        assert!(std::ptr::eq(
            groups[0].items().nth(1).unwrap(),
            &source.items()[1]
        ));
        assert!(std::ptr::eq(
            groups[1].items().next().unwrap(),
            &source.items()[1]
        ));
        assert_eq!(source.items().len(), 3);
        assert_eq!(
            groups[0].declaration().members(),
            &[ItemId::new(1), ItemId::new(2)]
        );
        assert!(!format!("{:?}", groups[0]).contains("reported summary"));
        assert!(!format!("{:?}", groups[0]).contains("lookup"));
    }
    #[test]
    fn dangling_duplicate_and_reversed_memberships_fail_at_the_core_owner() {
        let source = request();
        assert!(
            source
                .clone()
                .retain_items(|id, _| id != ItemId::new(2))
                .is_err()
        );
        for groups in [
            vec![group(1, &[1, 2]), group(1, &[2, 3])],
            vec![group(1, &[1, 99])],
            vec![group(1, &[2, 1])],
        ] {
            assert_eq!(
                source.clone().with_replay_groups(groups).unwrap_err(),
                GenerationError::InvalidReplayGroup
            );
        }
        assert_eq!(
            ReplayGroup::new(GroupId::new(LocalScope::new(7), 1), vec![]).unwrap_err(),
            GenerationError::InvalidReplayGroup
        );
        assert!(
            ReplayGroup::new(GroupId::new(LocalScope::new(7), 1), vec![ItemId::new(1); 2]).is_err()
        );
        assert_eq!(source.replay_groups().len(), 2);
    }

    #[test]
    fn aggregate_membership_budget_counts_shared_references_independently() {
        for count in [MAX_ITEMS - 1, MAX_ITEMS, MAX_ITEMS + 1] {
            let groups = (0..count).map(|n| group(n as u64, &[2])).collect();
            let result = request().with_replay_groups(groups);
            assert_eq!(result.is_ok(), count <= MAX_ITEMS);
            if let Err(error) = result {
                assert_eq!(error, GenerationError::Limit);
            }
        }
        assert_eq!(
            ReplayGroup::new(
                GroupId::new(LocalScope::ROOT, 1),
                vec![ItemId::new(1); MAX_ITEMS + 1]
            )
            .unwrap_err(),
            GenerationError::Limit
        );
    }

    #[test]
    fn normative_group_events_match_an_independent_static_declaration_and_cannot_reopen_terminal() {
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        for id in [1, 2, 3] {
            state = reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(id),
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
            )
            .unwrap();
            state = reduce(
                state,
                StreamEvent::ItemFinished {
                    item: ItemId::new(id),
                    status: ItemLifecycle::Completed,
                    replay: None,
                },
            )
            .unwrap();
        }
        for group in [group(1, &[1, 2]), group(2, &[2, 3])] {
            let event = StreamEvent::ReplayGroup(group);
            for profile in [Profile::Chat, Profile::Responses] {
                assert!(
                    morphiecore::protocol::openai::lowering::events::check_event(
                        &state,
                        &event,
                        profile,
                        &Contract::full()
                    )
                    .is_err()
                );
            }
            state = reduce(state, event).unwrap();
        }
        assert!(reduce(state.clone(), StreamEvent::ReplayGroup(group(1, &[1, 3]))).is_err());
        assert!(reduce(state.clone(), StreamEvent::ReplayGroup(group(3, &[1, 99]))).is_err());
        state = reduce(
            state,
            StreamEvent::Terminal {
                terminal: StreamTerminal::Completed,
                details: TerminalDetails::default(),
            },
        )
        .unwrap();
        let observed = materialize(&state).unwrap();
        let expected = GenerationResponse::new(
            (1..=3)
                .map(|id| {
                    (
                        ItemId::new(id),
                        Item::Message(Message {
                            role: MessageRole::Assistant,
                            parts: vec![],
                            status: ItemLifecycle::Completed,
                            phase: None,
                        }),
                    )
                })
                .collect(),
            Outcome::Completed,
        )
        .unwrap()
        .with_replay_groups(vec![group(1, &[1, 2]), group(2, &[2, 3])])
        .unwrap();
        assert_eq!(observed, expected);
        assert!(reduce(state, StreamEvent::ReplayGroup(group(3, &[1, 3]))).is_err());
    }
}
mod replay_ownership {
    //! Independent value/proof, mutation and finality oracles for Responses encrypted reasoning.
    use crate::events_support::{contract, envelope, metadata, origin};
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{
                Profile,
                lowering::{lower_request, lower_response},
                responses,
            },
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn wire(id: &str, token: &str, status: &str) -> Value {
        json!({"type":"reasoning","id":id,"status":status,"summary":[],"encrypted_content":token})
    }
    fn request(values: Value) -> morphiecore::protocol::openai::DecodedRequest {
        let mut decoded = responses::decode_generation(&json!({"input":values})).unwrap();
        decoded.fidelity.bind_replay_origin(&origin()).unwrap();
        decoded
    }
    fn encode(r: &GenerationRequest, f: &FidelityRecords) -> Result<Value, ()> {
        let lowered = lower_request(r, f, Profile::Responses, contract()).map_err(|_| ())?;
        responses::encode_generation(&lowered).map_err(|_| ())
    }

    #[test]
    fn opaque_value_is_semantic_and_debug_is_redacted() {
        let a = request(json!([wire("rs", "synthetic-secret-a", "completed")]));
        let b = request(json!([wire("rs", "synthetic-secret-b", "completed")]));
        assert_ne!(a.semantic, b.semantic);
        let debug = format!("{a:?}");
        assert!(!debug.contains("synthetic-secret-a"));
        let Item::Reasoning(r) = &a.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        assert_eq!(
            r.replay.as_ref().unwrap().replay_token(),
            Some("synthetic-secret-a")
        );
        assert_eq!(
            encode(&a.semantic, &a.fidelity).unwrap()["input"][0]["encrypted_content"],
            "synthetic-secret-a"
        );
    }

    #[test]
    fn partial_output_is_reportable_but_incomplete_owner_cannot_be_replayed() {
        for status in ["in_progress", "incomplete", "completed"] {
            let mut d = responses::decode_response(&envelope(
                "incomplete",
                json!([wire("rs", "synthetic-token", status)]),
            ))
            .unwrap();
            d.fidelity.bind_replay_origin(&origin()).unwrap();
            let m = metadata();
            let projected =
                lower_response(&d.semantic, &d.fidelity, &m, Profile::Responses, contract())
                    .unwrap();
            assert_eq!(
                responses::encode_response(&projected).unwrap()["output"][0]["encrypted_content"],
                "synthetic-token"
            );
            let history =
                GenerationRequest::new(d.semantic.items().to_vec(), GenerationControls::default())
                    .unwrap();
            assert_eq!(encode(&history, &d.fidelity).is_ok(), status == "completed");
        }
    }
}
mod parsed_replay {
    //! SDK derived `parsed` views are validated replay conveniences over the authoritative
    //! raw text; they are never IR state and never emitted.
    use morphiecore::{
        protocol::openai::{CodecError, DecodedRequest, chat_envelope, envelope},
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    #[test]
    fn sdk_float_views_round_only_the_view_not_the_raw_authority() {
        let raw = r#"{"n":0.123456789012345678901}"#;
        let parsed = json!({"n":0.12345678901234568});
        let decoded = decode_call(responses_call(raw, Some(parsed))).unwrap();
        assert_eq!(arguments(&decoded), raw);
    }

    fn chat_request(message: Value) -> Value {
        json!({"model":"fixture-model","messages":[message]})
    }
    fn decode_chat(message: Value) -> Result<DecodedRequest, CodecError> {
        Ok(chat_envelope::decode_request_bytes(
            &serde_json::to_vec(&chat_request(message)).unwrap(),
        )?
        .task)
    }
    fn chat_tool_call(function: Value) -> Value {
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"call","type":"function","function":function}]})
    }
    fn fn_view(arguments: &str, parsed: Option<Value>) -> Value {
        let mut f = json!({"name":"lookup","arguments":arguments});
        if let Some(v) = parsed {
            f["parsed_arguments"] = v;
        }
        f
    }
    fn responses_call(arguments: &str, parsed: Option<Value>) -> Value {
        let mut o = json!({"type":"function_call","id":"f","call_id":"c","name":"lookup","arguments":arguments});
        if let Some(v) = parsed {
            o["parsed_arguments"] = v;
        }
        o
    }
    fn decode_call(item: Value) -> Result<DecodedRequest, CodecError> {
        Ok(envelope::decode_request_bytes(
            &serde_json::to_vec(&json!({"model":"fixture-model","input":[item]})).unwrap(),
        )?
        .task)
    }
    fn arguments(d: &DecodedRequest) -> String {
        d.semantic
            .items()
            .iter()
            .find_map(|(_, i)| match i {
                Item::ToolCall(c) => c.arguments.as_raw().map(str::to_owned),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn underivable_or_inconsistent_function_views_cannot_pass_admission() {
        for bad in [
            json!({"n":2}),
            json!({"n":"1"}),
            json!([1]),
            json!(true),
            json!("{}"),
        ] {
            assert!(decode_chat(chat_tool_call(fn_view("{\"n\":1}", Some(bad.clone())))).is_err());
            assert!(decode_call(responses_call("{\"n\":1}", Some(bad))).is_err());
        }
        // A non-null view of arguments that are not valid JSON is underivable...
        assert!(decode_chat(chat_tool_call(fn_view("{n:1}", Some(json!({"n":1}))))).is_err());
        assert!(decode_call(responses_call("{n:1}", Some(json!({"n":1})))).is_err());
        // ...while a null view of opaque arguments is still just "no derived value".
        assert!(decode_chat(chat_tool_call(fn_view("{n:1}", Some(Value::Null)))).is_ok());
        assert!(decode_call(responses_call("{n:1}", Some(Value::Null))).is_ok());
    }
}
