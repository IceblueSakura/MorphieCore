//! Adapter/execution acceptance with independent wire and explicit I/O commit.
use crate::{chat_wire, wire};
use morphiecore::{
    execution::{Attempt, AttemptError, ResponseDelivery, admit, prepare},
    lowering::generation::ReportedFactPolicy,
    protocol::{
        openai::{
            Adapter, Dialect, Profile, TargetCaps as Contract, chat::sse::ChatSseDecoder,
            responses::sse::ResponsesSseDecoder, stream::Obfuscation,
        },
        stream::SseLimits,
    },
    provider::{ErrorClass, SecretMaterial, catalog as providers},
    semantic::{
        context::StreamOptions,
        task::generation::*,
        value::{Presence, ReplayOrigin},
    },
    topology::{EndpointId, catalog},
};
use serde_json::{Value, json};

fn standard(protocol: Profile) -> Adapter {
    Adapter::new(protocol, Dialect::Standard, None)
}
fn options() -> StreamOptions {
    StreamOptions {
        include_usage: Presence::Value(true),
        include_obfuscation: Presence::Value(false),
    }
}
fn delivery(adapter: Adapter, contract: Contract) -> ResponseDelivery {
    ResponseDelivery::new(
        morphiecore::adapter::ClientAdapter::openai(adapter, contract),
        "public-model",
        SseLimits::default(),
        options(),
        Obfuscation::Disabled,
    )
}
fn bytes(profile: Profile) -> Vec<u8> {
    match profile {
        Profile::Chat => {
            let mut body = chat_wire::events(2)
                .iter()
                .flat_map(|v| format!("data: {v}\n\n").into_bytes())
                .collect::<Vec<_>>();
            body.extend_from_slice(b"data: [DONE]\n\n");
            body
        }
        Profile::Responses => wire::events(2)
            .iter()
            .flat_map(|v| {
                morphiecore::protocol::openai::responses::sse::encode_frame(
                    v,
                    SseLimits::default().max_event_bytes,
                )
                .unwrap()
            })
            .collect(),
    }
}
fn consume(
    attempt: &mut Attempt,
    output: &mut ResponseDelivery,
    body: &[u8],
) -> Result<Vec<u8>, AttemptError> {
    let mut remaining = body;
    let mut wire = vec![];
    while !remaining.is_empty() {
        let (n, events) = attempt.push(remaining)?;
        assert!(n > 0);
        for frame in output.encode_events(attempt, &events)? {
            wire.extend_from_slice(&frame);
        }
        remaining = &remaining[n..];
    }
    Ok(wire)
}
#[test]
fn incremental_chain_emits_before_terminal_and_does_not_commit_by_encoding() {
    for profile in [Profile::Chat, Profile::Responses] {
        let mut intake = Attempt::new(standard(profile), 4096, SseLimits::default());
        let mut output = delivery(standard(profile), Contract::full());
        intake.begin(200, "text/event-stream").unwrap();
        let body = bytes(profile);
        let (n, events) = intake.push(&body).unwrap();
        assert!(
            n < body.len(),
            "one push must not drain the upstream stream"
        );
        let first = output.encode_events(&intake, &events).unwrap();
        assert!(
            !first.is_empty(),
            "first downstream frame precedes upstream terminal"
        );
        assert!(intake.response().is_err());
        assert!(
            output.lifecycle().may_retry_or_fallback(),
            "encoding is not I/O commit"
        );
        output.commit().unwrap();
        assert!(!output.lifecycle().may_retry_or_fallback());
        let mut encoded: Vec<u8> = first.into_iter().flatten().collect();
        encoded.extend(consume(&mut intake, &mut output, &body[n..]).unwrap());
        assert!(output.complete(&intake).is_err());
        intake.finish().unwrap();
        let before_terminal = encoded.len();
        encoded.extend(output.finish_stream(&intake).unwrap().into_iter().flatten());
        assert!(encoded.len() > before_terminal);
        output.complete(&intake).unwrap();
        assert!(output.finish_stream(&intake).is_err());
        let decoded = match profile {
            Profile::Chat => {
                let mut decoder =
                    ChatSseDecoder::new(200, "text/event-stream", SseLimits::default()).unwrap();
                let mut rest = encoded.as_slice();
                while !rest.is_empty() {
                    let (n, _) = decoder.consume(rest).unwrap();
                    rest = &rest[n..];
                }
                decoder.finish().unwrap();
                decoder.materialize().unwrap()
            }
            Profile::Responses => {
                let mut decoder =
                    ResponsesSseDecoder::new(200, "text/event-stream", SseLimits::default(), None)
                        .unwrap();
                let mut rest = encoded.as_slice();
                while !rest.is_empty() {
                    let (n, _) = decoder.consume(rest).unwrap();
                    rest = &rest[n..];
                }
                decoder.finish().unwrap();
                decoder.materialize().unwrap()
            }
        };
        assert_eq!(decoded.metadata.model, "public-model");
        assert_eq!(decoded.semantic, intake.response().unwrap().semantic);
        let observed = intake.response().unwrap().clone();
        assert_eq!(intake.finish().unwrap(), &observed);
        assert_eq!(intake.metadata(), Some(&observed.metadata));
        assert_eq!(intake.fidelity(), Some(&observed.fidelity));
        assert!(matches!(
            intake.begin(200, "text/event-stream"),
            Err(AttemptError::Protocol("attempt already began"))
        ));
        assert_eq!(intake.response().unwrap(), &observed);
        assert!(matches!(
            intake.push(b""),
            Err(AttemptError::Protocol("attempt is closed"))
        ));
        assert!(matches!(
            intake.finish(),
            Err(AttemptError::Protocol("rejected attempt"))
        ));
        assert!(intake.response().is_err());
        assert!(intake.metadata().is_none());
        assert!(intake.fidelity().is_none());
    }
}

#[test]
fn static_intake_finish_is_idempotent_and_cancellation_discards_the_result() {
    let mut intake = Attempt::new(standard(Profile::Responses), 1 << 20, SseLimits::default());
    intake.begin(200, "application/json").unwrap();
    let body = wire::response(2).to_string();
    assert_eq!(intake.push(body.as_bytes()).unwrap().0, body.len());
    let observed = intake.finish().unwrap().clone();
    assert_eq!(intake.finish().unwrap(), &observed);
    assert_eq!(intake.metadata(), Some(&observed.metadata));
    assert_eq!(intake.fidelity(), Some(&observed.fidelity));
    assert_eq!(intake.ignored_events(), 0);
    intake.cancel();
    assert!(intake.response().is_err());
    assert!(intake.metadata().is_none());
    assert!(intake.fidelity().is_none());
    assert!(matches!(
        intake.finish(),
        Err(AttemptError::Protocol("rejected attempt"))
    ));
}

#[test]
fn delivery_cannot_switch_attempts_even_when_wire_identity_matches() {
    let profile = Profile::Responses;
    let body = bytes(profile);
    let mut first = Attempt::new(standard(profile), 4096, SseLimits::default());
    first.begin(200, "text/event-stream").unwrap();
    let (_, events) = first.push(&body).unwrap();
    let mut output = delivery(standard(profile), Contract::full());
    output.encode_events(&first, &events).unwrap();
    output.commit().unwrap();
    let mut second = Attempt::new(standard(profile), 4096, SseLimits::default());
    second.begin(200, "text/event-stream").unwrap();
    let (used, _) = second.push(&body).unwrap();
    let (_, events) = second.push(&body[used..]).unwrap();
    assert!(output.encode_events(&second, &events).is_err());
    assert!(!output.lifecycle().may_retry_or_fallback());
}

#[test]
fn terminal_is_withheld_on_truncated_eof_or_trailing_invalid_data() {
    for profile in [Profile::Chat, Profile::Responses] {
        for trailing in [false, true] {
            let mut intake = Attempt::new(standard(profile), 4096, SseLimits::default());
            let mut output = delivery(standard(profile), Contract::full());
            intake.begin(200, "text/event-stream").unwrap();
            let mut body = bytes(profile);
            if trailing {
                body.extend_from_slice(b"data: broken\n\n");
            } else {
                body.pop();
            }
            let mut rest = body.as_slice();
            let mut encoded = vec![];
            let mut rejected = false;
            while !rest.is_empty() {
                match intake.push(rest) {
                    Ok((n, events)) => {
                        assert!(
                            events
                                .iter()
                                .all(|e| !matches!(e, StreamEvent::Terminal { .. }))
                        );
                        encoded.extend(
                            output
                                .encode_events(&intake, &events)
                                .unwrap()
                                .into_iter()
                                .flatten(),
                        );
                        if !encoded.is_empty() {
                            output.commit().unwrap();
                        }
                        rest = &rest[n..];
                    }
                    Err(_) => {
                        rejected = true;
                        break;
                    }
                }
            }
            assert!(rejected || intake.finish().is_err());
            assert!(output.finish_stream(&intake).is_err());
            assert!(!output.lifecycle().may_retry_or_fallback());
            assert!(
                !String::from_utf8(encoded)
                    .unwrap()
                    .contains(if profile == Profile::Chat {
                        "[DONE]"
                    } else {
                        "response.completed"
                    })
            );
            assert!(intake.push(b"\n\n").is_err());
        }
    }
}
#[test]
fn strict_completeness_can_fail_late_without_fabricating_a_terminal() {
    let adapter = Adapter::new(Profile::Responses, Dialect::Xiaomi, None);
    let mut intake = Attempt::new(adapter, 4096, SseLimits::default());
    let mut output = delivery(
        standard(Profile::Responses),
        Contract {
            reported_facts: ReportedFactPolicy::StrictComplete,
            ..Contract::full()
        },
    );
    intake.begin(200, "text/event-stream").unwrap();
    let mut frames = wire::events(2);
    for frame in &mut frames {
        if let Some(response) = frame.get_mut("response").and_then(Value::as_object_mut) {
            for key in ["tools", "tool_choice", "parallel_tool_calls"] {
                response.remove(key);
            }
        }
    }
    let body: Vec<u8> = frames
        .iter()
        .flat_map(|v| {
            morphiecore::protocol::openai::responses::sse::encode_frame(
                v,
                SseLimits::default().max_event_bytes,
            )
            .unwrap()
        })
        .collect();
    let encoded = consume(&mut intake, &mut output, &body).unwrap();
    assert!(!encoded.is_empty());
    output.commit().unwrap();
    intake.finish().unwrap();
    assert!(output.finish_stream(&intake).is_err());
    assert!(!output.lifecycle().may_retry_or_fallback());
    let mut intake = Attempt::new(standard(Profile::Chat), 4096, SseLimits::default());
    let mut output = ResponseDelivery::new(
        morphiecore::adapter::ClientAdapter::Messages,
        "public",
        SseLimits::default(),
        options(),
        Obfuscation::Disabled,
    );
    intake.begin(200, "text/event-stream").unwrap();
    let mut chunks = chat_wire::events(1);
    chunks[1]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"] = json!("{\"x\":1,");
    chunks[2]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"] = json!("\"x\":2}");
    chunks.last_mut().unwrap()["usage"]["prompt_tokens_details"] = json!({"cached_tokens":0});
    let mut body: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| format!("data: {chunk}\n\n").into_bytes())
        .collect();
    body.extend_from_slice(b"data: [DONE]\n\n");
    let encoded = consume(&mut intake, &mut output, &body).unwrap();
    assert!(!encoded.is_empty());
    output.commit().unwrap();
    intake.finish().unwrap();
    assert!(output.finish_stream(&intake).is_err());
    assert!(output.complete(&intake).is_err());
    assert!(
        !encoded
            .windows(b"message_stop".len())
            .any(|bytes| bytes == b"message_stop")
    );
}
#[test]
fn static_delivery_wraps_the_issuer_value_and_commit_is_explicit() {
    let scope = ReplayOrigin::new("scope-a").unwrap();
    let source = Adapter::new(Profile::Responses, Dialect::Standard, Some(scope.clone()));
    let mut body = wire::response(2);
    body["output"].as_array_mut().unwrap().insert(0, json!({"id":"rs1","type":"reasoning","status":"completed","summary":[],"encrypted_content":"synthetic-token"}));
    let mut intake = Attempt::new(source.clone(), 1 << 20, SseLimits::default());
    intake.begin(200, "application/json").unwrap();
    intake.push(body.to_string().as_bytes()).unwrap();
    intake.finish().unwrap();
    let mut permitted = delivery(
        source,
        Contract {
            replay_origin: Some(scope.clone()),
            ..Contract::full()
        },
    );
    let encoded: Value = serde_json::from_slice(&permitted.encode_json(&intake).unwrap()).unwrap();
    assert_eq!(encoded["model"], "public-model");
    let (issuer, format, payload) = morphiecore::protocol::replay_envelope::open(
        encoded["output"][0]["encrypted_content"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(issuer, scope);
    assert_eq!(format, ReplayFormat::ResponsesEncrypted);
    assert_eq!(payload.as_str(), "synthetic-token");
    assert!(permitted.lifecycle().may_retry_or_fallback());
    permitted.commit().unwrap();
    permitted.complete(&intake).unwrap();
    assert!(permitted.encode_json(&intake).is_err());
}
#[test]
fn streaming_carrier_seals_opaque_for_both_downstream_protocols() {
    let scope = morphiecore::protocol::replay_envelope::issuer("provider", "model");
    let reasoning = json!({"id":"rs","type":"reasoning","status":"completed",
        "summary":[],"encrypted_content":"synthetic-stream-token"});
    let message = json!({"id":"msg","type":"message","role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]});
    let completed = json!({"object":"response","id":"r","model":"m","created_at":1,
        "status":"completed","output":[reasoning, message]});
    let mut initial = completed.clone();
    initial["status"] = json!("in_progress");
    initial["output"] = json!([]);
    initial["usage"] = Value::Null;
    initial["completed_at"] = Value::Null;
    let events = [
        json!({"type":"response.created","response":initial}),
        json!({"type":"response.output_item.added","output_index":0,
            "item":{"id":"rs","type":"reasoning","status":"in_progress","summary":[]}}),
        json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
        json!({"type":"response.output_item.added","output_index":1,
            "item":{"id":"msg","type":"message","role":"assistant","status":"in_progress","content":[]}}),
        json!({"type":"response.content_part.added","output_index":1,"item_id":"msg","content_index":0,
            "part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta","output_index":1,"item_id":"msg","content_index":0,"delta":"answer","logprobs":[]}),
        json!({"type":"response.output_text.done","output_index":1,"item_id":"msg","content_index":0,"text":"answer","logprobs":[]}),
        json!({"type":"response.content_part.done","output_index":1,"item_id":"msg","content_index":0,"part":message["content"][0]}),
        json!({"type":"response.output_item.done","output_index":1,"item":message}),
        json!({"type":"response.completed","response":completed}),
    ];
    let body: Vec<u8> = events
        .into_iter()
        .enumerate()
        .flat_map(|(n, mut event)| {
            event["sequence_number"] = json!(n);
            morphiecore::protocol::openai::responses::sse::encode_frame(&event, 1 << 20).unwrap()
        })
        .collect();
    for profile in [Profile::Chat, Profile::Responses] {
        let source = Adapter::new(Profile::Responses, Dialect::Standard, Some(scope.clone()));
        let mut intake = Attempt::new(source, 1 << 20, SseLimits::default());
        let target = Adapter::new(profile, Dialect::MorphieCore, None);
        let mut output = delivery(target.clone(), Contract::full());
        intake.begin(200, "text/event-stream").unwrap();
        let mut bytes = consume(&mut intake, &mut output, &body).unwrap();
        assert!(!bytes.is_empty());
        output.commit().unwrap();
        intake.finish().unwrap();
        bytes.extend(output.finish_stream(&intake).unwrap().into_iter().flatten());
        output.complete(&intake).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("synthetic-stream-token"));
        let mut decoder = target.event_decoder();
        for line in text.lines().filter_map(|line| line.strip_prefix("data: ")) {
            if line == "[DONE]" {
                decoder.done().unwrap();
            } else {
                decoder.push(&serde_json::from_str(line).unwrap()).unwrap();
            }
        }
        decoder.finish().unwrap();
        let decoded = decoder.materialize().unwrap();
        let Item::Reasoning(r) = &decoded.semantic.items()[0].1 else {
            panic!("reasoning")
        };
        let (issuer, format, payload) =
            morphiecore::protocol::replay_envelope::open(r.replay.as_ref().unwrap().as_str())
                .unwrap();
        assert_eq!(issuer, scope);
        assert_eq!(format, ReplayFormat::ResponsesEncrypted);
        assert_eq!(payload.as_str(), "synthetic-stream-token");
    }
}

#[test]
fn intake_errors_are_classified_bounded_and_irrecoverable() {
    let mut rejected = Attempt::new(standard(Profile::Chat), 16, SseLimits::default());
    assert!(matches!(
        rejected.begin(429, "application/json"),
        Err(AttemptError::Status {
            class: ErrorClass::RateLimit,
            ..
        })
    ));
    assert!(rejected.begin(200, "application/json").is_err());
    let mut limited = Attempt::new(standard(Profile::Chat), 16, SseLimits::default());
    limited.begin(200, "application/json").unwrap();
    assert!(matches!(
        limited.push(&[b' '; 17]),
        Err(AttemptError::Limit)
    ));
    assert!(limited.finish().is_err());
    let mut malformed = Attempt::new(standard(Profile::Chat), 4096, SseLimits::default());
    malformed.begin(200, "application/json").unwrap();
    malformed.push(b"{}").unwrap();
    assert!(malformed.finish().is_err());
    assert!(malformed.push(b"{}").is_err());
}
#[test]
fn cancellation_closes_intake_and_delivery_without_success() {
    for profile in [Profile::Chat, Profile::Responses] {
        let mut intake = Attempt::new(standard(profile), 4096, SseLimits::default());
        let mut output = delivery(standard(profile), Contract::full());
        intake.begin(200, "text/event-stream").unwrap();
        let (_, events) = intake.push(&bytes(profile)).unwrap();
        output.encode_events(&intake, &events).unwrap();
        output.commit().unwrap();
        intake.cancel();
        output.cancel();
        assert!(intake.finish().is_err());
        assert!(intake.response().is_err());
        assert!(intake.metadata().is_none());
        assert!(intake.fidelity().is_none());
        assert!(output.finish_stream(&intake).is_err());
        assert!(!output.lifecycle().may_retry_or_fallback());
    }
}
#[test]
fn unified_request_projection_keeps_targets_trusted_and_debug_redacted() {
    let topology = catalog::default_topology().unwrap();
    let endpoint = topology
        .endpoint(&EndpointId::new("deepseek-chat").unwrap())
        .unwrap();
    let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
    let request = client.decode_request(json!({"model":"deepseek-flash","input":"hello","stream":true,"prompt_cache_key":"session"}).to_string().as_bytes()).unwrap();
    admit(
        catalog::default_topology()
            .unwrap()
            .model("deepseek-flash")
            .unwrap(),
        &request,
    )
    .unwrap();
    let secret = SecretMaterial::new("synthetic-only-secret").unwrap();
    let upstream = prepare(endpoint, &providers::deepseek(), &secret, &request).unwrap();
    let body: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(body["model"], "deepseek-flash");
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert!(body.get("prompt_cache_key").is_none());
    assert_eq!(upstream.origin, "https://api.deepseek.com");
    assert_eq!(upstream.path, "/chat/completions");
    assert!(!format!("{upstream:?}").contains("synthetic-only-secret"));
    assert!(!format!("{upstream:?}").contains("hello"));
    let mut no_stream = endpoint.clone();
    no_stream.execution.streaming = false;
    assert!(matches!(
        prepare(&no_stream, &providers::deepseek(), &secret, &request),
        Err(AttemptError::Delivery(_))
    ));
    assert!(prepare(endpoint, &providers::xiaomi(), &secret, &request).is_err());
    let mut no_temperature = catalog::default_topology()
        .unwrap()
        .model("deepseek-flash")
        .unwrap()
        .clone();
    no_temperature.contract.temperature = false;
    let request = client
        .decode_request(
            json!({"model":"deepseek-flash","input":"hello","temperature":0.5})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    assert!(admit(&no_temperature, &request).is_err());
    let request = client
        .decode_request(br#"{"model":"deepseek-flash","input":"hello","service_tier":"auto"}"#)
        .unwrap();
    assert!(
        admit(
            catalog::default_topology()
                .unwrap()
                .model("deepseek-flash")
                .unwrap(),
            &request
        )
        .is_err()
    );
    assert!(prepare(endpoint, &providers::deepseek(), &secret, &request).is_err());
}
#[test]
fn grok_public_binding_uses_the_responses_cache_carrier() {
    let topology = catalog::default_topology().unwrap();
    let secret = SecretMaterial::new("synthetic-oauth-token").unwrap();
    let binding = catalog::SUBSCRIPTION_BINDINGS
        .iter()
        .find(|binding| binding.profile == "grok")
        .unwrap();
    let provider = (binding.provider)();
    let endpoint = binding.endpoint();
    let request = standard(Profile::Responses)
        .decode_request(
            json!({"model":binding.model,"input":"hello","prompt_cache_key":"shared-workspace"})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    admit(&binding.public_model(), &request).unwrap();
    let plan = morphiecore::execution::ExecutionPlan::for_request(&topology, &request).unwrap();
    assert_eq!(plan.candidates.len(), 1);
    assert_eq!(plan.candidates[0].endpoint_id, endpoint.id);
    let prepared = prepare(&endpoint, &provider, &secret, &request).unwrap();
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(body["prompt_cache_key"], "shared-workspace");
    assert!(body.get("session_id").is_none());
    assert!(prepared.safe_headers.iter().all(|(name, _)| {
        ![
            "session-id",
            "thread-id",
            "x-grok-conv-id",
            "x-grok-session-id",
            "x-opencode-session",
            "x-client-request-id",
        ]
        .contains(&name.as_str())
    }));
    assert!(request.cache_session.is_none());
}

#[test]
fn opencode_go_projects_neutral_conversations_independently_of_cache_groups() {
    use morphiecore::semantic::context::ConversationContext;
    let topology = catalog::default_topology().unwrap();
    let endpoint = topology
        .endpoint(&EndpointId::new("opencode-go-chat").unwrap())
        .unwrap();
    let provider = providers::opencode_go();
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let secret = SecretMaterial::new("synthetic-go-key").unwrap();
    for (group, key, expected) in [
        (
            ConversationContext::conversation("conversation-one").unwrap(),
            None,
            "427d18da2021fadacd82a4b187acd8b4459e929ad27d1f77025fff66310a176e",
        ),
        (
            ConversationContext::conversation("conversation-one").unwrap(),
            Some("shared-workspace"),
            "427d18da2021fadacd82a4b187acd8b4459e929ad27d1f77025fff66310a176e",
        ),
        (
            ConversationContext::conversation("conversation-two").unwrap(),
            Some("shared-workspace"),
            "7577d45153ce9042751834fb6f264cc0fb919b44f4408bb37a6defc6d3256d9f",
        ),
        (
            ConversationContext::independent_request("conversation-one").unwrap(),
            None,
            "c5f1b4da1c5a286699d732449eb48d2f00c63c61f36cf2bd236e27de3d71ec25",
        ),
    ] {
        let mut body = json!({"model":"hy4-preview","messages":[{"role":"user","content":"keep"}],"max_completion_tokens":37});
        if let Some(key) = key {
            body["prompt_cache_key"] = json!(key);
        }
        let mut request = client.decode_request(body.to_string().as_bytes()).unwrap();
        request.conversation = Some(group);
        let original = request.clone();
        let prepared = prepare(endpoint, &provider, &secret, &request).unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["messages"], json!([{"role":"user","content":"keep"}]));
        assert_eq!(wire["max_tokens"], 37);
        assert!(wire.get("max_completion_tokens").is_none());
        assert!(wire.get("session_id").is_none());
        assert!(wire.get("prompt_cache_key").is_none());
        assert!(wire.get("provider").is_none());
        assert!(prepared.safe_headers.contains(&(
            "user-agent".into(),
            format!("MorphieCore/{}", env!("CARGO_PKG_VERSION"))
        )));
        let projected = prepared
            .safe_headers
            .iter()
            .find(|(name, _)| name == "x-opencode-session")
            .map(|(_, value)| value.as_str());
        assert_eq!(projected, Some(expected));
        assert_eq!(request, original);
        assert!(!format!("{prepared:?}").contains("synthetic-conversation"));
        let plan = morphiecore::execution::ExecutionPlan::for_request(&topology, &request).unwrap();
        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(plan.candidates[0].endpoint_id, endpoint.id);
        let mut streaming = request.clone();
        streaming.delivery.stream = Presence::Value(true);
        let streamed = prepare(endpoint, &provider, &secret, &streaming).unwrap();
        assert!(
            streamed
                .safe_headers
                .contains(&("x-opencode-session".into(), expected.into()))
        );
    }
    let missing = client
        .decode_request(br#"{"model":"hy4-preview","messages":[{"role":"user","content":"keep"}]}"#)
        .unwrap();
    assert!(prepare(endpoint, &provider, &secret, &missing).is_err());
    assert!(morphiecore::execution::ExecutionPlan::for_request(&topology, &missing).is_err());
    let unsupported = client.decode_request(br#"{"model":"hy4-preview","session_id":"conversation-\u2603","messages":[{"role":"user","content":"keep"}]}"#).unwrap();
    assert!(prepare(endpoint, &provider, &secret, &unsupported).is_err());
    assert!(morphiecore::execution::ExecutionPlan::for_request(&topology, &unsupported).is_err());
    assert!(
        client
            .decode_request(
                br#"{"model":"hy4-preview","session_id":"x\r\nInjected: yes","messages":[]}"#
            )
            .is_err()
    );
    let mut unsupported = client.decode_request(br#"{"model":"hy4-preview","messages":[{"role":"user","content":"keep"}],"response_format":{"type":"json_object"}}"#).unwrap();
    unsupported.conversation =
        Some(ConversationContext::conversation("synthetic-conversation").unwrap());
    assert!(admit(topology.model("hy4-preview").unwrap(), &unsupported).is_err());
    assert!(prepare(endpoint, &provider, &secret, &unsupported).is_err());
}

#[test]
fn router_request_policy_is_fixed_and_luna_controls_are_not_silently_ignored() {
    let topology = catalog::default_topology().unwrap();
    let secret = SecretMaterial::new("synthetic-router-secret").unwrap();
    for (profile, suffix, mut body) in [
        (
            Profile::Chat,
            "chat",
            json!({"model":"gpt-6-luna","messages":[
            {"role":"user","content":"hello"},
            {"role":"assistant","content":"pong","reasoning_content":"think"}],
            "max_completion_tokens":64,"reasoning_effort":"low","stream":true}),
        ),
        (
            Profile::Responses,
            "responses",
            json!({"model":"gpt-6-luna","input":"hello",
            "max_output_tokens":64,"reasoning":{"effort":"low"},"stream":true}),
        ),
    ] {
        let client = Adapter::new(profile, Dialect::MorphieCore, None);
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
        admit(
            catalog::default_topology()
                .unwrap()
                .model("gpt-6-luna")
                .unwrap(),
            &request,
        )
        .unwrap();
        let endpoint = topology
            .endpoint(&EndpointId::new(&format!("openrouter-{suffix}")).unwrap())
            .unwrap();
        let prepared = prepare(endpoint, &providers::openrouter(), &secret, &request).unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["model"], "openai/gpt-6-luna");
        assert_eq!(wire["provider"], json!({"require_parameters":true}));
        assert_eq!(prepared.origin, "https://openrouter.ai");
        assert_eq!(
            prepared.path,
            if profile == Profile::Chat {
                "/api/v1/chat/completions"
            } else {
                "/api/v1/responses"
            }
        );
        assert_eq!(
            prepared.auth_header,
            (
                "authorization".into(),
                "Bearer synthetic-router-secret".into()
            )
        );
        if profile == Profile::Chat {
            assert_eq!(wire["messages"][1]["reasoning"], "think");
            assert!(wire["messages"][1].get("reasoning_content").is_none());
            assert_eq!(wire["max_completion_tokens"], 64);
        } else {
            assert_eq!(wire["store"], false);
            assert_eq!(wire["max_output_tokens"], 64);
        }
        body["temperature"] = json!(0.5);
        let unsupported = client.decode_request(body.to_string().as_bytes()).unwrap();
        assert!(
            admit(
                catalog::default_topology()
                    .unwrap()
                    .model("gpt-6-luna")
                    .unwrap(),
                &unsupported
            )
            .is_err()
        );
        assert!(prepare(endpoint, &providers::openrouter(), &secret, &unsupported).is_err());
        body.as_object_mut().unwrap().remove("temperature");
        body["provider"] = json!({"require_parameters":false,"order":["untrusted"]});
        assert!(client.decode_request(body.to_string().as_bytes()).is_err());
    }
    let topology = catalog::topology(&std::collections::BTreeMap::from([(
        "gpt-6-luna".into(),
        vec!["openai".into(), "openrouter".into()],
    )]))
    .unwrap();
    let public = topology.model("gpt-6-luna").unwrap();
    let route = topology.route(&public.route).unwrap();
    assert_eq!(
        route.policy.fallback,
        morphiecore::topology::FallbackPolicy::BeforeCommit
    );
    let endpoint = topology.endpoint(&route.endpoints[0]).unwrap();
    assert_eq!(endpoint.provider.as_str(), "openai");
    assert_eq!(endpoint.credential.as_str(), "openai-oauth");
    let client = standard(Profile::Responses);
    let request = client
        .decode_request(br#"{"model":"gpt-6-luna","input":"hello"}"#)
        .unwrap();
    let prepared = prepare(endpoint, &providers::openai(), &secret, &request).unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["model"], "gpt-6-luna");
    assert_eq!(prepared.origin, "https://api.openai.com");
    assert_eq!(prepared.path, "/v1/responses");
    let plan = morphiecore::execution::ExecutionPlan::for_request(&topology, &request).unwrap();
    assert_eq!(plan.candidates[0].endpoint_id, endpoint.id);
    assert_eq!(plan.candidates.len(), 3);
    let capped = client
        .decode_request(br#"{"model":"gpt-6-luna","input":"hello","max_output_tokens":64}"#)
        .unwrap();
    let original = capped.clone();
    admit(public, &capped).unwrap();
    assert!(prepare(endpoint, &providers::openai(), &secret, &capped).is_err());
    let plan = morphiecore::execution::ExecutionPlan::for_request(&topology, &capped).unwrap();
    assert!(plan.candidates.iter().all(|c| c.endpoint_id != endpoint.id));
    assert!(plan.rejections.iter().any(|r| r.endpoint_id == endpoint.id));
    assert_eq!(plan.candidates.len(), 2);
    assert_eq!(capped, original);
}
