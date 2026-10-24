//! DashScope Chat wire and interleaved calls with independent history expectations.
use super::RESULT;
use serde_json::{Value, json};

pub fn snapshot(turn: usize, reasoning: bool) -> Value {
    let mut body = json!({"id":format!("chat-{turn}"),"object":"chat.completion","created":1,
            "model":"qwen3.8-flash","choices":[{"index":0,
                "message":if turn == 1 { json!({"role":"assistant","content":null,
                    "reasoning_content":"Plan 🧪","tool_calls":[{"id":"call-lookup","type":"function",
                    "function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}},
                    {"id":"call-second","type":"function",
                    "function":{"name":"lookup","arguments":"{\"key\":\"B\"}"}}]}) }
                else {json!({"role":"assistant","reasoning_content":"Check","content":"cedar-71"})},
                "finish_reason":if turn == 1 {"tool_calls"}else{"stop"}}],
            "usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,
                "completion_tokens_details":{"reasoning_tokens":2}}});
    if !reasoning {
        body["choices"][0]["message"]
            .as_object_mut()
            .unwrap()
            .remove("reasoning_content");
    }
    body
}
pub fn events(turn: usize, reasoning: bool) -> Vec<Value> {
    let full = snapshot(turn, reasoning);
    let chunk = |delta: Value, finish: Value| {
        json!({"id":full["id"],"object":"chat.completion.chunk",
        "created":1,"model":full["model"],"choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    };
    let mut events = vec![
        chunk(json!({"role":"assistant"}), Value::Null),
        chunk(
            json!({"reasoning_content":if turn==1{"Plan "}else{"Ch"}}),
            Value::Null,
        ),
        chunk(
            json!({"reasoning_content":if turn==1{"🧪"}else{"eck"}}),
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
            json!({"tool_calls":[{"index":1,"id":"call-second","type":"function",
            "function":{"name":"lookup","arguments":"{\"key\":"}}]}),
            Value::Null,
        ));
        events.push(chunk(
            json!({"tool_calls":[{"index":1,"function":{"arguments":"\"B\"}"}}]}),
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
    if !reasoning {
        events.retain(|e| e["choices"][0]["delta"].get("reasoning_content").is_none());
    }
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
            2 => vec!["user", "assistant", "tool", "tool"],
            3 => vec!["user", "assistant", "tool", "tool", "assistant", "user"],
            _ => unreachable!(),
        }
    );
    if turn >= 2 {
        let assistant = &history[1];
        let reasoning = history[0]["content"] == "reasoning";
        let mut expected = vec![
            json!({"role":"user","content":if reasoning {"reasoning"} else {"lookup"}}),
            json!({"role":"assistant","content":null,"tool_calls":[
                {"id":"call-lookup","type":"function","function":{"name":"lookup","arguments":"{ \"key\": \"A\" }"}},
                {"id":"call-second","type":"function","function":{"name":"lookup","arguments":"{\"key\":\"B\"}"}}
            ]}),
            json!({"role":"tool","tool_call_id":"call-second","content":"second-result"}),
            json!({"role":"tool","tool_call_id":"call-lookup","content":RESULT}),
        ];
        // JSON null and the streaming client's empty accumulator are distinct
        // valid observations; neither may change the calls or surrounding history.
        assert!(assistant["content"].is_null() || assistant["content"] == "");
        expected[1]["content"] = assistant["content"].clone();
        if reasoning {
            expected[1]["reasoning_content"] = json!("Plan 🧪");
        }
        if turn == 3 {
            expected.extend([
                json!({"role":"assistant","content":"cedar-71"}),
                json!({"role":"user","content":"next"}),
            ]);
            if reasoning {
                expected[4]["reasoning_content"] = json!("Check");
            }
        }
        assert_eq!(history, &expected, "complete Chat continuation turn {turn}");
    }
    turn
}
