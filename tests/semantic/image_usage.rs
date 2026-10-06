//! Image-token reports are typed facts with explicit profile projections.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
};
use serde_json::{Value, json};
fn response(count: Value) -> Value {
    json!({"id":"r","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":{"role":"assistant","content":"x"},"finish_reason":"stop"}],"usage":{
        "prompt_tokens":10,"completion_tokens":2,"total_tokens":12,
        "prompt_tokens_details":{"cached_tokens":1,"image_tokens":count},
        "completion_tokens_details":{"reasoning_tokens":0}
    }})
}
#[test]
fn named_image_usage_mapping_preserves_counts_and_rejects_standard_targets() {
    let source = response(json!(7));
    let provider = Adapter::new(Profile::Chat, Dialect::Xiaomi, None);
    let decoded = provider
        .decode_response(&serde_json::to_vec(&source).unwrap())
        .unwrap();
    for profile in [Profile::Chat, Profile::Responses] {
        let client = Adapter::new(profile, Dialect::MorphieCore, None);
        let encoded = client.encode_response(&decoded, &Contract::full()).unwrap();
        let detail = if profile == Profile::Chat {
            "prompt_tokens_details"
        } else {
            "input_tokens_details"
        };
        assert_eq!(encoded["usage"][detail]["image_tokens"], 7);
        assert!(
            Adapter::new(profile, Dialect::Standard, None)
                .encode_response(&decoded, &Contract::full())
                .is_err()
        );
    }
    assert!(
        Adapter::new(Profile::Chat, Dialect::Standard, None)
            .decode_response(&serde_json::to_vec(&source).unwrap())
            .is_err()
    );
    assert_eq!(
        decoded.semantic.usage().unwrap().input_image_tokens,
        Some(7)
    );
}

#[test]
fn readable_chat_modality_counters_remain_typed_without_claiming_responses_slots() {
    let mut source = response(json!(7));
    source["usage"]["prompt_tokens_details"]["text_tokens"] = json!(3);
    source["usage"]["completion_tokens_details"]["text_tokens"] = json!(2);
    let provider = Adapter::new(Profile::Chat, Dialect::Bailian, None);
    let mut decoded = provider
        .decode_response(&serde_json::to_vec(&source).unwrap())
        .unwrap();
    let usage = decoded.semantic.usage().unwrap();
    assert_eq!(usage.input_image_tokens, Some(7));
    assert_eq!(usage.input_text_tokens, Some(3));
    assert_eq!(usage.output_text_tokens, Some(2));
    let client = Adapter::new(Profile::Chat, Dialect::MorphieCore, None);
    assert_eq!(
        client.encode_response(&decoded, &Contract::full()).unwrap()["usage"],
        source["usage"]
    );
    let responses = Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        responses["usage"]["input_tokens_details"],
        json!({"cached_tokens":1,"image_tokens":7,"text_tokens":3})
    );
    assert_eq!(
        responses["usage"]["output_tokens_details"],
        json!({"reasoning_tokens":0,"text_tokens":2})
    );
    assert!(
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_response(&decoded, &Contract::full())
            .is_err()
    );
    decoded.semantic = decoded
        .semantic
        .clone()
        .with_usage(morphiecore::semantic::task::generation::Usage {
            scope: morphiecore::semantic::task::generation::UsageScope::Operation,
            basis: morphiecore::semantic::task::generation::UsageBasis::Final,
            output_relation:
                morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
            total_relation:
                morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
            input_image_tokens: Some(5),
            ..usage
        })
        .unwrap();
    let changed = client.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(changed["usage"]["prompt_tokens_details"]["image_tokens"], 5);
    assert_eq!(changed["usage"]["prompt_tokens_details"]["text_tokens"], 3);
}

#[test]
fn image_usage_presence_edits_and_bounds_do_not_restore_deleted_counts() {
    use morphiecore::semantic::task::generation::Usage;
    let provider = Adapter::new(Profile::Chat, Dialect::Xiaomi, None);
    for count in [Value::Null, json!(0), json!(10)] {
        let mut decoded = provider
            .decode_response(&serde_json::to_vec(&response(count.clone())).unwrap())
            .unwrap();
        let usage = decoded.semantic.usage().unwrap();
        assert_eq!(usage.input_image_tokens, count.as_u64());
        for profile in [Profile::Chat, Profile::Responses] {
            let client = Adapter::new(profile, Dialect::MorphieCore, None);
            let encoded = client.encode_response(&decoded, &Contract::full()).unwrap();
            let field = if profile == Profile::Chat {
                "prompt_tokens_details"
            } else {
                "input_tokens_details"
            };
            assert_eq!(
                encoded["usage"][field].get("image_tokens"),
                count.as_u64().map(|_| &count)
            );
            let changed = Usage {
                scope: morphiecore::semantic::task::generation::UsageScope::Operation,
                basis: morphiecore::semantic::task::generation::UsageBasis::Final,
                output_relation:
                    morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning,
                total_relation:
                    morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
                input_image_tokens: Some(3),
                ..usage
            };
            decoded.semantic = decoded.semantic.clone().with_usage(changed).unwrap();
            let encoded = client.encode_response(&decoded, &Contract::full()).unwrap();
            assert_eq!(encoded["usage"][field]["image_tokens"], 3);
            decoded.semantic = decoded
                .semantic
                .clone()
                .with_usage(Usage {
                    scope: morphiecore::semantic::task::generation::UsageScope::Operation, basis: morphiecore::semantic::task::generation::UsageBasis::Final, output_relation: morphiecore::semantic::task::generation::OutputTokenRelation::IncludesReasoning, total_relation: morphiecore::semantic::task::generation::TotalTokenRelation::InputAndOutput,
                    input_image_tokens: None,
                    ..usage
                })
                .unwrap();
            let encoded = client.encode_response(&decoded, &Contract::full()).unwrap();
            assert!(encoded["usage"][field].get("image_tokens").is_none());
            assert!(
                Adapter::new(profile, Dialect::Standard, None)
                    .encode_response(&decoded, &Contract::full())
                    .is_ok()
            );
            decoded.semantic = decoded.semantic.clone().with_usage(usage).unwrap();
        }
    }
    for count in [json!(-1), json!(11), json!(1.5), json!("1"), json!(false)] {
        assert!(
            provider
                .decode_response(&serde_json::to_vec(&response(count)).unwrap())
                .is_err()
        );
    }
}

#[test]
fn image_usage_static_and_events_close_under_named_slots_and_poison_unsupported_targets() {
    use morphiecore::protocol::openai::events::EventEncoder;
    use morphiecore::semantic::task::generation::StreamEvent;
    let provider = Adapter::new(Profile::Chat, Dialect::Xiaomi, None);
    let source = response(json!(7));
    let expected = provider
        .decode_response(&serde_json::to_vec(&source).unwrap())
        .unwrap();
    let mut decoder = provider.event_decoder();
    let mut events = vec![];
    for chunk in [
        json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"x"},"finish_reason":"stop"}]}),
        json!({"id":"r","object":"chat.completion.chunk","created":1,"model":"m","choices":[],"usage":source["usage"]}),
    ] {
        events.extend(decoder.push(&chunk).unwrap());
    }
    events.extend(decoder.done().unwrap());
    assert_eq!(decoder.materialize().unwrap().semantic, expected.semantic);
    for profile in [Profile::Chat, Profile::Responses] {
        let client = Adapter::new(profile, Dialect::MorphieCore, None);
        let mut encoder = EventEncoder::new(profile, expected.metadata.clone())
            .unwrap()
            .with_contract(client.contract(&Contract::full()));
        let mut wire = vec![];
        for event in &events {
            wire.extend(encoder.encode(event, &expected.fidelity).unwrap());
        }
        encoder.finish().unwrap();
        let reported = if profile == Profile::Chat {
            &wire.iter().find(|v| v["usage"].is_object()).unwrap()["usage"]
        } else {
            &wire.last().unwrap()["response"]["usage"]
        };
        let oracle = if profile == Profile::Chat {
            source["usage"].clone()
        } else {
            json!({"input_tokens":10,"output_tokens":2,"total_tokens":12,"input_tokens_details":{"cached_tokens":1,"image_tokens":7},"output_tokens_details":{"reasoning_tokens":0}})
        };
        assert_eq!(reported, &oracle);
        let mut decoded = client.event_decoder();
        for value in &wire {
            decoded.push(value).unwrap();
        }
        if profile == Profile::Chat {
            decoded.done().unwrap();
        }
        assert_eq!(
            decoded.materialize().unwrap().semantic.usage(),
            expected.semantic.usage()
        );
        let mut rejected = EventEncoder::new(profile, expected.metadata.clone()).unwrap();
        for event in &events {
            let result = rejected.encode(event, &expected.fidelity);
            if matches!(event, StreamEvent::Usage(_)) {
                assert!(result.is_err());
                break;
            }
            result.unwrap();
        }
        assert!(
            rejected
                .encode(events.last().unwrap(), &expected.fidelity)
                .is_err()
        );
        assert!(rejected.finish().is_err());
    }
}
