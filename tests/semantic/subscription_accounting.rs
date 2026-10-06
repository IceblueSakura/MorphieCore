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
