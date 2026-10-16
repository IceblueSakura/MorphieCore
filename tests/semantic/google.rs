//! Independent generateContent wire expectations: streamed intake and the next request.
use morphiecore::{
    adapter::Request,
    protocol::{
        DecodedResponse,
        google::{GenerationSseDecoder, encode_generation_request},
        native::ReplayTarget,
        openai::{Adapter, Dialect, Profile},
        stream::SseLimits,
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
    let empty = decode(&wire(&[json!({
        "responseId":"empty","modelVersion":"gemini-synthetic",
        "candidates":[{"content":{"role":"model","parts":[
            {"text":"","thoughtSignature":"U0lH"}]},"finishReason":"STOP"}]
    })]))
    .unwrap();
    assert!(
        matches!(&empty.semantic.items()[0].1, Item::Message(message)
        if matches!(&message.parts[0].content, ContentPart::Text(text) if text.as_str().is_empty()))
    );
    let history = GenerationRequest::new(
        empty.semantic.items().to_vec(),
        GenerationControls::default(),
    )
    .unwrap()
    .with_message_envelopes(empty.semantic.message_envelopes().to_vec())
    .unwrap();
    let (body, _) = encode_generation_request(&history, &empty.fidelity, &target()).unwrap();
    assert_eq!(
        body["contents"][0]["parts"],
        json!([{"text":"","thoughtSignature":"U0lH"}])
    );
    let missing = history
        .retain_items(|_, item| !matches!(item, Item::Message(_)))
        .unwrap();
    assert!(encode_generation_request(&missing, &empty.fidelity, &target()).is_err());

    let partial = decode(&wire(&[json!({
        "responseId":"partial","modelVersion":"gemini-synthetic",
        "candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"partial"}]},
            "finishReason":"MAX_TOKENS"}],
        "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}
    })]))
    .unwrap();
    assert_eq!(partial.semantic.outcome(), Outcome::Incomplete);
    assert_eq!(partial.semantic.progress(), InteractionProgress::Unreported);
    assert_eq!(
        partial.semantic.details().incomplete,
        Some(IncompleteReason::MaxOutputTokens)
    );
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
    let mut reported = frames();
    reported[2]["candidates"][0]["logprobsResult"] = json!({
        "chosenCandidates":[{"token":"Reading.","tokenId":42,"logProbability":-0.25}],
        "topCandidates":[{"candidates":[{"token":"Reading.","tokenId":42,"logProbability":-0.25}]}]
    });
    let reported = decode(&wire(&reported)).unwrap();
    let Item::Message(message) = &reported.semantic.items()[1].1 else {
        panic!()
    };
    let ContentPart::Text(content) = &message.parts[0].content else {
        panic!()
    };
    let probabilities = content.logprobs().value().unwrap();
    assert_eq!(probabilities[0].token.as_str(), "Reading.");
    assert_eq!(probabilities[0].logprob.as_f64(), Some(-0.25));
    assert_eq!(probabilities[0].bytes, None);
    assert_eq!(
        probabilities[0].top_logprobs.as_ref().unwrap()[0]
            .logprob
            .as_ref()
            .unwrap()
            .as_f64(),
        Some(-0.25)
    );
    // A signature ends its thought; the next signed thought has another owner.
    let mut signed = frames();
    signed[0]["candidates"][0]["content"]["parts"][0]["thoughtSignature"] = json!("U0lHMQ==");
    signed[1]["candidates"][0]["content"]["parts"][0]["thoughtSignature"] = json!("U0lHMg==");
    let observed = decode(&wire(&signed)).unwrap();
    assert_eq!(
        signature(&observed.semantic.items()[0].1),
        json!({"anchor":"thought","signature":"U0lHMQ=="})
    );
    assert_eq!(
        signature(&observed.semantic.items()[1].1),
        json!({"anchor":"thought","signature":"U0lHMg=="})
    );
    let (encoded, _) =
        encode_generation_request(&next_request(&observed), &observed.fidelity, &target()).unwrap();
    assert_eq!(
        &encoded["contents"][1]["parts"].as_array().unwrap()[..2],
        &[
            json!({"thought":true,"text":"Plan","thoughtSignature":"U0lHMQ=="}),
            json!({"thought":true,"text":" more","thoughtSignature":"U0lHMg=="}),
        ]
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
    let mut probabilities = frames();
    probabilities[2]["candidates"][0]["logprobsResult"] = json!({
        "chosenCandidates":[{"token":"other text","logProbability":-0.25}]
    });
    for bytes in [
        wire(&missing_id),
        wire(&late),
        wire(unfinished),
        wire(&known_type),
        wire(&probabilities),
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
                call_id: Some(Text::new(call, "call", 64).unwrap()),
                name: None,
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
        .build()
        .unwrap()
}

#[test]
fn next_request_restores_signatures_on_their_parts_in_any_item_order() {
    let tail = decode(&wire(&[
        json!({"responseId":"tail","modelVersion":"gemini-synthetic",
            "candidates":[{"content":{"role":"model","parts":[{"text":"A"}]}}]}),
        json!({"responseId":"tail","modelVersion":"gemini-synthetic",
            "candidates":[{"content":{"role":"model","parts":[
                {"text":"","thoughtSignature":"U0lH"}]},"finishReason":"STOP"}]}),
    ]))
    .unwrap();
    let source = client(Profile::Responses, json!({"model":"m","input":"go"}));
    let history = ClientManaged::new(source.task.semantic.settings().clone())
        .unwrap()
        .select_request(&source.task.semantic)
        .unwrap()
        .select_response(&tail.semantic)
        .unwrap()
        .build()
        .unwrap();
    let (body, _) = encode_generation_request(&history, &tail.fidelity, &target()).unwrap();
    assert_eq!(
        body["contents"][1]["parts"],
        json!([{"text":"A","thoughtSignature":"U0lH"}])
    );
    let mut edited = history.items().to_vec();
    let Item::Message(message) = &mut edited[1].1 else {
        panic!("signed text")
    };
    message.parts[0].content = ContentPart::Text(Text::new("B", "edited", 64).unwrap().into());
    let edited = history.with_items(edited).unwrap();
    assert!(encode_generation_request(&edited, &tail.fidelity, &target()).is_err());

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
    let mut settings = request.settings().clone();
    settings.text = TextOptions {
        presence: true,
        format: morphiecore::semantic::value::Presence::Value(OutputConstraint::Text),
        verbosity: morphiecore::semantic::value::Presence::Value(Verbosity::Low),
    };
    let advisory = request.clone().with_settings(settings.clone()).unwrap();
    let (body, rules) =
        encode_generation_request(&advisory, &observed.fidelity, &target()).unwrap();
    assert_eq!(body, expected);
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["gemini.omit-verbosity.v1"]
    );
    assert_eq!(advisory.settings(), &settings);
    settings.text.format =
        morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonObject);
    let constrained = request.clone().with_settings(settings).unwrap();
    assert!(encode_generation_request(&constrained, &observed.fidelity, &target()).is_err());
    let schema = json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false});
    let source = client(
        Profile::Chat,
        json!({
            "model":"public","messages":[{"role":"user","content":"go"}],
            "response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":schema,"strict":true}},
            "logprobs":true,"top_logprobs":1,
            "tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"},"strict":false}}],
            "tool_choice":{"type":"allowed_tools","allowed_tools":{"mode":"required","tools":[{"type":"function","function":{"name":"read"}}]}}
        }),
    );
    assert!(
        matches!(source.task.semantic.output(), OutputConstraint::JsonSchema { schema: s, strict: Some(true), .. } if s.root() == &schema)
    );
    let (body, rules) =
        encode_generation_request(&source.task.semantic, &source.task.fidelity, &target()).unwrap();
    assert_eq!(
        body["generationConfig"],
        json!({
            "responseMimeType":"application/json","responseJsonSchema":schema,
            "responseLogprobs":true,"logprobs":1
        })
    );
    assert_eq!(
        body["toolConfig"],
        json!({"functionCallingConfig":{"mode":"ANY","allowedFunctionNames":["read"]}})
    );
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["gemini.omit-schema-name.v1"]
    );
    let mut endpoint = morphiecore::topology::catalog::GOOGLE_BINDINGS[0].endpoint();
    assert!(endpoint.encode_request_with_rules(&source).is_err());
    endpoint.representation = morphiecore::protocol::google::TargetCaps {
        semantics: morphiecore::protocol::google::representation_contract(),
        ..Default::default()
    }
    .into();
    assert_eq!(endpoint.encode_request_with_rules(&source).unwrap().0, body);
    let mut settings = source.task.semantic.settings().clone();
    let morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonSchema {
        schema, ..
    }) = &mut settings.text.format
    else {
        panic!()
    };
    *schema = json!({"oneOf":[{"type":"object"},{"type":"object"}]}).into();
    let invalid = source
        .task
        .semantic
        .clone()
        .with_settings(settings)
        .unwrap();
    assert!(encode_generation_request(&invalid, &source.task.fidelity, &target()).is_err());
}

fn client(profile: Profile, body: Value) -> Request {
    Adapter::new(profile, Dialect::Standard, None)
        .decode_request(body.to_string().as_bytes())
        .unwrap()
}

#[test]
fn user_media_become_inline_or_file_data_parts() {
    let request = client(
        Profile::Chat,
        json!({"model":"gemini","messages":[{"role":"user","content":[
            {"type":"image_url","image_url":{"url":"https://example.invalid/a.png"}},
            {"type":"image_url","image_url":{"url":"data:image/png;base64,BwgJ"}},
            {"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}},
            {"type":"video_url","video_url":{"url":"data:video/mp4;base64,BAUG"}},
            {"type":"text","text":"Describe."}]}]}),
    );
    let (body, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target())
            .unwrap();
    assert_eq!(
        body["contents"],
        json!([{"role":"user","parts":[
            {"fileData":{"fileUri":"https://example.invalid/a.png"}},
            {"inlineData":{"mimeType":"image/png","data":"BwgJ"}},
            {"inlineData":{"mimeType":"audio/wav","data":"AQID"}},
            {"inlineData":{"mimeType":"video/mp4","data":"BAUG"}},
            {"text":"Describe."}]}])
    );
    assert!(rules.is_empty());
    for url in [
        "https://GENERATIVELANGUAGE.GOOGLEAPIS.COM./v1beta/%66iles/synthetic",
        "data:image/gif;base64,AQID",
    ] {
        let invalid = client(
            Profile::Chat,
            json!({"model":"gemini","messages":[{"role":"user","content":[
                {"type":"image_url","image_url":{"url":url}}]}]}),
        );
        assert!(
            encode_generation_request(&invalid.task.semantic, &invalid.task.fidelity, &target())
                .is_err()
        );
    }
}

#[test]
fn tool_media_ride_inline_in_the_function_response() {
    let input = |image: &str| {
        json!({"model":"gemini","input":[
            {"type":"function_call","call_id":"c1","name":"shot","arguments":"{}"},
            {"type":"function_call_output","call_id":"c1","output":[
                {"type":"input_text","text":"screen"},
                {"type":"input_image","image_url":image}]}],
            "tools":[{"type":"function","name":"shot","strict":false,"parameters":{"type":"object"}}]})
    };
    let request = client(Profile::Responses, input("data:image/png;base64,AQID"));
    let (body, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target())
            .unwrap();
    assert_eq!(
        body["contents"][1],
        json!({"role":"user","parts":[{"functionResponse":{"id":"c1","name":"shot",
            "response":{"output":"screen"},
            "parts":[{"inlineData":{"mimeType":"image/png","data":"AQID"}}]}}]})
    );
    assert!(rules.is_empty());
    let mut detailed = input("data:image/png;base64,AQID");
    detailed["input"][1]["output"][1]["detail"] = json!("auto");
    let detailed = client(Profile::Responses, detailed);
    assert!(
        encode_generation_request(&detailed.task.semantic, &detailed.task.fidelity, &target())
            .is_err()
    );
    let named = client(
        Profile::Responses,
        json!({"model":"gemini","input":[
        {"role":"user","content":[{"type":"input_file","filename":"report.pdf",
        "file_data":"data:application/pdf;base64,AQID"}]}]}),
    );
    assert!(
        encode_generation_request(&named.task.semantic, &named.task.fidelity, &target()).is_err()
    );
    // FunctionResponsePart has no fileData.
    let remote = client(Profile::Responses, input("https://example.invalid/a.png"));
    assert!(
        encode_generation_request(&remote.task.semantic, &remote.task.fidelity, &target()).is_err()
    );
    let unsupported = client(Profile::Responses, input("data:image/gif;base64,AQID"));
    assert!(
        encode_generation_request(
            &unsupported.task.semantic,
            &unsupported.task.fidelity,
            &target()
        )
        .is_err()
    );
    // Function responses admit only the documented image, PDF and plain-text types.
    let part = |part: Value| {
        let mut body = input("data:image/png;base64,AQID");
        body["input"][1]["output"] = json!([part]);
        client(Profile::Responses, body)
    };
    let pdf = part(json!({"type":"input_file","file_data":"data:application/pdf;base64,BAUG"}));
    let (body, _) =
        encode_generation_request(&pdf.task.semantic, &pdf.task.fidelity, &target()).unwrap();
    assert_eq!(
        body["contents"][1]["parts"][0]["functionResponse"]["parts"],
        json!([{"inlineData":{"mimeType":"application/pdf","data":"BAUG"}}])
    );
    let video = part(json!({"type":"input_video","video_url":"data:video/mp4;base64,BAUG"}));
    assert!(
        encode_generation_request(&video.task.semantic, &video.task.fidelity, &target()).is_err()
    );
}

#[test]
fn instruction_media_and_strict_image_sizes_have_no_gemini_carrier() {
    let instruction = client(
        Profile::Responses,
        json!({"model":"gemini","input":[
            {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
            {"role":"user","content":"Describe."}]}),
    );
    let messages = |oversized: &str| {
        let raw = json!({"model":"gemini","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                "transformations":{"oversized_image":oversized}}]}]});
        morphiecore::protocol::anthropic::client::decode_client_request(raw.to_string().as_bytes())
            .unwrap()
    };
    for (request, ok) in [
        (instruction, false),
        (messages("downsize"), true),
        (messages("error"), false),
    ] {
        let result =
            encode_generation_request(&request.task.semantic, &request.task.fidelity, &target());
        assert_eq!(result.is_ok(), ok);
    }
}
