//! Independent native Messages snapshots and SSE, shared by synthetic HTTP oracles.
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub fn native_stream_body(value: &Value) -> String {
    let mut initial = value.clone();
    initial["content"] = json!([]);
    initial["stop_reason"] = Value::Null;
    if initial.get("stop_details").is_some() {
        initial["stop_details"] = Value::Null;
    }
    initial["usage"]["output_tokens"] = json!(0);
    let mut events = vec![json!({"type":"message_start","message":initial})];
    for (index, block) in value["content"].as_array().unwrap().iter().enumerate() {
        match block["type"].as_str().unwrap() {
            "text" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":block["text"]}}));
            }
            "thinking" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":"","signature":""}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":block["thinking"]}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":block["signature"]}}));
            }
            "redacted_thinking" => {
                events.push(
                    json!({"type":"content_block_start","index":index,"content_block":block}),
                );
            }
            "tool_use" => {
                events.push(json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":block["id"],"name":block["name"],"input":{}}}));
                // A legal JSON prefix split before its first token must not break
                // downstream SDK incremental argument accumulation.
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":" \n\t"}}));
                events.push(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":block["input"].to_string()}}));
            }
            _ => panic!("fixture block"),
        }
        events.push(json!({"type":"content_block_stop","index":index}));
    }
    events.push(json!({"type":"message_delta","delta":{"stop_reason":value["stop_reason"]},"usage":{"output_tokens":value["usage"]["output_tokens"]}}));
    if let Some(details) = value.get("stop_details") {
        events.last_mut().unwrap()["delta"]["stop_details"] = details.clone();
    }
    events.push(json!({"type":"message_stop"}));
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect()
}

pub fn native_response(value: Value, stream: bool) -> Response {
    if !stream {
        return Json(value).into_response();
    }
    (
        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
        native_stream_body(&value),
    )
        .into_response()
}
