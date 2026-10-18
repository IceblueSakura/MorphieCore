//! Message envelopes, ownership, phase and scoped identity.
mod message_envelopes {
    //! Message envelopes own ordered membership, not tool-result association.
    use morphiecore::{protocol::openai::responses, semantic::task::generation::*};
    use serde_json::json;

    fn history() -> GenerationRequest {
        responses::decode_generation(&json!({"input":[
            {"role":"assistant","content":"before"},
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{ \"x\": 1 }"},
            {"role":"assistant","content":"after"},
            {"type":"function_call_output","call_id":"c","output":"result"},
            {"role":"user","content":"continue"}
        ]}))
        .unwrap()
        .semantic
    }

    #[test]
    fn duplicate_members_wrong_roles_and_dangling_empty_boundaries_fail() {
        let request = history();
        let ids: Vec<_> = request.items().iter().map(|(id, _)| *id).collect();
        let group = |n, role, members| {
            MessageEnvelope::new(GroupId::new(LocalScope::ROOT, n), role, members).unwrap()
        };
        assert!(
            request
                .clone()
                .with_message_envelopes(vec![
                    group(1, MessageEnvelopeRole::Assistant, vec![ids[0], ids[1]]),
                    group(2, MessageEnvelopeRole::Assistant, vec![ids[1]]),
                ])
                .is_err()
        );
        assert!(
            request
                .clone()
                .with_message_envelopes(vec![group(1, MessageEnvelopeRole::User, vec![ids[1]]),])
                .is_err()
        );
        let empty = MessageEnvelope::empty(
            GroupId::new(LocalScope::ROOT, 3),
            MessageEnvelopeRole::Assistant,
            Some(ids[0]),
        );
        let grouped = request.with_message_envelopes(vec![empty]).unwrap();
        assert!(grouped.retain_items(|id, _| id != ids[0]).is_err());
    }

    #[test]
    fn reasoning_as_text_takes_a_derived_id_inside_its_envelope() {
        let source = responses::decode_generation(&json!({"input":[
            {"type":"reasoning","id":"rs","status":"incomplete","summary":[
                {"type":"summary_text","text":"Plan"},{"type":"summary_text","text":" "}]},
            {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
            {"type":"function_call_output","call_id":"c","output":"result"}
        ]}))
        .unwrap()
        .semantic;
        let ids: Vec<_> = source.items().iter().map(|(id, _)| *id).collect();
        let Item::Reasoning(reasoning) = &source.items()[0].1 else {
            panic!("reasoning");
        };
        let part = reasoning.parts[0].0;
        let source = source
            .with_message_envelopes(vec![
                MessageEnvelope::new(
                    GroupId::new(LocalScope::ROOT, 1),
                    MessageEnvelopeRole::Assistant,
                    ids[..2].to_vec(),
                )
                .unwrap(),
            ])
            .unwrap();
        let converted = source.with_reasoning_as_text(&[ids[0]]).unwrap();
        let derived = ItemId::scoped(LocalScope::DERIVED, 0);
        assert_eq!(
            converted.items()[0],
            (
                derived,
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    parts: vec![Part {
                        replay: None,
                        id: part,
                        cache: None,
                        content: ContentPart::Text(
                            morphiecore::semantic::value::Text::new("Plan", "synthetic", 64)
                                .unwrap()
                                .into()
                        ),
                    }],
                    status: ItemLifecycle::Incomplete,
                    phase: None,
                })
            )
        );
        assert_eq!(
            converted.message_envelopes()[0].members(),
            &[derived, ids[1]]
        );
    }

    #[test]
    fn chat_sse_delivers_calls_and_a_real_responses_terminal() {
        use morphiecore::protocol::openai::{
            Profile,
            events::{EventDecoder, EventEncoder},
        };
        let chunk = |delta, finish| {
            json!({"id":"r","object":"chat.completion.chunk",
        "created":0,"model":"synthetic","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
        };
        let mut decoder = EventDecoder::new(Profile::Chat);
        let mut events = vec![];
        for wire in [
            chunk(
                json!({"role":"assistant","content":"checking","tool_calls":[
                    {"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":"{ \"x\""}}
                ]}),
                json!(null),
            ),
            chunk(
                json!({"tool_calls":[{"index":0,"function":{"arguments":": 1 }"}}]}),
                json!(null),
            ),
            chunk(json!({}), json!("tool_calls")),
        ] {
            events.extend(decoder.push(&wire).unwrap());
        }
        events.extend(decoder.done().unwrap());
        let source = decoder.materialize().unwrap();
        // As in a static message, a refusal cannot carry calls.
        let mut refused = EventDecoder::new(Profile::Chat);
        refused
            .push(&chunk(
                json!({"role":"assistant","refusal":"no","tool_calls":[
                    {"index":0,"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]}),
                json!(null),
            ))
            .unwrap();
        assert_eq!(
            refused
                .push(&chunk(json!({}), json!("tool_calls")))
                .unwrap_err(),
            morphiecore::protocol::CodecError::Invalid("refusal with tool calls")
        );
        let mut encoder = EventEncoder::new(Profile::Responses, source.metadata.clone()).unwrap();
        let mut wire = vec![];
        for event in &events {
            wire.extend(encoder.encode(event, &source.fidelity).unwrap());
        }
        encoder.finish().unwrap();
        assert_eq!(wire.last().unwrap()["type"], "response.completed");
        assert_eq!(
            wire.last().unwrap()["response"]["output"][1]["call_id"],
            "c"
        );
        assert_eq!(
            wire.last().unwrap()["response"]["output"][1]["arguments"],
            "{ \"x\": 1 }"
        );
        let mut consumer = EventDecoder::new(Profile::Responses);
        for value in wire {
            consumer.push(&value).unwrap();
        }
        assert_eq!(consumer.materialize().unwrap().semantic.items().len(), 2);
        assert!(
            encoder
                .projection()
                .iter()
                .any(|stage| stage.loss.is_some())
        );
        // The decoder groups the calls with an envelope event that both profiles carry.
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::MessageEnvelope(_)))
        );
        for profile in [Profile::Chat, Profile::Responses] {
            let mut encoder = EventEncoder::new(profile, source.metadata.clone()).unwrap();
            let mut consumer = EventDecoder::new(profile);
            for event in &events {
                for wire in encoder.encode(event, &source.fidelity).unwrap() {
                    consumer.push(&wire).unwrap();
                }
            }
            encoder.finish().unwrap();
            if profile == Profile::Chat {
                consumer.done().unwrap();
            }
            let response = consumer.materialize().unwrap();
            let Item::ToolCall(call) = &response.semantic.items()[1].1 else {
                panic!("call");
            };
            assert_eq!(call.call_id.as_str(), "c");
            assert_eq!(call.arguments, ToolArguments::Raw("{ \"x\": 1 }".into()));
        }
    }

    #[test]
    fn envelope_selection_keeps_the_declared_container() {
        let source = history();
        let ids: Vec<_> = source.items().iter().map(|(id, _)| *id).collect();
        let group = MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 1),
            MessageEnvelopeRole::Assistant,
            ids[..3].to_vec(),
        )
        .unwrap();
        let source = source.with_message_envelopes(vec![group.clone()]).unwrap();
        let rebuilt = ClientManaged::new(GenerationSettings::default())
            .unwrap()
            .select_request_items(&source, &ids[..3])
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(rebuilt.message_envelopes(), &[group]);
    }
}
mod message_ownership_owner {
    //! A08/T28: semantic membership is independent of target contiguity.
    use morphiecore::{
        protocol::{
            fidelity::FidelityRecords,
            openai::{Profile, TargetCaps as Contract, lowering::lower_request},
        },
        semantic::{task::generation::*, value::Text},
    };
    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", 64).unwrap()
    }
    #[test]
    fn noncontiguous_explicit_message_membership_is_core_data_not_a_target_permission() {
        let items = values();
        let request = GenerationRequest::new(items, GenerationControls::default())
            .unwrap()
            .with_message_envelopes(vec![envelope()])
            .unwrap();
        assert_eq!(
            request.message_envelopes()[0].members(),
            [ItemId::new(1), ItemId::new(3)]
        );
        for profile in [Profile::Chat, Profile::Responses] {
            assert!(
                lower_request(
                    &request,
                    &FidelityRecords::default(),
                    profile,
                    Contract::full()
                )
                .is_err()
            );
        }
    }
    fn envelope() -> MessageEnvelope {
        MessageEnvelope::new(
            GroupId::new(LocalScope::ROOT, 1),
            MessageEnvelopeRole::Assistant,
            vec![ItemId::new(1), ItemId::new(3)],
        )
        .unwrap()
    }
    fn values() -> Vec<(ItemId, Item)> {
        vec![
            (
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    parts: vec![],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ),
            (
                ItemId::new(2),
                Item::Reasoning(ReasoningItem {
                    parts: vec![],
                    status: ItemLifecycle::Completed,
                    replay: None,
                }),
            ),
            (
                ItemId::new(3),
                Item::ToolCall(ToolCall {
                    replay: None,
                    call_id: text("C"),
                    cache: None,
                    name: text("lookup"),
                    arguments: "{}".into(),
                    status: ItemLifecycle::Completed,
                    context: CallContext::default(),
                }),
            ),
        ]
    }
    #[test]
    fn continuation_rejects_added_message_membership() {
        let items = vec![values()[0].clone(), values()[2].clone()];
        let response = GenerationResponse::new(items.clone(), Outcome::Completed).unwrap();
        let exchange = ResponseContinuation::new(
            ResponseRelation::new(TurnId::new(1), ResponseId::new(1)),
            &response,
        );
        let history = GenerationRequest::new(items, GenerationControls::default()).unwrap();
        exchange.inspect(&history).unwrap();
        let changed = history.with_message_envelopes(vec![envelope()]).unwrap();
        assert_eq!(
            exchange.inspect(&changed).unwrap_err(),
            ContinuationError::ChangedResponse
        );
    }
}
mod phase {
    //! Assistant phase is exact on Responses and explicitly omitted on foreign requests.
    use crate::events_support::*;
    use morphiecore::{
        protocol::openai::{
            CodecError, DecodedRequest, Profile, events::EventDecoder, responses::envelope,
        },
        semantic::task::generation::*,
    };
    use serde_json::{Value, json};

    fn decode_input(items: Value) -> Result<DecodedRequest, CodecError> {
        let source = json!({"model":"fixture-model","input":items});
        Ok(envelope::decode_request_bytes(&serde_json::to_vec(&source).unwrap())?.task)
    }
    fn labelled(label: Value) -> Value {
        json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hi","annotations":[]}],"phase":label})
    }
    fn message(d: &DecodedRequest) -> &Message {
        let Item::Message(m) = &d.semantic.items()[0].1 else {
            panic!()
        };
        m
    }

    #[test]
    fn foreign_requests_omit_phase_without_editing_responses_history() {
        use morphiecore::protocol::{
            anthropic, google, interactions,
            native::ReplayTarget,
            openai::{Adapter, Dialect, TargetCaps},
        };
        let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let source = json!({"model":"fixture","max_output_tokens":64,"input":[
            {"role":"user","content":"go"},
            {"role":"assistant","content":"checking","phase":"commentary"},
            {"role":"assistant","content":"done","phase":"final_answer"},
            {"role":"user","content":"continue"}
        ]});
        let request = client
            .decode_request(source.to_string().as_bytes())
            .unwrap();
        let saved = request.clone();
        let chat = Adapter::new(Profile::Chat, Dialect::Standard, None);
        let (wire, rules) = chat
            .encode_request_with_rules(&request, "target", &TargetCaps::full())
            .unwrap();
        assert_eq!(
            wire["messages"],
            json!([
                {"role":"user","content":"go"},
                {"role":"assistant","content":"checking"},
                {"role":"assistant","content":"done"},
                {"role":"user","content":"continue"}
            ])
        );
        assert_eq!(
            rules
                .iter()
                .filter(|r| **r == "chat.omit-message-phase.v1")
                .count(),
            1
        );
        let target = ReplayTarget::new("fixture", "target").unwrap();
        let messages = anthropic::TargetCaps::new(
            anthropic::Profile::AdaptiveTextTools,
            anthropic::Dialect::Standard,
        )
        .encode_request(&request, &target)
        .unwrap();
        assert!(
            messages
                .rules
                .contains(&anthropic::RequestRule::OmitMessagePhase)
        );
        assert_eq!(
            messages.value["messages"][1]["content"],
            json!([{"type":"text","text":"checking"},{"type":"text","text":"done"}])
        );
        let (wire, rules) = google::encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &target,
            None,
        )
        .unwrap();
        assert!(rules.contains(&google::RequestRule::OmitMessagePhase));
        assert_eq!(
            wire["contents"][1],
            json!({"role":"model","parts":[{"text":"checking"},{"text":"done"}]})
        );
        let (wire, rules) = interactions::encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &target,
        )
        .unwrap();
        assert!(rules.contains(&interactions::RequestRule::OmitMessagePhase));
        assert_eq!(
            wire["input"],
            json!([
                {"type":"user_input","content":[{"type":"text","text":"go"}]},
                {"type":"model_output","content":[{"type":"text","text":"checking"}]},
                {"type":"model_output","content":[{"type":"text","text":"done"}]},
                {"type":"user_input","content":[{"type":"text","text":"continue"}]}
            ])
        );
        let exact = client
            .encode_request(&request, "target", &TargetCaps::full())
            .unwrap();
        assert_eq!(exact["input"][1]["phase"], "commentary");
        assert_eq!(exact["input"][2]["phase"], "final_answer");
        assert_eq!(request, saved);
    }

    #[test]
    fn phase_keys_are_assistant_only_and_fully_typed() {
        for bad in [
            json!("final"),
            json!(5),
            json!({}),
            json!(true),
            json!(["final_answer"]),
        ] {
            assert!(decode_input(json!([labelled(bad)])).is_err());
        }
        // Only assistant messages are labelled.
        let mut user = json!({"type":"message","id":"m","role":"user","status":"completed","content":[{"type":"input_text","text":"hi"}]});
        user["phase"] = Value::Null;
        assert!(decode_input(json!([user])).is_err());
        for role in ["system", "developer"] {
            let mut item = json!({"type":"message","id":"m","role":role,"content":[{"type":"input_text","text":"rules"}]});
            item["phase"] = json!("final_answer");
            assert!(decode_input(json!([item])).is_err());
        }
        // The shorthand message form carries the same typed label.
        let d = decode_input(json!([{"role":"assistant","content":"hi","phase":"commentary"}]))
            .unwrap();
        assert_eq!(message(&d).phase, Some(Phase::Commentary));
    }

    #[test]
    fn commentary_text_never_carries_a_parsed_view() {
        let item = |label: Value, parsed: Value| json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"{\"ok\":true}","annotations":[],"parsed":parsed}],"phase":label});
        // The pinned SDK never parses commentary text; contradictory dumps cannot replay.
        assert!(decode_input(json!([item(json!("commentary"), json!({"ok":true}))])).is_err());
        assert!(decode_input(json!([item(json!("commentary"), Value::Null)])).is_ok());
        assert!(decode_input(json!([item(json!("final_answer"), json!({"ok":true}))])).is_ok());
        assert!(decode_input(json!([item(Value::Null, json!({"ok":true}))])).is_ok());
    }

    fn stream(label: Option<Value>, done_label: Option<Value>) -> Vec<Value> {
        let mut started = json!({"id":"m","type":"message","role":"assistant","status":"in_progress","content":[]});
        let mut done = json!({"id":"m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hi","annotations":[]}]});
        if let Some(v) = &label {
            started["phase"] = v.clone();
        }
        if let Some(v) = &done_label {
            done["phase"] = v.clone();
        }
        vec![
            created(),
            json!({"type":"response.output_item.added","output_index":0,"item":started}),
            json!({"type":"response.content_part.added","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","output_index":0,"item_id":"m","content_index":0,"delta":"hi","logprobs":[]}),
            json!({"type":"response.output_text.done","output_index":0,"item_id":"m","content_index":0,"text":"hi","logprobs":[]}),
            json!({"type":"response.content_part.done","output_index":0,"item_id":"m","content_index":0,"part":{"type":"output_text","text":"hi","annotations":[]}}),
            json!({"type":"response.output_item.done","output_index":0,"item":done.clone()}),
            json!({"type":"response.completed","response":envelope("completed",json!([done]))}),
        ]
    }

    #[test]
    fn event_snapshots_preserve_phase_and_conflicts_fail() {
        let mut d = EventDecoder::new(Profile::Responses);
        for v in stream(Some(json!("commentary")), Some(json!("commentary"))) {
            d.push(&v).unwrap();
        }
        let r = d.materialize().unwrap().semantic;
        let Item::Message(m) = &r.items()[0].1 else {
            panic!()
        };
        assert_eq!(m.phase, Some(Phase::Commentary));
        // A final snapshot that contradicts the started label cannot close the item.
        for (started, done) in [
            (Some(json!("commentary")), Some(Value::Null)),
            (Some(json!("commentary")), Some(json!("final_answer"))),
            (None, Some(json!("final_answer"))),
        ] {
            let mut d = EventDecoder::new(Profile::Responses);
            let mut values = stream(started, done);
            let failed = values.split_off(6);
            for v in values {
                d.push(&v).unwrap();
            }
            assert!(d.push(&failed[0]).is_err());
        }
    }
}
mod scoped_identity {
    //! A03/T16: alias domains are explicit, independent of response coordinates.
    use morphiecore::semantic::{task::generation::*, value::Text};
    fn text(value: &str) -> Text {
        Text::new(value, "synthetic", 256).unwrap()
    }
    fn domain(scope: u64) -> NativeAliasDomain {
        NativeAliasDomain {
            source: text("abstract-contract-v0.2"),
            scope: LocalScope::new(scope),
        }
    }
    fn call(scope: u64) -> (ItemId, Item) {
        (
            ItemId::scoped(LocalScope::new(scope), 1),
            Item::ToolCall(ToolCall {
                replay: None,
                call_id: text("same"),
                cache: None,
                name: text("lookup"),
                arguments: "{}".into(),
                status: ItemLifecycle::Completed,
                context: CallContext {
                    alias_domain: Some(domain(scope)),
                    ..Default::default()
                },
            }),
        )
    }
    fn result(owner_scope: u64, reference_scope: u64) -> (ItemId, Item) {
        (
            ItemId::scoped(LocalScope::new(owner_scope), 2),
            Item::ToolResult(ToolResult {
                is_error: None,
                call_id: Some(text("same")),
                cache: None,
                name: None,
                output: "synthetic result".into(),
                execution: None,
                status: None,
                context: CallContext {
                    alias_domain: Some(domain(reference_scope)),
                    ..Default::default()
                },
            }),
        )
    }
    #[test]
    fn duplicate_aliases_and_foreign_domain_results_are_rejected() {
        let mut items = vec![call(1)];
        let mut duplicate = call(1);
        duplicate.0 = ItemId::scoped(LocalScope::new(1), 2);
        items.push(duplicate);
        assert_eq!(
            GenerationRequest::new(items, GenerationControls::default()).unwrap_err(),
            GenerationError::DuplicateCall
        );
        assert_eq!(
            GenerationRequest::new(vec![call(1), result(2, 2)], GenerationControls::default())
                .unwrap_err(),
            GenerationError::InvalidToolResult
        );
    }
}
