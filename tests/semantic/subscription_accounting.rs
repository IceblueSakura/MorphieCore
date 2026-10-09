//! Independent closed accounting and event-owned output counterexamples.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Contract,
    protocol::openai::Profile,
    semantic::value::ReplayOrigin,
};
use serde_json::{Value, json};
fn adapter(dialect: Dialect, scope: &str) -> Adapter {
    Adapter::new(
        Profile::Responses,
        dialect,
        Some(ReplayOrigin::new(scope).unwrap()),
    )
}

#[test]
fn public_siwc_empty_terminal_summary_requires_actual_item_closure() {
    let source = adapter(Dialect::Siwc, "one");
    for omit_done in [false, true] {
        let mut decoder = source.event_decoder();
        let mut rejected = false;
        for mut event in crate::wire::events(2) {
            if omit_done && event["type"] == "response.output_item.done" {
                continue;
            }
            if event["type"] == "response.completed" {
                event["response"]["output"] = json!([]);
            }
            if decoder.push(&event).is_err() {
                rejected = true;
                break;
            }
        }
        assert_eq!(rejected, omit_done);
        if !omit_done {
            decoder.finish().unwrap();
            let decoded = decoder.materialize().unwrap();
            assert_eq!(decoded.semantic.items().len(), 1);
            assert_eq!(
                decoded.semantic.outcome(),
                morphiecore::semantic::task::generation::Outcome::Completed
            );
        }
    }
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
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["access_programs"],
        json!({"cyber":"standard"})
    );
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["tool_usage"],
        zero_tools
    );
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["frequency_penalty"],
        json!(0.0)
    );
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["presence_penalty"],
        json!(0)
    );
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["moderation"],
        Value::Null
    );
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["usage"]["attribution"],
        attribution
    );
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
        json!({"items":{"x":{"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":[],"extra":0}}}),
        json!({"items":{"x":{"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0,"content":null}}}),
        json!({"items":{},"request_fields":{"unknown":{"input_tokens":0,"output_tokens":0,"cached_tokens":0,"cache_write_tokens":0}}}),
    ] {
        let mut invalid = response.clone();
        invalid["usage"]["attribution"] = value;
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
    for value in [
        json!({"unknown":"standard"}),
        json!({"cyber":17}),
        json!({"cyber":"contains spaces"}),
    ] {
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
    assert!(
        adapter(Dialect::Standard, "one")
            .decode_response(response.to_string().as_bytes())
            .is_err()
    );
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
    decoded.semantic = decoded.semantic.with_usage(usage).unwrap();
    assert!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["usage"]
            .get("cost_in_usd_ticks")
            .is_none()
    );
    for (path, value) in [
        ("/usage/num_sources_used", json!(1)),
        ("/usage/cost_in_usd_ticks", json!(-1)),
        ("/usage/context_details", json!({"input_tokens":"71"})),
    ] {
        let mut invalid = response.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}
#[test]
fn product_metadata_is_closed_and_never_authorizes_hosted_usage() {
    let source = adapter(Dialect::Codex, "one");
    let mut response = crate::wire::response(2);
    response["access_programs"] = json!({"cyber":"synthetic"});
    response["moderation"] = Value::Null;
    response["tool_usage"] = json!({"image_gen":{"input_tokens":0,"input_tokens_details":{"image_tokens":0,"text_tokens":0},"output_tokens":0,"output_tokens_details":{"image_tokens":0,"text_tokens":0},"total_tokens":0},"web_search":{"num_requests":0}});
    response["usage"]["attribution"] = json!({"items":{"synthetic-input":{"input_tokens":17,"output_tokens":0,"cached_tokens":3,"cache_write_tokens":0,"content":[{"input_tokens":17,"output_tokens":0,"cached_tokens":3,"cache_write_tokens":0}]}}});
    let decoded = source
        .decode_response(response.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        source.encode_response(&decoded, &Contract::full()).unwrap()["access_programs"],
        json!({"cyber":"synthetic"})
    );
    assert!(
        adapter(Dialect::MorphieCore, "one")
            .encode_response(&decoded, &Contract::full())
            .unwrap()
            .get("access_programs")
            .is_none()
    );
    for (path, value) in [
        ("/tool_usage/web_search/num_requests", json!(1)),
        ("/access_programs", json!({"unknown":"synthetic"})),
        ("/moderation", json!({"blocked":true})),
        ("/usage/attribution", json!({"items":{"other":1}})),
    ] {
        let mut invalid = response.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            source
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}
