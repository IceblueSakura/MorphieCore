//! Independent synthetic Chat wire fixtures; no codec-generated oracle or Provider.
use serde_json::{Value, json};
pub fn response(turn: u8) -> Value {
    json!({"id":"chat-local","object":"chat.completion","created":1,"model":"fixture-model",
        "choices":[{"index":0,"message":if turn==1 {json!({"role":"assistant","content":null,"tool_calls":[{"id":"call-local","type":"function","function":{"name":"lookup","arguments":"{\"n\":1}"}}]})} else {json!({"role":"assistant","content":"old 🧪"})},"finish_reason":if turn==1 {"tool_calls"} else {"stop"}}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}})
}
pub fn events(turn: u8) -> Vec<Value> {
    let chunk = |delta: Value, finish: Value| json!({"id":"chat-local","object":"chat.completion.chunk","created":1,"model":"fixture-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":null});
    let mut values = vec![chunk(
        json!({"role":"assistant","content":null,"refusal":null,"tool_calls":null}),
        Value::Null,
    )];
    if turn == 1 {
        values.push(chunk(json!({"tool_calls":[{"index":0,"id":"call-local","type":"function","function":{"name":"lookup","arguments":"{"}}]}), Value::Null));
        values.push(chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":"\"n\":1}"}}]}),
            Value::Null,
        ));
    } else {
        values.push(chunk(json!({"content":"old "}), Value::Null));
        values.push(chunk(json!({"content":"🧪"}), Value::Null));
    }
    values.push(chunk(
        json!({}),
        json!(if turn == 1 { "tool_calls" } else { "stop" }),
    ));
    let mut usage = chunk(json!({}), Value::Null);
    usage["choices"] = json!([]);
    usage["usage"] = response(turn)["usage"].clone();
    values.push(usage);
    values
}
