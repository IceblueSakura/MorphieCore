//! DashScope Chat wire and ordered reasoning/tool-result history expectations.
use super::RESULT;
use serde_json::{Value, json};

pub fn snapshot(turn: usize) -> Value {
    json!({"id":format!("chat-{turn}"),"object":"chat.completion","created":1,
            "model":"qwen3.8-flash","choices":[{"index":0,
                "message":if turn == 1 { json!({"role":"assistant","content":null,
                    "reasoning_content":"Plan 🧪","tool_calls":[{"id":"call-lookup","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}}]}) }
                else {json!({"role":"assistant","reasoning_content":"Check","content":"cedar-71"})},
                "finish_reason":if turn == 1 {"tool_calls"}else{"stop"}}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,
                "completion_tokens_details":{"reasoning_tokens":2}}})
}
pub fn events(turn: usize) -> Vec<Value> {
    let full = snapshot(turn);
    let chunk = |delta: Value, finish: Value| {
        json!({"id":full["id"],"object":"chat.completion.chunk",
        "created":1,"model":full["model"],"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let mut events = vec![
        chunk(json!({"role":"assistant"}), Value::Null),
        chunk(
            json!({"reasoning_content":if turn==1{"Plan 🧪"}else{"Check"}}),
            Value::Null,
        ),
    ];
    if turn == 1 {
        events.push(chunk(
            json!({"tool_calls":[{"index":0,"id":"call-lookup","type":"function",
            "function":{"name":"lookup","arguments":"{ \"key\":"}}]}),
            Value::Null,
        ));
        events.push(chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":" \"A\" }"}}]}),
            Value::Null,
        ));
    } else {
        events.push(chunk(json!({"content":"cedar-71"}), Value::Null));
    }
    events.push(chunk(
        json!({}),
        json!(if turn == 1 { "tool_calls" } else { "stop" }),
    ));
    events.push(
        json!({"id":full["id"],"object":"chat.completion.chunk","created":1,
        "model":full["model"],"choices":[],"usage":full["usage"]}),
    );
    events
}

pub fn assert_request(body: &Value) -> usize {
    assert_eq!(body["model"], "qwen3.8-flash");
    assert_eq!(body["max_completion_tokens"], 256);
    assert_eq!(body["reasoning_effort"], "low");
    let tool = &body["tools"][0]["function"];
    assert_eq!(tool["name"], "lookup");
    assert!(tool.get("strict").is_none_or(|v| v == false));
    assert_eq!(
        tool["parameters"],
        json!({"type":"object",
        "properties":{"key":{"type":"string"}},"required":["key"]})
    );
    let history = body["messages"].as_array().unwrap();
    let turn = if history.iter().any(|v| {
        v["role"] == "user" && (v["content"] == "next" || v["content"][0]["text"] == "next")
    }) {
        3
    } else if history.iter().any(|v| v["role"] == "tool") {
        2
    } else {
        1
    };
    let roles: Vec<_> = history
        .iter()
        .map(|v| v["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        match turn {
            1 => vec!["user"],
            2 => vec!["user", "assistant", "tool"],
            3 => vec!["user", "assistant", "tool", "assistant", "user"],
            _ => unreachable!(),
        }
    );
    if turn >= 2 {
        let assistant = &history[1];
        assert_eq!(assistant["reasoning_content"], "Plan 🧪");
        assert_eq!(assistant["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(
            assistant["tool_calls"][0],
            json!({"id":"call-lookup","type":"function",
            "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}})
        );
        assert_eq!(history[2]["tool_call_id"], "call-lookup");
        assert_eq!(history[2]["content"], RESULT);
    }
    if turn == 3 {
        assert_eq!(history[3]["content"], "cedar-71");
        assert_eq!(history[3]["reasoning_content"], "Check");
    }
    turn
}
