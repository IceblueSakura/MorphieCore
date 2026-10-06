//! Reported token details are typed facts, never inferred disjoint partitions.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::{
        Profile,
        chat_sse::{ChatSseDecoder, ChatSseEncoder},
        events::EventEncoder,
        sse::{Obfuscation, SseLimits},
    },
    semantic::{context::StreamOptions, task::generation::*, value::Presence},
};
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
fn text_and_prediction_counts_are_preserved_and_not_summed_with_reasoning() {
    let source = body();
    let chat = adapter(Profile::Chat);
    let decoded = chat.decode_response(source.to_string().as_bytes()).unwrap();
    let expected = Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        output_relation: OutputTokenRelation::IncludesReasoning,
        total_relation: TotalTokenRelation::InputAndOutput,
        input_tokens: Some(10),
        output_tokens: Some(20),
        total_tokens: Some(30),
        cached_input_tokens: Some(2),
        input_cache_write_tokens: Some(1),
        reasoning_tokens: Some(6),
        input_text_tokens: Some(9),
        input_image_tokens: None,
        input_audio_tokens: None,
        output_audio_tokens: None,
        output_text_tokens: Some(20),
        accepted_prediction_tokens: Some(7),
        rejected_prediction_tokens: Some(3),
    };
    assert_eq!(decoded.semantic.usage(), Some(expected));
    let output = chat.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(output["usage"], source["usage"]);
    assert!(
        adapter(Profile::Responses)
            .encode_response(&decoded, &Contract::full())
            .is_err()
    );
    let mut edited = decoded.clone();
    let changed = Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        output_relation: OutputTokenRelation::IncludesReasoning,
        total_relation: TotalTokenRelation::InputAndOutput,
        input_text_tokens: Some(6),
        output_text_tokens: Some(12),
        accepted_prediction_tokens: Some(1),
        rejected_prediction_tokens: Some(0),
        ..expected
    };
    edited.semantic = edited.semantic.with_usage(changed).unwrap();
    let output = chat.encode_response(&edited, &Contract::full()).unwrap();
    assert_eq!(output["usage"]["prompt_tokens_details"]["text_tokens"], 6);
    assert_eq!(
        output["usage"]["completion_tokens_details"],
        json!({"reasoning_tokens":6,"text_tokens":12,"accepted_prediction_tokens":1,"rejected_prediction_tokens":0})
    );
    let cleared = Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        output_relation: OutputTokenRelation::IncludesReasoning,
        total_relation: TotalTokenRelation::InputAndOutput,
        input_text_tokens: None,
        output_text_tokens: None,
        accepted_prediction_tokens: None,
        rejected_prediction_tokens: None,
        ..expected
    };
    edited.semantic = edited.semantic.with_usage(cleared).unwrap();
    let output = chat.encode_response(&edited, &Contract::full()).unwrap();
    assert_eq!(
        output["usage"]["prompt_tokens_details"],
        json!({"cached_tokens":2,"cache_write_tokens":1})
    );
    assert_eq!(
        output["usage"]["completion_tokens_details"],
        json!({"reasoning_tokens":6})
    );
    assert!(
        adapter(Profile::Responses)
            .encode_response(&edited, &Contract::full())
            .is_ok()
    );
    for usage in [
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_text_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            output_text_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            accepted_prediction_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            rejected_prediction_tokens: Some(0),
            ..cleared
        },
    ] {
        let mut inserted = edited.clone();
        inserted.semantic = inserted.semantic.with_usage(usage).unwrap();
        assert!(
            adapter(Profile::Responses)
                .encode_response(&inserted, &Contract::full())
                .is_err()
        );
        assert_eq!(
            chat.decode_response(
                chat.encode_response(&inserted, &Contract::full())
                    .unwrap()
                    .to_string()
                    .as_bytes()
            )
            .unwrap()
            .semantic
            .usage(),
            Some(usage)
        );
    }
    assert!(
        edited
            .semantic
            .with_usage(Usage {
                scope: UsageScope::Operation,
                basis: UsageBasis::Final,
                output_relation: OutputTokenRelation::IncludesReasoning,
                total_relation: TotalTokenRelation::InputAndOutput,
                output_text_tokens: Some(21),
                ..cleared
            })
            .is_err()
    );
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
            assert!(
                adapter(Profile::Responses)
                    .encode_response(&decoded, &Contract::full())
                    .is_err()
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
#[test]
fn details_close_chat_streams_but_cannot_disappear_at_a_responses_target() {
    let chat = adapter(Profile::Chat);
    let source = body();
    let expected = chat.decode_response(source.to_string().as_bytes()).unwrap();
    for tail in [false, true] {
        let chunk = |choices: Value, usage: Value| json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m","choices":choices,"usage":usage});
        let mut frames = vec![chunk(
            json!([{"index":0,"delta":{"role":"assistant","content":"x"},"finish_reason":"stop"}]),
            if tail {
                Value::Null
            } else {
                source["usage"].clone()
            },
        )];
        if tail {
            frames.push(chunk(json!([]), source["usage"].clone()));
        }
        let bytes = frames
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>()
            + "data: [DONE]\n\n";
        let mut decoder =
            ChatSseDecoder::new(200, "text/event-stream", SseLimits::default()).unwrap();
        let mut events = vec![];
        for byte in bytes.as_bytes() {
            let (n, values) = decoder.consume(&[*byte]).unwrap();
            assert_eq!(n, 1);
            events.extend(values);
        }
        decoder.finish().unwrap();
        assert_eq!(decoder.materialize().unwrap().semantic, expected.semantic);
        let mut encoder = ChatSseEncoder::new(
            expected.metadata.clone(),
            Contract::full(),
            SseLimits::default(),
            StreamOptions {
                include_usage: Presence::Value(true),
                include_obfuscation: Presence::Value(false),
            },
            Obfuscation::Disabled,
        )
        .unwrap();
        let mut wire = vec![];
        for e in &events {
            for frame in encoder.encode(e, &expected.fidelity).unwrap() {
                wire.extend(frame);
            }
        }
        encoder.finish().unwrap();
        let records: Vec<Value> = std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .filter_map(|s| s.strip_prefix("data: "))
            .filter(|s| *s != "[DONE]")
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(
            records.iter().find(|v| v["usage"].is_object()).unwrap()["usage"],
            source["usage"]
        );
        let mut responses =
            EventEncoder::new(Profile::Responses, expected.metadata.clone()).unwrap();
        for event in &events {
            let encoded = responses.encode(event, &expected.fidelity);
            if matches!(event, StreamEvent::Usage(_)) {
                assert!(encoded.is_err());
                break;
            } else {
                encoded.unwrap();
            }
        }
        assert!(
            responses
                .encode(events.last().unwrap(), &expected.fidelity)
                .is_err()
        );
        assert!(responses.finish().is_err());
    }
}
