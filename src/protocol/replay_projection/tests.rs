use super::*;
use crate::{
    adapter::ClientAdapter,
    execution::{
        Attempt, ResponseDelivery,
        replay::{open_from_client, seal_for_client},
    },
    protocol::{
        ClientProtocol,
        failure::CarrierReason,
        google::generate_content::{GenerationSseDecoder, encode_generation_request},
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
    let mut body = json!({"responseId":"reply","modelVersion":"fixture-model","candidates":[{
        "content":{"role":"model","parts":[
            {"thought":true,"text":"plan","thoughtSignature":"UjE="},
            {"text":"OK","thoughtSignature":"UzE="},
            {"text":"OK","thoughtSignature":"UzI="},
            {"functionCall":{"id":"c","name":"lookup","args":{"n":number}},"thoughtSignature":"UzM="}
        ]},"finishReason":"MAX_TOKENS"}],"usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":0,
            "candidatesTokenCount":5,"thoughtsTokenCount":1,"totalTokenCount":16}});
    if empty {
        body["candidates"][0]["content"]["parts"] = json!([{"text":"","thoughtSignature":"UzE="}]);
    }
    format!("data: {body}\n\n")
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
    let (n, events) = decoder.consume(wire.as_bytes()).unwrap();
    assert_eq!(n, wire.len());
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
    for index in [1, 3] {
        let mut changed = source.clone();
        let mut items = changed.semantic.items().to_vec();
        match &mut items[index].1 {
            Item::Message(m) => m.parts[0].replay = None,
            Item::ToolCall(c) => c.replay = None,
            _ => panic!(),
        }
        changed.semantic = changed.semantic.with_items(items).unwrap();
        assert!(seal_for_client(&mut changed, &CLIENT_ORIGIN).is_err());
        let mut changed = source.clone();
        let mut items = changed.semantic.items().to_vec();
        items.remove(index);
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
    let (owner, Item::Message(message)) = &mut items[1] else {
        panic!()
    };
    message.status = ItemLifecycle::Incomplete;
    changed
        .fidelity
        .record_part_replay(
            *owner,
            message.status,
            0,
            &message.parts[0],
            Some(target().origin()),
        )
        .unwrap();
    message.status = ItemLifecycle::Completed;
    changed.semantic = changed.semantic.with_items(items).unwrap();
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
        .position(|event| matches!(event, StreamEvent::PartReplay { .. }))
        .unwrap();
    let mut state = StreamState::new();
    for event in &events[..index] {
        state = reduce(state, event.clone()).unwrap();
    }
    let mut partial = events[index].clone();
    let StreamEvent::PartReplay { replay, .. } = &mut partial else {
        unreachable!()
    };
    replay.value = ReplayValue::partial(
        replay.value.format(),
        Text::new(replay.value.as_str(), "fixture", 1024).unwrap(),
    );
    let state = reduce(state, partial).unwrap();
    let mut unfinished = state.clone();
    unfinished = reduce(unfinished, events[index + 1].clone()).unwrap();
    assert!(matches!(
        reduce(unfinished, events[index + 2].clone()),
        Err(EventError::Lifecycle)
    ));
    let complete = reduce(state.clone(), events[index].clone()).unwrap();
    assert!(matches!(
        reduce(complete, events[index].clone()),
        Err(EventError::Lifecycle)
    ));
    let mut rebound = events[index].clone();
    let StreamEvent::PartReplay { replay, .. } = &mut rebound else {
        unreachable!()
    };
    replay.origin = Some(ReplayTarget::new("other", "other").unwrap().origin());
    assert!(matches!(reduce(state, rebound), Err(EventError::Identity)));
    let mut items = source.semantic.items().to_vec();
    let Item::Message(message) = &mut items[1].1 else {
        panic!()
    };
    message.parts[0].replay = Some(ReplayValue::final_value(
        ReplayFormat::GoogleGenerateContentPart,
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
    let (call_id, Item::ToolCall(call)) = items.last_mut().unwrap() else {
        panic!()
    };
    let call_id = *call_id;
    call.replay = None;
    source.fidelity.remove_replay(call_id);
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
        .without_foreign_reasoning(Some(&ReplayTarget::new("other", "other").unwrap().origin()))
        .unwrap()
        .unwrap();
    assert_eq!(copied.task.semantic.items().len(), 3);
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
        GoogleGenerateContentPart as Thought, GoogleGenerateContentSignature as Signed,
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
        assert_eq!(projected.items().len(), if reasoning { 4 } else { 3 });
        assert_eq!(
            projected
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::Reasoning(_))),
            reasoning
        );
        for (owner, item) in projected.items() {
            match item {
                Item::Message(message) => {
                    for part in &message.parts {
                        assert_eq!(part.replay.is_some(), attachments);
                        assert_eq!(
                            pruned.part_replay_origin(*owner, part.id).is_some(),
                            attachments
                        );
                    }
                }
                Item::ToolCall(call) => {
                    assert_eq!(call.replay.is_some(), attachments);
                    assert_eq!(pruned.replay_origin(*owner).is_some(), attachments);
                }
                _ => {}
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
    let (owner, Item::Message(message)) = &mut items[1] else {
        panic!("signed message")
    };
    message.status = ItemLifecycle::Incomplete;
    let part = &mut message.parts[0];
    let replay = part.replay.take().unwrap();
    part.replay = Some(ReplayValue::partial(
        replay.format(),
        Text::new(replay.as_str(), "fixture", MAX_TEXT_BYTES).unwrap(),
    ));
    fidelity.remove_part_replay(*owner, part.id);
    fidelity
        .record_part_replay(*owner, message.status, 0, part, Some(target().origin()))
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
        request.without_foreign_reasoning(Some(&ReplayTarget::new("other", "other").unwrap().origin())),
        Err(CodecError::Unsupported(reason)) if reason == CarrierReason::Replay
    ));
}

#[test]
fn ordinary_reasoning_cannot_hide_an_unrestored_carrier() {
    let client = client(ClientProtocol::Responses);
    let input = json!({"model":"public","input":[{"type":"reasoning","id":"legacy","summary":[],
        "encrypted_content":"mca1.synthetic"}]});
    assert!(
        matches!(client.decode_client_request(input.to_string().as_bytes()),
        Err(CodecError::Unsupported(reason)) if reason == CarrierReason::Replay)
    );
    let raw = GenerationCodec::new(GenerationApi::Responses, Dialect::Standard, None);
    let mut request = raw.decode_request(input.to_string().as_bytes()).unwrap();
    assert!(matches!(
        open_from_client(&mut request),
        Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
}

#[test]
fn opening_revalidates_the_inner_attachment_format_for_the_actual_part_owner() {
    for (inner, accepted) in [
        (ReplayFormat::GoogleGenerateContentSignature, true),
        (ReplayFormat::GoogleInteractionsV1FunctionCall, false),
    ] {
        let mut request = GenerationCodec::new(GenerationApi::Chat, Dialect::Standard, None)
            .decode_request(
                json!({"model":"public","messages":[
                    {"role":"user","content":"go"},
                    {"role":"assistant","content":"OK"}
                ]})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let mut items = request.task.semantic.items().to_vec();
        let (_, Item::Message(message)) = items
            .iter_mut()
            .find(|(_, item)| matches!(item, Item::Message(m) if m.role == MessageRole::Assistant))
            .unwrap()
        else {
            panic!("message")
        };
        message.parts[0].replay = Some(ReplayValue::final_value(
            ReplayFormat::GoogleGenerateContentSignature,
            replay_envelope::seal(&CLIENT_ORIGIN, inner, "synthetic-signature").unwrap(),
        ));
        // The outer part format is valid before opening; only the inner purpose differs.
        request.task.semantic = request.task.semantic.clone().with_items(items).unwrap();
        let result = open_from_client(&mut request);
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert!(result.unwrap_err().to_string().contains("replay"));
        }
    }
}

#[test]
fn legacy_surrogate_signatures_are_dropped_with_a_receipt() {
    let seal = |payload: &str| {
        replay_envelope::seal(
            &target().origin(),
            ReplayFormat::GoogleGenerateContentPart,
            payload,
        )
        .unwrap()
    };
    let hash = json!([7u8; 32].to_vec());
    let request = json!({"model":"public","input":[
        {"role":"user","content":"go"},
        {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"plan"}],
            "encrypted_content":seal(r#"{"anchor":"thought","signature":"VEhU"}"#).as_str()},
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"OK","annotations":[]}]},
        {"type":"function_call","call_id":"c","name":"lookup","arguments":"{}"},
        {"type":"reasoning","id":"rs_2","summary":[],
            "encrypted_content":seal(r#"{"anchor":"call","call":"c","signature":"U0lH"}"#).as_str()},
        {"type":"reasoning","id":"rs_3","summary":[],
            "encrypted_content":seal(&format!(r#"{{"anchor":"tail","text_hash":{hash},"signature":"U0lH"}}"#)).as_str()},
        {"type":"function_call_output","call_id":"c","output":"ok"}
    ]});
    let mut decoded = client(ClientProtocol::Responses)
        .decode_client_request(request.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut decoded).unwrap();
    let (wire, rules) = encode_generation_request(
        &decoded.task.semantic,
        &decoded.task.fidelity,
        &target(),
        None,
    )
    .unwrap();
    assert_eq!(
        wire["contents"][1]["parts"],
        json!([
            {"thought":true,"text":"plan","thoughtSignature":"VEhU"},
            {"text":"OK"},
            {"functionCall":{"id":"c","name":"lookup","args":{}}}
        ])
    );
    assert!(
        rules
            .iter()
            .any(|rule| rule.name() == "gemini.omit-legacy-signature.v1")
    );
    // A malformed current payload is not a legacy surrogate and still rejects.
    let mut damaged = request;
    damaged["input"][4]["encrypted_content"] =
        json!(seal(r#"{"anchor":"elsewhere","signature":"U0lH"}"#).as_str());
    let mut decoded = client(ClientProtocol::Responses)
        .decode_client_request(damaged.to_string().as_bytes())
        .unwrap();
    open_from_client(&mut decoded).unwrap();
    assert!(matches!(
        encode_generation_request(
            &decoded.task.semantic,
            &decoded.task.fidelity,
            &target(),
            None,
        ),
        Err(CodecError::Invalid(
            crate::protocol::failure::ShapeReason::InvalidValue
        ))
    ));
}
