//! Independent Chat probability and context oracles against the pinned SDK contract.
use morphiecore::protocol::openai::{
    Dialect, GenerationApi, GenerationCodec, TargetContract as Contract,
};
use serde_json::{Value, json};

#[test]
fn audit_probability_extensions_do_not_weaken_known_field_validation() {
    use morphiecore::semantic::{task::generation::*, value::Presence};
    let mut p = probability("ok");
    p["future_token_field"] = json!({"x":true});
    p["top_logprobs"][0]["future_top_field"] = json!(42);
    let body = response(false, json!({"content":[p.clone()],"refusal":null}));
    let decoded = adapter(GenerationApi::Chat)
        .decode_response(body.to_string().as_bytes())
        .unwrap();
    let Item::Message(message) = &decoded.semantic.items()[0].1 else {
        panic!("message")
    };
    let ContentPart::Text(text) = &message.parts[0].content else {
        panic!("text")
    };
    assert_eq!(
        text.logprobs(),
        &Presence::Value(vec![Logprob {
            token: "ok".into(),
            logprob: serde_json::from_str("-0.1234567890123456").unwrap(),
            bytes: None,
            top_logprobs: Some(vec![TopLogprob {
                token: Some("ok".into()),
                logprob: Some(serde_json::from_str("-0.1234567890123456").unwrap()),
                bytes: Some(vec![111, 107]),
            }]),
        }])
    );
    for bad in [json!("not-a-number"), json!(true)] {
        p["logprob"] = bad;
        let body = response(false, json!({"content":[p.clone()],"refusal":null}));
        assert!(
            adapter(GenerationApi::Chat)
                .decode_response(body.to_string().as_bytes())
                .is_err()
        );
    }
}

fn adapter(profile: GenerationApi) -> GenerationCodec {
    GenerationCodec::new(profile, Dialect::Standard, None)
}
fn probability(token: &str) -> Value {
    json!({"token":token,"logprob":-0.1234567890123456,"bytes":null,
        "top_logprobs":[{"token":token,"logprob":-0.1234567890123456,"bytes":token.as_bytes()}]})
}
fn response(refusal: bool, probabilities: Value) -> Value {
    let message = if refusal {
        json!({"role":"assistant","content":null,"refusal":"no"})
    } else {
        json!({"role":"assistant","content":"ok"})
    };
    json!({"id":"c","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":message,"finish_reason":"stop","logprobs":probabilities}]})
}
fn chunk(delta: Value, probabilities: Value, finish: Value) -> Value {
    json!({"id":"c","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":delta,"logprobs":probabilities,"finish_reason":finish}]})
}

#[test]
fn chat_probability_request_controls_project_and_reject_invalid_combinations() {
    let wire = json!({"model":"m","messages":[{"role":"user","content":"hi"}],"logprobs":true,"top_logprobs":2});
    let mut request = adapter(GenerationApi::Chat)
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    let encoded = adapter(GenerationApi::Chat)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(encoded["logprobs"], true);
    assert_eq!(encoded["top_logprobs"], 2);
    let responses = adapter(GenerationApi::Responses)
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(
        responses["include"],
        json!(["message.output_text.logprobs"])
    );
    assert_eq!(responses["top_logprobs"], 2);
    let mut settings = request.task.semantic.settings().clone();
    settings.controls.top_logprobs = None;
    request.task.semantic = request.task.semantic.with_settings(settings).unwrap();
    assert!(
        adapter(GenerationApi::Chat)
            .encode_request(&request, "m", &Contract::full())
            .unwrap()
            .get("top_logprobs")
            .is_none()
    );
    let mut contract = Contract::full();
    contract.dispositions.set_native(
        morphiecore::semantic::task::generation::Feature::Logprobs,
        false,
    );
    let (omitted, rules) = adapter(GenerationApi::Chat)
        .encode_request_with_rules(&request, "m", &contract)
        .unwrap();
    assert!(omitted.get("logprobs").is_none());
    assert!(omitted.get("top_logprobs").is_none());
    assert!(rules.contains(&"generation.omit-logprobs.v1"));
    for value in [Value::Null, json!(false)] {
        let mut wire = wire.clone();
        wire["logprobs"] = value.clone();
        wire.as_object_mut().unwrap().shift_remove("top_logprobs");
        let request = adapter(GenerationApi::Chat)
            .decode_request(wire.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            adapter(GenerationApi::Chat)
                .encode_request(&request, "m", &Contract::full())
                .unwrap()["logprobs"],
            value
        );
    }
    for (enabled, top) in [
        (json!(false), json!(1)),
        (Value::Null, json!(0)),
        (json!(true), json!(21)),
        (json!(true), json!(-1)),
        (json!(true), json!(0.5)),
    ] {
        let mut wire = wire.clone();
        wire["logprobs"] = enabled;
        wire["top_logprobs"] = top;
        assert!(
            adapter(GenerationApi::Chat)
                .decode_request(wire.to_string().as_bytes())
                .is_err()
        );
    }
}

#[test]
fn chat_probability_arrays_are_reported_even_when_empty_and_bad_owners_fail() {
    let codec = adapter(GenerationApi::Chat);
    let absent = codec
        .decode_response(response(false, Value::Null).to_string().as_bytes())
        .unwrap();
    assert_eq!(
        codec.encode_response(&absent, &Contract::full()).unwrap()["choices"][0].get("logprobs"),
        Some(&Value::Null)
    );
    for refusal in [false, true] {
        let key = if refusal { "refusal" } else { "content" };
        let decoded = adapter(GenerationApi::Chat)
            .decode_response(response(refusal, json!({key:[]})).to_string().as_bytes())
            .unwrap();
        assert_eq!(
            adapter(GenerationApi::Chat)
                .encode_response(&decoded, &Contract::full())
                .unwrap()["choices"][0]["logprobs"][key],
            json!([])
        );
        let other = if refusal { "content" } else { "refusal" };
        assert_eq!(
            adapter(GenerationApi::Chat)
                .encode_response(&decoded, &Contract::full())
                .unwrap()["choices"][0]["logprobs"]
                .get(other),
            Some(&Value::Null)
        );
        assert!(
            adapter(GenerationApi::Chat)
                .decode_response(response(refusal, json!({other:[]})).to_string().as_bytes())
                .is_err()
        );
    }
    for probability in [
        json!({"token":"ok","logprob":-1}),
        json!({"token":"ok","logprob":1,"top_logprobs":[]}),
        json!({"token":"ok","logprob":-1,"top_logprobs":[{"token":"other"}]}),
        json!({"token":"ok","logprob":-1,"top_logprobs":[],"bytes":[256]}),
    ] {
        assert!(
            adapter(GenerationApi::Chat)
                .decode_response(
                    response(false, json!({"content":[probability]}))
                        .to_string()
                        .as_bytes()
                )
                .is_err()
        );
    }
    let mut decoder = adapter(GenerationApi::Chat).event_decoder();
    assert!(
        decoder
            .push(&chunk(
                json!({"content":"ok"}),
                json!({"refusal":[probability("no")]}),
                Value::Null
            ))
            .is_err()
    );
    assert!(
        decoder
            .push(&chunk(json!({}), Value::Null, json!("stop")))
            .is_err()
    );
    assert!(decoder.done().is_err());
}
