//! Explicit Chat dialect mappings over one semantic model.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::{GenerationRepresentationContract as Contract, lower_request},
    protocol::openai::{Profile, chat, events::EventEncoder},
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::{Value, json};
#[test]
fn chat_text_arrays_keep_order_and_follow_typed_edits() {
    let wire = json!({"model":"m","messages":[
        {"role":"system","content":[{"type":"text","text":"first"},{"type":"text","text":"second"}]},
        {"role":"user","content":[{"type":"text","text":"alpha"},{"type":"text","text":"beta"}]}
    ]});
    let mut request = client()
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&request.task.semantic.items()[0].1, Item::Instruction(i) if i.parts.len()==2 && i.parts[1].1.as_str()=="second")
    );
    assert!(
        matches!(&request.task.semantic.items()[1].1, Item::Message(m) if m.parts.len()==2 && matches!(&m.parts[1].content, ContentPart::Text(t) if t.as_str()=="beta"))
    );
    let encoded = client()
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(encoded["messages"], wire["messages"]);
    let mut items = request.task.semantic.items().to_vec();
    if let Item::Message(m) = &mut items[1].1 {
        m.parts.remove(0);
        m.parts[0].content =
            ContentPart::Text(Text::new("changed", "fixture", 128).unwrap().into());
    }
    request.task.semantic = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let encoded = client()
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(encoded["messages"][1]["content"], "changed");
    let mut items = request.task.semantic.items().to_vec();
    if let Item::Message(m) = &mut items[1].1 {
        m.parts.push(Part {
            id: PartId::new(99),
            content: ContentPart::Text(Text::new("inserted", "fixture", 128).unwrap().into()),
        });
    }
    request.task.semantic = GenerationRequest::new(items, GenerationControls::default()).unwrap();
    let encoded = client()
        .encode_request(&request, "m", &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["messages"][1]["content"],
        json!([{"type":"text","text":"changed"},{"type":"text","text":"inserted"}])
    );
    let mut output = response("pong");
    output["choices"][0]["message"]["content"] = json!([{"type":"text","text":"pong"}]);
    assert!(
        client()
            .decode_response(output.to_string().as_bytes())
            .is_err()
    );
    assert!(client().decode_request(br#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"text","text":"x"}],"refusal":"no"}]}"#).is_err());
    for content in [
        json!([]),
        json!([{"type":"text"}]),
        json!([{"type":"text","text":3}]),
        json!([{"type":"text","text":"x".repeat(MAX_TEXT_BYTES + 1)}]),
        json!([{"type":"image_url","image_url":{"url":"file:///invalid/x"}}]),
        json!([{"type":"text","text":"x","unknown":true}]),
    ] {
        let invalid = json!({"model":"m","messages":[{"role":"user","content":content}]});
        assert!(
            client()
                .decode_request(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}
fn vendor() -> Adapter {
    Adapter::new(Profile::Chat, Dialect::Xiaomi, None)
}
fn client() -> Adapter {
    Adapter::new(Profile::Chat, Dialect::MorphieCore, None)
}
fn response(content: &str) -> Value {
    json!({"id":"c1","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":content,"reasoning_content":"think"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}})
}
fn chunk(delta: Value, finish: Value) -> Value {
    json!({"id":"c1","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":null})
}
#[test]
fn readable_reasoning_has_independent_static_and_event_oracles() {
    let decoded = vendor()
        .decode_response(response("pong").to_string().as_bytes())
        .unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1, Item::Reasoning(r) if matches!(&r.parts[0].1, ReasoningContent::Text(t) if t.as_str()=="think"))
    );
    let output = client()
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        output["choices"][0]["message"]["reasoning_content"],
        "think"
    );
    assert_eq!(output["choices"][0]["message"]["content"], "pong");
    let mut decoder = vendor().event_decoder();
    let mut events = vec![];
    for delta in [
        json!({"role":"assistant","content":"","tool_calls":null,"reasoning_content":null}),
        json!({"role":null,"content":null,"reasoning_content":"think"}),
        json!({"content":"pong"}),
    ] {
        events.extend(decoder.push(&chunk(delta, Value::Null)).unwrap());
    }
    let mut finish = chunk(json!({}), json!("stop"));
    finish["usage"] = response("pong")["usage"].clone();
    events.extend(decoder.push(&finish).unwrap());
    events.extend(decoder.done().unwrap());
    decoder.finish().unwrap();
    assert_eq!(decoder.materialize().unwrap().semantic, decoded.semantic);
    let mut encoder = EventEncoder::new(Profile::Chat, decoded.metadata.clone())
        .unwrap()
        .with_contract(client().contract(&Contract::full()));
    let mut frames = vec![];
    for event in &events {
        frames.extend(encoder.encode(event, decoder.fidelity()).unwrap());
    }
    encoder.finish().unwrap();
    assert!(
        frames
            .iter()
            .any(|f| f.pointer("/choices/0/delta/reasoning_content") == Some(&json!("think")))
    );
    let mut replay = client().event_decoder();
    for frame in frames {
        replay.push(&frame).unwrap();
    }
    replay.done().unwrap();
    assert_eq!(replay.materialize().unwrap().semantic, decoded.semantic);
}
#[test]
fn history_reasoning_maps_to_a_carrier_and_rejects_unrepresentable_edits() {
    let wire = json!({"model":"m","messages":[{"role":"user","content":"hi"},
        {"role":"assistant","content":"checking","reasoning_content":"think","tool_calls":[{"id":"call_a","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"call_a","content":"ok"}],
        "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{}},"strict":false}}]});
    let request = client()
        .decode_request(wire.to_string().as_bytes())
        .unwrap();
    let output = vendor()
        .encode_request(&request, "bound-model", &Contract::full())
        .unwrap();
    assert_eq!(output["messages"][1]["reasoning_content"], "think");
    assert_eq!(output["messages"][1]["tool_calls"][0]["id"], "call_a");
    let mut edited = request.task.semantic.items().to_vec();
    for (_, item) in &mut edited {
        if let Item::Reasoning(r) = item {
            r.parts[0].1 = ReasoningContent::Summary(Text::new("think", "fixture", 128).unwrap());
        }
    }
    let ir = GenerationRequest::new(edited, GenerationControls::default()).unwrap();
    assert!(
        lower_request(
            &ir,
            &request.task.fidelity,
            Profile::Chat,
            vendor().contract(&Contract::full())
        )
        .is_err()
    );
    // The MorphieCore client now has the scoped structured-summary carrier;
    // readable-only vendor profiles still cannot flatten a summary into text.
    assert!(
        lower_request(
            &ir,
            &request.task.fidelity,
            Profile::Chat,
            client().contract(&Contract::full())
        )
        .is_ok()
    );
    let mut unpaired = request.task.semantic.items().to_vec();
    unpaired.retain(|(_, item)| !matches!(item,Item::Message(m) if m.role==MessageRole::Assistant));
    // Removing the carrier also invalidates associated calls before projection.
    assert!(GenerationRequest::new(unpaired, GenerationControls::default()).is_err());
    for message in [
        json!({"role":"user","content":"x","reasoning_content":"bad"}),
        json!({"role":"assistant","content":"x","reasoning_content":5}),
    ] {
        assert!(
            client()
                .decode_request(
                    json!({"model":"m","messages":[message]})
                        .to_string()
                        .as_bytes()
                )
                .is_err()
        );
    }
}
#[test]
fn fingerprint_presence_and_vendor_metadata_drift_are_explicit() {
    for fingerprint in [None, Some(Value::Null), Some(json!("fp-1"))] {
        let mut body = response("pong");
        if let Some(fp) = &fingerprint {
            body["system_fingerprint"] = fp.clone();
        }
        let decoded = vendor()
            .decode_response(body.to_string().as_bytes())
            .unwrap();
        let wire = client()
            .encode_response(&decoded, &Contract::full())
            .unwrap();
        assert_eq!(wire.get("system_fingerprint"), fingerprint.as_ref());
        if fingerprint.is_some() {
            assert!(
                Adapter::new(Profile::Responses, Dialect::Standard, None)
                    .encode_response(&decoded, &Contract::full())
                    .is_err()
            );
        }
    }
    let mut first = chunk(json!({"content":"x"}), Value::Null);
    first["system_fingerprint"] = json!("fp-1");
    let mut next = first.clone();
    next["created"] = json!(2);
    next["system_fingerprint"] = json!("fp-2");
    let mut decoder = vendor().event_decoder();
    decoder.push(&first).unwrap();
    decoder.push(&next).unwrap();
    assert_eq!(decoder.metadata().unwrap().created.as_u64(), Some(1));
    assert_eq!(
        decoder.metadata().unwrap().context.system_fingerprint,
        Presence::Value("fp-1".into())
    );
    next["id"] = json!("other");
    assert!(decoder.push(&next).is_err());
    let mut strict = client().event_decoder();
    strict.push(&first).unwrap();
    next["id"] = json!("c1");
    assert!(strict.push(&next).is_err());
}
#[test]
fn aliases_are_normalized_without_losing_counts_or_masking_disagreement() {
    let adapter = Adapter::new(Profile::Chat, Dialect::DeepSeek, None);
    let mut body = response("pong");
    body["usage"] = json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,
        "prompt_tokens_details":{"cached_tokens":1},"completion_tokens_details":{"reasoning_tokens":1},
        "prompt_cache_hit_tokens":1,"prompt_cache_miss_tokens":2});
    let decoded = adapter
        .decode_response(body.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        decoded.semantic.usage().unwrap().cached_input_tokens,
        Some(1)
    );
    let encoded = client()
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert_eq!(
        encoded["usage"]["prompt_tokens_details"]["cached_tokens"],
        1
    );
    assert!(encoded["usage"].get("prompt_cache_hit_tokens").is_none());
    for (key, value) in [
        ("prompt_cache_hit_tokens", 0),
        ("prompt_cache_miss_tokens", 9),
        ("mystery_tokens", 1),
    ] {
        let mut invalid = body.clone();
        invalid["usage"][key] = json!(value);
        assert!(
            adapter
                .decode_response(invalid.to_string().as_bytes())
                .is_err()
        );
    }
}
#[test]
fn usage_only_belongs_to_finish_or_tail_and_duplicates_fail() {
    let usage = json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5});
    let mut early = chunk(json!({"content":"pong"}), Value::Null);
    early["usage"] = usage.clone();
    assert!(vendor().event_decoder().push(&early).is_err());
    let mut decoder = vendor().event_decoder();
    decoder
        .push(&chunk(json!({"content":"pong"}), Value::Null))
        .unwrap();
    let mut finish = chunk(json!({}), json!("stop"));
    finish["usage"] = usage.clone();
    assert!(
        decoder
            .push(&finish)
            .unwrap()
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage(_)))
    );
    let duplicate = json!({"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[],"usage":usage});
    assert!(decoder.push(&duplicate).is_err());
    assert!(decoder.done().is_err());
}
#[test]
fn deferred_empty_carriers_and_nullable_call_identity_remain_consistent() {
    let chunks = [
        chunk(json!({"role":"assistant","content":""}), Value::Null),
        chunk(
            json!({"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"lookup","arguments":""}}]}),
            Value::Null,
        ),
        chunk(
            json!({"tool_calls":[{"index":0,"id":null,"type":"function","function":{"name":null,"arguments":"{\"k\":1}"}}]}),
            Value::Null,
        ),
        chunk(json!({}), json!("tool_calls")),
    ];
    let mut decoder = vendor().event_decoder();
    for frame in &chunks {
        decoder.push(frame).unwrap();
    }
    decoder.done().unwrap();
    let decoded = decoder.materialize().unwrap();
    assert!(
        matches!(&decoded.semantic.items()[0].1,Item::Message(m) if m.parts.len()==1 && matches!(&m.parts[0].content,ContentPart::Text(t) if t.as_str().is_empty()))
    );
    assert!(
        matches!(&decoded.semantic.items()[1].1,Item::ToolCall(c) if c.call_id.as_str()=="call_a" && c.arguments.as_raw()==Some("{\"k\":1}"))
    );
    let mut conflict = vendor().event_decoder();
    conflict.push(&chunks[0]).unwrap();
    conflict.push(&chunks[1]).unwrap();
    let mut wrong = chunks[2].clone();
    wrong["choices"][0]["delta"]["tool_calls"][0]["id"] = json!("call_b");
    assert!(conflict.push(&wrong).is_err());
    let mut replay = json!({"messages":[{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"lookup","arguments":"{}"}}]}]});
    assert!(chat::decode_generation(&replay).is_ok());
    replay["messages"][0]["tool_calls"][0]["index"] = json!(7);
    assert!(chat::decode_generation(&replay).is_err());
}
#[test]
fn empty_tool_calls_do_not_parse_message_text_into_a_call() {
    let mut body = response("<tool_call>");
    body["choices"][0]["message"]["tool_calls"] = json!([]);
    body["choices"][0]["finish_reason"] = json!("length");
    let decoded = vendor()
        .decode_response(body.to_string().as_bytes())
        .unwrap();
    assert!(
        !decoded
            .semantic
            .items()
            .iter()
            .any(|(_, item)| matches!(item, Item::ToolCall(_)))
    );
    assert_eq!(
        decoded.semantic.details().incomplete,
        Some(IncompleteReason::MaxOutputTokens)
    );
    let output = client()
        .encode_response(&decoded, &Contract::full())
        .unwrap();
    assert!(output["choices"][0]["message"].get("tool_calls").is_none());
    assert_eq!(output["choices"][0]["message"]["content"], "<tool_call>");
    assert_eq!(output["choices"][0]["finish_reason"], "length");
}
