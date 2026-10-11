//! Reported token details are typed facts, never inferred disjoint partitions.
use morphiecore::protocol::openai::{Adapter, Dialect, Profile, TargetCaps as Contract};
use serde_json::{Value, json};
fn adapter(profile: Profile) -> Adapter {
    Adapter::new(profile, Dialect::Standard, None)
}
fn body() -> Value {
    json!({"id":"r","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"x"},"finish_reason":"stop"}],"usage":{
        "prompt_tokens":10,"completion_tokens":20,"total_tokens":30,
        "prompt_tokens_details":{"cached_tokens":2,"cache_write_tokens":1,"text_tokens":9},
        "completion_tokens_details":{"reasoning_tokens":6,"text_tokens":20,"accepted_prediction_tokens":7,"rejected_prediction_tokens":3}
    }})
}
#[test]
fn detail_zero_is_reported_but_null_is_not_and_invalid_counters_fail_closed() {
    let chat = adapter(Profile::Chat);
    for value in [Value::Null, json!(0)] {
        let mut source = body();
        source["usage"]["prompt_tokens_details"]["text_tokens"] = value.clone();
        for k in [
            "text_tokens",
            "accepted_prediction_tokens",
            "rejected_prediction_tokens",
        ] {
            source["usage"]["completion_tokens_details"][k] = value.clone();
        }
        let decoded = chat.decode_response(source.to_string().as_bytes()).unwrap();
        let output = chat.encode_response(&decoded, &Contract::full()).unwrap();
        if value.is_null() {
            assert!(
                output["usage"]["prompt_tokens_details"]
                    .get("text_tokens")
                    .is_none()
            );
            assert!(
                adapter(Profile::Responses)
                    .encode_response(&decoded, &Contract::full())
                    .is_ok()
            );
        } else {
            assert_eq!(
                output["usage"]["completion_tokens_details"]["accepted_prediction_tokens"],
                0
            );
            let projected = adapter(Profile::Responses)
                .encode_response(&decoded, &Contract::full())
                .unwrap();
            assert_eq!(projected["usage"]["total_tokens"], 30);
            assert_eq!(
                decoded.semantic.usage().unwrap().accepted_prediction_tokens,
                Some(0)
            );
        }
    }
    for (detail, key, limit) in [
        ("prompt_tokens_details", "text_tokens", 10),
        ("completion_tokens_details", "text_tokens", 20),
        (
            "completion_tokens_details",
            "accepted_prediction_tokens",
            20,
        ),
        (
            "completion_tokens_details",
            "rejected_prediction_tokens",
            20,
        ),
    ] {
        for invalid in [
            json!(-1),
            json!(1.5),
            json!("1"),
            json!(false),
            json!(limit + 1),
        ] {
            let mut source = body();
            source["usage"][detail][key] = invalid;
            assert!(chat.decode_response(source.to_string().as_bytes()).is_err());
        }
    }
    let mut source = body();
    source["usage"]["completion_tokens_details"]["rejected_prediction_tokens"] = json!(14);
    assert!(chat.decode_response(source.to_string().as_bytes()).is_err());
    let mut source = body();
    source["usage"]["prompt_tokens_details"]["audio_tokens"] = json!(0);
    let decoded_audio = chat.decode_response(source.to_string().as_bytes()).unwrap();
    assert_eq!(
        decoded_audio.semantic.usage().unwrap().input_audio_tokens,
        Some(0)
    );
    assert_eq!(
        chat.encode_response(&decoded_audio, &Contract::full())
            .unwrap()["usage"]["prompt_tokens_details"]["audio_tokens"],
        0
    );
    let mut large = body();
    let count = 9_007_199_254_740_993_u64;
    large["usage"] = json!({"prompt_tokens":1,"completion_tokens":count,"total_tokens":count+1,"prompt_tokens_details":{"text_tokens":1},"completion_tokens_details":{"text_tokens":count,"accepted_prediction_tokens":count-1,"rejected_prediction_tokens":1}});
    let decoded = chat.decode_response(large.to_string().as_bytes()).unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().output_text_tokens,
        Some(count)
    );
    assert_eq!(
        chat.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
        large["usage"]
    );
    let mut source = body();
    source["usage"] = json!({"prompt_tokens":0,"completion_tokens":u64::MAX,"total_tokens":u64::MAX,"completion_tokens_details":{"accepted_prediction_tokens":u64::MAX,"rejected_prediction_tokens":1}});
    assert!(chat.decode_response(source.to_string().as_bytes()).is_err());
}
