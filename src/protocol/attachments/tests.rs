use super::*;
use crate::{
    adapter::{ClientAdapter, ClientProtocol},
    execution::{
        Attempt, AttemptError, ResponseDelivery,
        replay::{CLIENT_ORIGIN, open_from_client, seal_for_client},
    },
    protocol::{
        google::{GenerationSseDecoder, encode_generation_request},
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile, TargetCaps, stream::Obfuscation},
        stream::SseLimits,
    },
};

#[path = "phase_tests.rs"]
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
            Adapter::new(
                if protocol == ClientProtocol::Chat {
                    Profile::Chat
                } else {
                    Profile::Responses
                },
                Dialect::MorphieCore,
                None,
            ),
            TargetCaps::full(),
        ),
        ClientProtocol::Messages => ClientAdapter::Messages,
    };
    client.bind_replay_origin(&CLIENT_ORIGIN);
    client
}
fn delivered(client: &ClientAdapter, source: &DecodedResponse) -> (DecodedResponse, Value) {
    let mut sealed = source.clone();
    seal_for_client(&mut sealed, &CLIENT_ORIGIN).unwrap();
    client.project_native(&mut sealed).unwrap();
    client.snapshot(sealed).unwrap()
}
/// Emulate the observed text coalescing, but retain each actual tool result explicitly.
fn returned(protocol: ClientProtocol, body: &Value) -> Value {
    match protocol {
        ClientProtocol::Chat => json!({"model":"public","messages":[{"role":"user","content":"go"},
            body["choices"][0]["message"],{"role":"tool","tool_call_id":"c","content":"ok"}]}),
        ClientProtocol::Responses => {
            let mut input = vec![json!({"role":"user","content":"go"})];
            let mut text: Option<usize> = None;
            for item in body["output"].as_array().unwrap() {
                if item["type"] == "message" {
                    if let Some(index) = text {
                        let old = input[index]["content"][0]["text"].as_str().unwrap();
                        input[index]["content"][0]["text"] = json!(format!(
                            "{old}{}",
                            item["content"][0]["text"].as_str().unwrap()
                        ));
                    } else {
                        text = Some(input.len());
                        input.push(item.clone());
                    }
                } else {
                    input.push(item.clone());
                }
            }
            input.push(json!({"type":"function_call_output","call_id":"c","output":"ok"}));
            json!({"model":"public","input":input})
        }
        ClientProtocol::Messages => {
            let mut content: Vec<Value> = vec![];
            let mut text: Option<usize> = None;
            for block in body["content"].as_array().unwrap() {
                if block["type"] == "text" {
                    if let Some(index) = text {
                        let old = content[index]["text"].as_str().unwrap();
                        content[index]["text"] =
                            json!(format!("{old}{}", block["text"].as_str().unwrap()));
                    } else {
                        text = Some(content.len());
                        content.push(block.clone());
                    }
                } else {
                    content.push(block.clone());
                }
            }
            json!({"model":"public","max_tokens":64,"messages":[{"role":"user","content":"go"},
                {"role":"assistant","content":content},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c","content":"ok"}]}]})
        }
    }
}
fn expected_parts() -> Value {
    json!([
        {"thought":true,"text":"plan","thoughtSignature":"UjE="},
        {"text":"OK","thoughtSignature":"UzE="},
        {"text":"OK","thoughtSignature":"UzI="},
        {"functionCall":{"id":"c","name":"lookup","args":{"n":9007199254740993u64}},"thoughtSignature":"UzM="}
    ])
}
fn marker(body: &Value, protocol: ClientProtocol) -> &str {
    match protocol {
        ClientProtocol::Chat => body["choices"][0]["message"]["reasoning_details"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["data"]
            .as_str()
            .unwrap(),
        ClientProtocol::Responses => {
            body["output"].as_array().unwrap().last().unwrap()["encrypted_content"]
                .as_str()
                .unwrap()
        }
        ClientProtocol::Messages => {
            body["content"].as_array().unwrap().last().unwrap()["signature"]
                .as_str()
                .unwrap()
        }
    }
}
fn stream_delivery(protocol: ClientProtocol) -> (Attempt, ResponseDelivery) {
    let mut attempt = Attempt::new(
        crate::adapter::UpstreamAdapter::GoogleGenerateContent { target: target() },
        2 << 20,
        SseLimits::default(),
    );
    attempt.begin(200, "text/event-stream").unwrap();
    let delivery = ResponseDelivery::new(
        client(protocol),
        "public",
        SseLimits::default(),
        crate::semantic::context::StreamOptions {
            include_obfuscation: crate::semantic::value::Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    (attempt, delivery)
}
#[test]
fn client_carriers_restore_distinct_equal_text_owners_and_calls() {
    let (source, _) = source();
    assert_eq!(source.semantic.items().len(), 4);
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        let client = client(protocol);
        let (retained, body) = delivered(&client, &source);
        assert_eq!(retained.semantic.items().len(), 4);
        assert!(marker(&body, protocol).starts_with(PREFIX));
        let request = returned(protocol, &body);
        let mut decoded = client
            .decode_client_request(request.to_string().as_bytes())
            .unwrap();
        open_from_client(&mut decoded).unwrap();
        let (wire, _) = encode_generation_request(
            &decoded.task.semantic,
            &decoded.task.fidelity,
            &target(),
            None,
        )
        .unwrap();
        assert_eq!(
            wire["contents"][1]["parts"],
            expected_parts(),
            "{protocol:?}"
        );
        assert_eq!(
            wire["contents"][2]["parts"],
            json!([{"functionResponse":{"id":"c","name":"lookup","response":{"output":"ok"}}}])
        );
    }
}
/// Client edits make the carrier unusable. Visible history still proceeds; only the
/// replay it carried is dropped, and the receipt records that omission.
fn assert_dropped(
    client: &ClientAdapter,
    request: &Value,
    context: &str,
) -> crate::adapter::Request {
    let mut decoded = client
        .decode_client_request(request.to_string().as_bytes())
        .unwrap_or_else(|error| panic!("{context}: {error:?}"));
    assert!(
        decoded
            .task
            .fidelity
            .normalizations()
            .contains(&crate::protocol::fidelity::REPLAY_MANIFEST_OMISSION),
        "{context}"
    );
    assert!(
        decoded
            .task
            .semantic
            .items()
            .iter()
            .all(|(_, item)| !item.has_replay_attachment()),
        "{context}"
    );
    open_from_client(&mut decoded).unwrap();
    decoded
}
/// Only the thought keeps its own carrier; text and call signatures are gone.
fn assert_unsigned_history(request: &crate::adapter::Request, context: &str) {
    let (wire, _) = encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &target(),
        None,
    )
    .unwrap();
    let signatures: Vec<_> = wire["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|content| content["parts"].as_array().unwrap())
        .filter_map(|part| part.get("thoughtSignature"))
        .collect();
    assert_eq!(signatures, [&json!("UjE=")], "{context}");
}
#[test]
fn changed_visible_history_drops_its_carrier_but_keeps_the_request() {
    let (source, _) = source();
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        let client = client(protocol);
        let (_, body) = delivered(&client, &source);
        let request = returned(protocol, &body);
        let mut changed = request.clone();
        match protocol {
            ClientProtocol::Chat => changed["messages"][1]["content"] = json!("OKchanged"),
            ClientProtocol::Responses => {
                changed["input"][2]["content"][0]["text"] = json!("OKchanged")
            }
            ClientProtocol::Messages => {
                changed["messages"][1]["content"][1]["text"] = json!("OKchanged")
            }
        }
        let context = format!("{protocol:?} text");
        assert_unsigned_history(&assert_dropped(&client, &changed, &context), &context);
        let mut changed = request.clone();
        match protocol {
            ClientProtocol::Chat => {
                changed["messages"][1]["tool_calls"][0]["function"]["arguments"] =
                    json!("{\"n\":9007199254740992}")
            }
            ClientProtocol::Responses => {
                changed["input"][3]["arguments"] = json!("{\"n\":9007199254740992}")
            }
            ClientProtocol::Messages => {
                changed["messages"][1]["content"][2]["input"]["n"] = json!(9007199254740992u64)
            }
        }
        let context = format!("{protocol:?} arguments");
        assert_unsigned_history(&assert_dropped(&client, &changed, &context), &context);
        if protocol == ClientProtocol::Responses {
            let mut changed = request.clone();
            changed["input"].as_array_mut().unwrap().swap(2, 3);
            assert_unsigned_history(&assert_dropped(&client, &changed, "reordered"), "reordered");
            let mut changed = request.clone();
            changed["input"][2]["status"] = json!("incomplete");
            // generateContent has no partial-history carrier; that limit is independent.
            assert_dropped(&client, &changed, "status");
            let mut changed = request.clone();
            changed["input"].as_array_mut().unwrap().remove(2);
            assert_unsigned_history(&assert_dropped(&client, &changed, "deleted"), "deleted");
            let mut changed = request.clone();
            let carrier = changed["input"].as_array().unwrap().len() - 2;
            changed["input"][carrier]["encrypted_content"] = json!("mca1.!");
            assert_unsigned_history(&assert_dropped(&client, &changed, "malformed"), "malformed");
        }
    }
}
#[test]
fn stream_carrier_is_published_only_with_the_eof_gated_terminal() {
    let (source, _) = source();
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        let client = client(protocol);
        let (_, body) = delivered(&client, &source);
        let (mut attempt, mut delivery) = stream_delivery(protocol);
        let wire = source_wire(9007199254740993);
        let (n, events) = attempt.push(wire.as_bytes()).unwrap();
        assert_eq!(n, wire.len());
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, StreamEvent::Terminal { .. }))
        );
        let mut bytes = delivery.encode_events(&attempt, &events).unwrap().concat();
        assert!(!String::from_utf8_lossy(&bytes).contains(PREFIX));
        assert!(matches!(
            attempt.terminal_event(),
            Err(AttemptError::Protocol("attempt not finished"))
        ));
        delivery.commit().unwrap();
        attempt.finish().unwrap();
        bytes.extend(delivery.finish_stream(&attempt).unwrap().concat());
        // JSON and SSE record the same applied losses, including grouping that the
        // carrier restores only on return.
        let (mut json_attempt, mut json_delivery) = stream_delivery(protocol);
        json_attempt.push(wire.as_bytes()).unwrap();
        json_attempt.finish().unwrap();
        json_delivery.encode_json(&json_attempt).unwrap();
        let receipts = |losses: &[crate::lowering::generation::LossEvidence]| {
            losses
                .iter()
                .map(|loss| (loss.rule, loss.field))
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(
            receipts(json_delivery.losses()),
            receipts(delivery.losses()),
            "{protocol:?}"
        );
        assert!(
            receipts(delivery.losses())
                .iter()
                .any(|(_, field)| *field == "generation.usage.reports"),
            "{protocol:?}"
        );
        let text = String::from_utf8(bytes).unwrap();
        let count = text.matches(marker(&body, protocol)).count();
        assert_eq!(
            count,
            if protocol == ClientProtocol::Responses {
                2
            } else {
                1
            },
            "{protocol:?}"
        );
        if protocol == ClientProtocol::Responses {
            let end = text
                .lines()
                .filter_map(|l| l.strip_prefix("data: "))
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .next_back()
                .unwrap();
            assert_eq!(end["type"], "response.incomplete");
            assert_eq!(
                end["response"]["output"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap(),
                &json!({
                    "type":"reasoning",
                    "id":body["output"].as_array().unwrap().last().unwrap()["id"],
                    "status":"completed",
                    "summary":[],
                    "encrypted_content":marker(&body, protocol)
                })
            );
        }
    }
}
#[test]
fn premature_or_truncated_eof_never_publishes_the_carrier() {
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        for truncated in [false, true] {
            let (mut attempt, mut delivery) = stream_delivery(protocol);
            let wire = source_wire(9007199254740993);
            let (n, events) = attempt.push(wire.as_bytes()).unwrap();
            assert_eq!(n, wire.len());
            let bytes = delivery.encode_events(&attempt, &events).unwrap().concat();
            assert!(!String::from_utf8_lossy(&bytes).contains(PREFIX));
            delivery.commit().unwrap();
            if truncated {
                let suffix = b"data: {";
                let (n, events) = attempt.push(suffix).unwrap();
                assert_eq!(n, suffix.len());
                assert!(events.is_empty());
                assert!(matches!(
                    attempt.finish(),
                    Err(AttemptError::Sse(
                        crate::protocol::stream::SseError::Framing(
                            crate::transport::sse::SseDecodeError::UnexpectedEof
                        )
                    ))
                ));
                assert!(matches!(
                    delivery.finish_stream(&attempt),
                    Err(AttemptError::Protocol("rejected attempt"))
                ));
            } else {
                assert!(matches!(
                    delivery.finish_stream(&attempt),
                    Err(AttemptError::Protocol("attempt not finished"))
                ));
            }
            assert!(!delivery.lifecycle().may_retry_or_fallback());
            assert!(matches!(
                delivery.finish_stream(&attempt),
                Err(AttemptError::Protocol("delivery closed"))
            ));
        }
    }
}
#[test]
fn client_carriers_have_independent_wire_shapes() {
    assert_eq!(
        response_item("mca1.synthetic"),
        json!({
            "type":"reasoning",
            "id":"rs_mca_5caf1c16dabeb0a8c98b1a50a7e431dd",
            "status":"completed",
            "summary":[],
            "encrypted_content":"mca1.synthetic"
        })
    );
    assert_eq!(
        chat_detail("mca1.synthetic"),
        json!({
            "type":"reasoning.encrypted",
            "id":"rs_mca_5caf1c16dabeb0a8c98b1a50a7e431dd",
            "format":"openai-responses-v1",
            "data":"mca1.synthetic"
        })
    );
    assert_eq!(
        messages_block("mca1.synthetic"),
        json!({"type":"thinking","thinking":"","signature":"mca1.synthetic"})
    );
    // Without readable reasoning, Chat delivery still owns the details carrier.
    let (source, _) = decode_source_wire(&source_wire_case(7, true));
    let (_, body) = delivered(&client(ClientProtocol::Chat), &source);
    let token = marker(&body, ClientProtocol::Chat);
    assert_eq!(
        body["choices"][0]["message"]["reasoning_details"],
        json!([chat_detail(token)])
    );
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
        Err(CodecError::Invalid("part replay dependency"))
    ));
}

/// Separate real-client codec probe, not binding admission or Provider acceptance.
#[test]
#[ignore = "requires the pinned public pi checkout and isolated SDK dependencies"]
fn pinned_pi_owned_replay() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let pi_source = std::path::PathBuf::from(
        std::env::var("MORPHIECORE_PI_SOURCE").expect("set MORPHIECORE_PI_SOURCE"),
    );
    let protocols = [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ];
    let mut fixtures = vec![];
    for (number, empty) in [(7, false), (9007199254740993, false), (7, true)] {
        for protocol in protocols {
            let mut attempt = crate::execution::Attempt::new(
                crate::adapter::UpstreamAdapter::GoogleGenerateContent { target: target() },
                2 << 20,
                SseLimits::default(),
            );
            let mut delivery = crate::execution::ResponseDelivery::new(
                client(protocol),
                "public",
                SseLimits::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: crate::semantic::value::Presence::Value(false),
                    ..Default::default()
                },
                Obfuscation::Disabled,
            );
            attempt.begin(200, "text/event-stream").unwrap();
            let wire = source_wire_case(number, empty);
            let (n, events) = attempt.push(wire.as_bytes()).unwrap();
            assert_eq!(n, wire.len());
            let mut bytes = delivery.encode_events(&attempt, &events).unwrap().concat();
            assert!(!String::from_utf8_lossy(&bytes).contains(PREFIX));
            if !bytes.is_empty() {
                delivery.commit().unwrap();
            }
            attempt.finish().unwrap();
            bytes.extend(delivery.finish_stream(&attempt).unwrap().concat());
            let api = match protocol {
                ClientProtocol::Chat => "openai-completions",
                ClientProtocol::Responses => "openai-responses",
                ClientProtocol::Messages => "anthropic-messages",
            };
            fixtures
                .push(json!({"api":api,"wire":String::from_utf8(bytes).unwrap(),"empty":empty}));
        }
    }
    for protocol in protocols {
        let mut attempt = Attempt::new(
            Adapter::new(Profile::Responses, Dialect::Standard, None),
            2 << 20,
            SseLimits::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            client(protocol),
            "public",
            SseLimits::default(),
            crate::semantic::context::StreamOptions {
                include_obfuscation: crate::semantic::value::Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        );
        let mut bytes = vec![];
        for (sequence, mut frame) in phase_tests::frames().into_iter().enumerate() {
            frame["sequence_number"] = json!(sequence);
            let wire = format!(
                "event: {}\ndata: {frame}\n\n",
                frame["type"].as_str().unwrap()
            );
            let (_, events) = attempt.push(wire.as_bytes()).unwrap();
            bytes.extend(delivery.encode_events(&attempt, &events).unwrap().concat());
        }
        attempt.finish().unwrap();
        bytes.extend(delivery.finish_stream(&attempt).unwrap().concat());
        let api = match protocol {
            ClientProtocol::Chat => "openai-completions",
            ClientProtocol::Responses => "openai-responses",
            ClientProtocol::Messages => "anthropic-messages",
        };
        fixtures.push(json!({"api":api,"wire":String::from_utf8(bytes).unwrap(),"phase":true}));
    }
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new("node")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", home.path())
        .arg("--import")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/sdk/pi/register.mjs"
        ))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/sdk/pi_owned_replay.mjs"
        ))
        .arg(pi_source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&fixtures).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let returned: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(returned.len(), 12);
    for (n, body) in returned.iter().enumerate() {
        let protocol = protocols[n % 3];
        if n >= 9 {
            let request = client(protocol)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            let phases: Vec<_> = request
                .task
                .semantic
                .items()
                .iter()
                .filter_map(|(_, item)| match item {
                    Item::Message(m) if m.role == MessageRole::Assistant => Some(m.phase),
                    _ => None,
                })
                .collect();
            assert_eq!(
                phases,
                [
                    None,
                    None,
                    Some(Phase::Commentary),
                    Some(Phase::FinalAnswer)
                ],
                "{protocol:?}"
            );
            continue;
        }
        let rounded = (3..6).contains(&n);
        let decoded = if rounded {
            // pi rounds the large integer; the changed call drops the whole carrier.
            assert_dropped(&client(protocol), body, &format!("{protocol:?} rounded"))
        } else {
            let mut decoded = client(protocol)
                .decode_client_request(body.to_string().as_bytes())
                .unwrap();
            open_from_client(&mut decoded).unwrap();
            decoded
        };
        // Request controls/tools have separate frozen admission contracts; this probe
        // re-encodes the returned history, including the actual synthetic tool result.
        let history = GenerationRequest::from_resources(
            decoded.task.semantic.items().to_vec(),
            GenerationSettings::default(),
            decoded.task.semantic.resources().clone(),
        )
        .unwrap()
        .with_message_envelopes(decoded.task.semantic.message_envelopes().to_vec())
        .unwrap();
        let (wire, _) =
            encode_generation_request(&history, &decoded.task.fidelity, &target(), None).unwrap();
        if rounded {
            let parts = wire["contents"][1]["parts"].as_array().unwrap();
            let signatures: Vec<_> = parts
                .iter()
                .filter_map(|part| part.get("thoughtSignature"))
                .collect();
            assert_eq!(signatures, [&json!("UjE=")], "{protocol:?}");
            assert_eq!(
                parts.last().unwrap()["functionCall"]["args"]["n"],
                json!(9007199254740992u64),
                "{protocol:?}"
            );
            continue;
        }
        if n == 6 {
            // Accepted client limitation: pinned pi Chat drops an assistant turn with
            // neither text nor calls, so its signature is lost but the request proceeds.
            assert!(!body.to_string().contains(PREFIX));
            let contents = wire["contents"].as_array().unwrap();
            assert!(contents.iter().all(|content| content["role"] == "user"));
            continue;
        }
        let mut expected = if n >= 6 {
            json!([{"text":"","thoughtSignature":"UzE="}])
        } else {
            expected_parts()
        };
        if n < 3 {
            expected[3]["functionCall"]["args"]["n"] = json!(7);
        }
        assert_eq!(wire["contents"][1]["parts"], expected, "{protocol:?}");
        if n < 3 {
            assert_eq!(
                wire["contents"][2]["parts"][0]["functionResponse"]["id"],
                "c"
            );
        }
    }
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
    let request = crate::adapter::Request::from_generation(
        crate::protocol::DecodedRequest {
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
fn manifests_reject_duplicate_keys_and_oversized_parts() {
    let baseline = r#"{"items":[{"type":"message","status":"completed","phase":null,"parts":[{"bytes":2,"hash":"fixture-hash","replay":"fixture-replay"}]}],"groups":[]}"#;
    let encoded = |raw: &[u8]| format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(raw));
    assert!(Packet::decode(&encoded(baseline.as_bytes())).is_ok());
    let mut value: Value = serde_json::from_str(baseline).unwrap();
    value["items"][0]["parts"][0]["bytes"] = json!(MAX_TEXT_BYTES + 1);
    let oversized = encoded(&serde_json::to_vec(&value).unwrap());
    assert!(matches!(Packet::decode(&oversized), Err(CodecError::Limit)));
    let duplicate = baseline.replace(r#""groups":[]"#, r#""groups":[],"groups":[]"#);
    assert!(matches!(
        Packet::decode(&encoded(duplicate.as_bytes())),
        Err(CodecError::Invalid("replay attachment manifest"))
    ));
}
#[test]
fn ordinary_reasoning_cannot_hide_an_unrestored_carrier() {
    let (source, _) = source();
    let client = client(ClientProtocol::Responses);
    let (_, body) = delivered(&client, &source);
    let raw = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut request = raw
        .decode_request(
            json!({"model":"public","input":body["output"]})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    assert!(matches!(
        open_from_client(&mut request),
        Err(CodecError::Invalid("unrestored attachment carrier"))
    ));
}
#[test]
fn unsupported_chat_carrier_is_rejected_by_static_and_stream_delivery() {
    let (source, _) = decode_source_wire(&source_wire_case(7, true));
    let mut sealed = source.clone();
    seal_for_client(&mut sealed, &CLIENT_ORIGIN).unwrap();
    let mut client = ClientAdapter::openai(
        Adapter::new(Profile::Chat, Dialect::Standard, None),
        TargetCaps::full(),
    );
    client.bind_replay_origin(&CLIENT_ORIGIN);
    let token = Packet::from_response(&sealed)
        .unwrap()
        .unwrap()
        .encode()
        .unwrap();
    assert!(
        matches!(client.snapshot(sealed),Err(crate::adapter::AdapterError::Codec(CodecError::Unsupported(reason))) if reason=="Chat attachment carrier")
    );
    let mut encoder = client
        .stream_encoder(
            source.metadata,
            SseLimits::default(),
            crate::semantic::context::StreamOptions {
                include_obfuscation: crate::semantic::value::Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        )
        .unwrap();
    assert!(
        matches!(encoder.set_attachment(token),Err(crate::protocol::stream::SseError::Codec(CodecError::Unsupported(reason))) if reason=="Chat attachment carrier")
    );
}

#[test]
fn independent_empty_carriers_rebuild_once_and_share_aggregate_budgets() {
    let (source, _) = decode_source_wire(&source_wire_case(7, true));
    let (_, body) = delivered(&client(ClientProtocol::Responses), &source);
    let token = marker(&body, ClientProtocol::Responses);
    let mut items = vec![];
    let mut markers = vec![];
    for n in 1..=4096 {
        items.push((
            ItemId::new(n),
            Item::Message(Message {
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(n),
                    ContentPart::Text(Text::allowing_empty("", "fixture", 64).unwrap().into()),
                )],
            }),
        ));
        let marker = Marker::prefix(Carried::Manifest(token.into()), &items, markers.last());
        assert_eq!((marker.start, marker.end), (n as usize - 1, n as usize));
        markers.push(marker);
    }
    let mut groups = vec![];
    let mut fidelity = FidelityRecords::default();
    restore(&mut items, &mut groups, &mut fidelity, markers).unwrap();
    assert_eq!(items.len(), 4096);
    assert_eq!(groups.len(), 4096);
    assert!(items.iter().all(|(_, item)| item.has_replay_attachment()));
    let mut packet = Packet::decode(token).unwrap();
    packet.items.extend((1..MAX_ITEMS).map(|_| Entry::Message {
        status: Status::Completed,
        phase: None,
        parts: vec![],
    }));
    packet.validate().unwrap();
    let mut items = vec![(
        ItemId::new(1),
        Item::Message(Message {
            role: MessageRole::User,
            status: ItemLifecycle::Completed,
            phase: None,
            parts: vec![Part::new(
                PartId::new(1),
                ContentPart::Text(Text::new("go", "fixture", 64).unwrap().into()),
            )],
        }),
    )];
    assert!(matches!(
        restore(
            &mut items,
            &mut vec![],
            &mut FidelityRecords::default(),
            vec![Marker {
                start: 1,
                end: 1,
                packet: Some(packet)
            }]
        ),
        Err(CodecError::Limit)
    ));
}
#[test]
fn restore_keeps_described_phase_and_drops_unsplittable_carriers() {
    let token = replay_envelope::seal(
        &CLIENT_ORIGIN,
        ReplayFormat::GoogleGenerateContentSignature,
        "UzE=",
    )
    .unwrap();
    let segment = |text: &str| Segment {
        bytes: text.len(),
        hash: digest(&[text]),
        replay: Some(token.as_str().to_owned()),
    };
    let marker = |phase: Option<&str>, parts| Marker {
        start: 0,
        end: 1,
        packet: Some(Packet {
            items: vec![Entry::Message {
                status: Status::Completed,
                phase: phase.map(str::to_owned),
                parts,
            }],
            groups: vec![],
        }),
    };
    let history = |phase, parts: Vec<Part>| {
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::Assistant,
                status: ItemLifecycle::Completed,
                phase,
                parts,
            }),
        )]
    };
    let text = |id, value: &str| {
        Part::new(
            PartId::new(id),
            ContentPart::Text(Text::new(value, "fixture", 64).unwrap().into()),
        )
    };
    let run = |mut items: Vec<(ItemId, Item)>, marker| {
        let mut fidelity = FidelityRecords::default();
        restore(&mut items, &mut vec![], &mut fidelity, vec![marker]).unwrap();
        (items, fidelity)
    };
    // A mismatch keeps the returned visible history unchanged and records the omission.
    let dropped = |items: Vec<(ItemId, Item)>, marker: Marker, reason: &str| {
        let failure = restore_marker(
            &items,
            Packet::decode(marker.packet.as_ref().unwrap().encode().unwrap().as_str()).unwrap(),
            &mut FidelityRecords::default(),
            &mut Ids {
                item: 9,
                part: 9,
                group: 0,
            },
        );
        assert!(
            matches!(&failure, Err(CodecError::Invalid(r)) if *r == reason)
                || matches!(&failure, Err(CodecError::Unsupported(r)) if r == reason),
            "{reason}"
        );
        let (restored, fidelity) = run(items.clone(), marker);
        assert_eq!(restored, items);
        assert!(
            fidelity
                .normalizations()
                .contains(&crate::protocol::fidelity::REPLAY_MANIFEST_OMISSION)
        );
    };

    // Responses-style returned history has no phase; the manifest supplies it.
    let (restored, fidelity) = run(
        history(None, vec![text(1, "OKOK")]),
        marker(Some("commentary"), vec![segment("OK"), segment("OK")]),
    );
    assert!(fidelity.normalizations().is_empty());
    let Item::Message(message) = &restored[0].1 else {
        panic!("restored message")
    };
    assert_eq!(message.phase, Some(Phase::Commentary));
    assert_eq!(
        message
            .parts
            .iter()
            .map(|part| match &part.content {
                ContentPart::Text(text) => (text.as_str(), part.replay.is_some()),
                _ => panic!("restored text"),
            })
            .collect::<Vec<_>>(),
        [("OK", true), ("OK", true)]
    );
    dropped(
        history(Some(Phase::FinalAnswer), vec![text(1, "OKOK")]),
        marker(Some("commentary"), vec![segment("OK"), segment("OK")]),
        "replay attachment manifest",
    );

    // A cache marker ending inside a restored part has no faithful owner.
    let mut cached = text(1, "O");
    cached.cache = Some(PrefixCacheTtl::FiveMinutes);
    dropped(
        history(None, vec![cached, text(2, "K")]),
        marker(None, vec![segment("OK")]),
        "attachment cache boundary",
    );
    // Splitting probabilities or citations would reassign them to guessed boundaries.
    let mut annotated = text(1, "OKOK");
    annotated.content = ContentPart::Text(
        TextContent::new(
            Text::new("OKOK", "fixture", 64).unwrap(),
            vec![],
            crate::semantic::value::Presence::Value(vec![]),
        )
        .unwrap(),
    );
    dropped(
        history(None, vec![annotated]),
        marker(None, vec![segment("OK"), segment("OK")]),
        "attachment metadata split",
    );
}
/// Envelopes issued before part/call ownership trailed signature-only reasoning items
/// anchored by call ID or text hash. They are dropped, not rejected or re-anchored.
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
        Err(CodecError::Invalid("Gemini signature payload"))
    ));
}
