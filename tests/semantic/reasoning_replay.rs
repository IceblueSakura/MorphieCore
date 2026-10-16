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
        let mut reasoning = ReasoningRequest::present(Some(ReasoningSummary::Disabled));
        reasoning.activation =
            morphiecore::semantic::value::Presence::Value(ReasoningActivation::Disabled);
        let settings = GenerationSettings {
            instructions: morphiecore::semantic::value::Presence::Value(text("hello")),
            reasoning,
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
        let target = lower_response(
            &response,
            &fidelity,
            &reported,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        assert!(target.projection().iter().any(|stage| stage.loss.is_some()));
        assert_eq!(
            target
                .metadata()
                .context
                .settings
                .as_ref()
                .unwrap()
                .reasoning
                .summary(),
            None
        );
        let mut encoder = EventEncoder::new(Profile::Responses, reported).unwrap();
        let wire = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
        assert_eq!(wire[0]["response"]["reasoning"]["effort"], "none");
        assert!(wire[0]["response"]["reasoning"].get("summary").is_none());
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
            let mut reasoning = ReasoningRequest::present(None);
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
        assert!(decoded.semantic.settings().effort.is_absent());
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
        update.effort = Some(EffortControl {
            scope: EffortScope::Reasoning,
            level: EffortLevel::High,
        });
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
        assert_eq!(
            update.effort,
            Some(EffortControl {
                scope: EffortScope::Reasoning,
                level: EffortLevel::Max
            })
        );
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
        assert_eq!(
            update.effort,
            Some(EffortControl {
                scope: EffortScope::Reasoning,
                level: EffortLevel::Low
            })
        );
        use morphiecore::protocol::{
            openai::{Adapter, Dialect},
            reasoning::ReasoningEffortMapping,
        };
        static MAPPING: ReasoningEffortMapping = ReasoningEffortMapping {
            canonical_model: "m",
            rule: "synthetic.configuration-effort.v1",
            scope: EffortScope::Reasoning,
            native_levels: &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::High],
            encode: &[
                (EffortLevel::Low, EffortLevel::High),
                (EffortLevel::High, EffortLevel::High),
            ],
            decode: &[
                (EffortLevel::Low, EffortLevel::High),
                (EffortLevel::Medium, EffortLevel::High),
                (EffortLevel::High, EffortLevel::High),
            ],
        };
        let mut adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        adapter.adaptation.effort_mapping = Some(&MAPPING);
        let mut source = source;
        source["model"] = json!("m");
        let request = adapter
            .decode_request(source.to_string().as_bytes())
            .unwrap();
        let before = request.clone();
        let (wire, rules) = adapter
            .encode_request_with_rules(&request, "m", &Contract::full())
            .unwrap();
        assert_eq!(wire["input"][0]["reasoning"]["effort"], "high");
        assert_eq!(rules, ["synthetic.configuration-effort.v1"]);
        assert_eq!(request, before);
        let scoped = morphiecore::protocol::reasoning::ReasoningRequestPolicy {
            canonical_model: "m",
            rule: "synthetic.configuration-scope.v1",
            conversion: morphiecore::protocol::reasoning::ReasoningConversion::Scope {
                from: EffortScope::Reasoning,
                to: EffortScope::Response,
                levels: &[(EffortLevel::Low, EffortLevel::High)],
            },
        }
        .project_request(&request.task.semantic)
        .unwrap()
        .unwrap();
        assert!(
            matches!(&scoped.items()[0].1, Item::ConfigurationUpdate(update)
            if update.effort.unwrap().scope == EffortScope::Response)
        );
        assert_eq!(request, before);
        let item = json!({"id":"cu","type":"configuration_update","reasoning":{"effort":"low"}});
        let mut calibrated = adapter.event_decoder();
        calibrated.push(&created()).unwrap();
        calibrated
            .push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
            .unwrap();
        calibrated
            .push(&json!({"type":"response.output_item.done","output_index":0,"item":item}))
            .unwrap();
        calibrated.push(&json!({"type":"response.completed","response":envelope("completed",json!([item]))})).unwrap();
        let delivered = calibrated.materialize().unwrap();
        assert!(
            matches!(&delivered.semantic.items()[0].1, Item::ConfigurationUpdate(update)
            if update.effort.unwrap().level == EffortLevel::High)
        );
        let mut conflicting = adapter.event_decoder();
        conflicting.push(&created()).unwrap();
        conflicting
            .push(&json!({"type":"response.output_item.added","output_index":0,"item":item}))
            .unwrap();
        let mut changed = item;
        changed["reasoning"]["effort"] = json!("medium");
        assert!(
            conflicting
                .push(&json!({"type":"response.output_item.done","output_index":0,"item":changed}))
                .is_err()
        );
    }
}
mod reasoning_controls {
    //! Directional request projections and independently validated incidental reports.
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
    fn requests_reject_unselected_controls_while_incidental_reports_project() {
        use morphiecore::protocol::openai::{events::EventEncoder, lowering::lower_response};
        use morphiecore::protocol::reasoning::{
            ReasoningConversion, ReasoningRequestPolicy, SHARED_BUDGETS, ThinkingBudgets,
        };
        assert_eq!(
            SHARED_BUDGETS.entries,
            &[
                (EffortLevel::Minimal, 1024),
                (EffortLevel::Low, 2048),
                (EffortLevel::Medium, 4096),
                (EffortLevel::High, 8192),
                (EffortLevel::XHigh, 16384),
                (EffortLevel::Max, 24576),
            ]
        );
        assert_eq!(SHARED_BUDGETS.nearest(1536).unwrap().0, EffortLevel::Low);
        assert_eq!(SHARED_BUDGETS.nearest(3072).unwrap().0, EffortLevel::Medium);
        assert_eq!(SHARED_BUDGETS.nearest(6144).unwrap().0, EffortLevel::High);
        assert_eq!(SHARED_BUDGETS.nearest(20480).unwrap().0, EffortLevel::Max);
        assert_eq!(
            SHARED_BUDGETS.nearest(12288).unwrap(),
            (EffortLevel::XHigh, 16384)
        );
        assert_eq!(
            SHARED_BUDGETS.nearest(8192).unwrap(),
            (EffortLevel::High, 8192)
        );
        assert!(SHARED_BUDGETS.nearest(0).is_err());
        let repeated = ThinkingBudgets {
            entries: &[
                (EffortLevel::XHigh, 1000),
                (EffortLevel::Low, 500),
                (EffortLevel::High, 1000),
                (EffortLevel::Medium, 500),
            ],
        };
        assert_eq!(repeated.nearest(750).unwrap().0, EffortLevel::XHigh);
        assert_eq!(repeated.nearest(500).unwrap().0, EffortLevel::Medium);
        assert_eq!(repeated.nearest(1000).unwrap().0, EffortLevel::XHigh);
        assert_eq!(repeated.nearest(u64::MAX).unwrap().0, EffortLevel::XHigh);
        static QUANTIZE: ReasoningRequestPolicy = ReasoningRequestPolicy {
            canonical_model: "m",
            rule: "synthetic.budget-to-effort.v1",
            conversion: ReasoningConversion::BudgetToEffort {
                scope: EffortScope::Reasoning,
                table: SHARED_BUDGETS,
            },
        };
        let mut settings = GenerationSettings {
            instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
            reasoning: ReasoningRequest::present(None),
            ..Default::default()
        };
        settings.reasoning.budget_tokens = Some(12288);
        settings.reasoning.mode = Presence::Value(ReasoningMode::Budgeted);
        settings.controls.max_output_tokens = Some(16384);
        let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
        let source = request.clone();
        let semantic = QUANTIZE.project_request(&request).unwrap().unwrap();
        assert_eq!(
            semantic.settings().effort.value().unwrap().level,
            EffortLevel::XHigh
        );
        assert_eq!(request, source);
        let mut adapter = morphiecore::protocol::openai::Adapter::new(
            Profile::Responses,
            morphiecore::protocol::openai::Dialect::Standard,
            None,
        );
        adapter.adaptation.reasoning_policy = Some(&QUANTIZE);
        let mut envelope = adapter
            .decode_request(br#"{"model":"m","input":"hi"}"#)
            .unwrap();
        envelope.task.semantic = request;
        let (wire, rules) = adapter
            .encode_request_with_rules(&envelope, "m", &TargetCaps::full())
            .unwrap();
        assert_eq!(wire["reasoning"]["effort"], "xhigh");
        assert_eq!(rules, ["synthetic.budget-to-effort.v1"]);
        let allocate = ReasoningRequestPolicy {
            canonical_model: "m",
            rule: "synthetic.effort-to-budget.v1",
            conversion: ReasoningConversion::EffortToBudget {
                scope: EffortScope::Reasoning,
                table: SHARED_BUDGETS,
            },
        };
        let mut effort = semantic.settings().clone();
        effort.controls.max_output_tokens = Some(4096);
        assert!(allocate.project(&effort).is_err());
        effort.controls.max_output_tokens = Some(16384);
        let allocated = allocate.project(&effort).unwrap();
        assert_eq!(allocated.reasoning.budget_tokens, Some(16384));
        effort.reasoning.mode = Presence::Value(ReasoningMode::Pro);
        assert!(allocate.project(&effort).is_err());
        assert_eq!(effort.reasoning.mode, Presence::Value(ReasoningMode::Pro));
        effort.reasoning.mode = Presence::Absent;
        let allocated = GenerationRequest::from_settings(vec![], allocated).unwrap();
        let (wire, _) = morphiecore::protocol::google::encode_generation_request(
            &allocated,
            &FidelityRecords::default(),
            &morphiecore::protocol::native::ReplayTarget::new("synthetic", "m").unwrap(),
        )
        .unwrap();
        assert_eq!(
            wire["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            16384
        );
        let scoped = ReasoningRequestPolicy {
            canonical_model: "m",
            rule: "synthetic.scope.v1",
            conversion: ReasoningConversion::Scope {
                from: EffortScope::Reasoning,
                to: EffortScope::Response,
                levels: &[(EffortLevel::XHigh, EffortLevel::High)],
            },
        }
        .project(&effort)
        .unwrap();
        assert_eq!(scoped.effort.value().unwrap().scope, EffortScope::Response);
        let mut disabled = scoped;
        disabled.reasoning.activation = Presence::Value(ReasoningActivation::Disabled);
        let disabled = ReasoningRequestPolicy {
            canonical_model: "m",
            rule: "synthetic.preserve-off.v1",
            conversion: ReasoningConversion::PreserveDisabled,
        }
        .project(&disabled)
        .unwrap();
        assert!(disabled.effort.is_absent());
        assert_eq!(
            disabled.reasoning.activation.value(),
            Some(&ReasoningActivation::Disabled)
        );
        let mut client = morphiecore::protocol::openai::Adapter::new(
            Profile::Chat,
            morphiecore::protocol::openai::Dialect::Standard,
            None,
        );
        let instruction = client
            .decode_request(br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#)
            .unwrap();
        let mut instruction = instruction;
        let mut settings = instruction.task.semantic.settings().clone();
        settings.instructions = Presence::Value(Text::new("instructions", "test", 32).unwrap());
        instruction.task.semantic = instruction.task.semantic.with_settings(settings).unwrap();
        assert_eq!(
            client
                .encode_request(&instruction, "m", &TargetCaps::full())
                .unwrap()["messages"][0]["role"],
            "developer"
        );
        client.adaptation.rules.chat_system_instructions = true;
        let (wire, rules) = client
            .encode_request_with_rules(&instruction, "m", &TargetCaps::full())
            .unwrap();
        assert_eq!(
            wire["messages"][0],
            serde_json::json!({"role":"system","content":"instructions"})
        );
        assert_eq!(rules, ["chat.instructions-as-system.v1"]);
        for dimension in 0..4 {
            let mut control = ReasoningRequest::present(None);
            match dimension {
                0 => control.budget_tokens = Some(1024),
                1 => control.display = Presence::Value(ReasoningDisplay::Omitted),
                2 => control.display = Presence::Value(ReasoningDisplay::Summary),
                _ => control.mode = Presence::Value(ReasoningMode::Adaptive),
            }
            let settings = GenerationSettings {
                instructions: Presence::Value(Text::new("hello", "test", 32).unwrap()),
                reasoning: control,
                effort: Presence::Value(EffortControl {
                    scope: EffortScope::Reasoning,
                    level: EffortLevel::High,
                }),
                ..Default::default()
            };
            let request = GenerationRequest::from_settings(vec![], settings.clone()).unwrap();
            let mut metadata = crate::events_support::metadata();
            metadata.context.settings = Some(settings);
            let response = GenerationResponse::new(
                vec![(
                    ItemId::new(1),
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        phase: None,
                        status: ItemLifecycle::Completed,
                        parts: vec![Part {
                            id: PartId::new(1),
                            content: ContentPart::Text(Text::new("ok", "test", 32).unwrap().into()),
                        }],
                    }),
                )],
                Outcome::Completed,
            )
            .unwrap();
            let fidelity = FidelityRecords::default();
            for profile in [Profile::Responses, Profile::Chat] {
                assert_eq!(
                    lower_request(&request, &fidelity, profile, TargetCaps::full()).err(),
                    Some(RepresentationError::Reasoning)
                );
                let target =
                    lower_response(&response, &fidelity, &metadata, profile, TargetCaps::full())
                        .unwrap();
                assert!(target.projection().iter().any(|stage| stage.loss.is_some()));
                assert_eq!(
                    metadata.context.settings.as_ref().unwrap().reasoning,
                    request.settings().reasoning
                );
                let json = match profile {
                    Profile::Responses => {
                        morphiecore::protocol::openai::responses::envelope::encode_response(&target)
                            .unwrap()
                    }
                    Profile::Chat => {
                        morphiecore::protocol::openai::chat::envelope::encode_response(&target)
                            .unwrap()
                    }
                };
                if profile == Profile::Responses {
                    assert_eq!(json["reasoning"], serde_json::json!({"effort":"high"}));
                } else {
                    assert!(target.metadata().context.settings.is_none());
                }
                let mut encoder = EventEncoder::new(profile, metadata.clone()).unwrap();
                let mut frames = encoder.encode(&StreamEvent::Started, &fidelity).unwrap();
                frames.extend(
                    encoder
                        .encode(
                            &crate::events_support::start(1, ItemKind::Message { phase: None }),
                            &fidelity,
                        )
                        .unwrap(),
                );
                for event in crate::events_support::part(1, 1, PartKind::Text, "ok") {
                    frames.extend(encoder.encode(&event, &fidelity).unwrap());
                }
                frames.extend(
                    encoder
                        .encode(
                            &crate::events_support::close(1, ItemLifecycle::Completed),
                            &fidelity,
                        )
                        .unwrap(),
                );
                let terminal = StreamEvent::Terminal {
                    terminal: StreamTerminal::Completed,
                    details: Default::default(),
                };
                frames.extend(encoder.encode(&terminal, &fidelity).unwrap());
                encoder.finish().unwrap();
                let mut intake = morphiecore::protocol::openai::events::EventDecoder::new(profile);
                for frame in &frames {
                    intake.push(frame).unwrap();
                }
                if profile == Profile::Chat {
                    intake.done().unwrap();
                }
                let materialized = intake.materialize().unwrap();
                assert_eq!(
                    materialized.metadata.context.settings,
                    target.metadata().context.settings
                );
                assert!(
                    matches!(&materialized.semantic.items()[0].1, Item::Message(message)
                    if matches!(&message.parts[0].content, ContentPart::Text(text) if text.as_str() == "ok"))
                );
                assert!(
                    encoder
                        .projection()
                        .iter()
                        .any(|stage| stage.loss.is_some())
                );
            }
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
        let mut edited = upstream("openai/model-a")
            .decode_response(wire::response(1).to_string().as_bytes())
            .unwrap();
        let mut items = edited.semantic.items().to_vec();
        let Item::Reasoning(reasoning) = &mut items[0].1 else {
            panic!("reasoning")
        };
        let (_, content) = &mut reasoning.parts[0];
        *content = morphiecore::semantic::task::generation::ReasoningContent::Summary(
            morphiecore::semantic::value::Text::new("edited", "synthetic", 64).unwrap(),
        );
        edited.semantic = edited.semantic.with_items(items).unwrap();
        let before = edited.clone();
        assert!(seal_for_client(&mut edited, &CLIENT_ORIGIN).is_err());
        assert_eq!(edited, before);

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
    fn chat_details_keep_item_boundaries_and_survive_pi_and_opencode_replays() {
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
        // opencode (e5a557e) merges alike and also returns the concatenated view.
        for view in [None, Some("AaB")] {
            let mut assistant =
                json!({"role":"assistant","content":"ok","reasoning_details":merged});
            if let Some(view) = view {
                assistant["reasoning_content"] = json!(view);
            }
            let history = json!({"model":"m","messages":[
                {"role":"user","content":"q"},
                assistant,
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
        // pi's DeepSeek compat adds an empty `reasoning_content` beside the details.
        assert!(
            chat_client()
                .decode_client_request(history("").to_string().as_bytes())
                .is_ok()
        );
        let mut empty = history("");
        empty["messages"][1]["reasoning_details"][0]["text"] = json!("");
        empty["messages"][1]["reasoning_details"][1]["summary"] = json!("");
        let decoded = chat_client()
            .decode_client_request(empty.to_string().as_bytes())
            .unwrap();
        assert!(
            matches!(&decoded.task.semantic.items()[1].1, Item::Reasoning(r)
            if matches!(&r.parts[..], [(_, ReasoningContent::Text(t))] if t.as_str().is_empty())
                && r.replay.as_ref().unwrap().replay_token() == Some("sig-1"))
        );
        assert!(
            matches!(&decoded.task.semantic.items()[2].1, Item::Reasoning(r)
            if matches!(&r.parts[..], [(_, ReasoningContent::Summary(t))] if t.as_str().is_empty())
                && r.replay.as_ref().unwrap().replay_token() == Some("tok-2"))
        );
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
        protocol::openai::{
            CodecError, DecodedRequest, chat::envelope as chat_envelope, responses::envelope,
        },
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
