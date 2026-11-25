use crate::protocol::replay_projection::strip;
use crate::protocol::{CodecError, DecodedResponse};
use crate::semantic::task::generation::*;
use crate::{
    adapter::ClientAdapter,
    execution::{
        Attempt, ResponseDelivery,
        replay::{open_from_client, seal_for_client},
    },
    protocol::{
        ClientProtocol,
        failure::CarrierReason,
        google::interactions::GenerationSseDecoder,
        native::ReplayTarget,
        openai::{
            Dialect, GenerationApi, GenerationCodec, TargetContract, obfuscation::Obfuscation,
        },
        replay_envelope::{self, CLIENT_ORIGIN},
        stream::SseLimits,
    },
    semantic::value::Text,
};
use serde_json::{Value, json};
mod phase_tests;
fn target() -> ReplayTarget {
    ReplayTarget::new("fixture-google", "fixture-model").unwrap()
}

fn source() -> (DecodedResponse, Vec<StreamEvent>) {
    source_number(9007199254740993)
}

fn source_wire(number: u64) -> String {
    source_wire_case(number, false)
}

fn source_wire_case(number: u64, empty: bool) -> String {
    let mut frames = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"reply","model":"fixture-model"}}),
        json!({"event_type":"step.start","index":0,"step":{"type":"thought","signature":"UjE=","summary":[{"type":"text","text":"plan"}]}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"step.start","index":1,"step":{"type":"model_output"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"text","text":if empty {""} else {"OK"}}}),
        json!({"event_type":"step.stop","index":1}),
    ];
    if !empty {
        frames.extend([
            json!({"event_type":"step.start","index":2,"step":{"type":"function_call","id":"c","name":"lookup","arguments":{"n":number},"signature":"UzM="}}),
            json!({"event_type":"step.stop","index":2}),
        ]);
    }
    frames.push(json!({"event_type":"interaction.completed","interaction":{
        "status":if empty {"completed"} else {"requires_action"},
        "usage":{"total_input_tokens":10,"total_cached_tokens":0,"total_output_tokens":5,"total_thought_tokens":1,"total_tokens":16}}}));
    frames
        .into_iter()
        .map(|f| {
            format!(
                "event: {}\ndata: {f}\n\n",
                f["event_type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        + "event: done\ndata: [DONE]\n\n"
}

fn source_number(number: u64) -> (DecodedResponse, Vec<StreamEvent>) {
    let wire = source_wire(number);
    decode_source_wire(&wire)
}

fn decode_source_wire(wire: &str) -> (DecodedResponse, Vec<StreamEvent>) {
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        target(),
    )
    .unwrap();
    let mut events = vec![];
    let mut rest = wire.as_bytes();
    while !rest.is_empty() {
        let (n, batch) = decoder.consume(rest).unwrap();
        assert!(n > 0);
        events.extend(batch);
        rest = &rest[n..];
    }
    let mut source = decoder.finish().unwrap();
    source.metadata.created = Some(1.into());
    source.metadata.model = "public".into();
    (source, events)
}

fn client(protocol: ClientProtocol) -> ClientAdapter {
    let mut client = match protocol {
        ClientProtocol::Chat | ClientProtocol::Responses => ClientAdapter::openai(
            GenerationCodec::new(
                if protocol == ClientProtocol::Chat {
                    GenerationApi::Chat
                } else {
                    GenerationApi::Responses
                },
                Dialect::Standard,
                None,
            ),
            TargetContract::full(),
        ),
        ClientProtocol::Messages => ClientAdapter::Messages,
    };
    client.bind_replay_origin(&CLIENT_ORIGIN);
    client
}

#[test]
fn attachment_dependencies_detect_removed_values_owners_and_lifecycle_edits() {
    let (source, _) = source();
    {
        let mut changed = source.clone();
        let mut items = changed.semantic.items().to_vec();
        let Item::ToolCall(c) = &mut items[2].1 else {
            panic!()
        };
        c.replay = None;
        changed.semantic = changed.semantic.with_items(items).unwrap();
        assert!(seal_for_client(&mut changed, &CLIENT_ORIGIN).is_err());

        let mut changed = source.clone();
        let mut items = changed.semantic.items().to_vec();
        items.remove(2);
        changed.semantic = changed
            .semantic
            .with_progress(InteractionProgress::Unreported)
            .unwrap()
            .with_items(items)
            .unwrap();
        assert!(seal_for_client(&mut changed, &CLIENT_ORIGIN).is_err());
    }
    let mut changed = source.clone();
    let mut items = changed.semantic.items().to_vec();
    let (owner, Item::ToolCall(call)) = &mut items[2] else {
        panic!()
    };
    let mut modified = call.clone();
    modified.name = Text::new("other", "fixture", 64).unwrap();
    changed.fidelity.remove_replay(*owner);
    changed
        .fidelity
        .record_call_replay(*owner, &modified, Some(target().origin()))
        .unwrap();
    assert!(matches!(
        seal_for_client(&mut changed, &CLIENT_ORIGIN),
        Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
}

#[test]
fn partial_attachment_events_need_finalization_and_keep_owner_kind() {
    let (source, events) = source();
    let index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                StreamEvent::ItemFinished {
                    replay: Some(_),
                    ..
                }
            )
        })
        .unwrap();
    let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    let partial_replay = {
        let StreamEvent::ItemFinished {
            replay: Some(replay),
            ..
        } = &events[index]
        else {
            unreachable!()
        };
        ItemReplay {
            value: ReplayValue::partial(
                replay.value.format(),
                Text::new(replay.value.as_str(), "fixture", 1024).unwrap(),
            ),
            origin: replay.origin.clone(),
        }
    };
    state = reduce(
        state,
        StreamEvent::ItemStarted {
            item: ItemId::new(1),
            kind: ItemKind::Reasoning,
            replay: Some(partial_replay),
        },
    )
    .unwrap();
    let mut rebound = events[index].clone();
    let StreamEvent::ItemFinished {
        item,
        replay: Some(replay),
        ..
    } = &mut rebound
    else {
        unreachable!()
    };
    *item = ItemId::new(1);
    replay.origin = Some(ReplayTarget::new("other", "other").unwrap().origin());
    assert!(matches!(reduce(state, rebound), Err(EventError::Identity)));
    let mut items = source.semantic.items().to_vec();
    let Item::Message(message) = &mut items[1].1 else {
        panic!()
    };
    message.parts[0].replay = Some(ReplayValue::final_value(
        ReplayFormat::GoogleInteractionsV1Thought,
        Text::new(
            r#"{"anchor":"thought","signature":"UzE="}"#,
            "fixture",
            1024,
        )
        .unwrap(),
    ));
    assert!(matches!(
        source.semantic.with_items(items),
        Err(GenerationError::InvalidReplay)
    ));
}

#[test]
fn foreign_signed_text_drops_only_replay_and_affected_call_aliases() {
    let (mut source, _) = source();
    let mut items = source.semantic.items().to_vec();
    items.retain(|(_, item)| !matches!(item, Item::Reasoning(_)));
    let (call_id, Item::ToolCall(_)) = items.last().unwrap() else {
        panic!()
    };
    let call_id = *call_id;
    source.fidelity.retain_owners(&items);
    source
        .fidelity
        .record_response_item_id(call_id, "fc_fixture")
        .unwrap();
    let request = crate::protocol::GenerationRequestEnvelope::from_generation(
        crate::protocol::DecodedRequest {
            evidence: Default::default(),
            semantic: GenerationRequest::new(items, GenerationControls::default()).unwrap(),
            fidelity: source.fidelity,
        },
        "public",
    )
    .unwrap();
    let copied = request
        .without_uncarried_reasoning(
            Some(&ReplayTarget::new("other", "other").unwrap().origin()),
            None,
            |_, _| Ok(false),
        )
        .unwrap()
        .unwrap();
    assert_eq!(copied.task.semantic.items().len(), 2);
    assert!(
        copied
            .task
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !item.has_replay_attachment())
    );
    assert_eq!(copied.task.fidelity.response_item_id(call_id), None);
    assert_eq!(
        request.task.fidelity.response_item_id(call_id),
        Some("fc_fixture")
    );
    assert!(
        request
            .task
            .semantic
            .items()
            .iter()
            .any(|(_, item)| item.has_replay_attachment())
    );
}

#[test]
fn native_history_edits_only_foreign_carriers_and_prunes_stale_aliases() {
    let (source, _) = source();
    let request = GenerationRequest::new(
        source.semantic.items().to_vec(),
        GenerationControls::default(),
    )
    .unwrap();
    let mut fidelity = source.fidelity;
    let stale = ItemId::new(999);
    fidelity.record_response_item_id(stale, "stale").unwrap();
    let before = fidelity.clone();
    use ReplayFormat::{
        GoogleInteractionsV1FunctionCall as Signed, GoogleInteractionsV1Thought as Thought,
    };
    for (formats, reasoning, attachments) in [
        (vec![Thought, Signed], true, true),
        (vec![Signed], false, true),
        (vec![Thought], true, false),
        (vec![], false, false),
    ] {
        let (projected, pruned, transferred) = crate::protocol::native::native_reasoning_history(
            &request,
            &fidelity,
            &target(),
            &formats,
        )
        .unwrap();
        assert_eq!(projected.items().len(), if reasoning { 3 } else { 2 });
        assert_eq!(
            projected
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::Reasoning(_))),
            reasoning
        );
        for (owner, item) in projected.items() {
            if let Item::ToolCall(call) = item {
                assert_eq!(call.replay.is_some(), attachments);
                assert_eq!(pruned.replay_origin(*owner).is_some(), attachments);
            }
        }
        assert_eq!(transferred, !reasoning || !attachments);
        if !transferred {
            assert!(matches!(projected, std::borrow::Cow::Borrowed(_)));
        }
        assert_eq!(pruned.response_item_id(stale), None);
        assert_eq!(fidelity, before);
        assert_eq!(request.items(), source.semantic.items());
    }
    let reasoning = request
        .clone()
        .retain_items(|_, item| matches!(item, Item::Reasoning(_)))
        .unwrap();
    fidelity.retain_owners(reasoning.items());
    assert!(matches!(
        crate::protocol::native::native_reasoning_history(&reasoning, &fidelity, &target(), &[]),
        Err(CodecError::Semantic(GenerationError::EmptyInput))
    ));
}

#[test]
fn native_history_validates_deleted_anchors_before_partial_attachments() {
    let (source, _) = source();
    let mut fidelity = source.fidelity;
    let mut items = source.semantic.items().to_vec();
    let anchor = items[0].0;
    let (owner, Item::ToolCall(call)) = &mut items[2] else {
        panic!("signed call")
    };
    call.status = ItemLifecycle::Incomplete;
    let replay = call.replay.take().unwrap();
    call.replay = Some(ReplayValue::partial(
        replay.format(),
        Text::new(replay.as_str(), "fixture", MAX_TEXT_BYTES).unwrap(),
    ));
    fidelity.remove_replay(*owner);
    fidelity
        .record_call_replay(*owner, call, Some(target().origin()))
        .unwrap();
    let request = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    assert!(matches!(
        crate::protocol::native::native_reasoning_history(&request, &fidelity, &target(), &[]),
        Err(CodecError::Unsupported(reason)) if reason == CarrierReason::Replay
    ));
    let request = request
        .with_message_envelopes(vec![MessageEnvelope::empty(
            GroupId::new(LocalScope::ROOT, 99),
            MessageEnvelopeRole::Assistant,
            Some(anchor),
        )])
        .unwrap();
    assert!(matches!(
        crate::protocol::native::native_reasoning_history(&request, &fidelity, &target(), &[]),
        Err(CodecError::Semantic(GenerationError::InvalidMessageGroup))
    ));
    let request = crate::protocol::GenerationRequestEnvelope::from_generation(
        crate::protocol::DecodedRequest {
            evidence: Default::default(),
            semantic: request,
            fidelity,
        },
        "public",
    )
    .unwrap();
    assert!(matches!(
        request.without_uncarried_reasoning(
            Some(&ReplayTarget::new("other", "other").unwrap().origin()), None, |_, _| Ok(false),
        ),
        Err(CodecError::Unsupported(reason)) if reason == CarrierReason::Replay
    ));
}

#[test]
fn unsealed_reasoning_is_omitted_but_malformed_recognized_envelopes_reject() {
    let client = client(ClientProtocol::Responses);
    let input = json!({"model":"public","input":[{"type":"reasoning","id":"legacy","summary":[],
        "encrypted_content":"mca1.synthetic"},{"role":"user","content":"visible"}]});
    client
        .decode_client_request(input.to_string().as_bytes())
        .unwrap();
    let raw = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut request = raw.decode_request(input.to_string().as_bytes()).unwrap();
    open_from_client(&mut request).unwrap();
    let (body, rules) = raw
        .encode_request_with_rules(&request, "public", &TargetContract::full())
        .unwrap();
    assert_eq!(
        body["input"],
        json!([{"type":"message","role":"user",
        "content":[{"type":"input_text","text":"visible"}]}])
    );
    assert_eq!(rules, ["responses.transfer-visible-history.v1"]);
    let mut malformed = input;
    malformed["input"][0]["encrypted_content"] = json!("mc1.invalid");
    let mut request = raw
        .decode_client_request(malformed.to_string().as_bytes())
        .unwrap();
    assert!(open_from_client(&mut request).is_err());
}

#[test]
fn opening_revalidates_the_inner_attachment_format_for_the_actual_call_owner() {
    for (inner, accepted) in [
        (ReplayFormat::GoogleInteractionsV1FunctionCall, true),
        (ReplayFormat::GoogleInteractionsV1Thought, false),
    ] {
        let mut request = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_request(
                json!({"model":"public","messages":[
                    {"role":"user","content":"go"},
                    {"role":"assistant","tool_calls":[{"id":"c","type":"function","function":{"name":"lookup","arguments":"{}"}}]}
                ]})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let mut items = request.task.semantic.items().to_vec();
        let (_, Item::ToolCall(call)) = items
            .iter_mut()
            .find(|(_, item)| matches!(item, Item::ToolCall(_)))
            .unwrap()
        else {
            panic!("call")
        };
        call.replay = Some(ReplayValue::final_value(
            ReplayFormat::GoogleInteractionsV1FunctionCall,
            replay_envelope::seal(&CLIENT_ORIGIN, inner, "synthetic-signature").unwrap(),
        ));
        // The outer call format is valid before opening; only the inner purpose differs.
        request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
        let result = open_from_client(&mut request);
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert!(result.unwrap_err().to_string().contains("replay"));
        }
    }
}
