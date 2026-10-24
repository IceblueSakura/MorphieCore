//! Test-only Chat handler over the same IR and bounded byte adapters as offline tests.
use super::{Suite, chat_wire as wire, failure};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::Response,
};
use morphiecore::{
    protocol::{
        openai::{
            Profile, TargetCaps as Contract,
            chat::{
                envelope,
                sse::{ChatSseDecoder, ChatSseEncoder},
            },
            lowering::{lower_request, lower_response},
            obfuscation::Obfuscation,
        },
        stream::SseLimits,
    },
    semantic::{
        task::generation::{
            ContentPart, CustomFormat, GrammarSyntax, Item, MAX_TEXT_BYTES, OutputConstraint,
            StreamEvent, ToolChoice, ToolDefinition, ToolKind, ToolOutput,
        },
        value::Text,
    },
};

fn usage() -> serde_json::Value {
    // Three text tokens plus room for the one image token added by the typed edit below.
    serde_json::json!({"prompt_tokens":4,"completion_tokens":2,"total_tokens":6,"prompt_tokens_details":{"text_tokens":3},"completion_tokens_details":{"text_tokens":2,"reasoning_tokens":1,"accepted_prediction_tokens":1,"rejected_prediction_tokens":0}})
}

pub(super) async fn handle(
    State(state): State<Suite>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok())
        != Some("Bearer synthetic-local-token")
        || headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|s| !s.starts_with("application/json"))
    {
        return failure(StatusCode::UNAUTHORIZED, "Chat authentication", &state);
    }
    let request = match envelope::decode_request_bytes(&body) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("synthetic Chat request rejected: {error}");
            return failure(StatusCode::BAD_REQUEST, "Chat request", &state);
        }
    };
    if request.context.model != "fixture-model" {
        return failure(StatusCode::BAD_REQUEST, "Chat model", &state);
    }
    let Ok(target) = lower_request(
        &request.task.semantic,
        &request.task.fidelity,
        Profile::Chat,
        Contract::full(),
    ) else {
        return failure(StatusCode::BAD_REQUEST, "Chat lowering", &state);
    };
    let Ok(encoded) = envelope::encode_request(&target, &request.context) else {
        return failure(StatusCode::BAD_REQUEST, "Chat projection", &state);
    };
    let turn = {
        let mut seen = state.0.lock().unwrap();
        seen.0 += 1;
        seen.0
    };
    if turn <= 3 {
        for (index, name) in ["policy", "author", "requester"].into_iter().enumerate() {
            assert_eq!(encoded["messages"][index]["name"], name);
            assert_eq!(
                request.task.semantic.items()[index]
                    .1
                    .participant()
                    .map(Text::as_str),
                Some(name)
            );
        }
        for (from, name) in [(2, "planner"), (3, "writer")] {
            if turn >= from {
                assert!(
                    encoded["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|m| m["name"] == name)
                );
            }
        }
    }
    if turn > 3 {
        let grammar = request.task.semantic.tools().iter().any(|tool| matches!(tool,
            ToolDefinition::Custom(custom) if custom.name.as_str()=="sql"
                && matches!(&custom.format,Some(CustomFormat::Grammar{syntax:GrammarSyntax::Regex,definition})
                    if definition.as_str()=="SELECT [0-9]+")));
        if turn > 5
            || request.context.streaming()
            || !grammar
            || turn == 4
                && !matches!(request.task.semantic.tool_choice(),Some(ToolChoice::Custom(name)) if name.as_str()=="sql")
            || turn == 5
                && !request.task.semantic.items().iter().any(|(_, item)| {
                    matches!(item,
                Item::CustomResult(result) if result.required_call_id().ok()==Some("call-custom")
                    && matches!(&result.output,ToolOutput::Parts(parts) if parts.len()==2
                        && parts[0].content.as_text()==Some("1") && parts[1].content.as_text()==Some("")))
                })
        {
            return failure(StatusCode::BAD_REQUEST, "Chat custom history", &state);
        }
        let response = serde_json::json!({
            "id":"chat-custom","object":"chat.completion","created":1,"model":"fixture-model",
            "choices":[{"index":0,"message":if turn==4 {
                serde_json::json!({"role":"assistant","content":null,"tool_calls":[
                    {"id":"call-custom","type":"custom","custom":{"name":"sql","input":"SELECT 1"}}]})
            } else {serde_json::json!({"role":"assistant","content":"custom done"})},
                "finish_reason":if turn==4 {"tool_calls"} else {"stop"}}],
            "usage":usage()
        });
        let decoded = envelope::decode_response_bytes(response.to_string().as_bytes()).unwrap();
        let target = lower_response(
            &decoded.semantic,
            &decoded.fidelity,
            &decoded.metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        let wire = envelope::encode_response(&target).unwrap();
        return Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(wire.to_string()))
            .unwrap();
    }
    if turn > 3
        || turn == 2
            && !request
                .task
                .semantic
                .items()
                .iter()
                .any(|(_, i)| matches!(i, Item::ToolResult(r) if r.required_call_id().ok()==Some("call-local") && matches!(&r.output,ToolOutput::Parts(parts) if parts.len()==2 && parts[0].content.as_text()==Some("synthetic result") && parts[1].content.as_text()==Some(""))))
    {
        return failure(StatusCode::BAD_REQUEST, "Chat continuation", &state);
    }
    if turn == 1
        && !matches!(request.task.semantic.tool_choice(),Some(ToolChoice::Allowed{required:true,tools}) if tools.len()==1 && tools[0].kind==ToolKind::Function && tools[0].name.as_str()=="lookup")
    {
        return failure(StatusCode::BAD_REQUEST, "Chat allowed functions", &state);
    }
    // The full envelope must have admitted the structured-output request as typed IR.
    if turn == 2
        && !matches!(
            request.task.semantic.output(),
            OutputConstraint::JsonSchema { name: Some(name), strict: Some(true), .. } if name.as_str() == "Answer"
        )
    {
        return failure(StatusCode::BAD_REQUEST, "Chat response_format", &state);
    }
    // The replayed parsed view passed consistency admission; the raw body stays authoritative.
    if turn == 3
        && !request.task.semantic.items().iter().any(|(_, i)| {
            matches!(i, Item::Message(m) if m.parts.iter().any(|p| matches!(
                &p.content,
                ContentPart::Text(t) if t.as_str() == "{\"answer\":\"new 🧪\"}"
            )))
        })
    {
        return failure(StatusCode::BAD_REQUEST, "Chat parsed replay", &state);
    }
    // Replayed function views never replace the authoritative raw arguments.
    if turn >= 2
        && !request
            .task
            .semantic
            .items()
            .iter()
            .any(|(_, i)| matches!(i, Item::ToolCall(c) if c.arguments.as_raw()==Some("{\"n\":1}")))
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "Chat parsed_arguments replay",
            &state,
        );
    }
    if !request.context.streaming() {
        let mut body = wire::response(turn as u8);
        body["usage"] = usage();
        let mut d = envelope::decode_response_bytes(&serde_json::to_vec(&body).unwrap()).unwrap();
        // Standard Chat preserves its official reported image-token detail.
        let source_usage = morphiecore::semantic::task::generation::Usage {
            input_image_tokens: Some(1),
            ..d.semantic.usage().unwrap()
        };
        d.semantic = d.semantic.with_usage(Some(source_usage)).unwrap();
        if turn == 2 {
            let mut items = d.semantic.items().to_vec();
            let Item::Message(m) = &mut items[0].1 else {
                panic!()
            };
            let ContentPart::Text(text) = &m.parts[0].content else {
                panic!()
            };
            m.parts[0].content = ContentPart::Text(
                text.clone().replace_text(
                    Text::allowing_empty("{\"answer\":\"new 🧪\"}", "synthetic", MAX_TEXT_BYTES)
                        .unwrap(),
                ),
            );
            d.semantic = d.semantic.with_items(items).unwrap();
        }
        let projected = lower_response(
            &d.semantic,
            &d.fidelity,
            &d.metadata,
            Profile::Chat,
            Contract::full(),
        )
        .unwrap();
        assert!(
            projected
                .projection()
                .iter()
                .all(|stage| !stage.change().is_loss())
        );
        let out = envelope::encode_response(&projected).unwrap();
        assert_eq!(d.semantic.usage(), Some(source_usage));
        assert_eq!(out["usage"]["prompt_tokens_details"]["image_tokens"], 1);
        return Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(out.to_string()))
            .unwrap();
    }
    let mut decoder = ChatSseDecoder::new(200, "text/event-stream", SseLimits::default()).unwrap();
    let mut bytes = vec![];
    for mut value in wire::events(turn as u8) {
        if value["usage"].is_object() {
            value["usage"] = usage();
        }
        bytes.extend(format!("data: {value}\n\n").bytes());
    }
    bytes.extend(b"data: [DONE]\n\n");
    let mut events = vec![];
    let mut rest = bytes.as_slice();
    while !rest.is_empty() {
        let (n, next) = decoder.consume(rest).unwrap();
        assert!(n > 0);
        rest = &rest[n..];
        events.extend(next);
    }
    decoder.finish().unwrap();
    let d = decoder.materialize().unwrap();
    let options = request
        .context
        .stream_options
        .value()
        .cloned()
        .unwrap_or_default();
    // This fixture opts out of padding; no production entropy source is exercised.
    let mut encoder = ChatSseEncoder::new(
        d.metadata,
        Contract::full(),
        SseLimits::default(),
        options,
        Obfuscation::Disabled,
    )
    .unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::convert::Infallible>>(1);
    tokio::spawn(async move {
        for mut event in events {
            if let StreamEvent::Usage(usage) = &mut event {
                usage.input_image_tokens = Some(1);
            }
            if turn == 2 {
                match &mut event {
                    // The synthesized structured text is rendered from final IR, not source JSON.
                    StreamEvent::Delta { fragment, .. } if fragment == "old " => {
                        *fragment = "{\"answer\":\"new ".into();
                    }
                    StreamEvent::Delta { fragment, .. } if fragment == "🧪" => {
                        *fragment = "🧪\"}".into();
                    }
                    _ => {}
                }
            }
            let Ok(frames) = encoder.encode(&event, &d.fidelity) else {
                return;
            };
            for frame in frames {
                if tx.send(Ok(frame)).await.is_err() {
                    return;
                }
            }
        }
        encoder.finish().unwrap();
    });
    Response::builder()
        .header(CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(futures_util::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|v| (v, rx)) },
        )))
        .unwrap()
}
