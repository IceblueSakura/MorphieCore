//! SDK derived `parsed` views are validated replay conveniences over the authoritative
//! raw text; they are never IR state and never emitted.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, lower_request},
    protocol::openai::{
        CodecError, DecodedRequest, Profile, chat, chat_envelope, envelope, responses,
    },
    semantic::task::generation::*,
};
use serde_json::{Value, json};

const RAW: &str = "{\"ok\":true}";

#[test]
fn parsed_views_preserve_reserved_object_keys_and_naive_duplicate_semantics() {
    for (raw, parsed) in [
        (
            r#"{"$serde_json::private::Number":"1"}"#,
            json!({"$serde_json::private::Number":"1"}),
        ),
        (r#"{"a":1,"a":2}"#, json!({"a":2})),
    ] {
        let d = decode_call(responses_call(raw, Some(parsed.clone()))).unwrap();
        assert_eq!(arguments(&d), raw);
        let d = decode_chat(chat_tool_call(fn_view(raw, Some(parsed)))).unwrap();
        assert_eq!(arguments(&d), raw);
    }
}

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
fn message_text(d: &DecodedRequest) -> String {
    let Item::Message(m) = &d.semantic.items()[0].1 else {
        panic!()
    };
    let ContentPart::Text(t) = &m.parts[0].content else {
        panic!()
    };
    t.as_str().to_owned()
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
fn wire_pair(d: &DecodedRequest) -> (Value, Value) {
    (
        responses::encode_generation(
            &lower_request(
                &d.semantic,
                &d.fidelity,
                Profile::Responses,
                Contract::full(),
            )
            .unwrap(),
        )
        .unwrap(),
        chat::encode_generation(
            &lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).unwrap(),
        )
        .unwrap(),
    )
}

#[test]
fn history_replay_admits_a_consistent_parsed_and_keeps_the_raw_text_authoritative() {
    let expected = json!({"ok":true});
    // Missing, null and a consistent value are one admitted history shape with the same IR.
    let admitted = [
        decode_responses(output_part(None)).unwrap(),
        decode_responses(output_part(Some(Value::Null))).unwrap(),
        decode_responses(output_part(Some(expected.clone()))).unwrap(),
    ];
    assert_eq!(admitted[0].semantic, admitted[1].semantic);
    assert_eq!(admitted[1].semantic, admitted[2].semantic);
    for d in &admitted {
        assert_eq!(message_text(d), RAW);
        let (r, c) = wire_pair(d);
        assert_eq!(r["input"][0]["content"][0]["text"], json!(RAW));
        assert!(r["input"][0]["content"][0].get("parsed").is_none());
        assert_eq!(c["messages"][0]["content"], json!(RAW));
        assert!(c["messages"][0].get("parsed").is_none());
    }
    // The Chat message shell has the same rule on its own wire shape.
    let admitted = [
        decode_chat(json!({"role":"assistant","content":RAW})).unwrap(),
        decode_chat(json!({"role":"assistant","content":RAW,"parsed":Value::Null})).unwrap(),
        decode_chat(json!({"role":"assistant","content":RAW,"parsed":expected})).unwrap(),
    ];
    assert_eq!(admitted[0].semantic, admitted[1].semantic);
    assert_eq!(admitted[1].semantic, admitted[2].semantic);
    for d in &admitted {
        assert_eq!(message_text(d), RAW);
        let (r, c) = wire_pair(d);
        assert_eq!(c["messages"][0]["content"], json!(RAW));
        assert!(c["messages"][0].get("parsed").is_none());
        assert_eq!(r["input"][0]["content"][0]["text"], json!(RAW));
        assert!(!r.to_string().contains("parsed"));
    }
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
fn replaced_text_cannot_resurrect_a_parsed_value() {
    let d = decode_responses(output_part(Some(json!({"ok":true})))).unwrap();
    let items: Vec<_> = d
        .semantic
        .items()
        .iter()
        .map(|(id, item)| {
            let Item::Message(m) = item else { panic!() };
            let mut m = m.clone();
            let ContentPart::Text(t) = m.parts[0].content.clone() else {
                panic!()
            };
            m.parts[0].content = ContentPart::Text(
                t.replace_text(
                    morphiecore::semantic::value::Text::allowing_empty(
                        "{\"ok\":false}",
                        "synthetic",
                        MAX_TEXT_BYTES,
                    )
                    .unwrap(),
                ),
            );
            (*id, Item::Message(m))
        })
        .collect();
    let semantic = d.semantic.clone().with_items(items).unwrap();
    for value in [
        responses::encode_generation(
            &lower_request(&semantic, &d.fidelity, Profile::Responses, Contract::full()).unwrap(),
        )
        .unwrap(),
        chat::encode_generation(
            &lower_request(&semantic, &d.fidelity, Profile::Chat, Contract::full()).unwrap(),
        )
        .unwrap(),
    ] {
        assert!(!value.to_string().contains("parsed"));
        assert!(!value.to_string().contains("{\"ok\":true}"));
    }
}

#[test]
fn function_parsed_arguments_follow_the_same_replay_rule_in_both_shells() {
    let expected = json!({"n":1});
    // Missing, null and a consistent view are one admitted history shape with the same IR.
    let admitted = [
        decode_chat(chat_tool_call(fn_view("{\"n\":1}", None))).unwrap(),
        decode_chat(chat_tool_call(fn_view("{\"n\":1}", Some(Value::Null)))).unwrap(),
        decode_chat(chat_tool_call(fn_view("{\"n\":1}", Some(expected.clone())))).unwrap(),
    ];
    assert_eq!(admitted[0].semantic, admitted[1].semantic);
    assert_eq!(admitted[1].semantic, admitted[2].semantic);
    for d in &admitted {
        assert_eq!(arguments(d), "{\"n\":1}");
        let target =
            lower_request(&d.semantic, &d.fidelity, Profile::Chat, Contract::full()).unwrap();
        let c = chat::encode_generation(&target).unwrap();
        assert!(!c.to_string().contains("parsed_arguments"));
        let target = lower_request(
            &d.semantic,
            &d.fidelity,
            Profile::Responses,
            Contract::full(),
        )
        .unwrap();
        let wire = responses::encode_generation(&target).unwrap();
        assert_eq!(wire["input"][1]["arguments"], "{\"n\":1}");
        assert!(!wire.to_string().contains("parsed_arguments"));
    }
    // The Responses function_call shell has the same rule on its own wire shape.
    let admitted = [
        decode_call(responses_call("{\"n\":1}", None)).unwrap(),
        decode_call(responses_call("{\"n\":1}", Some(Value::Null))).unwrap(),
        decode_call(responses_call("{\"n\":1}", Some(expected))).unwrap(),
    ];
    assert_eq!(admitted[0].semantic, admitted[1].semantic);
    assert_eq!(admitted[1].semantic, admitted[2].semantic);
    for d in &admitted {
        assert_eq!(arguments(d), "{\"n\":1}");
        let (r, c) = wire_pair(d);
        assert!(!r.to_string().contains("parsed_arguments"));
        assert!(!c.to_string().contains("parsed_arguments"));
    }
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

#[test]
fn dumped_sdk_views_replay_through_the_full_envelope_and_never_leak_into_wire() {
    let source = json!({"model":"fixture-model","messages":[
        {"role":"user","content":"hello"},
        {"role":"assistant","content":null,"parsed":null,"tool_calls":[{"id":"call","type":"function","function":{"name":"lookup","arguments":"{\"n\":1}","parsed_arguments":{"n":1}}}]},
        {"role":"tool","tool_call_id":"call","content":"ok"},
        {"role":"assistant","content":RAW,"parsed":{"ok":true}}]});
    let d = chat_envelope::decode_request_bytes(&serde_json::to_vec(&source).unwrap()).unwrap();
    let target = lower_request(
        &d.task.semantic,
        &d.task.fidelity,
        Profile::Chat,
        Contract::full(),
    )
    .unwrap();
    let dumped = chat::encode_generation(&target).unwrap().to_string();
    assert!(!dumped.contains("parsed_arguments"));
    assert!(!dumped.contains("\"parsed\""));
    assert!(dumped.contains("lookup"));
    let target = lower_request(
        &d.task.semantic,
        &d.task.fidelity,
        Profile::Responses,
        Contract::full(),
    )
    .unwrap();
    let projected = responses::encode_generation(&target).unwrap().to_string();
    assert!(!projected.contains("parsed_arguments"));
    assert!(!projected.contains("\"parsed\""));
    assert!(projected.contains("lookup"));
}

#[test]
fn stream_accumulation_index_artifacts_stay_position_consistent_on_replay() {
    // The pinned SDK's chat stream accumulation leaks the chunk index into message calls.
    let entry = |id: &str, index: Option<Value>| {
        let mut e = json!({"id":id,"type":"function","function":{"name":"lookup","arguments":"{\"n\":1}","parsed_arguments":{"n":1}}});
        if let Some(v) = index {
            e["index"] = v;
        }
        e
    };
    let message =
        |entries: Vec<Value>| json!({"role":"assistant","content":null,"tool_calls":entries});
    // Missing, null and a position-consistent index are one admitted history shape.
    for e in [
        entry("call", None),
        entry("call", Some(Value::Null)),
        entry("call", Some(json!(0))),
    ] {
        assert!(decode_chat(message(vec![e])).is_ok());
    }
    let two = vec![
        entry("call", Some(json!(0))),
        entry("call2", Some(json!(1))),
    ];
    assert!(decode_chat(message(two)).is_ok());
    // An index that contradicts the call position or its integer shape cannot replay.
    for entries in [
        vec![entry("call", Some(json!(1)))],
        vec![
            entry("call", Some(json!(0))),
            entry("call2", Some(json!(0))),
        ],
        vec![entry("call", Some(json!("0")))],
        vec![entry("call", Some(json!(-1)))],
    ] {
        assert!(decode_chat(message(entries)).is_err());
    }
}
