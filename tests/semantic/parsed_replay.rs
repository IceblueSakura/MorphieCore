//! SDK derived `parsed` views are validated replay conveniences over the authoritative
//! raw text; they are never IR state and never emitted.
use morphiecore::{
    protocol::openai::{CodecError, DecodedRequest, chat_envelope, envelope},
    semantic::task::generation::*,
};
use serde_json::{Value, json};

const RAW: &str = "{\"ok\":true}";

#[test]
fn sdk_float_views_round_only_the_view_not_the_raw_authority() {
    let raw = r#"{"n":0.123456789012345678901}"#;
    let parsed = json!({"n":0.12345678901234568});
    let decoded = decode_call(responses_call(raw, Some(parsed))).unwrap();
    assert_eq!(arguments(&decoded), raw);
}

fn output_part(parsed: Option<Value>) -> Value {
    let mut p = json!({"type":"output_text","text":RAW,"annotations":[]});
    if let Some(v) = parsed {
        p["parsed"] = v;
    }
    p
}
fn responses_input(part: Value) -> Value {
    json!({"model":"fixture-model","input":[{"type":"message","id":"m","role":"assistant","status":"completed","content":[part]}]})
}
fn decode_responses(part: Value) -> Result<DecodedRequest, CodecError> {
    Ok(envelope::decode_request_bytes(&serde_json::to_vec(&responses_input(part)).unwrap())?.task)
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
fn inconsistent_or_underivable_parsed_cannot_pass_admission() {
    // A pydantic-shaped value that is not the raw text's parse cannot prove its derivation.
    for parsed in [
        json!({"ok":true,"extra":1}),
        json!({"ok":"true"}),
        json!([true]),
        json!(true),
        json!("{}"),
    ] {
        assert!(decode_responses(output_part(Some(parsed.clone()))).is_err());
        assert!(decode_chat(json!({"role":"assistant","content":RAW,"parsed":parsed})).is_err());
    }
    // Non-null views of a body that is not valid JSON are underivable.
    let mut part = output_part(Some(json!({"ok":1})));
    part["text"] = json!("hello");
    assert!(decode_responses(part).is_err());
    // A null view of unparseable text is still just "no derived value".
    let mut part = output_part(Some(Value::Null));
    part["text"] = json!("hello");
    assert!(decode_responses(part).is_ok());
    assert!(decode_chat(json!({"role":"assistant","content":"hello","parsed":{"ok":1}})).is_err());
    assert!(
        decode_chat(json!({"role":"assistant","content":"hello","parsed":Value::Null})).is_ok()
    );
    // A missing or null Chat body never derives a non-null view.
    for message in [
        json!({"role":"assistant","content":null,"parsed":{"ok":true}}),
        json!({"role":"assistant","parsed":{"ok":true}}),
        json!({"role":"assistant","content":"","parsed":{"ok":true}}),
    ] {
        assert!(decode_chat(message).is_err());
    }
    // Only assistant messages carry the SDK view; other roles reject the key outright.
    for message in [
        json!({"role":"user","content":RAW,"parsed":{"ok":true}}),
        json!({"role":"user","content":RAW,"parsed":Value::Null}),
        json!({"role":"system","content":"rules","parsed":Value::Null}),
        json!({"role":"developer","content":"rules","parsed":Value::Null}),
        json!({"role":"tool","tool_call_id":"call","content":"out","parsed":Value::Null}),
    ] {
        assert!(decode_chat(message).is_err());
    }
    // Input-text parts are user inputs and never carry a derived view.
    assert!(
        decode_responses(json!({"type":"input_text","text":RAW,"parsed":{"ok":true}})).is_err()
    );
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
