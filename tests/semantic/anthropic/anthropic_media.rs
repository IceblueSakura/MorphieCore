//! Independent native image/document oracles; inert sources, no fetch or file API.
#[test]
fn native_image_count_includes_tool_result_images() {
    use morphiecore::protocol::anthropic::messages::Profile;
    use serde_json::json;
    let image = json!({"type":"image","source":{
            "type":"base64","media_type":"image/png","data":"AQID"}});
    let mut wire = json!({"model":"synthetic","max_tokens":32,"messages":[
            {"role":"user","content":vec![image.clone();599]},
            {"role":"assistant","content":[
                {"type":"tool_use","id":"c","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c",
                "content":[image.clone()]}]}]});
    let profile = Profile::AdaptiveTextTools;
    let request = profile.decode_request(wire.to_string().as_bytes()).unwrap();
    assert!(profile.encode_request(&request).is_ok());
    wire["messages"][2]["content"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(image);
    assert!(profile.decode_request(wire.to_string().as_bytes()).is_err());
}

#[test]
fn native_image_size_counts_encoded_base64_bytes() {
    use morphiecore::protocol::anthropic::messages::{
        Block, Media, MessageContent, Profile, Source,
    };
    use serde_json::json;
    let profile = Profile::AdaptiveTextTools;
    let mut request = profile
        .decode_request(
            json!({"model":"synthetic","max_tokens":32,"messages":[
                {"role":"user","content":[{"type":"image","source":{
                    "type":"base64","media_type":"image/png","data":"AQID"}}]}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
        panic!("image blocks")
    };
    blocks[0].content = Block::Media(Media::Image {
        source: Source::Base64 {
            media_type: "image/png".into(),
            data: "AAAA".repeat(10_000_000 / 4),
        },
        reject_oversized: false,
    });
    assert!(profile.encode_request(&request).is_ok());
    let MessageContent::Blocks(blocks) = &mut request.messages[0].content else {
        unreachable!()
    };
    let Block::Media(Media::Image {
        source: Source::Base64 { data, .. },
        ..
    }) = &mut blocks[0].content
    else {
        unreachable!()
    };
    data.push_str("AAAA");
    assert!(profile.encode_request(&request).is_err());
}
use morphiecore::{
    protocol::{anthropic::messages::Profile, native::ReplayTarget},
    semantic::task::generation::*,
    topology::catalog,
};
use serde_json::{Value, json};

fn target() -> ReplayTarget {
    ReplayTarget::new("synthetic-provider", "synthetic-model").unwrap()
}
fn messages() -> Value {
    json!([
        {"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                "transformations":{"oversized_image":"error"}},
            {"type":"image","source":{"type":"url","url":"https://example.invalid/a.jpg"}},
            {"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"BAUG"},"title":"Report"},
            {"type":"document","source":{"type":"text","media_type":"text/plain","data":"notes"}},
            {"type":"text","text":"Compare."}]},
        {"role":"assistant","content":[
            {"type":"tool_use","id":"toolu_1","name":"shot","input":{}}]},
        {"role":"user","content":[
            {"type":"tool_result","tool_use_id":"toolu_1","content":[
                {"type":"text","text":"screen"},
                {"type":"image","source":{"type":"base64","media_type":"image/webp","data":"BwgJ"},
                    "transformations":{"oversized_image":"error"}}]}]}
    ])
}
fn view<'a>(request: &'a GenerationRequest, resource: &'a ResourceUse) -> ResourceView<'a> {
    resource.media(request.resources()).unwrap()
}

#[test]
fn images_and_documents_map_to_inert_resources_and_back() {
    let raw = json!({"model":"synthetic-model","max_tokens":64,"messages":messages()});
    let p = Profile::AdaptiveTextTools;
    let decoded = p
        .decode_generation_request(
            &serde_json::to_vec(&raw).unwrap(),
            LocalScope::new(1),
            &target(),
        )
        .unwrap();
    let semantic = &decoded.semantic;
    let needs = GenerationRequirements::derive(semantic);
    assert_eq!(
        (
            needs.image_inputs,
            needs.file_inputs,
            needs.tool_result_media
        ),
        (3, 2, 1)
    );
    let inputs: Vec<_> = semantic
        .items()
        .iter()
        .filter_map(|(_, item)| match item {
            Item::Message(m) => match &m.parts[0].content {
                ContentPart::Resource(r) => Some(view(semantic, r)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let inline = |media: &str, data: &str| -> bool {
        inputs.iter().any(|v| {
            matches!(v.location, ResourceLocation::Inline { media_type, data_base64 }
                    if media_type.as_str() == media && data_base64.as_str() == data)
        })
    };
    assert_eq!(inputs.len(), 4);
    assert_eq!(inputs[0].kind(), ResourceKind::Image);
    assert!(inline("image/png", "AQID"));
    assert!(
        matches!(inputs[1].location, ResourceLocation::Url(u) if u.as_str() == "https://example.invalid/a.jpg")
    );
    assert_eq!(inputs[2].kind(), ResourceKind::File);
    // A document title is the model-visible file name.
    assert!(
        matches!(inputs[2].description, ResourceDescription::File(FileDescription {
            filename: Some(name), detail: None }) if name.as_str() == "Report")
    );
    assert!(inline("application/pdf", "BAUG"));
    // A text document is an inline text/plain file: base64("notes").
    assert!(inline("text/plain", "bm90ZXM="));
    let Item::ToolResult(result) = &semantic.items()[6].1 else {
        panic!("tool result")
    };
    let ToolOutput::Parts(parts) = &result.output else {
        panic!("parts")
    };
    let InputPart::Resource(shot) = &parts[1].content else {
        panic!("tool image")
    };
    assert_eq!(view(semantic, shot).kind(), ResourceKind::Image);
    let encoded = p
        .encode_generation_request(semantic, &decoded.fidelity, &target())
        .unwrap();
    assert_eq!(encoded.value["messages"], messages());
    assert!(encoded.rules.is_empty());
}

#[test]
fn unrepresentable_sources_and_uncalibrated_options_fail_without_losing_filenames() {
    for block in [
        json!({"type":"image","source":{"type":"file","file_id":"file_synthetic"}}),
        json!({"type":"document","source":{"type":"content","content":"x"}}),
        json!({"type":"document","context":"private","source":{"type":"url","url":"https://example.invalid/a.pdf"}}),
        json!({"type":"image","source":{"type":"base64","media_type":"image/bmp","data":"AQID"}}),
        // Citations are deferred; dropping one would lose a known field silently.
        json!({"type":"text","text":"x","citations":[{"type":"char_location","cited_text":"x",
                "document_index":0,"document_title":null,"start_char_index":0,"end_char_index":1}]}),
    ] {
        let raw =
            json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[block]}]});
        assert!(
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                &serde_json::to_vec(&raw).unwrap()
            )
            .is_err()
        );
    }
    let uncited = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"text","text":"x","citations":null}]}]});
    assert!(
        morphiecore::protocol::anthropic::messages::client::decode_client_request(
            &serde_json::to_vec(&uncited).unwrap()
        )
        .is_ok()
    );
    use morphiecore::protocol::openai::{Adapter, Dialect, Profile as Client};
    let endpoint = catalog::MESSAGES_BINDINGS[0].endpoint();
    let chat = Adapter::new(Client::Chat, Dialect::Standard, None);
    let decode = |content: Value| {
        let mut request = chat
            .decode_client_request(
                &serde_json::to_vec(
                    &json!({"model":"m","messages":[{"role":"user","content":content}]}),
                )
                .unwrap(),
            )
            .unwrap();
        // The Go binding needs a session grouping.
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let request = decode(json!([
            {"type":"image_url","image_url":{"url":"https://example.invalid/a.png","detail":"low"}}]));
    assert!(endpoint.encode_request_with_rules(&request).is_err());
    let mut context = decode(json!("hello"));
    let mut settings = context.task.semantic.settings().clone();
    settings.reasoning = morphiecore::semantic::task::generation::ReasoningRequest::present(None);
    settings.reasoning.context = morphiecore::semantic::value::Presence::Value(
        morphiecore::semantic::task::generation::ReasoningContext::CurrentTurn,
    );
    context.task.semantic = context.task.semantic.with_settings(settings).unwrap();
    assert!(endpoint.encode_request_with_rules(&context).is_err());
    let named = decode(json!([{"type":"file","file":{
            "file_data":"data:application/pdf;base64,AQID","filename":"report.pdf"}}]));
    let (wire, rules) = endpoint.encode_request_with_rules(&named).unwrap();
    assert_eq!(
        wire["messages"][0]["content"],
        json!([{"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"AQID"},"title":"report.pdf"}])
    );
    // The Chat source has no cap; Messages requires one and records the default.
    assert_eq!(wire["max_tokens"], 32_000);
    assert_eq!(
        rules,
        [
            "messages.default-max-tokens.v1",
            "messages.default-automatic-cache.v1"
        ]
    );
    // A later developer message becomes a mid-conversation system message on the
    // reviewed Haiku binding; the profile default keeps admission closed.
    let ordered = |messages: Value| {
        let mut request = chat
            .decode_client_request(
                &serde_json::to_vec(&json!({"model":"m","messages":messages})).unwrap(),
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let later = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."}]));
    let mut closed = endpoint.clone();
    closed.representation = morphiecore::protocol::anthropic::messages::TargetCaps::new(
        Profile::AdaptiveTextTools,
        morphiecore::protocol::anthropic::messages::Dialect::OpenCodeGo,
    )
    .into();
    assert!(closed.encode_request_with_rules(&later).is_err());
    let widened = endpoint.clone();
    let (wire, rules) = widened.encode_request_with_rules(&later).unwrap();
    assert_eq!(
        wire["messages"],
        json!([{"role":"user","content":[{"type":"text","text":"go"}]},
                {"role":"system","content":[{"type":"text","text":"Be brief."}]}])
    );
    assert!(rules.contains(&"messages.developer-as-system.v1"));
    let between = ordered(json!([{"role":"user","content":"go"},
            {"role":"developer","content":"Be brief."},{"role":"user","content":"again"}]));
    assert!(widened.encode_request_with_rules(&between).is_err());
    // Messages has no audio or video carrier, and its system prompt is text.
    let audio =
        decode(json!([{"type":"input_audio","input_audio":{"data":"AQID","format":"wav"}}]));
    assert!(endpoint.encode_request_with_rules(&audio).is_err());
    let video =
        decode(json!([{"type":"video_url","video_url":{"url":"data:video/mp4;base64,AQID"}}]));
    assert!(endpoint.encode_request_with_rules(&video).is_err());
    assert_eq!(
            Adapter::new(Client::Responses, Dialect::Standard, None)
                .decode_client_request(
                    json!({"model":"m","input":[
                    {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
                    {"role":"user","content":"Describe."}]})
                    .to_string()
                    .as_bytes(),
                )
                .unwrap_err(),
            morphiecore::protocol::CodecError::Unsupported("instruction content".into())
        );
    // `downsize` is the default; `error` survives only on a Messages target.
    let messages = |oversized: &str| {
        let raw = json!({"model":"m","max_tokens":8,"messages":[{"role":"user","content":[
                {"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"},
                    "transformations":{"oversized_image":oversized}}]}]});
        let mut request =
            morphiecore::protocol::anthropic::messages::client::decode_client_request(
                &serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
        request.conversation = Some(
            morphiecore::semantic::context::ConversationContext::conversation("synthetic").unwrap(),
        );
        request
    };
    let image =
        json!({"type":"image","source":{"type":"url","url":"https://example.invalid/a.png"}});
    let (wire, _) = endpoint
        .encode_request_with_rules(&messages("downsize"))
        .unwrap();
    assert_eq!(wire["messages"][0]["content"], json!([image]));
    let full = morphiecore::protocol::openai::TargetCaps::full();
    assert!(
        chat.encode_request(&messages("downsize"), "synthetic", &full)
            .is_ok()
    );
    let strict = messages("error");
    let (wire, _) = endpoint.encode_request_with_rules(&strict).unwrap();
    let mut expected = image;
    expected["transformations"] = json!({"oversized_image":"error"});
    assert_eq!(wire["messages"][0]["content"], json!([expected]));
    assert!(chat.encode_request(&strict, "synthetic", &full).is_err());
}
