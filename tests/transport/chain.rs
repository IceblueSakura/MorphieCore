//! Adapter/execution acceptance with independent wire and explicit I/O commit.
use crate::{chat_wire, wire};
use morphiecore::{
    adapter::{Adapter, Dialect},
    execution::{Attempt, AttemptError, ResponseDelivery, admit, prepare},
    lowering::generation::{GenerationRepresentationContract as Contract, ReportedFactPolicy},
    protocol::openai::{
        Profile,
        chat_sse::ChatSseDecoder,
        sse::{Obfuscation, ResponsesSseDecoder, SseLimits},
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
        adapter,
        contract,
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
                morphiecore::protocol::openai::sse::encode_frame(
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
    }
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
            morphiecore::protocol::openai::sse::encode_frame(
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
}
#[test]
fn static_delivery_scope_is_owned_by_the_target_and_commit_is_explicit() {
    let scope = ReplayOrigin::new("scope-a").unwrap();
    let source = Adapter::new(Profile::Responses, Dialect::Standard, Some(scope.clone()));
    let mut body = wire::response(2);
    body["output"].as_array_mut().unwrap().insert(0, json!({"id":"rs1","type":"reasoning","status":"completed","summary":[],"encrypted_content":"synthetic-token"}));
    let mut intake = Attempt::new(source.clone(), 1 << 20, SseLimits::default());
    intake.begin(200, "application/json").unwrap();
    intake.push(body.to_string().as_bytes()).unwrap();
    intake.finish().unwrap();
    let mut denied = delivery(standard(Profile::Responses), Contract::full());
    assert!(denied.encode_json(&intake).is_err());
    assert!(denied.lifecycle().may_retry_or_fallback());
    let mut permitted = delivery(
        source,
        Contract {
            replay_origin: Some(scope),
            ..Contract::full()
        },
    );
    let encoded: Value = serde_json::from_slice(&permitted.encode_json(&intake).unwrap()).unwrap();
    assert_eq!(encoded["model"], "public-model");
    assert_eq!(encoded["output"][0]["encrypted_content"], "synthetic-token");
    assert!(permitted.lifecycle().may_retry_or_fallback());
    permitted.commit().unwrap();
    permitted.complete(&intake).unwrap();
    assert!(permitted.encode_json(&intake).is_err());
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
    let mut intake = Attempt::new(standard(Profile::Chat), 4096, SseLimits::default());
    let mut output = delivery(standard(Profile::Chat), Contract::full());
    intake.begin(200, "text/event-stream").unwrap();
    let (_, events) = intake.push(&bytes(Profile::Chat)).unwrap();
    output.encode_events(&intake, &events).unwrap();
    output.commit().unwrap();
    intake.cancel();
    output.cancel();
    assert!(intake.finish().is_err());
    assert!(output.finish_stream(&intake).is_err());
    assert!(!output.lifecycle().may_retry_or_fallback());
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
    for extra in [
        json!({"service_tier":"auto"}),
        json!({"client_metadata":{"trace":"opaque"}}),
    ] {
        let mut body = json!({"model":"deepseek-flash","input":"hello"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
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
}
#[test]
fn opencode_go_projects_only_fixed_identity_and_explicit_session_headers() {
    let topology = catalog::default_topology().unwrap();
    let endpoint = topology
        .endpoint(&EndpointId::new("opencode-go-chat").unwrap())
        .unwrap();
    let provider = providers::opencode_go();
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let secret = SecretMaterial::new("synthetic-go-key").unwrap();
    for session in [None, Some("synthetic-conversation")] {
        let mut body = json!({"model":"hy4-preview","messages":[{"role":"user","content":"keep"}],"max_completion_tokens":37});
        if let Some(session) = session {
            body["session_id"] = json!(session);
        }
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
        let original = request.clone();
        let prepared = prepare(endpoint, &provider, &secret, &request).unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire["messages"], json!([{"role":"user","content":"keep"}]));
        assert_eq!(wire["max_tokens"], 37);
        assert!(wire.get("max_completion_tokens").is_none());
        assert!(wire.get("session_id").is_none());
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
        assert_eq!(projected, session);
        assert_eq!(request, original);
        assert!(!format!("{prepared:?}").contains("synthetic-conversation"));
    }
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
    let unsupported = client.decode_request(br#"{"model":"hy4-preview","messages":[{"role":"user","content":"keep"}],"response_format":{"type":"json_object"}}"#).unwrap();
    assert!(admit(topology.model("hy4-preview").unwrap(), &unsupported).is_err());
    assert!(prepare(endpoint, &provider, &secret, &unsupported).is_err());
}

#[test]
fn openrouter_luna_pdf_is_bounded_responses_with_strict_replay() {
    let topology = catalog::default_topology().unwrap();
    let endpoint = topology
        .endpoint(&EndpointId::new("openrouter-responses").unwrap())
        .unwrap();
    let client = Adapter::new(
        Profile::Responses,
        Dialect::MorphieCore,
        endpoint.representation.adaptation.scope.clone(),
    );
    let body = json!({"model":"gpt-6-luna","input":[{"role":"user","content":[
        {"type":"input_text","text":"Read."},
        {"type":"input_file","filename":"synthetic.pdf","file_data":"data:application/pdf;base64,JVBERi0xLjQK"}
    ]}]});
    let request = client.decode_request(body.to_string().as_bytes()).unwrap();
    assert!(admit(topology.model("gpt-6-luna").unwrap(), &request).is_ok());
    let secret = SecretMaterial::new("synthetic").unwrap();
    let prepared = prepare(endpoint, &providers::openrouter(), &secret, &request).unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(
        wire["input"],
        json!([{"type":"message","role":"user","content":[
            {"type":"input_text","text":"Read."},
            {"type":"input_file","filename":"synthetic.pdf","file_data":"data:application/pdf;base64,JVBERi0xLjQK"}
        ]}])
    );
    assert_eq!(wire["model"], "openai/gpt-6-luna");
    assert_eq!(prepared.path, "/api/v1/responses");
    assert!(wire.get("plugins").is_none());
    let mut url_body = body.clone();
    url_body["input"][0]["content"][1] = json!({"type":"input_file","file_url":"https://example.invalid/opaque?sig=synthetic-private"});
    let url_request = client
        .decode_request(url_body.to_string().as_bytes())
        .unwrap();
    let prepared_url = prepare(endpoint, &providers::openrouter(), &secret, &url_request).unwrap();
    let url_wire: Value = serde_json::from_slice(&prepared_url.body).unwrap();
    assert_eq!(
        url_wire["input"][0]["content"][1],
        url_body["input"][0]["content"][1]
    );
    assert_eq!(prepared_url.origin, "https://openrouter.ai");
    assert!(!format!("{prepared_url:?}").contains("synthetic-private"));
    let mut disabled = endpoint.clone();
    disabled.representation.files.urls = false;
    assert!(prepare(&disabled, &providers::openrouter(), &secret, &url_request).is_err());
    assert_eq!(
        endpoint.representation.files.max_total_inline_bytes,
        128 * 1024
    );
    let mut invalid = body.clone();
    invalid["input"][0]["content"][1]["file_data"] = json!("data:text/plain;base64,QQ==");
    let invalid = client
        .decode_request(invalid.to_string().as_bytes())
        .unwrap();
    assert!(prepare(endpoint, &providers::openrouter(), &secret, &invalid).is_err());
    let chat = topology
        .endpoint(&EndpointId::new("openrouter-chat").unwrap())
        .unwrap();
    assert!(prepare(chat, &providers::openrouter(), &secret, &request).is_err());
}

#[test]
fn router_pdf_budgets_and_history_do_not_expand_other_models() {
    let topology = catalog::default_topology().unwrap();
    let model = topology.model("gpt-6-luna").unwrap();
    assert!(model.contract.file_input);
    assert!(topology.model("gpt-6-luna-go").is_none());
    assert!(
        topology
            .endpoint(&EndpointId::new("opencode-go-luna-responses").unwrap())
            .is_none()
    );
    for other in topology.models().filter(|m| m.id != model.id) {
        assert!(!other.contract.file_input);
    }
    let endpoint = topology
        .endpoint(&EndpointId::new("openrouter-responses").unwrap())
        .unwrap();
    let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
    let body = json!({"model":"gpt-6-luna","max_output_tokens":512,"input":[{
        "role":"user","content":[{"type":"input_text","text":"Read the build marker."},
        {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"synthetic.pdf"}]
    }]});
    let request = client.decode_request(body.to_string().as_bytes()).unwrap();
    let original = request.clone();
    admit(model, &request).unwrap();
    let prepared = prepare(
        endpoint,
        &providers::openrouter(),
        &SecretMaterial::new("synthetic-router-key").unwrap(),
        &request,
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(
        wire["input"],
        json!([{"type":"message","role":"user","content":body["input"][0]["content"]}])
    );
    assert_eq!(wire["model"], "openai/gpt-6-luna");
    assert_eq!(wire["max_output_tokens"], 512);
    assert_eq!(request, original);
    for location in [
        json!({"type":"input_file","file_data":"data:text/plain;base64,AQID","filename":"synthetic.txt"}),
        json!({"type":"input_file","file_url":"file:///tmp/private.pdf"}),
        json!({"type":"input_file","file_id":"file-synthetic"}),
    ] {
        let mut changed = body.clone();
        changed["input"][0]["content"][1] = location;
        if let Ok(request) = client.decode_request(changed.to_string().as_bytes()) {
            assert!(
                prepare(
                    endpoint,
                    &providers::openrouter(),
                    &SecretMaterial::new("synthetic-router-key").unwrap(),
                    &request
                )
                .is_err()
            );
        }
    }
    use base64::{Engine, engine::general_purpose::STANDARD};
    for (bytes, count) in [(129 * 1024, 1), (70 * 1024, 2)] {
        let part = json!({"type":"input_file","filename":"synthetic.pdf",
            "file_data":format!("data:application/pdf;base64,{}", STANDARD.encode(vec![0; bytes]))});
        let mut oversized = body.clone();
        oversized["input"][0]["content"] = json!(vec![part; count]);
        let request = client
            .decode_request(oversized.to_string().as_bytes())
            .unwrap();
        assert!(
            prepare(
                endpoint,
                &providers::openrouter(),
                &SecretMaterial::new("synthetic-router-key").unwrap(),
                &request
            )
            .is_err()
        );
    }
    let mut history = body.clone();
    history["input"].as_array_mut().unwrap().extend([
        json!({"type":"reasoning","id":"synthetic-reasoning","summary":[],"status":"completed"}),
        json!({"type":"message","role":"assistant","id":"synthetic-reply","status":"completed","content":[{"type":"output_text","text":"synthetic-marker","annotations":[],"logprobs":[]}]}),
        json!({"role":"user","content":"Read the next marker."}),
    ]);
    let request = client
        .decode_request(history.to_string().as_bytes())
        .unwrap();
    admit(model, &request).unwrap();
    let prepared = prepare(
        endpoint,
        &providers::openrouter(),
        &SecretMaterial::new("synthetic-router-key").unwrap(),
        &request,
    )
    .unwrap();
    let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
    assert_eq!(wire["input"][2]["content"][0]["logprobs"], json!([]));
    let mut too_many = body.clone();
    too_many["input"][0]["content"] = json!(vec![body["input"][0]["content"][1].clone(); 5]);
    let request = client
        .decode_request(too_many.to_string().as_bytes())
        .unwrap();
    assert!(
        prepare(
            endpoint,
            &providers::openrouter(),
            &SecretMaterial::new("synthetic-router-key").unwrap(),
            &request
        )
        .is_err()
    );
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
}

#[test]
fn opaque_replay_reaches_prepared_body_only_for_its_bound_origin() {
    let topology = catalog::default_topology().unwrap();
    let secret = SecretMaterial::new("synthetic-replay-secret").unwrap();
    for (profile, suffix, body, pointer) in [
        (
            Profile::Chat,
            "chat",
            json!({"model":"gpt-6-luna","messages":[{"role":"assistant","content":"answer","reasoning_details":[{"type":"reasoning.encrypted","format":"openai-responses-v1","index":0,"id":"rs","data":"synthetic-cipher"}]},{"role":"user","content":"next"}]}),
            "/messages/0/reasoning_details/0/data",
        ),
        (
            Profile::Responses,
            "responses",
            json!({"model":"gpt-6-luna","input":[{"type":"reasoning","id":"rs","summary":[],"encrypted_content":"synthetic-cipher"},{"role":"user","content":"next"}]}),
            "/input/0/encrypted_content",
        ),
    ] {
        let endpoint = topology
            .endpoint(&EndpointId::new(&format!("openrouter-{suffix}")).unwrap())
            .unwrap();
        let client = Adapter::new(
            profile,
            Dialect::MorphieCore,
            endpoint.representation.adaptation.scope.clone(),
        );
        let request = client.decode_request(body.to_string().as_bytes()).unwrap();
        let prepared = prepare(endpoint, &providers::openrouter(), &secret, &request).unwrap();
        let wire: Value = serde_json::from_slice(&prepared.body).unwrap();
        assert_eq!(wire.pointer(pointer), Some(&json!("synthetic-cipher")));
        let mut wrong = endpoint.clone();
        wrong.representation.replay_origin = Some(
            morphiecore::semantic::value::ReplayOrigin::new("other-credential-owner").unwrap(),
        );
        assert!(prepare(&wrong, &providers::openrouter(), &secret, &request).is_err());
    }
}

#[tokio::test]
async fn synthetic_http_chain_uses_prepared_request_and_adapter_response() {
    use axum::{Router, routing::post};
    use std::time::Duration;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
    let app = Router::new().route(
        "/chat/completions",
        post(
            move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                let sender = sender.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer synthetic-http-only");
                    sender.lock().unwrap().take().unwrap().send(body).unwrap();
                    axum::Json(chat_wire::response(2))
                }
            },
        ),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    struct Guard(tokio::task::JoinHandle<()>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _guard = Guard(server);
    let mut provider = providers::deepseek();
    provider.origin = morphiecore::provider::TrustedOrigin::parse(&origin).unwrap();
    let topology = catalog::default_topology().unwrap();
    let mut endpoint = topology
        .endpoint(&EndpointId::new("deepseek-chat").unwrap())
        .unwrap()
        .clone();
    endpoint.target.origin = provider.origin.clone();
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    let request = client
        .decode_request(
            json!({"model":"deepseek-flash","messages":[{"role":"user","content":"hello"}]})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    let upstream = prepare(
        &endpoint,
        &provider,
        &SecretMaterial::new("synthetic-http-only").unwrap(),
        &request,
    )
    .unwrap();
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let response = http
        .post(format!("{}{}", upstream.origin, upstream.path))
        .header(&upstream.auth_header.0, &upstream.auth_header.1)
        .header("content-type", "application/json")
        .body(upstream.body)
        .send()
        .await
        .unwrap();
    let body = response.bytes().await.unwrap();
    let mut intake = Attempt::new(endpoint.adapter(), 4096, SseLimits::default());
    intake.begin(200, "application/json").unwrap();
    intake.push(&body).unwrap();
    intake.finish().unwrap();
    let mut output = delivery(client, Contract::full());
    let wire: Value = serde_json::from_slice(&output.encode_json(&intake).unwrap()).unwrap();
    assert_eq!(wire["model"], "public-model");
    assert_eq!(
        wire["usage"]["prompt_tokens_details"]["cache_write_tokens"],
        0
    );
    output.commit().unwrap();
    output.complete(&intake).unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(3), receiver)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed["model"], "deepseek-flash");
}
