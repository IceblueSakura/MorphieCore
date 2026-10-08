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
        input_relation: morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
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
    assert_eq!(
        adapter(Profile::Responses)
            .encode_response(&decoded, &Contract::full())
            .unwrap()["usage"],
        json!({"input_tokens":10,"output_tokens":20,"total_tokens":30,
            "input_tokens_details":{"cached_tokens":2,"cache_write_tokens":1},
            "output_tokens_details":{"reasoning_tokens":6}})
    );
    let mut edited = decoded.clone();
    let changed = Usage {
        scope: UsageScope::Operation,
        basis: UsageBasis::Final,
        input_relation: morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
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
        input_relation: morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
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
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            input_text_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            output_text_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            accepted_prediction_tokens: Some(0),
            ..cleared
        },
        Usage {
            scope: UsageScope::Operation,
            basis: UsageBasis::Final,
            input_relation:
                morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
            output_relation: OutputTokenRelation::IncludesReasoning,
            total_relation: TotalTokenRelation::InputAndOutput,
            rejected_prediction_tokens: Some(0),
            ..cleared
        },
    ] {
        let mut inserted = edited.clone();
        inserted.semantic = inserted.semantic.with_usage(usage).unwrap();
        let projected = adapter(Profile::Responses)
            .encode_response(&inserted, &Contract::full())
            .unwrap();
        assert_eq!(projected["usage"]["total_tokens"], 30);
        assert!(
            projected["usage"]["output_tokens_details"]
                .get("accepted_prediction_tokens")
                .is_none()
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
                input_relation:
                    morphiecore::semantic::task::generation::InputTokenRelation::IncludesCache,
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
#[test]
fn details_close_chat_streams_and_responses_uses_the_named_projection() {
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
        let mut output = vec![];
        for event in &events {
            output.extend(responses.encode(event, &expected.fidelity).unwrap());
        }
        responses.finish().unwrap();
        assert_eq!(
            output.last().unwrap()["response"]["usage"],
            json!({
                "input_tokens":10,"output_tokens":20,"total_tokens":30,
                "input_tokens_details":{"cached_tokens":2,"cache_write_tokens":1},
                "output_tokens_details":{"reasoning_tokens":6}
            })
        );
        assert_eq!(
            responses
                .projection()
                .iter()
                .filter(|s| s.loss.is_some())
                .count(),
            4
        );
    }
}

#[test]
fn absent_and_null_usage_remain_unknown_while_reported_zero_and_errors_stay_distinct() {
    for (profile, mut source, zero) in [
        (
            Profile::Chat,
            body(),
            json!({"prompt_tokens":0,"completion_tokens":0,"total_tokens":0}),
        ),
        (
            Profile::Responses,
            json!({"id":"r","object":"response","created_at":1,"model":"m",
                "status":"completed","output":[]}),
            json!({"input_tokens":0,"output_tokens":0,"total_tokens":0}),
        ),
    ] {
        for dialect in [Dialect::Standard, Dialect::Bailian] {
            let codec = Adapter::new(profile, dialect, None);
            for present in [false, true] {
                source.as_object_mut().unwrap().shift_remove("usage");
                if present {
                    source["usage"] = Value::Null;
                }
                let decoded = codec
                    .decode_response(source.to_string().as_bytes())
                    .unwrap();
                assert_eq!(decoded.semantic.usage(), None);
                assert!(decoded.semantic.usage_reports().is_empty());
                let output = codec.encode_response(&decoded, &Contract::full()).unwrap();
                assert_eq!(output.get("usage"), Some(&Value::Null));
            }
            source["usage"] = zero.clone();
            let mut decoded = codec
                .decode_response(source.to_string().as_bytes())
                .unwrap();
            assert_eq!(decoded.semantic.usage(), Some(Usage::operation(0, 0, 0)));
            assert_eq!(
                codec.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
                zero
            );
            decoded.semantic = decoded.semantic.with_usage_reports(vec![]).unwrap();
            assert_eq!(
                codec
                    .encode_response(&decoded, &Contract::full())
                    .unwrap()
                    .get("usage"),
                Some(&Value::Null)
            );
            let input_key = if profile == Profile::Chat {
                "prompt_tokens"
            } else {
                "input_tokens"
            };
            for bad in [
                json!({}),
                json!("unavailable"),
                json!(-1),
                {
                    let mut bad = zero.clone();
                    bad[input_key] = json!("0");
                    bad
                },
                {
                    let mut bad = zero.clone();
                    bad["total_tokens"] = json!(1);
                    bad
                },
            ] {
                source["usage"] = bad;
                assert!(
                    codec
                        .decode_response(source.to_string().as_bytes())
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn responses_events_keep_unknown_usage_null_and_reject_contradictory_reports() {
    use morphiecore::protocol::openai::events::EventDecoder;
    let mut values = crate::wire::events(2);
    values.last_mut().unwrap()["response"]["usage"] = Value::Null;
    let mut decoder = EventDecoder::new(Profile::Responses);
    let mut events = vec![];
    for value in &values {
        events.extend(decoder.push(value).unwrap());
    }
    decoder.finish().unwrap();
    let decoded = decoder.materialize().unwrap();
    assert_eq!(decoded.semantic.usage(), None);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, StreamEvent::Usage(_)))
    );
    let mut encoder = EventEncoder::new(Profile::Responses, decoded.metadata.clone()).unwrap();
    let mut output = vec![];
    for event in &events {
        output.extend(encoder.encode(event, &decoded.fidelity).unwrap());
    }
    encoder.finish().unwrap();
    assert_eq!(
        output.last().unwrap()["response"].get("usage"),
        Some(&Value::Null)
    );
    values.last_mut().unwrap()["response"]["usage"] =
        json!({"input_tokens":1,"output_tokens":2,"total_tokens":4});
    let mut invalid = EventDecoder::new(Profile::Responses);
    for value in &values[..values.len() - 1] {
        invalid.push(value).unwrap();
    }
    assert!(invalid.push(values.last().unwrap()).is_err());
    assert!(invalid.finish().is_err());
    assert!(invalid.materialize().is_err());
}
