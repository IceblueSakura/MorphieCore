//! Independent closed accounting and event-owned output counterexamples.
use morphiecore::{
    protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract as Contract},
    semantic::value::ReplayOrigin,
};
use serde_json::{Value, json};
fn adapter(dialect: Dialect, scope: &str) -> GenerationCodec {
    GenerationCodec::new(
        GenerationApi::Responses,
        dialect,
        Some(ReplayOrigin::new(scope).unwrap()),
    )
}

#[test]
fn siwc_uses_only_closed_public_programs_in_static_and_event_snapshots() {
    let source = adapter(Dialect::Siwc, "one");
    let mut response = crate::wire::response(2);
    response["access_programs"] = json!({"cyber":"standard"});
    response["frequency_penalty"] = json!(0.0);
    response["presence_penalty"] = json!(0);
    response["moderation"] = Value::Null;
    let attribution = json!({"items":{"input.0":{"input_tokens":7,"output_tokens":0,
        "cached_tokens":0,"cache_write_tokens":0,"content":[]},
        "output.0":{"input_tokens":0,"output_tokens":2,"cached_tokens":0,"cache_write_tokens":0}},
        "request_fields":{"tools":{"input_tokens":3,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0}}});
    response["usage"]["attribution"] = attribution.clone();
    let zero_tools = json!({"image_gen":{"input_tokens":0,"input_tokens_details":{"image_tokens":0,"text_tokens":0},"output_tokens":0,"output_tokens_details":{"image_tokens":0,"text_tokens":0},"total_tokens":0},"web_search":{"num_requests":0}});
    response["tool_usage"] = zero_tools.clone();
    let decoded = source
        .decode_response(response.to_string().as_bytes())
        .unwrap();
    let encoded = source.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(encoded["access_programs"], json!({"cyber":"standard"}));
    assert_eq!(encoded["tool_usage"], zero_tools);
    assert_eq!(encoded["frequency_penalty"], json!(0.0));
    assert_eq!(encoded["presence_penalty"], json!(0));
    assert_eq!(encoded["moderation"], Value::Null);
    assert_eq!(encoded["usage"]["attribution"], attribution);
    let mut extended = response.clone();
    extended["access_programs"]["future"] = json!({"ignored":true});
    extended["usage"]["attribution"]["future"] = json!("ignored");
    extended["usage"]["attribution"]["request_fields"]["future"] = json!("ignored");
    extended["usage"]["attribution"]["request_fields"]["tools"]["future"] = json!("ignored");
    extended["usage"]["attribution"]["items"]["input.0"]["future"] = json!("ignored");
    let mut expected_attribution = attribution.clone();
    expected_attribution["items"]["input.0"]["content"] = json!([{
            "input_tokens":1,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0}]);
    extended["usage"]["attribution"]["items"]["input.0"]["content"] = json!([{
            "input_tokens":1,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"future":"ignored"}]);
    let filtered = source
        .decode_response(extended.to_string().as_bytes())
        .unwrap();
    let projected = source
        .encode_response(&filtered, &Contract::full())
        .unwrap();
    assert_eq!(projected["access_programs"], json!({"cyber":"standard"}));
    assert_eq!(projected["usage"]["attribution"], expected_attribution);
    let mut decoder = source.event_decoder();
    for mut event in crate::wire::events(2) {
        if event["type"] == "response.completed" {
            event["response"] = extended.clone();
        }
        decoder.push(&event).unwrap();
    }
    decoder.finish().unwrap();
    let projected = source
        .encode_response(&decoder.materialize().unwrap(), &Contract::full())
        .unwrap();
    assert_eq!(projected["usage"]["attribution"], expected_attribution);
    assert_eq!(decoded.semantic.usage().unwrap().input_tokens, Some(3));
    assert!(
        adapter(Dialect::MorphieCore, "one")
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("access_programs")
            .is_none()
    );
    let mut decoder = source.event_decoder();
    for mut event in crate::wire::events(2) {
        if let Some(snapshot) = event.get_mut("response").and_then(Value::as_object_mut) {
            snapshot.insert("access_programs".into(), json!({"cyber":"standard"}));
            snapshot.insert("tool_usage".into(), zero_tools.clone());
            snapshot.insert("frequency_penalty".into(), json!(0));
            snapshot.insert("presence_penalty".into(), json!(0.0));
            snapshot.insert("moderation".into(), Value::Null);
            if let Some(usage) = snapshot.get_mut("usage").and_then(Value::as_object_mut) {
                usage.insert("attribution".into(), attribution.clone());
            }
        }
        decoder.push(&event).unwrap();
    }
    decoder.finish().unwrap();
    let streamed = decoder.materialize().unwrap();
    assert_eq!(
        source
            .encode_response(&streamed, &Contract::full())
            .unwrap()["usage"]["attribution"],
        attribution
    );
    for value in [
        json!({"unknown":{}}),
        json!({"items":{"x":{"input_tokens":-1,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":[]}}}),
        json!({"items":{"x":{"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":null}}}),
    ] {
        let mut invalid = response.clone();
        invalid["usage"]["attribution"] = value.clone();
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err(),
            "{value}"
        );
    }
    for value in [json!({"cyber":17}), json!({"cyber":"contains spaces"})] {
        let mut invalid = response.clone();
        invalid["access_programs"] = value;
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
    for field in ["frequency_penalty", "presence_penalty"] {
        for value in [json!(1), json!("0"), Value::Null] {
            let mut invalid = response.clone();
            invalid[field] = value;
            assert!(
                source
                    .decode_response(invalid.to_string().as_bytes())
                    .is_err()
            );
        }
    }
    response["moderation"] = json!({"blocked":true});
    assert!(
        source
            .decode_response(response.to_string().as_bytes())
            .is_err()
    );
    response.as_object_mut().unwrap().remove("moderation");
    response["tool_usage"]["web_search"]["num_requests"] = json!(1);
    assert!(
        source
            .decode_response(response.to_string().as_bytes())
            .is_err()
    );
    response["tool_usage"] = json!({"web_search":{"num_requests":0}});
    assert!(
        source
            .decode_response(response.to_string().as_bytes())
            .is_err()
    );
}

#[test]
fn accounting_keeps_distinct_counters_scoped_and_invalidates_after_edits() {
    let source = adapter(Dialect::Grok, "one");
    let mut response = crate::wire::response(2);
    response["frequency_penalty"] = json!(0.0);
    response["usage"]["num_sources_used"] = json!(0);
    response["usage"]["num_server_side_tools_used"] = json!(0);
    response["usage"]["cost_in_usd_ticks"] = json!(9007199254740993u64);
    response["usage"]["context_details"] = json!({"input_tokens":71,"output_tokens":83});
    let mut decoded = source
        .decode_response(response.to_string().as_bytes())
        .unwrap();
    assert_eq!(decoded.semantic.usage().unwrap().output_tokens, Some(5));
    let wire = source.encode_response(&decoded, &Contract::full()).unwrap();
    assert_eq!(
        wire["usage"]["context_details"],
        json!({"input_tokens":71,"output_tokens":83})
    );
    assert_eq!(wire["usage"]["cost_in_usd_ticks"], 9007199254740993u64);
    assert_eq!(decoded.semantic.usage().unwrap().input_tokens, Some(3));
    for future in [json!("ignored"), json!(1), json!({"nested":"ignored"})] {
        let mut extended = response.clone();
        extended["usage"]["context_details"]["future"] = future;
        let clean = source
            .decode_response(extended.to_string().as_bytes())
            .unwrap();
        let projected = source.encode_response(&clean, &Contract::full()).unwrap();
        assert_eq!(
            projected["usage"]["context_details"],
            json!({"input_tokens":71,"output_tokens":83})
        );
        let mut decoder = source.event_decoder();
        for mut event in crate::wire::events(2) {
            if event["type"] == "response.completed" {
                event["response"] = extended.clone();
            }
            decoder.push(&event).unwrap();
        }
        decoder.finish().unwrap();
        assert_eq!(
            source
                .encode_response(&decoder.materialize().unwrap(), &Contract::full())
                .unwrap()["usage"]["context_details"],
            json!({"input_tokens":71,"output_tokens":83})
        );
    }
    for field in ["frequency_penalty", "presence_penalty"] {
        for value in [Value::Null, json!({}), json!("0"), json!(true)] {
            let mut invalid = response.clone();
            invalid[field] = value;
            assert!(matches!(
                source.decode_response(invalid.to_string().as_bytes()),
                Err(morphiecore::protocol::CodecError::Invalid(
                    morphiecore::protocol::failure::ShapeReason::InvalidValue
                ))
            ));
            let mut decoder = source.event_decoder();
            for mut event in crate::wire::events(2) {
                if event["type"] == "response.completed" {
                    event["response"] = invalid.clone();
                    assert!(decoder.push(&event).is_err());
                } else {
                    decoder.push(&event).unwrap();
                }
            }
            assert!(decoder.finish().is_err());
        }
    }
    for other in [
        adapter(Dialect::MorphieCore, "one"),
        adapter(Dialect::Grok, "two"),
    ] {
        let wire = other.encode_response(&decoded, &Contract::full()).unwrap();
        assert!(wire["usage"].get("context_details").is_none());
    }
    let mut usage = decoded.semantic.usage().unwrap();
    usage.output_tokens = Some(7);
    usage.total_tokens = Some(10);
    decoded.semantic = decoded.semantic.with_usage(Some(usage)).unwrap();
    assert!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["usage"]
            .get("cost_in_usd_ticks")
            .is_none()
    );
    for (path, value) in [
        ("/usage/num_sources_used", json!(1)),
        ("/usage/num_server_side_tools_used", json!(1)),
        ("/usage/cost_in_usd_ticks", json!(-1)),
        ("/usage/context_details", json!({"input_tokens":"71"})),
    ] {
        let mut invalid = response.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err(),
            "{path}"
        );
    }
}
