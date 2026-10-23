//! Independent v1 step, signature, argument-delta and full-history expectations.
use super::{ERROR, RESULT, Upstream};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, Uri},
    response::Response,
};
use morphiecore::protocol::openai::Profile;
use serde_json::{Value, json};

fn expected_input(turn: usize, messages: bool) -> Value {
    let mut input = vec![json!({"type":"user_input","content":[{"type":"text","text":"Read."}]})];
    if turn >= 2 {
        let mut result =
            json!({"type":"function_result","call_id":"c1","name":"read","result":ERROR});
        if messages {
            result["is_error"] = json!(true);
        }
        input.push(json!({"type":"thought","summary":[{"type":"text","text":"Plan 1"}],"signature":"synthetic-thought-1"}));
        input.extend([
            json!({"type":"function_call","id":"c1","name":"read","arguments":{"path":"missing"},"signature":"synthetic-call-1"}),
            result,
        ]);
    }
    if turn >= 3 {
        let mut result =
            json!({"type":"function_result","call_id":"c2","name":"read","result":RESULT});
        if messages {
            result["is_error"] = json!(false);
        }
        input.push(json!({"type":"thought","summary":[{"type":"text","text":"Plan 2"}],"signature":"synthetic-thought-2"}));
        input.extend([
            json!({"type":"function_call","id":"c2","name":"read","arguments":{"path":"found"},"signature":"synthetic-call-2"}),
            result,
        ]);
    }
    if turn >= 4 {
        input.extend([
            json!({"type":"thought","summary":[{"type":"text","text":"Plan 3"}],"signature":"synthetic-thought-3"}),
            json!({"type":"model_output","content":[{"type":"text","text":RESULT}]}),
            json!({"type":"user_input","content":[{"type":"text","text":"Continue."}]}),
        ]);
    }
    json!(input)
}

pub(super) async fn answer(
    State(state): State<Upstream>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(uri.path(), "/v1/interactions");
    assert!(uri.query().is_none());
    assert_eq!(headers["x-goog-api-key"], "synthetic-google-loop-upstream");
    assert!(headers.get("authorization").is_none());
    let turn = match body["input"].as_array().unwrap().len() {
        1 => 1,
        4 => 2,
        7 => 3,
        10 => 4,
        _ => panic!("unexpected Interactions history length: {body}"),
    };
    let mut config = json!({"max_output_tokens":256,"thinking_level":"low"});
    if state.profile == Profile::Responses {
        config["thinking_summaries"] = json!("auto");
    }
    assert_eq!(
        body,
        json!({"model":"gemini-3.8-flash","store":false,"stream":true,
        "input":expected_input(turn, state.messages), "generation_config":config,
        "tools":[{"type":"function","name":"read","parameters":{
            "type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}]})
    );
    let mut seen = state.seen.lock().unwrap();
    assert!(seen.len() < 8, "no retries or excess dispatch");
    assert_eq!(turn, seen.len() % 4 + 1);
    seen.push(body);
    drop(seen);
    let mut frames = vec![
        json!({"event_type":"interaction.created","interaction":{"model":"gemini-3.8-flash"}}),
        json!({"event_type":"interaction.status_update","status":"in_progress"}),
    ];
    let mut steps = vec![];
    if turn <= 3 {
        let summary = format!("Plan {turn}");
        let signature = format!("synthetic-thought-{turn}");
        frames.extend([
            json!({"event_type":"step.start","index":0,"step":{"type":"thought"}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":"Plan "}}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":turn.to_string()}}}),
            json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":signature}}),
            json!({"event_type":"step.stop","index":0}),
        ]);
        steps.push(json!({"type":"thought","summary":[{"type":"text","text":summary}],"signature":signature}));
    }
    if turn <= 2 {
        let id = format!("c{turn}");
        let path = if turn == 1 { "missing" } else { "found" };
        let tail = if turn == 1 {
            "\"missing\"}"
        } else {
            "\"found\"}"
        };
        frames.extend([
            json!({"event_type":"step.start","index":1,"step":{"type":"function_call","id":id,"name":"read","arguments":{},"signature":format!("synthetic-call-{turn}")}}),
            json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":"{\"path\":"}}),
            json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":tail}}),
            json!({"event_type":"step.stop","index":1}),
        ]);
        steps.push(json!({"type":"function_call","id":id,"name":"read","arguments":{"path":path},"signature":format!("synthetic-call-{turn}")}));
    } else {
        let index = usize::from(turn == 3);
        let text = if turn == 3 { RESULT } else { "continued" };
        frames.extend([
            json!({"event_type":"step.start","index":index,"step":{"type":"model_output"}}),
            json!({"event_type":"step.delta","index":index,"delta":{"type":"text","text":text}}),
            json!({"event_type":"step.stop","index":index}),
        ]);
        steps.push(json!({"type":"model_output","content":[{"type":"text","text":text}]}));
    }
    // Missing counters remain unknown in source IR; OpenAI delivery completes
    // a separate view. Messages also exercises exact native accounting.
    let usage = if state.messages {
        json!({"total_input_tokens":10,"total_cached_tokens":0,"total_output_tokens":3,
            "total_thought_tokens":2,"total_tool_use_tokens":0,"total_tokens":15})
    } else {
        json!({"total_input_tokens":10,"total_output_tokens":5,"total_tokens":15})
    };
    frames.push(json!({"event_type":"interaction.completed","interaction":{
        "status":if turn <= 2 {"requires_action"} else {"completed"}, "steps":steps,
        "usage":usage}}));
    // The last Messages turn proves mandatory downstream accounting completion,
    // rather than merely projecting an already complete upstream report.
    if state.messages && turn == 4 {
        frames.last_mut().unwrap()["interaction"]
            .as_object_mut()
            .unwrap()
            .remove("usage");
    }
    let mut wire = frames
        .iter()
        .map(|frame| {
            format!(
                "event: {}\ndata: {frame}\n\n",
                frame["event_type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    wire.push_str("event: done\ndata: [DONE]\n\n");
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(axum::body::Body::from_stream(futures_util::stream::iter(
            wire.as_bytes()
                .chunks(13)
                .map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk)))
                .collect::<Vec<_>>(),
        )))
        .unwrap()
}
