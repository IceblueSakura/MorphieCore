//! Independent generateContent wire expectations: streamed intake and the next request.
use morphiecore::{
    adapter::Request,
    protocol::{
        DecodedResponse,
        google::{GenerationSseDecoder, encode_generation_request},
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile, sse::SseLimits},
    },
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap()
}
fn frames() -> Vec<Value> {
    let meta = |mut v: Value| {
        v["responseId"] = json!("resp-1");
        v["modelVersion"] = json!("gemini-synthetic");
        v
    };
    vec![
        meta(json!({"candidates":[{"content":{"role":"model","parts":[
            {"text":"Plan","thought":true}]},"safetyRatings":[]}]})),
        meta(json!({"candidates":[{"content":{"role":"model","parts":[
            {"text":" more","thought":true,"partMetadata":{"future":1}}]}}]})),
        meta(
            json!({"candidates":[{"content":{"role":"model","parts":[{"text":"Reading."}]}}],
            "futureField":{"nested":[1]}}),
        ),
        meta(json!({"candidates":[{"content":{"role":"model","parts":[
            {"functionCall":{"id":"c1","name":"read","args":{"path":"a.txt"}},"thoughtSignature":"U0lH"},
            {"functionCall":{"id":"c2","name":"read","args":{"path":"b.txt"}}}]},
            "finishReason":"STOP","index":0}],
            "usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":4,
                "candidatesTokenCount":5,"thoughtsTokenCount":7,"totalTokenCount":22,
                "promptTokensDetails":[{"modality":"TEXT","tokenCount":10}]}})),
    ]
}
fn wire(frames: &[Value]) -> Vec<u8> {
    frames
        .iter()
        .map(|f| format!("data: {f}\r\n\r\n"))
        .collect::<String>()
        .into_bytes()
}
fn decode(bytes: &[u8]) -> Result<DecodedResponse, String> {
    let mut d = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        target(),
    )
    .unwrap();
    let mut offset = 0;
    while offset < bytes.len() {
        let (n, _) = d.consume(&bytes[offset..]).map_err(|e| e.to_string())?;
        offset += n;
    }
    d.finish().map_err(|e| e.to_string())
}
fn signature(item: &Item) -> Value {
    let Item::Reasoning(r) = item else {
        panic!("reasoning")
    };
    let value = r.replay.as_ref().unwrap();
    assert_eq!(value.format(), ReplayFormat::GoogleGenerateContentPart);
    serde_json::from_str(value.replay_token().unwrap()).unwrap()
}

#[test]
fn stream_maps_thoughts_text_calls_and_anchored_signatures() {
    let observed = decode(&wire(&frames())).unwrap();
    assert_eq!(observed.metadata.id.as_deref(), Some("resp-1"));
    assert_eq!(observed.metadata.created, None);
    let items = observed.semantic.items();
    assert_eq!(items.len(), 5);
    assert!(matches!(&items[0].1, Item::Reasoning(r)
        if matches!(r.parts.as_slice(), [(_, ReasoningContent::Summary(t))] if t.as_str() == "Plan more")));
    assert_eq!(signature(&items[0].1), json!({"anchor":"thought"}));
    assert!(matches!(&items[1].1, Item::Message(m)
        if m.role == MessageRole::Assistant
            && matches!(&m.parts[0].content, ContentPart::Text(t) if t.as_str() == "Reading.")));
    // Signature-only items trail the calls; their anchor, not position, binds them.
    assert!(matches!(&items[4].1, Item::Reasoning(r) if r.parts.is_empty()));
    assert_eq!(
        signature(&items[4].1),
        json!({"anchor":"call","call":"c1","signature":"U0lH"})
    );
    for (index, (id, path)) in [(2, ("c1", "a.txt")), (3, ("c2", "b.txt"))] {
        assert!(matches!(&items[index].1, Item::ToolCall(c)
            if c.call_id.as_str() == id && c.name.as_str() == "read"
                && c.arguments.as_structured().unwrap().value() == &json!({"path": path})));
    }
    assert_eq!(observed.semantic.outcome(), Outcome::Completed);
    assert_eq!(
        observed.semantic.progress(),
        InteractionProgress::AwaitingToolResults
    );
    let usage = observed.semantic.usage().unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.cached_input_tokens,
            usage.output_tokens,
            usage.reasoning_tokens,
            usage.total_tokens
        ),
        (Some(10), Some(4), Some(12), Some(7), Some(22))
    );
    assert_eq!(usage.input_relation, InputTokenRelation::IncludesCache);
    assert_eq!(
        usage.output_relation,
        OutputTokenRelation::IncludesReasoning
    );
}

#[test]
fn stream_rejects_unidentified_calls_late_chunks_and_unfinished_eof() {
    let mut missing_id = frames();
    missing_id[3]["candidates"][0]["content"]["parts"][1]["functionCall"]
        .as_object_mut()
        .unwrap()
        .remove("id");
    let mut late = frames();
    late.push(late[2].clone());
    let unfinished = &frames()[..3];
    let mut known_type = frames();
    known_type[2]["candidates"][0]["content"]["parts"][0]["text"] = json!(7);
    for bytes in [
        wire(&missing_id),
        wire(&late),
        wire(unfinished),
        wire(&known_type),
    ] {
        assert!(decode(&bytes).is_err());
    }
}

fn next_request(observed: &DecodedResponse) -> GenerationRequest {
    let source: Request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            json!({"model":"gemini","max_output_tokens":1024,
                "reasoning":{"effort":"high","summary":"auto"},
                "tools":[{"type":"function","name":"read","strict":false,
                    "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
                "input":[{"role":"user","content":"Read a and b."}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let result = |n: u64, call: &str, output: &str, error: bool| {
        (
            ItemId::scoped(LocalScope::new(3), n),
            Item::ToolResult(ToolResult {
                call_id: Text::new(call, "call", 64).unwrap(),
                output: output.into(),
                is_error: error.then_some(true),
                execution: None,
                status: None,
                context: Default::default(),
            }),
        )
    };
    ClientManaged::new(source.task.semantic.settings().clone())
        .unwrap()
        .select_request(&source.task.semantic)
        .unwrap()
        .select_response(&observed.semantic)
        .unwrap()
        .append_items(vec![
            result(1, "c1", "A", false),
            result(2, "c2", "missing", true),
        ])
        .unwrap()
        .build(&[])
        .unwrap()
}

#[test]
fn next_request_restores_signatures_on_their_parts_in_any_item_order() {
    let observed = decode(&wire(&frames())).unwrap();
    let request = next_request(&observed);
    let expected = json!({
        "contents":[
            {"role":"user","parts":[{"text":"Read a and b."}]},
            {"role":"model","parts":[
                {"thought":true,"text":"Plan more"},
                {"text":"Reading."},
                {"functionCall":{"id":"c1","name":"read","args":{"path":"a.txt"}},"thoughtSignature":"U0lH"},
                {"functionCall":{"id":"c2","name":"read","args":{"path":"b.txt"}}}]},
            {"role":"user","parts":[
                {"functionResponse":{"id":"c1","name":"read","response":{"output":"A"}}},
                {"functionResponse":{"id":"c2","name":"read","response":{"error":"missing"}}}]}],
        "tools":[{"functionDeclarations":[{"name":"read",
            "parametersJsonSchema":{"type":"object","properties":{"path":{"type":"string"}}}}]}],
        "generationConfig":{"maxOutputTokens":1024,
            "thinkingConfig":{"thinkingLevel":"HIGH","includeThoughts":true}}});
    let (body, rules) = encode_generation_request(&request, &observed.fidelity, &target()).unwrap();
    assert_eq!(body, expected);
    assert!(rules.is_empty());
    // Chat delivers reasoning ahead of the message; the anchor, not the position, binds it.
    let mut items = request.items().to_vec();
    let signed = items.remove(5);
    items.insert(1, signed);
    let hoisted = request
        .clone()
        .with_message_envelopes(vec![])
        .unwrap()
        .with_items(items)
        .unwrap();
    let (body, _) = encode_generation_request(&hoisted, &observed.fidelity, &target()).unwrap();
    assert_eq!(body, expected);
    let other = ReplayTarget::new("synthetic-google", "gemini-other").unwrap();
    assert!(encode_generation_request(&request, &observed.fidelity, &other).is_err());
}
