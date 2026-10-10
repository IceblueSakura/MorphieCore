//! Gateway-issued tokens are not Provider-issued encrypted reasoning.
use morphiecore::{
    protocol::{
        anthropic::{Profile, ReplayTarget},
        gateway_replay::{GatewayReplayCodec, ReplayDisposition},
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
}
fn fixture() -> (morphiecore::protocol::DecodedRequest, ItemId) {
    let p = Profile::AdaptiveTextTools;
    let source = p.decode_generation_request(br#"{"model":"synthetic-model","max_tokens":32,"system":"fixed","messages":[{"role":"user","content":"go"}]}"#, LocalScope::new(1), &target()).unwrap();
    let observed = p.decode_generation_response(br#"{"id":"r","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#,LocalScope::new(2),&source,&target()).unwrap();
    let semantic = ClientManaged::new(source.semantic.settings().clone())
        .unwrap()
        .select_request(&source.semantic)
        .unwrap()
        .select_response(&observed.semantic)
        .unwrap()
        .build(&[])
        .unwrap();
    (
        morphiecore::protocol::DecodedRequest {
            semantic,
            fidelity: observed.fidelity,
        },
        ItemId::scoped(LocalScope::new(2), 1),
    )
}
fn client_view(
    mut decoded: morphiecore::protocol::DecodedRequest,
    owner: ItemId,
    token: &str,
) -> morphiecore::protocol::DecodedRequest {
    let mut items = decoded.semantic.items().to_vec();
    let Item::Reasoning(r) = &mut items.iter_mut().find(|(id, _)| *id == owner).unwrap().1 else {
        panic!()
    };
    // The explicit Gateway carrier boundary labels its own format; generic
    // OpenAI codecs do not infer this format or authenticate it from a prefix.
    r.replay = Some(ReplayValue::final_value(
        ReplayFormat::GatewayContinuation,
        Text::new(token, "opaque", MAX_TEXT_BYTES).unwrap(),
    ));
    decoded.semantic = GenerationRequest::from_settings(items, decoded.semantic.settings().clone())
        .unwrap()
        .with_message_envelopes(decoded.semantic.message_envelopes().to_vec())
        .unwrap();
    decoded.fidelity = Default::default();
    decoded
}
#[test]
fn explicit_responses_carrier_survives_real_flat_history_without_guessing_format() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let item = codec
        .seal_responses_reasoning_item(&source, owner, &target(), "principal", 100, "rs_one")
        .unwrap();
    assert_eq!(item["type"], "reasoning");
    assert_eq!(item["summary"], serde_json::json!([]));
    assert!(!item.to_string().contains("synthetic-signature"));
    let input = serde_json::json!({"max_output_tokens":32,"instructions":"fixed","input":[
        {"role":"user","content":"go"},item,{"role":"assistant","content":"answer"},
        {"role":"user","content":"next"}]});
    let decoded = morphiecore::protocol::openai::responses::decode_generation(&input).unwrap();
    assert!(decoded.semantic.message_envelopes().is_empty());
    let owner = decoded.semantic.items()[1].0;
    assert!(matches!(&decoded.semantic.items()[1].1,Item::Reasoning(r)
        if r.replay.as_ref().unwrap().format()==ReplayFormat::ResponsesEncrypted));
    let restored = codec
        .restore_responses_replay(&decoded, owner, &target(), "principal", 99)
        .unwrap();
    let projected = Profile::AdaptiveTextTools
        .encode_generation_request(
            &restored.request.semantic,
            &restored.request.fidelity,
            &target(),
        )
        .unwrap();
    assert_eq!(
        projected.value["messages"],
        serde_json::json!([
        {"role":"user","content":[{"type":"text","text":"go"}]},
        {"role":"assistant","content":[{"type":"thinking","thinking":"","signature":"synthetic-signature"},
            {"type":"text","text":"answer"}]},
        {"role":"user","content":[{"type":"text","text":"next"}]}])
    );
    assert!(
        codec
            .restore_responses_replay(&decoded, owner, &target(), "other", 99)
            .is_err()
    );
    assert!(decoded.semantic.message_envelopes().is_empty());
}

#[test]
fn flat_multiple_signatures_restore_in_order_without_rebinding_edited_prefixes() {
    let p = Profile::AdaptiveTextTools;
    let source=p.decode_generation_request(br#"{"model":"synthetic-model","max_tokens":32,"messages":[{"role":"user","content":"go"}]}"#,LocalScope::new(1),&target()).unwrap();
    let response=p.decode_generation_response(br#"{"id":"r","type":"message","role":"assistant","model":"synthetic-model","content":[{"type":"thinking","thinking":"first","signature":"sig-one"},{"type":"thinking","thinking":"second","signature":"sig-two"},{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}"#,LocalScope::new(2),&source,&target()).unwrap();
    let history = morphiecore::protocol::DecodedRequest {
        semantic: ClientManaged::new(source.semantic.settings().clone())
            .unwrap()
            .select_request(&source.semantic)
            .unwrap()
            .select_response(&response.semantic)
            .unwrap()
            .build(&[])
            .unwrap(),
        fidelity: response.fidelity,
    };
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let mut input = vec![serde_json::json!({"role":"user","content":"go"})];
    for (owner, item) in history.semantic.items() {
        if matches!(item, Item::Reasoning(_)) {
            input.push(
                codec
                    .seal_responses_reasoning_item(
                        &history,
                        *owner,
                        &target(),
                        "principal",
                        100,
                        &format!("rs_{}", owner.get()),
                    )
                    .unwrap(),
            );
        }
    }
    input.extend([
        serde_json::json!({"role":"assistant","content":"answer"}),
        serde_json::json!({"role":"user","content":"next"}),
    ]);
    let decoded = morphiecore::protocol::openai::responses::decode_generation(
        &serde_json::json!({"max_output_tokens":32,"input":input}),
    )
    .unwrap();
    let mut restored = decoded.clone();
    for owner in [decoded.semantic.items()[1].0, decoded.semantic.items()[2].0] {
        restored = codec
            .restore_responses_replay(&restored, owner, &target(), "principal", 99)
            .unwrap()
            .request;
    }
    let wire = p
        .encode_generation_request(&restored.semantic, &restored.fidelity, &target())
        .unwrap()
        .value;
    assert_eq!(wire["messages"][1]["content"][0]["signature"], "sig-one");
    assert_eq!(wire["messages"][1]["content"][1]["signature"], "sig-two");
    let mut edited = decoded.clone();
    let mut items = edited.semantic.items().to_vec();
    items.remove(0);
    edited.semantic = edited.semantic.with_items(items).unwrap();
    assert!(
        codec
            .restore_responses_replay(
                &edited,
                edited.semantic.items()[0].0,
                &target(),
                "principal",
                99
            )
            .is_err()
    );
}

#[test]
fn authenticated_signature_only_replay_restores_native_format_without_a_second_body() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let token = codec
        .seal(&source, owner, &target(), "synthetic-principal", 100)
        .unwrap();
    assert!(!token.contains("synthetic-signature"));
    let client = client_view(source.clone(), owner, &token);
    let restored = codec
        .restore(&client, owner, &target(), "synthetic-principal", 99)
        .unwrap();
    assert_eq!(restored.disposition, ReplayDisposition::Restored);
    let Item::Reasoning(r) = &restored.request.semantic.items()[1].1 else {
        panic!()
    };
    assert_eq!(
        r.replay.as_ref().unwrap().format(),
        ReplayFormat::AnthropicMessagesThinking
    );
    assert_eq!(r.replay.as_ref().unwrap().as_str(), "synthetic-signature");
    assert!(r.parts.is_empty());
    assert!(!format!("{codec:?}").contains("7"));
}
#[test]
fn wrong_key_principal_tampering_expiry_and_prefix_edits_cannot_bless_a_token() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let token = codec
        .seal(&source, owner, &target(), "principal", 100)
        .unwrap();
    let client = client_view(source.clone(), owner, &token);
    assert!(
        GatewayReplayCodec::new([8; 32])
            .unwrap()
            .restore(&client, owner, &target(), "principal", 99)
            .is_err()
    );
    assert!(
        codec
            .restore(&client, owner, &target(), "other", 99)
            .is_err()
    );
    assert!(
        codec
            .restore(&client, owner, &target(), "principal", 100)
            .is_err()
    );
    let mut settings = client.semantic.settings().clone();
    settings.instructions = Presence::Value(Text::new("edited", "instruction", 256).unwrap());
    let edited = morphiecore::protocol::DecodedRequest {
        semantic: client.semantic.clone().with_settings(settings).unwrap(),
        fidelity: Default::default(),
    };
    assert!(
        codec
            .restore(&edited, owner, &target(), "principal", 99)
            .is_err()
    );
    let mut corrupted = token.clone().into_bytes();
    let index = 20;
    corrupted[index] = if corrupted[index] == b'A' { b'B' } else { b'A' };
    let bad = client_view(
        source.clone(),
        owner,
        std::str::from_utf8(&corrupted).unwrap(),
    );
    assert!(
        codec
            .restore(&bad, owner, &target(), "principal", 99)
            .is_err()
    );
    assert!(
        codec
            .restore(
                &client_view(source, owner, &"x".repeat(MAX_TEXT_BYTES)),
                owner,
                &target(),
                "principal",
                99
            )
            .is_err()
    );
}
#[test]
fn provider_or_model_change_discards_opaque_without_disclosing_native_signature() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let token = codec
        .seal(&source, owner, &target(), "principal", 100)
        .unwrap();
    let client = client_view(source.clone(), owner, &token);
    for other in [
        ReplayTarget::new("other", "synthetic-model").unwrap(),
        ReplayTarget::new("synthetic-provider", "other").unwrap(),
    ] {
        let result = codec
            .restore(&client, owner, &other, "principal", 99)
            .unwrap();
        assert_eq!(
            result.disposition,
            ReplayDisposition::DiscardedOnTargetChange
        );
        assert!(result.request.semantic.items().iter().all(|(_,item)| !matches!(item,Item::Reasoning(r) if r.replay.is_some() || r.parts.is_empty())));
        assert!(matches!(&client.semantic.items()[1].1,Item::Reasoning(r) if r.replay.is_some()));
    }
}
#[test]
fn sampling_changes_are_not_prefix_edits_but_readable_owner_changes_are() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let token = codec
        .seal(&source, owner, &target(), "principal", 100)
        .unwrap();
    let mut client = client_view(source, owner, &token);
    let mut settings = client.semantic.settings().clone();
    settings.controls.max_output_tokens = Some(128);
    client.semantic = client.semantic.clone().with_settings(settings).unwrap();
    assert!(
        codec
            .restore(&client, owner, &target(), "principal", 99)
            .is_ok()
    );
    let mut items = client.semantic.items().to_vec();
    let Item::Reasoning(r) = &mut items[1].1 else {
        panic!()
    };
    r.parts.push((
        PartId::scoped(LocalScope::new(2), 900),
        ReasoningContent::Summary(Text::new("edited", "text", 256).unwrap()),
    ));
    client.semantic = client.semantic.with_items(items).unwrap();
    assert!(
        codec
            .restore(&client, owner, &target(), "principal", 99)
            .is_err()
    );
}
#[test]
fn a_new_intake_scope_does_not_change_the_authenticated_prefix_protocol() {
    let (source, owner) = fixture();
    let codec = GatewayReplayCodec::new([7; 32]).unwrap();
    let token = codec
        .seal(&source, owner, &target(), "principal", 100)
        .unwrap();
    let client = client_view(source, owner, &token);
    let remap = |id: ItemId| ItemId::scoped(LocalScope::new(9), id.get() + id.scope().get() * 10);
    let mut items = client.semantic.items().to_vec();
    for (id, item) in &mut items {
        *id = remap(*id);
        match item {
            Item::Message(m) => {
                for part in &mut m.parts {
                    part.id = PartId::scoped(
                        LocalScope::new(9),
                        part.id.get() + part.id.scope().get() * 100_000,
                    );
                }
            }
            Item::Reasoning(r) => {
                for (id, _) in &mut r.parts {
                    *id = PartId::scoped(LocalScope::new(9), id.get() + id.scope().get() * 100_000);
                }
            }
            _ => {}
        }
    }
    let groups = client
        .semantic
        .message_envelopes()
        .iter()
        .enumerate()
        .map(|(i, g)| {
            MessageEnvelope::new(
                GroupId::new(LocalScope::new(9), i as u64 + 1),
                g.role(),
                g.members().iter().copied().map(remap).collect(),
            )
            .unwrap()
        })
        .collect();
    let decoded = morphiecore::protocol::DecodedRequest {
        semantic: GenerationRequest::from_settings(items, client.semantic.settings().clone())
            .unwrap()
            .with_message_envelopes(groups)
            .unwrap(),
        fidelity: Default::default(),
    };
    assert!(
        codec
            .restore(&decoded, remap(owner), &target(), "principal", 99)
            .is_ok()
    );
}
