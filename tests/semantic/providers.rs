//! Provider dialects and observations over the shared IR.
mod provider_profiles {
    //! Independent synthetic oracles for scoped provider boundary differences.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::value::ReplayOrigin,
    };
    use serde_json::{Value, json};
    fn adapter(dialect: Dialect) -> Adapter {
        Adapter::new(
            Profile::Chat,
            dialect,
            Some(ReplayOrigin::new("synthetic-account").unwrap()),
        )
    }
    fn body() -> Value {
        json!({"id":"c1","object":"chat.completion","model":"m","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning_content":"think"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}})
    }

    #[test]
    fn zhipu_effort_lands_on_its_three_levels_by_a_named_rule() {
        use morphiecore::topology::{ProtocolProfile, catalog::API_KEY_BINDINGS};
        let endpoint = API_KEY_BINDINGS
            .iter()
            .find(|binding| binding.endpoint_prefix == "zhipu")
            .unwrap()
            .endpoint(ProtocolProfile::OpenAiChat)
            .unwrap();
        // glm-5.3 always thinks and takes only low, high or max (error 1210 otherwise).
        for (effort, sent) in [("medium", "high"), ("minimal", "low"), ("high", "high")] {
            let body = json!({"model":"m","messages":[{"role":"user","content":"q"}],"reasoning_effort":effort});
            let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
            let request = client.decode_request(body.to_string().as_bytes()).unwrap();
            let (wire, rules) = endpoint.encode_request_with_rules(&request).unwrap();
            assert_eq!(wire["reasoning_effort"], sent, "{effort}");
            assert_eq!(rules.contains(&"zhipu.glm-5.3.effort.v1"), effort != "high");
        }
    }
    #[test]
    fn unversioned_readable_views_are_closed_and_never_restore_deleted_reasoning() {
        use morphiecore::semantic::task::generation::{Item, ReasoningContent};
        use morphiecore::semantic::value::Text;
        let provider = adapter(Dialect::OpenCodeGo);
        let details =
            json!([{"type":"reasoning.text","text":"think","format":"unknown","index":0}]);
        let mut source = body();
        let message = source["choices"][0]["message"].as_object_mut().unwrap();
        message.remove("reasoning_content");
        message.insert("reasoning".into(), json!("think"));
        message.insert("reasoning_details".into(), details.clone());
        source["usage"]["prompt_tokens_details"] =
            json!({"audio_tokens":0,"cached_tokens":0,"cache_write_tokens":0});
        source["usage"]["completion_tokens_details"] =
            json!({"audio_tokens":0,"reasoning_tokens":1});
        let decoded = provider
            .decode_response(source.to_string().as_bytes())
            .unwrap();
        let client = adapter(Dialect::MorphieCore);
        let wire = client.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(wire["choices"][0]["message"]["reasoning_content"], "think");
        assert_eq!(
            wire["usage"]["completion_tokens_details"]["reasoning_tokens"],
            1
        );
        assert_eq!(
            wire["usage"]["completion_tokens_details"]["audio_tokens"],
            0
        );
        // OpenRouter sends the same view for non-OpenAI models (`z-ai/glm-5.3`).
        assert!(
            adapter(Dialect::OpenRouter)
                .decode_response(source.to_string().as_bytes())
                .is_ok()
        );
        assert!(
            adapter(Dialect::DeepSeek)
                .decode_response(source.to_string().as_bytes())
                .is_err()
        );
        for (path, value) in [
            ("/choices/0/message/reasoning", json!("disagree")),
            (
                "/choices/0/message/reasoning_details/0/type",
                json!("reasoning.encrypted"),
            ),
            (
                "/choices/0/message/reasoning_details/0/format",
                json!("arbitrary"),
            ),
            ("/choices/0/message/reasoning_details/0/index", json!(1)),
            ("/usage/completion_tokens_details/audio_tokens", json!(-1)),
            (
                "/usage/prompt_tokens_details/audio_tokens",
                json!("unknown"),
            ),
        ] {
            let mut invalid = source.clone();
            *invalid.pointer_mut(path).unwrap() = value;
            assert!(
                provider
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
        let chunk = |delta: Value, finish: Value| json!({"id":"c1","object":"chat.completion.chunk","model":"m","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
        let mut stream = provider.event_decoder();
        stream
            .push(&chunk(
                json!({"role":"assistant","reasoning":"think","reasoning_details":details}),
                Value::Null,
            ))
            .unwrap();
        stream
            .push(&chunk(json!({"content":"pong"}), Value::Null))
            .unwrap();
        stream.push(&chunk(json!({}), json!("stop"))).unwrap();
        stream.push(&json!({"id":"c1","object":"chat.completion.chunk","model":"m","created":1,"choices":[],"usage":source["usage"]})).unwrap();
        stream.done().unwrap();
        assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
        let history = json!({"model":"hy4-preview","messages":[source["choices"][0]["message"],{"role":"user","content":"next"}],"max_tokens":37});
        let mut request = provider
            .decode_request(history.to_string().as_bytes())
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation(
                "synthetic-conversation",
            )
            .unwrap(),
        );
        let mut items = request.task.semantic.items().to_vec();
        let Item::Reasoning(reason) = &mut items[0].1 else {
            panic!("reasoning owner")
        };
        reason.parts[0].1 =
            ReasoningContent::Text(Text::new("replacement", "reasoning", 256).unwrap());
        request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
        let changed = provider
            .encode_request(&request, "hy4-preview", &Contract::full())
            .unwrap();
        assert_eq!(changed["messages"][0]["reasoning"], "replacement");
        assert!(changed["messages"][0].get("reasoning_details").is_none());
        let projected = provider
            .decode_request(changed.to_string().as_bytes())
            .unwrap();
        assert_eq!(projected.task.semantic, request.task.semantic);
        request.task.semantic = request
            .task
            .semantic
            .clone()
            .retain_items(|_, i| !matches!(i, Item::Reasoning(_)))
            .unwrap();
        let deleted = provider
            .encode_request(&request, "hy4-preview", &Contract::full())
            .unwrap();
        assert!(deleted["messages"][0].get("reasoning").is_none());
        assert!(deleted["messages"][0].get("reasoning_details").is_none());
        assert_eq!(deleted["messages"][0]["content"], "pong");
    }

    #[test]
    fn queued_creation_cannot_mask_missing_progress_or_relax_standard_events() {
        let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
        let source = crate::wire::events(2);
        let mut queued = source[0].clone();
        queued["response"]["status"] = json!("queued");
        assert!(
            Adapter::new(Profile::Responses, Dialect::Standard, None)
                .event_decoder()
                .push(&queued)
                .is_err()
        );
        for next in [
            queued.clone(),
            source[0].clone(),
            source[1].clone(),
            source.last().unwrap().clone(),
        ] {
            let mut decoder = provider.event_decoder();
            decoder.push(&queued).unwrap();
            let mut next = next;
            next["sequence_number"] = json!(1);
            assert!(decoder.push(&next).is_err());
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        }
        let mut missing = queued.clone();
        missing["response"]
            .as_object_mut()
            .unwrap()
            .shift_remove("output");
        assert!(provider.event_decoder().push(&missing).is_err());
        // A plain queued event is not proof of a created event under the alias rule.
        queued["type"] = json!("response.queued");
        let mut decoder = provider.event_decoder();
        decoder.push(&queued).unwrap();
        let mut progress = source[0].clone();
        progress["type"] = json!("response.in_progress");
        progress["sequence_number"] = json!(1);
        assert!(decoder.push(&progress).is_err());
    }

    fn summary_alias_fixture() -> (Value, Vec<Value>) {
        let item = json!({"id":"r","type":"reasoning","status":null,"content":null,
        "summary":[{"type":"summary_text","text":"brief"}]});
        let mut expected = crate::wire::response(2);
        expected["output"]
            .as_array_mut()
            .unwrap()
            .insert(0, item.clone());
        let base = crate::wire::events(2);
        let mut events = vec![
            base[0].clone(),
            json!({"type":"response.output_item.added","output_index":0,"item":{"id":"r","type":"reasoning","status":null,"summary":[],"content":null}}),
            json!({"type":"response.reasoning_text.delta","output_index":0,"item_id":"r","content_index":0,"delta":"brief"}),
            json!({"type":"response.reasoning_text.done","output_index":0,"item_id":"r","content_index":0,"text":"brief"}),
            json!({"type":"response.output_item.done","output_index":0,"item":item}),
        ];
        for mut event in base.into_iter().skip(1) {
            if event.get("output_index").is_some() {
                event["output_index"] = json!(1);
            }
            if event["type"] == "response.completed" {
                event["response"] = expected.clone();
            }
            events.push(event);
        }
        for (n, event) in events.iter_mut().enumerate() {
            event["sequence_number"] = json!(n);
        }
        (expected, events)
    }

    #[test]
    fn reasoning_event_alias_rejects_incomplete_or_conflicting_summary_snapshots() {
        let provider = Adapter::new(Profile::Responses, Dialect::Bailian, None);
        let (_, source) = summary_alias_fixture();
        let mut missing_done = source.clone();
        missing_done.remove(3);
        let mut wrong_done = source.clone();
        wrong_done[3]["text"] = json!("changed");
        let mut wrong_snapshot = source.clone();
        wrong_snapshot[4]["item"]["summary"][0]["text"] = json!("changed");
        let mut wrong_domain = source.clone();
        wrong_domain[2]["summary_index"] = json!(0);
        for invalid in [missing_done, wrong_done, wrong_snapshot, wrong_domain] {
            let mut decoder = provider.event_decoder();
            assert!(invalid.iter().any(|event| decoder.push(event).is_err()));
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        }
    }

    #[test]
    fn inactive_response_state_placeholders_do_not_enable_remote_state() {
        let provider = Adapter::new(Profile::Responses, Dialect::Zhipu, None);
        let mut value = crate::wire::response(2);
        value["previous_response_id"] = json!("");
        value["conversation_id"] = json!("");
        let decoded = provider
            .decode_response(value.to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.semantic.usage().unwrap().total_tokens, Some(8));
        let encoded = provider
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(encoded["previous_response_id"], Value::Null);
        assert!(encoded.get("conversation_id").is_none());
        for dialect in [Dialect::Standard, Dialect::MorphieCore, Dialect::Bailian] {
            assert!(
                Adapter::new(Profile::Responses, dialect, None)
                    .decode_response(value.to_string().as_bytes())
                    .is_err()
            );
        }
        assert!(
            provider
                .decode_request(br#"{"model":"m","input":"hi","previous_response_id":""}"#)
                .is_err()
        );
        for field in ["previous_response_id", "conversation_id", "conversation"] {
            for invalid in [json!("synthetic-state"), json!({}), json!(0), json!(false)] {
                let mut bad = value.clone();
                bad[field] = invalid;
                assert!(
                    provider
                        .decode_response(bad.to_string().as_bytes())
                        .is_err()
                );
            }
        }
        for conflict in [false, true] {
            use morphiecore::protocol::{
                openai::responses::sse::ResponsesSseDecoder, stream::SseLimits,
            };
            let mut decoder = ResponsesSseDecoder::with_decoder(
                200,
                "text/event-stream",
                SseLimits::default(),
                provider.event_decoder(),
            )
            .unwrap();
            for mut event in crate::wire::events(2) {
                let terminal = event["type"] == "response.completed";
                if let Some(snapshot) = event.get_mut("response") {
                    snapshot["previous_response_id"] = json!("");
                    snapshot["conversation_id"] = if terminal && conflict {
                        json!("synthetic-state")
                    } else {
                        Value::Null
                    };
                }
                let frame = format!("data: {event}\n\n");
                let result = decoder.consume(frame.as_bytes());
                if terminal && conflict {
                    assert!(result.is_err());
                } else {
                    assert_eq!(result.unwrap().0, frame.len());
                }
            }
            if conflict {
                assert!(decoder.finish().is_err());
                assert!(decoder.materialize().is_err());
            } else {
                decoder.finish().unwrap();
                assert_eq!(decoder.materialize().unwrap().semantic, decoded.semantic);
            }
        }
    }

    fn terminal_reasoning_fixture() -> (Value, Vec<Value>) {
        let (mut snapshot, source) = summary_alias_fixture();
        snapshot["output"][0] = json!({"id":"r","type":"reasoning","status":"completed","summary":[],"content":[{"text":"brief"}]});
        let mut events = vec![];
        for mut event in source {
            if event["type"] == "response.reasoning_text.done"
                || (event["type"] == "response.output_item.done" && event["output_index"] == 0)
            {
                continue;
            }
            if event["type"] == "response.output_item.added" && event["output_index"] == 0 {
                event["item"] = json!({"id":"r","type":"reasoning","status":"in_progress","summary":[],"content":[]});
            }
            if event["type"] == "response.completed" {
                event["response"] = snapshot.clone();
            }
            event["sequence_number"] = json!(events.len());
            events.push(event);
        }
        (snapshot, events)
    }

    #[test]
    fn terminal_reasoning_snapshot_cannot_recover_missing_text_identity_or_other_lifecycles() {
        let provider = Adapter::new(Profile::Responses, Dialect::Nvidia, None);
        let (_, source) = terminal_reasoning_fixture();
        for (path, value) in [
            ("/response/output/0/id", json!("other")),
            ("/response/output/0/status", json!("in_progress")),
            ("/response/output/0/status", Value::Null),
            ("/response/output/0/content/0/text", json!("changed")),
            (
                "/response/output/0/content/0",
                json!({"text":"brief","unknown":true}),
            ),
        ] {
            let mut invalid = source.clone();
            *invalid.last_mut().unwrap().pointer_mut(path).unwrap() = value;
            let mut decoder = provider.event_decoder();
            assert!(invalid.iter().any(|e| decoder.push(e).is_err()));
            assert!(decoder.push(source.last().unwrap()).is_err());
            assert!(decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        }
        for removed_type in [
            "response.reasoning_text.delta",
            "response.output_item.done",
            "response.completed",
        ] {
            let mut decoder = provider.event_decoder();
            let failed = source
                .iter()
                .filter(|e| e["type"] != removed_type)
                .any(|e| decoder.push(e).is_err());
            assert!(failed || decoder.finish().is_err());
            assert!(decoder.materialize().is_err());
        }
    }

    fn billing_view(value: &mut Value) {
        value["billing"] = Value::Null;
        value["frequency_penalty"] = json!(0.25);
        if value.get("usage").is_some_and(Value::is_object) {
            value["usage"]["x_details"] = json!([{
                "x_billing_type":"response_api", "input_tokens":3,"output_tokens":5,"total_tokens":8,
                "prompt_tokens_details":{"cached_tokens":1,"cache_write_tokens":0},
                "output_tokens_details":{"reasoning_tokens":2}
            }]);
        }
    }

    #[test]
    fn response_billing_view_events_match_static_and_conflicts_poison_terminal() {
        let provider = Adapter::new(
            Profile::Responses,
            Dialect::Bailian,
            Some(ReplayOrigin::new("a").unwrap()),
        );
        let mut value = crate::wire::response(2);
        billing_view(&mut value);
        let expected = provider
            .decode_response(value.to_string().as_bytes())
            .unwrap();
        for conflict in [false, true] {
            let mut decoder = provider.event_decoder();
            for mut event in crate::wire::events(2) {
                let terminal = event["type"] == "response.completed";
                if let Some(response) = event.get_mut("response") {
                    billing_view(response);
                    if terminal && conflict {
                        response["usage"]["x_details"][0]["total_tokens"] = json!(99);
                    }
                }
                let result = decoder.push(&event);
                if terminal && conflict {
                    assert!(result.is_err());
                } else {
                    result.unwrap();
                }
            }
            if conflict {
                assert!(decoder.finish().is_err());
                assert!(decoder.materialize().is_err());
            } else {
                decoder.finish().unwrap();
                let actual = decoder.materialize().unwrap();
                assert_eq!(actual.semantic, expected.semantic);
                let projection = provider
                    .encode_response(&actual, &Contract::full())
                    .unwrap();
                assert_eq!(projection["frequency_penalty"], 0.25);
                assert!(projection["usage"].get("x_details").is_none());
            }
        }
    }

    #[test]
    fn request_diagnostics_are_source_bound_not_public_facts() {
        let provider = adapter(Dialect::Zhipu);
        let mut value = body();
        value["request_id"] = json!("synthetic-request");
        let decoded = provider
            .decode_response(value.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            provider
                .encode_response(&decoded, &Contract::full())
                .unwrap()["request_id"],
            "synthetic-request"
        );
        let other = Adapter::new(
            Profile::Chat,
            Dialect::Zhipu,
            Some(ReplayOrigin::new("other").unwrap()),
        );
        assert!(
            other
                .encode_response(&decoded, &Contract::full())
                .unwrap()
                .get("request_id")
                .is_none()
        );
        assert!(
            adapter(Dialect::MorphieCore)
                .encode_response(&decoded, &Contract::full())
                .unwrap()
                .get("request_id")
                .is_none()
        );
        let mut edited = decoded.clone();
        let items = edited
            .semantic
            .items()
            .iter()
            .filter(|(_, i)| {
                !matches!(
                    i,
                    morphiecore::semantic::task::generation::Item::Reasoning(_)
                )
            })
            .cloned()
            .collect();
        edited.semantic = edited.semantic.with_items(items).unwrap();
        assert!(
            provider
                .encode_response(&edited, &Contract::full())
                .unwrap()
                .get("request_id")
                .is_none()
        );
        value["request_id"] = json!({"not":"a string"});
        assert!(
            provider
                .decode_response(value.to_string().as_bytes())
                .is_err()
        );
    }
    #[test]
    fn token_plan_created_drift_does_not_relax_other_profiles_or_identity() {
        assert!(
            !Adapter::new(Profile::Responses, Dialect::BailianTokenPlan, None)
                .adaptation
                .rules
                .chunk_created_drift
        );
        let first = json!({"id":"r","object":"chat.completion.chunk","model":"synthetic","created":1,
        "choices":[{"index":0,"delta":{"role":"assistant","content":"pong"},"finish_reason":null}]});
        let last = json!({"id":"r","object":"chat.completion.chunk","model":"synthetic","created":2,
        "choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
        for (dialect, admitted) in [
            (Dialect::Standard, false),
            (Dialect::Bailian, false),
            (Dialect::BailianTokenPlan, true),
        ] {
            let provider = adapter(dialect);
            let mut decoder = provider.event_decoder();
            decoder.push(&first).unwrap();
            assert_eq!(decoder.push(&last).is_ok(), admitted);
            if admitted {
                decoder.done().unwrap();
                let decoded = decoder.materialize().unwrap();
                assert_eq!(
                    provider
                        .encode_response(&decoded, &Contract::full())
                        .unwrap()["created"],
                    1
                );
            } else {
                assert!(decoder.finish().is_err());
            }
        }
        for (key, old, changed) in [
            ("id", json!("r"), json!("other")),
            ("model", json!("synthetic"), json!("other")),
            ("created", json!(1), json!(-1)),
            ("system_fingerprint", json!("a"), json!("b")),
            ("service_tier", json!("priority"), json!("default")),
        ] {
            let mut a = first.clone();
            let mut b = last.clone();
            a[key] = old;
            b[key] = changed;
            let mut decoder = adapter(Dialect::BailianTokenPlan).event_decoder();
            decoder.push(&a).unwrap();
            assert!(decoder.push(&b).is_err(), "{key}");
            assert!(decoder.done().is_err());
        }
    }

    #[test]
    fn token_plan_schema_alias_is_response_only_and_rejects_ambiguity() {
        let provider = Adapter::new(Profile::Responses, Dialect::BailianTokenPlan, None);
        let mut source = crate::wire::response(2);
        source["text"] = json!({"format":{
            "type":"json_schema","name":"synthetic","strict":false,"schema_":{"type":"object"}
        }});
        for dialect in [Dialect::Standard, Dialect::MorphieCore, Dialect::Bailian] {
            assert!(
                Adapter::new(Profile::Responses, dialect, None)
                    .decode_response(source.to_string().as_bytes())
                    .is_err()
            );
        }
        assert!(
            provider
                .decode_request(
                    json!({
                        "model":"synthetic","input":"hello","text":source["text"]
                    })
                    .to_string()
                    .as_bytes()
                )
                .is_err()
        );
        for value in [
            Value::Null,
            json!({"type":"object"}),
            json!({"type":"string"}),
        ] {
            let mut ambiguous = source.clone();
            ambiguous["text"]["format"]["schema"] = value;
            assert!(
                provider
                    .decode_response(ambiguous.to_string().as_bytes())
                    .is_err()
            );
            let mut stream = provider.event_decoder();
            ambiguous["status"] = json!("in_progress");
            ambiguous["output"] = json!([]);
            assert!(
                stream
                    .push(&json!({
                        "type":"response.created","sequence_number":0,"response":ambiguous
                    }))
                    .is_err()
            );
        }
        for (key, value) in [
            ("schema_", Value::Null),
            ("schema_", json!(42)),
            ("type", json!("json_object")),
        ] {
            let mut invalid = source.clone();
            invalid["text"]["format"][key] = value;
            assert!(
                provider
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
    }
}
mod provider_observations {
    //! Abstract Provider observations, not synthetic Google/Anthropic codecs.
    use morphiecore::semantic::{task::generation::*, value::Text};

    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", 256).unwrap()
    }
    fn domain() -> NativeAliasDomain {
        NativeAliasDomain {
            source: text("abstract-provider"),
            scope: LocalScope::new(7),
        }
    }
    fn operation() -> ProviderToolObservation {
        ProviderToolObservation {
            source: domain(),
            operation: ProviderOperation::Reported {
                tool: text("search"),
                alias: Some(text("S")),
                requester: ProviderRequester::Model,
                action: Some(ProviderAction::Search {
                    query: Some(text("synthetic query")),
                    queries: None,
                }),
            },
            progress: None,
            execution: None,
            output: None,
            artifact_status: None,
        }
    }
    fn report() -> ProviderToolObservation {
        ProviderToolObservation {
            source: domain(),
            operation: ProviderOperation::Reference(ProviderOperationReference::Native(text("S"))),
            progress: None,
            execution: Some(ToolExecution::Unknown),
            output: None,
            artifact_status: None,
        }
    }
    fn with_action(action: ProviderAction) -> ProviderToolObservation {
        let mut observed = operation();
        let ProviderOperation::Reported { action: value, .. } = &mut observed.operation else {
            unreachable!()
        };
        *value = Some(action);
        observed
    }

    #[test]
    fn observed_actions_are_not_edited_in_place_and_references_need_an_earlier_report() {
        let source = GenerationRequest::new(
            vec![(ItemId::new(1), Item::ProviderTool(operation()))],
            GenerationControls::default(),
        )
        .unwrap();
        let changed = with_action(ProviderAction::Search {
            query: Some(text("changed query")),
            queries: None,
        });
        for owner in [ItemId::new(1), ItemId::new(2)] {
            assert!(
                source
                    .clone()
                    .with_items(vec![(owner, Item::ProviderTool(changed.clone()))])
                    .is_err()
            );
        }
        assert!(
            source
                .clone()
                .with_items(vec![
                    (ItemId::new(1), Item::ProviderTool(operation())),
                    (ItemId::new(2), Item::ProviderTool(report())),
                ])
                .is_ok()
        );
        assert!(
            GenerationRequest::new(
                vec![(ItemId::new(2), Item::ProviderTool(report()))],
                GenerationControls::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn fragmented_provider_results_have_independent_static_values_and_no_snapshot_repair() {
        for (format, parts, expected) in [
            (
                ProviderResultFormat::Text,
                vec!["你", "好"],
                ToolOutput::Text("你好".into()),
            ),
            (
                ProviderResultFormat::Text,
                vec![""],
                ToolOutput::Text(String::new()),
            ),
            (
                ProviderResultFormat::Json,
                vec![r#"{"n":"#, "9007199254740993", "}"],
                ToolOutput::Structured(
                    StructuredValue::from_bytes(br#"{"n":9007199254740993}"#).unwrap(),
                ),
            ),
        ] {
            for fragments in [parts.clone(), vec![]] {
                let joined = parts.concat();
                let fragments = if fragments.is_empty() {
                    vec![joined.as_str()]
                } else {
                    fragments
                };
                let mut observed = report();
                observed.execution = None;
                let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
                state = reduce(
                    state,
                    StreamEvent::ItemStarted {
                        item: ItemId::new(1),
                        kind: ItemKind::ProviderResult {
                            observation: observed.clone(),
                            format,
                        },
                        replay: None,
                    },
                )
                .unwrap();
                state = reduce(
                    state,
                    StreamEvent::PartStarted {
                        item: ItemId::new(1),
                        part: PartId::new(1),
                        kind: if format == ProviderResultFormat::Json {
                            PartKind::ResultJson
                        } else {
                            PartKind::ResultText
                        },
                    },
                )
                .unwrap();
                for fragment in fragments {
                    state = reduce(
                        state,
                        StreamEvent::Delta {
                            item: ItemId::new(1),
                            part: PartId::new(1),
                            fragment: fragment.into(),
                            logprobs: vec![],
                        },
                    )
                    .unwrap();
                }
                assert!(snapshot_items(&state).is_err());
                assert!(
                    reduce(
                        state.clone(),
                        StreamEvent::ItemFinished {
                            item: ItemId::new(1),
                            status: ItemLifecycle::Completed,
                            replay: None
                        }
                    )
                    .is_err()
                );
                state = reduce(
                    state,
                    StreamEvent::ValueFinished {
                        item: ItemId::new(1),
                        part: PartId::new(1),
                    },
                )
                .unwrap();
                state = reduce(
                    state,
                    StreamEvent::PartFinished {
                        item: ItemId::new(1),
                        part: PartId::new(1),
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
                observed.output = Some(expected.clone());
                assert_eq!(
                    materialize(&state).unwrap(),
                    GenerationResponse::new(
                        vec![(ItemId::new(1), Item::ProviderTool(observed))],
                        Outcome::Completed
                    )
                    .unwrap()
                );
                assert!(
                    reduce(
                        state,
                        StreamEvent::ValueFinished {
                            item: ItemId::new(1),
                            part: PartId::new(1)
                        }
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn report_payload_execution_and_artifact_facts_are_independent_and_bounded() {
        let mut observed = report();
        observed.execution = None;
        assert!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
                Outcome::Completed
            )
            .is_err()
        );
        observed.execution = Some(ToolExecution::Cancelled);
        let response = GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
            Outcome::Completed,
        )
        .unwrap();
        assert_eq!(response.outcome(), Outcome::Completed);
        observed.execution = Some(ToolExecution::Failed {
            code: Some(text("denied")),
        });
        observed.output = Some("diagnostic".into());
        observed.artifact_status = Some(ItemLifecycle::Incomplete);
        GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(observed.clone()))],
            Outcome::Completed,
        )
        .unwrap();
        observed.output = Some(ToolOutput::Text("x".repeat(MAX_TEXT_BYTES + 1)));
        assert!(
            GenerationResponse::new(
                vec![(ItemId::new(1), Item::ProviderTool(observed))],
                Outcome::Completed
            )
            .is_err()
        );
    }

    #[test]
    fn local_references_need_no_native_alias_and_later_facts_do_not_erase_unknown_reports() {
        let mut initial = operation();
        let ProviderOperation::Reported { alias, .. } = &mut initial.operation else {
            unreachable!()
        };
        *alias = None;
        let mut unknown = report();
        unknown.operation =
            ProviderOperation::Reference(ProviderOperationReference::Local(ItemId::new(1)));
        let mut later = unknown.clone();
        later.execution = Some(ToolExecution::Succeeded);
        later.output = Some(ToolOutput::Text(String::new()));
        let history = GenerationRequest::new(
            vec![
                (ItemId::new(1), Item::ProviderTool(initial)),
                (ItemId::new(2), Item::ProviderTool(unknown.clone())),
                (ItemId::new(3), Item::ProviderTool(later)),
            ],
            GenerationControls::default(),
        )
        .unwrap();
        assert_eq!(history.items()[1].1, Item::ProviderTool(unknown));
        assert_eq!(history.continuation(), Continuation::Unreported);
        let mut changed = history.items().to_vec();
        changed.swap(0, 1);
        assert!(history.with_items(changed).is_err());
    }

    #[test]
    fn provider_observations_are_not_implicitly_admitted_by_existing_product_contracts() {
        // A core type is not a product capability.
        let response = GenerationResponse::new(
            vec![(ItemId::new(1), Item::ProviderTool(operation()))],
            Outcome::Completed,
        )
        .unwrap();
        let request =
            GenerationRequest::new(response.items().to_vec(), GenerationControls::default())
                .unwrap();
        let contract = morphiecore::protocol::openai::TargetCaps::full().semantics;
        assert!(contract.check(&request).is_err());
        assert!(contract.check_response(&response).is_err());
    }

    #[test]
    fn stream_alias_conflicts_and_existing_call_capability_checks_remain_strict() {
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        let state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::ProviderTool(operation()),
                replay: None,
            },
        )
        .unwrap();
        assert!(
            reduce(
                state,
                StreamEvent::ItemStarted {
                    item: ItemId::new(2),
                    kind: ItemKind::ProviderTool(operation()),
                    replay: None
                }
            )
            .is_err()
        );
        let state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        let state = reduce(
            state,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
        )
        .unwrap();
        let call = StreamEvent::ItemStarted {
            item: ItemId::new(2),
            kind: ItemKind::ToolCall {
                format: ArgumentFormat::Raw,
                call_id: text("C"),
                name: text("lookup"),
                context: CallContext::default(),
            },
            replay: None,
        };
        let mut contract = morphiecore::protocol::openai::TargetCaps::full();
        contract.semantics.tools = false;
        assert!(
            morphiecore::protocol::openai::lowering::events::check_event(
                &state,
                &call,
                morphiecore::protocol::openai::Profile::Chat,
                &contract
            )
            .is_err()
        );
    }
}
mod router_adapter {
    //! Synthetic router wire shapes; no account, network or model-quality oracle.
    use morphiecore::{
        protocol::{
            openai::{
                Adapter, Dialect, Profile, TargetCaps as Contract,
                chat::sse::{ChatSseDecoder, ChatSseEncoder},
                events::EventEncoder,
                stream::Obfuscation,
            },
            stream::SseLimits,
        },
        semantic::{
            context::StreamOptions,
            value::{Presence, ReplayOrigin},
        },
    };
    use serde_json::{Value, json};

    fn adapter(profile: Profile, dialect: Dialect, scope: &str) -> Adapter {
        Adapter::new(profile, dialect, Some(ReplayOrigin::new(scope).unwrap()))
    }
    fn chat() -> Value {
        json!({"id":"r1","object":"chat.completion","created":2,"model":"openai/gpt-6-luna",
        "provider":"Synthetic","choices":[{"index":0,"finish_reason":"stop","native_finish_reason":"stop",
        "message":{"role":"assistant","content":"pong","reasoning":"think","reasoning_details":null}}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,
        "cost":0.00000123,"is_byok":false,"cost_details":{"upstream_inference_cost":null},
        "prompt_tokens_details":{"cached_tokens":1},"completion_tokens_details":{"reasoning_tokens":1}}})
    }
    #[test]
    fn router_rejects_unknown_or_unowned_shapes_and_never_masks_invalid_usage() {
        let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
        for (pointer, value) in [
            ("/usage/cost", json!("not-money")),
            ("/usage/prompt_tokens", json!(-1)),
            (
                "/choices/0/message/reasoning_details",
                json!([{"type":"reasoning.encrypted","data":"synthetic"}]),
            ),
            ("/provider", json!("x".repeat(4097))),
        ] {
            let mut body = chat();
            *body.pointer_mut(pointer).unwrap() = value;
            assert!(
                source.decode_response(body.to_string().as_bytes()).is_err(),
                "{pointer}"
            );
        }
        let mut body = chat();
        body["choices"][0]["message"]["reasoning_content"] = json!("conflict");
        assert!(source.decode_response(body.to_string().as_bytes()).is_err());
        assert!(
            Adapter::new(Profile::Chat, Dialect::OpenRouter, None)
                .decode_response(chat().to_string().as_bytes())
                .is_err()
        );
    }

    fn chunk(delta: Value, reason: Value) -> Value {
        json!({"id":"r1","object":"chat.completion.chunk","created":2,"model":"openai/gpt-6-luna",
        "provider":"Synthetic","choices":[{"index":0,"delta":delta,"finish_reason":reason,"native_finish_reason":reason}]})
    }

    #[test]
    fn responses_done_trailer_requires_a_real_terminal_and_cannot_repeat() {
        use morphiecore::protocol::openai::responses::sse::ResponsesSseDecoder;
        let frames = crate::wire::events(2);
        let body = frames
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        for (dialect, wire, accepted) in [
            (Dialect::OpenRouter, format!("{body}data: [DONE]\n\n"), true),
            (Dialect::Zhipu, format!("{body}data: [DONE]\n\n"), true),
            (Dialect::Nvidia, format!("{body}data: [DONE]\n\n"), true),
            (Dialect::Nvidia, "data: [DONE]\n\n".into(), false),
            (
                Dialect::Nvidia,
                format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
                false,
            ),
            (Dialect::Zhipu, "data: [DONE]\n\n".into(), false),
            (
                Dialect::Zhipu,
                format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
                false,
            ),
            (Dialect::Standard, format!("{body}data: [DONE]\n\n"), false),
            (Dialect::OpenRouter, "data: [DONE]\n\n".into(), false),
            (
                Dialect::OpenRouter,
                format!("{body}data: [DONE]\n\ndata: [DONE]\n\n"),
                false,
            ),
        ] {
            let mut decoder = ResponsesSseDecoder::with_decoder(
                200,
                "text/event-stream",
                SseLimits::default(),
                adapter(Profile::Responses, dialect, "source").event_decoder(),
            )
            .unwrap();
            let mut remaining = wire.as_bytes();
            let mut ok = true;
            while !remaining.is_empty() {
                match decoder.consume(remaining) {
                    Ok((n, _)) if n > 0 => remaining = &remaining[n..],
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            ok = decoder.finish().is_ok() && ok;
            assert_eq!(ok, accepted);
        }
    }

    #[test]
    fn router_reasoning_format_is_a_checked_marker_not_an_opaque_escape() {
        let source = adapter(Profile::Responses, Dialect::OpenRouter, "source");
        let mut body = json!({"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[
        {"type":"reasoning","id":"rs","status":"completed","summary":[],"encrypted_content":"synthetic","format":"openai-responses-v1"}
    ],"usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2,"cost_details":{"upstream_inference_input_cost":0.01,"upstream_inference_output_cost":0.02}}});
        let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
        let c = Contract {
            replay_origin: Some(ReplayOrigin::new("source").unwrap()),
            ..Contract::full()
        };
        let wire = source.encode_response(&decoded, &c).unwrap();
        assert!(wire["output"][0].get("format").is_none());
        assert_eq!(
            wire["usage"]["cost_details"]["upstream_inference_output_cost"],
            0.02
        );
        body["output"][0]["format"] = json!("unrecognized-signature-v1");
        assert!(source.decode_response(body.to_string().as_bytes()).is_err());
        // Unclassified reasoning (observed for z-ai/glm-5.3-flash) is readable text only.
        body["output"][0] = json!({"type":"reasoning","id":"rs","status":"completed","summary":[],
            "content":[{"type":"reasoning_text","text":"think"}],"format":"unknown"});
        let decoded = source.decode_response(body.to_string().as_bytes()).unwrap();
        use morphiecore::semantic::task::generation::{Item, ReasoningContent};
        let Item::Reasoning(reasoning) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        assert!(reasoning.replay.is_none());
        assert!(
            matches!(&reasoning.parts[..], [(_, ReasoningContent::Text(t))] if t.as_str() == "think")
        );
        body["output"][0]["encrypted_content"] = json!("synthetic");
        assert!(source.decode_response(body.to_string().as_bytes()).is_err());
    }

    #[test]
    fn router_repeated_finish_usage_is_content_free_and_matches_static() {
        let source = adapter(Profile::Chat, Dialect::OpenRouter, "source");
        let mut tail = chunk(json!({"role":"assistant","content":""}), json!("stop"));
        tail["usage"] = chat()["usage"].clone();
        let frames = vec![
            chunk(json!({"role":"assistant","reasoning":"think"}), Value::Null),
            chunk(json!({"content":"pong"}), Value::Null),
            chunk(json!({}), json!("stop")),
            tail.clone(),
        ];
        let wire = frames
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>()
            + "data: [DONE]\n\n";
        let mut stream = ChatSseDecoder::with_decoder(
            200,
            "text/event-stream",
            SseLimits::default(),
            source.event_decoder(),
        )
        .unwrap();
        let mut events = vec![];
        for byte in wire.as_bytes() {
            let (n, batch) = stream.consume(&[*byte]).unwrap();
            assert_eq!(n, 1);
            events.extend(batch);
        }
        stream.finish().unwrap();
        let decoded = stream.materialize().unwrap();
        let expected = source
            .decode_response(chat().to_string().as_bytes())
            .unwrap();
        assert_eq!(decoded.semantic, expected.semantic);
        assert_eq!(
            source.encode_response(&decoded, &Contract::full()).unwrap(),
            source
                .encode_response(&expected, &Contract::full())
                .unwrap()
        );
        let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
            .unwrap()
            .with_contract(source.contract(&Contract::full()));
        let mut encoded = vec![];
        for event in &events {
            encoded.extend(encoder.encode(event, &decoded.fidelity).unwrap());
        }
        encoder.finish().unwrap();
        assert!(encoded[0].get("provider").is_none());
        assert_eq!(encoded.last().unwrap()["usage"]["cost"], json!(0.00000123));
        for include_usage in [false, true] {
            let mut framed = ChatSseEncoder::new(
                decoded.metadata.clone(),
                source.contract(&Contract::full()),
                SseLimits::default(),
                StreamOptions {
                    include_usage: Presence::Value(include_usage),
                    include_obfuscation: Presence::Value(false),
                },
                Obfuscation::Disabled,
            )
            .unwrap();
            let mut bytes = Vec::new();
            for event in &events {
                for frame in framed.encode(event, &decoded.fidelity).unwrap() {
                    bytes.extend_from_slice(&frame);
                }
            }
            let text = String::from_utf8(bytes).unwrap();
            assert_eq!(text.contains("\"cost\""), include_usage);
            assert_eq!(text.matches("data: [DONE]").count(), 1);
        }
        for bad in [
            {
                let mut v = tail.clone();
                v["choices"][0]["delta"]["content"] = json!("late");
                v
            },
            {
                let mut v = tail.clone();
                v["choices"][0]["finish_reason"] = json!("length");
                v
            },
            {
                let mut v = tail.clone();
                v["choices"][0]["delta"]["tool_calls"] = json!([]);
                v
            },
        ] {
            let mut decoder = source.event_decoder();
            for frame in &frames[..3] {
                decoder.push(frame).unwrap();
            }
            assert!(decoder.push(&bad).is_err());
            assert!(decoder.done().is_err());
        }
        let mut decoder = source.event_decoder();
        for frame in &frames {
            decoder.push(frame).unwrap();
        }
        assert!(decoder.finish().is_err(), "finish/usage is not DONE");
        assert!(decoder.push(&tail).is_err(), "duplicate usage must fail");
        assert!(decoder.done().is_err());
    }
}
mod modelbest {
    //! Independent wire fixtures for the selected hosted Chat adapter, not model inventory.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract},
        semantic::value::ReplayOrigin,
    };
    use serde_json::{Value, json};

    fn provider() -> Adapter {
        Adapter::new(
            Profile::Chat,
            Dialect::ModelBest,
            Some(ReplayOrigin::new("synthetic-source").unwrap()),
        )
    }
    fn chunk(delta: Value, finish: Value) -> Value {
        json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    }
    #[test]
    fn modelbest_flat_reasoning_counts_and_stop_diagnostics_are_scoped_and_editable() {
        let wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","reasoning_content":null,"tool_calls":null},"finish_reason":"stop","logprobs":null,"matched_stop":777}],
        "metadata":{"serving_tag":"synthetic"},
        "usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7,"reasoning_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}});
        let decoded = provider()
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        assert_eq!(decoded.semantic.usage().unwrap().reasoning_tokens, Some(2));
        let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
        let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(
            delivered["usage"]["completion_tokens_details"]["reasoning_tokens"],
            2
        );
        assert!(delivered["usage"].get("reasoning_tokens").is_none());
        assert!(delivered["choices"][0].get("matched_stop").is_none());
        let replayed = provider()
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(replayed["usage"]["reasoning_tokens"], 2);
        assert!(replayed["usage"].get("completion_tokens_details").is_none());
        for count in [Some(1), None] {
            let mut edited = decoded.clone();
            let mut usage = edited.semantic.usage().unwrap();
            usage.reasoning_tokens = count;
            edited.semantic = edited.semantic.with_usage(Some(usage)).unwrap();
            let encoded = provider()
                .encode_response(&edited, &Contract::full())
                .unwrap();
            assert_eq!(
                encoded["usage"].get("reasoning_tokens"),
                count.map(|value| json!(value)).as_ref()
            );
        }
        for count in [json!(-1), json!(1.5), json!("2"), json!(true)] {
            let mut invalid = wire.clone();
            invalid["usage"]["reasoning_tokens"] = count;
            assert!(
                provider()
                    .decode_response(&serde_json::to_vec(&invalid).unwrap())
                    .is_err()
            );
        }
        let mut conflicting = wire.clone();
        conflicting["usage"]["completion_tokens_details"] = json!({"reasoning_tokens":1});
        assert!(
            provider()
                .decode_response(&serde_json::to_vec(&conflicting).unwrap())
                .is_err()
        );
        conflicting["usage"]["completion_tokens_details"] = json!({"reasoning_tokens":2});
        assert_eq!(
            provider()
                .decode_response(&serde_json::to_vec(&conflicting).unwrap())
                .unwrap()
                .semantic,
            decoded.semantic
        );
        let mut unknown = wire.clone();
        unknown["usage"]["reasoning_tokens"] = Value::Null;
        assert_eq!(
            provider()
                .decode_response(&serde_json::to_vec(&unknown).unwrap())
                .unwrap()
                .semantic
                .usage()
                .unwrap()
                .reasoning_tokens,
            None
        );
        let mut stream = provider().event_decoder();
        for event in [
            chunk(json!({"role":"assistant","content":"pong"}), Value::Null),
            chunk(json!({}), json!("stop")),
            json!({"id":"r","object":"chat.completion.chunk","model":"synthetic-model","created":1,"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7,"reasoning_tokens":2,"prompt_tokens_details":{"cached_tokens":0}}}),
        ] {
            stream.push(&event).unwrap();
        }
        stream.done().unwrap();
        assert_eq!(stream.materialize().unwrap().semantic, decoded.semantic);
    }

    #[test]
    fn modelbest_inactive_server_shapes_do_not_erase_active_data_or_reported_counters() {
        let mut wire = json!({"id":"r","object":"chat.completion","model":"synthetic-model","created":1,
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong","refusal":null,"reasoning":null,"annotations":null,"audio":null,"function_call":null},"finish_reason":"stop","logprobs":null,"stop_reason":null,"token_ids":null,"routed_experts":null}],
        "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10,"prompt_tokens_details":{"cached_tokens":2,"created_cache_tokens":3,"multimodal_tokens":{"image":1}}},
        "prompt_logprobs":null,"prompt_token_ids":null,"prompt_text":null,"kv_transfer_params":null,"ec_transfer_params":null,"metrics":null});
        let decoded = provider()
            .decode_response(&serde_json::to_vec(&wire).unwrap())
            .unwrap();
        let usage = decoded.semantic.usage().unwrap();
        assert_eq!(usage.input_cache_write_tokens, Some(3));
        assert_eq!(usage.input_image_tokens, Some(1));
        let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
        let delivered = client.encode_response(&decoded, &Contract::full()).unwrap();
        assert_eq!(
            delivered["usage"]["prompt_tokens_details"],
            json!({"cached_tokens":2,"cache_write_tokens":3,"image_tokens":1})
        );
        let encoded = provider()
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            encoded["usage"]["prompt_tokens_details"],
            json!({"cached_tokens":2,"created_cache_tokens":3,"multimodal_tokens":{"image":1}})
        );
        for (write, image) in [(Some(0), None), (None, Some(0)), (None, None)] {
            let mut edited = decoded.clone();
            let mut report = edited.semantic.usage().unwrap();
            report.input_cache_write_tokens = write;
            report.input_image_tokens = image;
            edited.semantic = edited.semantic.with_usage(Some(report)).unwrap();
            let encoded = provider()
                .encode_response(&edited, &Contract::full())
                .unwrap();
            assert_eq!(
                encoded["usage"]["prompt_tokens_details"].get("created_cache_tokens"),
                write.map(|v| json!(v)).as_ref()
            );
            assert_eq!(
                encoded["usage"]["prompt_tokens_details"]
                    .get("multimodal_tokens")
                    .and_then(|v| v.get("image")),
                image.map(|v| json!(v)).as_ref()
            );
        }
        for path in [
            "/prompt_logprobs",
            "/prompt_token_ids",
            "/prompt_text",
            "/kv_transfer_params",
            "/ec_transfer_params",
            "/metrics",
            "/choices/0/token_ids",
            "/choices/0/stop_reason",
            "/choices/0/routed_experts",
            "/choices/0/message/annotations",
            "/choices/0/message/audio",
        ] {
            let mut active = wire.clone();
            *active.pointer_mut(path).unwrap() = json!("active");
            assert!(
                provider()
                    .decode_response(&serde_json::to_vec(&active).unwrap())
                    .is_err(),
                "{path}"
            );
        }
        wire["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(2);
        assert!(
            provider()
                .decode_response(&serde_json::to_vec(&wire).unwrap())
                .is_err()
        );
        wire["usage"]["prompt_tokens_details"]
            .as_object_mut()
            .unwrap()
            .remove("cache_write_tokens");
        wire["usage"]["prompt_tokens_details"]["multimodal_tokens"] = Value::Null;
        assert_eq!(
            provider()
                .decode_response(&serde_json::to_vec(&wire).unwrap())
                .unwrap()
                .semantic
                .usage()
                .unwrap()
                .input_image_tokens,
            None
        );
    }
}
mod extensions {
    //! Provider-scoped extension carriers: Codex header lifecycle.
    use morphiecore::protocol::extensions::{CodexHeaders, RoutingHint};

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }
    #[test]
    fn codex_header_lifecycle_fields_round_trip_with_distinct_owners() {
        let wire = headers(&[
            ("session-id", "cache-or-session"),
            ("thread-id", "thread-1"),
            ("x-client-request-id", "thread-1"),
            ("x-codex-window-id", "window-1"),
            ("x-codex-turn-state", "opaque-token"),
            ("x-codex-routing-hint", "model=gpt-6-astra;tier=priority"),
            ("x-codex-parent-thread-id", "thread-0"),
            ("x-openai-subagent", "review"),
            ("x-codex-turn-metadata", "{\"turn_id\":\"t1\"}"),
        ]);
        let decoded = CodexHeaders::decode(&wire).unwrap();
        assert_eq!(decoded.session.value().unwrap(), "cache-or-session");
        assert_eq!(decoded.thread.value().unwrap(), "thread-1");
        assert_eq!(decoded.window.value().unwrap(), "window-1");
        assert_eq!(decoded.turn_state.value().unwrap(), "opaque-token");
        assert_eq!(
            decoded.routing.value().unwrap(),
            &RoutingHint {
                model: "gpt-6-astra".into(),
                tier: Some("priority".into())
            }
        );
        assert_eq!(decoded.parent_thread.value().unwrap(), "thread-0");
        assert_eq!(decoded.subagent.value().unwrap(), "review");
        assert_eq!(
            decoded.turn_metadata.value().unwrap(),
            "{\"turn_id\":\"t1\"}"
        );
        let encoded = decoded.encode();
        for (name, value) in &wire {
            assert!(
                encoded.iter().any(|(k, v)| k == name && v == value),
                "{name}"
            );
        }
        // Conflicting thread identities fail instead of guessing a winner.
        let mut bad = wire.clone();
        bad[2].1 = "other".into();
        assert!(CodexHeaders::decode(&bad).is_err());
        // Duplicated typed names are not silently merged.
        let mut bad = wire.clone();
        bad.push(("session-id".to_string(), "second".to_string()));
        assert!(CodexHeaders::decode(&bad).is_err());
    }
    #[test]
    fn credential_and_transport_headers_are_never_context() {
        for name in [
            "authorization",
            "Authorization",
            "proxy-authorization",
            "cookie",
            "set-cookie",
            "x-api-key",
            "chatgpt-account-id",
            "X-OpenAI-Fedramp",
            "x-oai-attestation",
            "accept",
            "content-type",
            "content-encoding",
            "content-length",
            "host",
            "connection",
            "transfer-encoding",
        ] {
            assert!(
                CodexHeaders::decode(&headers(&[(name, "value")])).is_err(),
                "{name}"
            );
        }
    }
    #[test]
    fn unmodeled_safe_headers_round_trip_opaque_within_budget() {
        let wire = headers(&[
            ("originator", "codex_cli_rs"),
            ("x-codex-beta-features", "a,b"),
            ("x-request-id", "req-1"),
        ]);
        let decoded = CodexHeaders::decode(&wire).unwrap();
        let encoded = decoded.encode();
        for (name, value) in &wire {
            assert!(
                encoded.iter().any(|(k, v)| k == name && v == value),
                "{name}"
            );
        }
        let mut many: Vec<(String, String)> = (0..40)
            .map(|i| (format!("x-custom-{i}"), "v".into()))
            .collect();
        assert!(CodexHeaders::decode(&many).is_err());
        many.truncate(4);
        assert!(CodexHeaders::decode(&many).is_ok());
        assert!(CodexHeaders::decode(&headers(&[("x-custom", "a\r\nInjected: 1")])).is_err());
        assert!(CodexHeaders::decode(&headers(&[("x custom", "v")])).is_err());
        assert!(CodexHeaders::decode(&headers(&[("", "v")])).is_err());
        assert!(CodexHeaders::decode(&headers(&[("x-custom", "x\0z")])).is_err());
        assert!(
            CodexHeaders::decode(&headers(&[("x-custom", "x"), ("x-custom", "y")])).is_err(),
            "duplicates"
        );
        let long = "x".repeat(5000);
        assert!(CodexHeaders::decode(&headers(&[("x-custom", &long)])).is_err());
        assert!(CodexHeaders::decode(&headers(&[("x-codex-turn-state", &long)])).is_err());
    }
}
mod cache_projection {
    //! Cache affinity is Provider functionality, not gateway sessions or route selection.
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract};
    use serde_json::{Value, json};
    fn client(p: Profile) -> Adapter {
        Adapter::new(p, Dialect::MorphieCore, None)
    }
    fn request(p: Profile) -> Value {
        match p {
            Profile::Responses => {
                json!({"model":"public","input":[{"role":"user","content":[{"type":"input_text","text":"stable prefix"},{"type":"input_image","image_url":"https://example.invalid/image","detail":"auto"}]}]})
            }
            Profile::Chat => {
                json!({"model":"public","messages":[{"role":"user","content":[{"type":"text","text":"stable prefix"},{"type":"image_url","image_url":{"url":"https://example.invalid/image","detail":"auto"}}]}]})
            }
        }
    }

    #[test]
    fn scoped_session_and_standard_cache_key_remain_independent_on_both_wires() {
        let mut empty = request(Profile::Chat);
        empty["prompt_cache_options"] = json!({});
        let decoded = client(Profile::Chat)
            .decode_request(empty.to_string().as_bytes())
            .unwrap();
        let saved = decoded.context.clone();
        let without_options = Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
            .encode_request(&decoded, "upstream", &Contract::full())
            .unwrap();
        assert!(without_options.get("prompt_cache_options").is_none());
        assert_eq!(decoded.context, saved);
        empty["prompt_cache_options"] = json!({"mode":"explicit"});
        let active = client(Profile::Chat)
            .decode_request(empty.to_string().as_bytes())
            .unwrap();
        assert!(
            Adapter::new(Profile::Chat, Dialect::DeepSeek, None)
                .encode_request(&active, "upstream", &Contract::full())
                .is_err()
        );
        empty["prompt_cache_options"] = json!({"prewarm":true});
        assert!(
            client(Profile::Chat)
                .decode_client_request(empty.to_string().as_bytes())
                .is_err()
        );
        for p in [Profile::Chat, Profile::Responses] {
            let mut wire = request(p);
            wire["prompt_cache_key"] = json!("key-a");
            wire["session_id"] = json!("session-b");
            let req = client(p)
                .decode_request(&serde_json::to_vec(&wire).unwrap())
                .unwrap();
            let target = Adapter::new(p, Dialect::OpenRouter, None);
            let out = target
                .encode_request(&req, "upstream", &Contract::full())
                .unwrap();
            assert_eq!(out["session_id"], "session-b");
            assert_eq!(out["prompt_cache_key"], "key-a");
            let mut edited = req.clone();
            edited.cache_session = None;
            edited.context.cache.prompt_cache_key = morphiecore::semantic::value::Presence::Absent;
            let erased = target
                .encode_request(&edited, "upstream", &Contract::full())
                .unwrap();
            assert!(erased.get("session_id").is_none());
            assert!(erased.get("prompt_cache_key").is_none());
            assert!(
                Adapter::new(p, Dialect::Standard, None)
                    .decode_request(&serde_json::to_vec(&wire).unwrap())
                    .is_err()
            );
            assert!(
                Adapter::new(p, Dialect::DeepSeek, None)
                    .encode_request(&req, "upstream", &Contract::full())
                    .is_err()
            );
            for invalid in [
                Value::Null,
                json!(""),
                json!("x".repeat(257)),
                json!({"token":"synthetic"}),
            ] {
                wire["session_id"] = invalid;
                assert!(
                    client(p)
                        .decode_request(&serde_json::to_vec(&wire).unwrap())
                        .is_err()
                );
            }
        }
    }
}
mod chat_wire {
    //! Explicit Chat dialect mappings over one semantic model.
    use morphiecore::{
        protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract, chat},
        semantic::{task::generation::*, value::Text},
    };
    use serde_json::{Value, json};
    #[test]
    fn chat_text_arrays_keep_order_and_follow_typed_edits() {
        let wire = json!({"model":"m","messages":[
            {"role":"system","content":[{"type":"text","text":"first"},{"type":"text","text":"second"}]},
            {"role":"user","content":[{"type":"text","text":"alpha"},{"type":"text","text":"beta"}]}
        ]});
        let mut request = client()
            .decode_request(wire.to_string().as_bytes())
            .unwrap();
        assert!(
            matches!(&request.task.semantic.items()[0].1, Item::Instruction(i) if i.parts.len()==2 && i.parts[1].1.as_text()==Some("second"))
        );
        assert!(
            matches!(&request.task.semantic.items()[1].1, Item::Message(m) if m.parts.len()==2 && matches!(&m.parts[1].content, ContentPart::Text(t) if t.as_str()=="beta"))
        );
        let encoded = client()
            .encode_request(&request, "m", &Contract::full())
            .unwrap();
        assert_eq!(encoded["messages"], wire["messages"]);
        let mut items = request.task.semantic.items().to_vec();
        if let Item::Message(m) = &mut items[1].1 {
            m.parts.remove(0);
            m.parts[0].content =
                ContentPart::Text(Text::new("changed", "fixture", 128).unwrap().into());
        }
        request.task.semantic =
            GenerationRequest::new(items, GenerationControls::default()).unwrap();
        let encoded = client()
            .encode_request(&request, "m", &Contract::full())
            .unwrap();
        assert_eq!(encoded["messages"][1]["content"], "changed");
        let mut items = request.task.semantic.items().to_vec();
        if let Item::Message(m) = &mut items[1].1 {
            m.parts.push(Part {
                id: PartId::new(99),
                content: ContentPart::Text(Text::new("inserted", "fixture", 128).unwrap().into()),
            });
        }
        request.task.semantic =
            GenerationRequest::new(items, GenerationControls::default()).unwrap();
        let encoded = client()
            .encode_request(&request, "m", &Contract::full())
            .unwrap();
        assert_eq!(
            encoded["messages"][1]["content"],
            json!([{"type":"text","text":"changed"},{"type":"text","text":"inserted"}])
        );
        let mut output = response("pong");
        output["choices"][0]["message"]["content"] = json!([{"type":"text","text":"pong"}]);
        assert!(
            client()
                .decode_response(output.to_string().as_bytes())
                .is_err()
        );
        assert!(client().decode_request(br#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"text","text":"x"}],"refusal":"no"}]}"#).is_err());
        for content in [
            json!([]),
            json!([{"type":"text"}]),
            json!([{"type":"text","text":3}]),
            json!([{"type":"text","text":"x".repeat(MAX_TEXT_BYTES + 1)}]),
            json!([{"type":"image_url","image_url":{"url":"file:///invalid/x"}}]),
            json!([{"type":"text","text":"x","unknown":true}]),
        ] {
            let invalid = json!({"model":"m","messages":[{"role":"user","content":content}]});
            assert!(
                client()
                    .decode_request(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
    }
    fn vendor() -> Adapter {
        Adapter::new(Profile::Chat, Dialect::Xiaomi, None)
    }
    fn client() -> Adapter {
        Adapter::new(Profile::Chat, Dialect::MorphieCore, None)
    }
    fn response(content: &str) -> Value {
        json!({"id":"c1","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":content,"reasoning_content":"think"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}})
    }
    fn chunk(delta: Value, finish: Value) -> Value {
        json!({"id":"c1","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":null})
    }
    #[test]
    fn aliases_are_normalized_without_losing_counts_or_masking_disagreement() {
        let adapter = Adapter::new(Profile::Chat, Dialect::DeepSeek, None);
        let mut body = response("pong");
        let standard = Adapter::new(Profile::Chat, Dialect::Standard, None);
        let mut unknown = body.clone();
        unknown["usage"]["prompt_cache_hit_tokens"] = json!({"not":"a count"});
        unknown["usage"]["prompt_cache_miss_tokens"] = json!(9);
        assert_eq!(
            standard
                .decode_response(unknown.to_string().as_bytes())
                .unwrap()
                .semantic
                .usage(),
            standard
                .decode_response(body.to_string().as_bytes())
                .unwrap()
                .semantic
                .usage()
        );
        body["usage"] = json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,
        "prompt_tokens_details":{"cached_tokens":1},"completion_tokens_details":{"reasoning_tokens":1},
        "prompt_cache_hit_tokens":1,"prompt_cache_miss_tokens":2});
        let decoded = adapter
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            decoded.semantic.usage().unwrap().cached_input_tokens,
            Some(1)
        );
        let encoded = client()
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(
            encoded["usage"]["prompt_tokens_details"]["cached_tokens"],
            1
        );
        assert!(encoded["usage"].get("prompt_cache_hit_tokens").is_none());
        let mut stream = adapter.event_decoder();
        let mut finish = chunk(json!({"content":"pong"}), json!("stop"));
        finish["usage"] = body["usage"].clone();
        stream.push(&finish).unwrap();
        stream.done().unwrap();
        assert_eq!(
            stream.materialize().unwrap().semantic.usage(),
            decoded.semantic.usage()
        );
        for (key, value) in [
            ("prompt_cache_hit_tokens", 0),
            ("prompt_cache_miss_tokens", 9),
        ] {
            let mut invalid = body.clone();
            invalid["usage"][key] = json!(value);
            assert!(
                adapter
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
    }
    #[test]
    fn usage_only_belongs_to_finish_or_tail_and_duplicates_fail() {
        let usage = json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5});
        let mut early = chunk(json!({"content":"pong"}), Value::Null);
        early["usage"] = usage.clone();
        assert!(vendor().event_decoder().push(&early).is_err());
        let mut decoder = vendor().event_decoder();
        decoder
            .push(&chunk(json!({"content":"pong"}), Value::Null))
            .unwrap();
        let mut finish = chunk(json!({}), json!("stop"));
        finish["usage"] = usage.clone();
        assert!(
            decoder
                .push(&finish)
                .unwrap()
                .iter()
                .any(|e| matches!(e, StreamEvent::Usage(_)))
        );
        let duplicate = json!({"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[],"usage":usage});
        assert!(decoder.push(&duplicate).is_err());
        assert!(decoder.done().is_err());
    }
    #[test]
    fn deferred_empty_carriers_and_nullable_call_identity_remain_consistent() {
        let chunks = [
            chunk(json!({"role":"assistant","content":""}), Value::Null),
            chunk(
                json!({"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"lookup","arguments":""}}]}),
                Value::Null,
            ),
            chunk(
                json!({"tool_calls":[{"index":0,"id":null,"type":"function","function":{"name":null,"arguments":"{\"k\":1}"}}]}),
                Value::Null,
            ),
            chunk(json!({}), json!("tool_calls")),
        ];
        let mut decoder = vendor().event_decoder();
        for frame in &chunks {
            decoder.push(frame).unwrap();
        }
        decoder.done().unwrap();
        let decoded = decoder.materialize().unwrap();
        assert!(
            matches!(&decoded.semantic.items()[0].1,Item::Message(m) if m.parts.len()==1 && matches!(&m.parts[0].content,ContentPart::Text(t) if t.as_str().is_empty()))
        );
        assert!(
            matches!(&decoded.semantic.items()[1].1,Item::ToolCall(c) if c.call_id.as_str()=="call_a" && c.arguments.as_raw()==Some("{\"k\":1}"))
        );
        let mut conflict = vendor().event_decoder();
        conflict.push(&chunks[0]).unwrap();
        conflict.push(&chunks[1]).unwrap();
        let mut wrong = chunks[2].clone();
        wrong["choices"][0]["delta"]["tool_calls"][0]["id"] = json!("call_b");
        assert!(conflict.push(&wrong).is_err());
        let mut replay = json!({"messages":[{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"lookup","arguments":"{}"}}]}]});
        assert!(chat::decode_generation(&replay).is_ok());
        replay["messages"][0]["tool_calls"][0]["index"] = json!(7);
        assert!(chat::decode_generation(&replay).is_err());
    }
    #[test]
    fn empty_tool_calls_do_not_parse_message_text_into_a_call() {
        let mut body = response("<tool_call>");
        body["choices"][0]["message"]["tool_calls"] = json!([]);
        body["choices"][0]["finish_reason"] = json!("length");
        let decoded = vendor()
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        assert!(
            !decoded
                .semantic
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::ToolCall(_)))
        );
        assert_eq!(
            decoded.semantic.details().incomplete,
            Some(IncompleteReason::MaxOutputTokens)
        );
        let output = client()
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert!(output["choices"][0]["message"].get("tool_calls").is_none());
        assert_eq!(output["choices"][0]["message"]["content"], "<tool_call>");
        assert_eq!(output["choices"][0]["finish_reason"], "length");
    }
}
