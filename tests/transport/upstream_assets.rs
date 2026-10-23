//! Upstream-owned inputs, independent semantic expectations and deliberate SDK-tolerance limits.
use crate::upstream_assets::{asset, decode, jsonl_sse, messages_with_delimiter};
use morphiecore::{
    adapter::UpstreamAdapter,
    protocol::{
        anthropic::messages::{self, Profile as MessagesProfile},
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile},
    },
    semantic::task::generation::{ContentPart, IncompleteReason, Item, Outcome},
    transport::sse::SseDecoder,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn openai(profile: Profile) -> UpstreamAdapter {
    Adapter::new(profile, Dialect::Standard, None).into()
}
fn messages(model: &str) -> UpstreamAdapter {
    UpstreamAdapter::AnthropicMessages {
        profile: MessagesProfile::AdaptiveTextTools,
        target: ReplayTarget::new("public-fixture", model).unwrap(),
    }
}
fn text(attempt: &morphiecore::execution::Attempt) -> String {
    attempt
        .response()
        .unwrap()
        .semantic
        .items()
        .iter()
        .flat_map(|(_, item)| match item {
            Item::Message(message) => message
                .parts
                .iter()
                .filter_map(|part| match &part.content {
                    ContentPart::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect()
}

#[test]
fn asset_bytes_and_revisions_match_the_reviewed_sources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/upstream/sources.lock.json")).unwrap(),
    )
    .unwrap();
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/upstream/assets.lock.json")).unwrap(),
    )
    .unwrap();
    for entry in manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .chain(manifest["framing"].as_array().unwrap())
    {
        let local = entry["local"].as_str().unwrap();
        let digest: String = Sha256::digest(asset(local))
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(digest, entry["sha256"].as_str().unwrap(), "{local}");
        if let Some(source) = entry["source"].as_str() {
            assert_eq!(
                entry["revision"], sources["sources"][source]["revision"],
                "{local}"
            );
        }
    }
}

#[test]
fn upstream_framing_literals_preserve_cr_crlf_and_multiline_data() {
    for (name, expected, event) in [
        (
            "multiline",
            ["YHOO\n+2\n10", "GOOG\n-8\n1881"],
            Some("stock"),
        ),
        ("cr", ["dog\nbark", "cat\nmeow"], None),
        ("crlf", ["sheep\nbleat", "pig\noink"], None),
    ] {
        let bytes = asset(&format!("eventsource-parser/{name}.sse"));
        for split in 0..=bytes.len() {
            let mut decoder = SseDecoder::new(1024);
            let mut events = decoder.push(&bytes[..split]).unwrap();
            events.extend(decoder.push(&bytes[split..]).unwrap());
            events.extend(decoder.finish_strict().unwrap());
            assert_eq!(
                events.iter().map(|v| v.data()).collect::<Vec<_>>(),
                expected,
                "{name}: {split}"
            );
            assert!(events.iter().all(|v| v.event() == event));
        }
    }
}

#[test]
fn official_chat_tool_fragments_and_length_are_not_repaired() {
    for chunk in [1, 17, 8192] {
        let attempt = decode(openai(Profile::Chat), &asset("openai/chat-tool.sse"), chunk).unwrap();
        let response = attempt.response().unwrap();
        let calls: Vec<_> = response
            .semantic
            .items()
            .iter()
            .filter_map(|(_, item)| match item {
                Item::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id.as_str(), "call_c91SqDXlYFuETYv8mUHzz6pp");
        assert_eq!(calls[0].name.as_str(), "GetWeatherArgs");
        assert_eq!(
            calls[0].arguments.as_raw(),
            Some("{\"city\":\"Edinburgh\",\"country\":\"UK\",\"units\":\"c\"}")
        );
        assert_eq!(response.semantic.usage().unwrap().input_tokens, Some(76));
        assert_eq!(response.semantic.usage().unwrap().output_tokens, Some(24));
        let attempt = decode(
            openai(Profile::Chat),
            &asset("openai/chat-length.sse"),
            chunk,
        )
        .unwrap();
        assert_eq!(text(&attempt), "{\"");
        assert_eq!(
            attempt.response().unwrap().semantic.outcome(),
            Outcome::Incomplete
        );
        assert_eq!(
            attempt.response().unwrap().semantic.details().incomplete,
            Some(IncompleteReason::MaxOutputTokens)
        );
    }
}

#[test]
fn official_messages_text_tools_and_sparse_usage_reach_ir_and_delivery() {
    for (name, model, expected, input, output) in [
        ("basic", "claude-3-opus-latest", "Hello there!", 11, 6),
        (
            "tool-use",
            "claude-sonnet-4-20250514",
            "I'll check the current weather in Paris for you.",
            377,
            65,
        ),
        ("omitted-usage", "claude-sonnet-4-5", "Hello there!", 25, 8),
    ] {
        let source = format!("anthropic/{name}.sse");
        for chunk in [1, 19, 8192] {
            let attempt =
                decode(messages(model), &messages_with_delimiter(&source), chunk).unwrap();
            let response = attempt.response().unwrap();
            assert_eq!(text(&attempt), expected);
            // Native input counts are disjoint from cache counts; inspect native delivery below.
            let wire = messages::client::encode_client_response(response, model).unwrap();
            assert_eq!(wire["usage"]["input_tokens"], input);
            assert_eq!(wire["usage"]["output_tokens"], output);
            if name == "tool-use" {
                assert_eq!(wire["content"][1]["id"], "toolu_01NRLabsLyVHZPKxbKvkfSMn");
                assert_eq!(wire["content"][1]["input"], json!({"location":"Paris"}));
                assert_eq!(wire["stop_reason"], "tool_use");
            } else {
                assert_eq!(wire["stop_reason"], "end_turn");
            }
            if name == "omitted-usage" {
                assert_eq!(wire["usage"]["cache_creation_input_tokens"], 10);
                assert_eq!(wire["usage"]["cache_read_input_tokens"], 5);
            }
        }
        let error = decode(messages(model), &asset(&source), 17)
            .err()
            .expect("strict EOF");
        assert!(format!("{error:?}").contains("UnexpectedEof"), "{error:?}");
    }
}

#[test]
fn permissive_sdk_fixtures_do_not_override_event_grammar() {
    let error = decode(
        messages("claude-3-opus-latest"),
        &messages_with_delimiter("anthropic/unknown-events.sse"),
        13,
    )
    .err()
    .expect("invalid event order");
    assert!(
        matches!(
            error,
            morphiecore::execution::AttemptError::Sse(
                morphiecore::protocol::stream::SseError::Codec(
                    morphiecore::protocol::CodecError::Invalid(
                        "Anthropic stream ordering or closure"
                    )
                )
            )
        ),
        "{error:?}"
    );
    // Vercel's synthetic custom-tool stream lacks required sequence_number fields.
    let error = decode(
        openai(Profile::Responses),
        &jsonl_sse("vercel/responses-custom.jsonl", Some("type")),
        13,
    )
    .err()
    .expect("missing sequence");
    assert!(
        matches!(
            error,
            morphiecore::execution::AttemptError::Sse(
                morphiecore::protocol::stream::SseError::Codec(
                    morphiecore::protocol::CodecError::Invalid("sequence")
                )
            )
        ),
        "{error:?}"
    );
}

#[test]
fn vercel_custom_tool_output_replays_distinct_item_and_call_identity() {
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let source: Value = serde_json::from_slice(&asset("vercel/responses-custom.json")).unwrap();
    // Whole retained upstream responses are outside the stateless binding contract.
    // Reuse the exact output array as official returned history, not a rewritten response.
    let input = json!({"model":"public","input":source["output"]});
    let request = adapter
        .decode_client_request(input.to_string().as_bytes())
        .unwrap();
    let calls: Vec<_> = request
        .task
        .semantic
        .items()
        .iter()
        .filter_map(|(_, item)| match item {
            Item::CustomCall(call) => Some(call),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].call_id.as_str(), "call_custom_sql_001");
    assert_eq!(calls[0].name.as_str(), "write_sql");
    assert_eq!(
        calls[0].input.as_str(),
        "SELECT * FROM users WHERE age > 25"
    );
    let output = adapter
        .encode_request(
            &request,
            "public",
            &morphiecore::protocol::openai::TargetCaps::full(),
        )
        .unwrap();
    assert_eq!(output["input"][0]["id"], "ct_abc123def456");
    assert_eq!(output["input"][0]["call_id"], "call_custom_sql_001");
}

#[test]
fn vercel_google_stream_preserves_content_and_source_accounting() {
    let target = ReplayTarget::new("public-fixture", "gemini-3-pro-preview").unwrap();
    let adapter = UpstreamAdapter::GoogleGenerateContent { target };
    let bytes = jsonl_sse("vercel/google-text.jsonl", None);
    for chunk in [1, 23, 8192] {
        let attempt = decode(adapter.clone(), &bytes, chunk).unwrap();
        let response = attempt.response().unwrap();
        assert_eq!(
            text(&attempt),
            "There are **3** \"r\"s in strawberry.\n\nst**r**awbe**rr**y"
        );
        let counts = &response
            .semantic
            .usage_reports()
            .operation()
            .unwrap()
            .counts;
        assert_eq!(counts.input_tokens, Some(9));
        assert_eq!(counts.output_tokens, Some(23));
        assert_eq!(counts.reasoning_tokens, Some(185));
        let signed_parts: Vec<_> = response
            .semantic
            .items()
            .iter()
            .flat_map(|(_, item)| match item {
                Item::Message(message) => message
                    .parts
                    .iter()
                    .filter_map(|part| part.replay.as_ref())
                    .collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect();
        assert_eq!(signed_parts.len(), 1);
        let original = String::from_utf8(asset("vercel/google-text.jsonl")).unwrap();
        let last: Value = serde_json::from_str(original.lines().last().unwrap()).unwrap();
        assert_eq!(
            signed_parts[0].format(),
            morphiecore::semantic::task::generation::ReplayFormat::GoogleGenerateContentSignature
        );
        assert_eq!(
            signed_parts[0].replay_token(),
            last["candidates"][0]["content"]["parts"][0]["thoughtSignature"].as_str()
        );
        assert_eq!(response.semantic.outcome(), Outcome::Completed);
    }
}

#[test]
#[ignore = "known standards gap: first terminal service_tier is incorrectly treated as drift"]
fn interactions_allows_first_service_tier_report_at_terminal() {
    let target = ReplayTarget::new("public-fixture", "gemini-2.5-flash").unwrap();
    let bytes = jsonl_sse("vercel/interactions-basic.jsonl", Some("event_type"));
    let attempt = decode(UpstreamAdapter::GoogleInteractions { target }, &bytes, 23).unwrap();
    assert_eq!(
        text(&attempt),
        "I'm doing great, thank you for asking!\n\nHow are you doing today? And what can I do for you?"
    );
    let counts = &attempt
        .response()
        .unwrap()
        .semantic
        .usage_reports()
        .operation()
        .unwrap()
        .counts;
    assert_eq!(counts.input_tokens, Some(7));
    assert_eq!(counts.output_tokens, Some(26));
}
