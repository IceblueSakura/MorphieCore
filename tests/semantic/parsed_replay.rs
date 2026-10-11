//! SDK derived `parsed` views are validated replay conveniences over the authoritative
//! raw text; they are never IR state and never emitted.
use morphiecore::{
    protocol::openai::{CodecError, DecodedRequest, chat_envelope, envelope},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

#[test]
fn sdk_float_views_round_only_the_view_not_the_raw_authority() {
    let raw = r#"{"n":0.123456789012345678901}"#;
    let parsed = json!({"n":0.12345678901234568});
    let decoded = decode_call(responses_call(raw, Some(parsed))).unwrap();
    assert_eq!(arguments(&decoded), raw);
}

fn chat_request(message: Value) -> Value {
    json!({"model":"fixture-model","messages":[message]})
}
fn decode_chat(message: Value) -> Result<DecodedRequest, CodecError> {
    Ok(
        chat_envelope::decode_request_bytes(&serde_json::to_vec(&chat_request(message)).unwrap())?
            .task,
    )
}
fn chat_tool_call(function: Value) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":[{"id":"call","type":"function","function":function}]})
}
fn fn_view(arguments: &str, parsed: Option<Value>) -> Value {
    let mut f = json!({"name":"lookup","arguments":arguments});
    if let Some(v) = parsed {
        f["parsed_arguments"] = v;
    }
    f
}
fn responses_call(arguments: &str, parsed: Option<Value>) -> Value {
    let mut o = json!({"type":"function_call","id":"f","call_id":"c","name":"lookup","arguments":arguments});
    if let Some(v) = parsed {
        o["parsed_arguments"] = v;
    }
    o
}
fn decode_call(item: Value) -> Result<DecodedRequest, CodecError> {
    Ok(envelope::decode_request_bytes(
        &serde_json::to_vec(&json!({"model":"fixture-model","input":[item]})).unwrap(),
    )?
    .task)
}
fn arguments(d: &DecodedRequest) -> String {
    d.semantic
        .items()
        .iter()
        .find_map(|(_, i)| match i {
            Item::ToolCall(c) => c.arguments.as_raw().map(str::to_owned),
            _ => None,
        })
        .unwrap()
}

#[test]
fn underivable_or_inconsistent_function_views_cannot_pass_admission() {
    for bad in [
        json!({"n":2}),
        json!({"n":"1"}),
        json!([1]),
        json!(true),
        json!("{}"),
    ] {
        assert!(decode_chat(chat_tool_call(fn_view("{\"n\":1}", Some(bad.clone())))).is_err());
        assert!(decode_call(responses_call("{\"n\":1}", Some(bad))).is_err());
    }
    // A non-null view of arguments that are not valid JSON is underivable...
    assert!(decode_chat(chat_tool_call(fn_view("{n:1}", Some(json!({"n":1}))))).is_err());
    assert!(decode_call(responses_call("{n:1}", Some(json!({"n":1})))).is_err());
    // ...while a null view of opaque arguments is still just "no derived value".
    assert!(decode_chat(chat_tool_call(fn_view("{n:1}", Some(Value::Null)))).is_ok());
    assert!(decode_call(responses_call("{n:1}", Some(Value::Null))).is_ok());
}
