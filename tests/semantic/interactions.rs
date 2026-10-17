//! Independent Interactions request expectations; no native encoder builds the oracle.
use morphiecore::protocol::{
    interactions::encode_generation_request,
    native::ReplayTarget,
    openai::{Adapter, Dialect, Profile},
};
use morphiecore::{
    protocol::{fidelity::FidelityRecords, interactions::RequestRule, replay_envelope::issuer},
    semantic::{task::generation::*, value::Text},
};
use serde_json::json;

fn interaction_frames() -> Vec<serde_json::Value> {
    vec![
        json!({"event_type":"interaction.created","interaction":{"object":"interaction","model":"gemini-synthetic"}}),
        json!({"event_type":"interaction.status_update","status":"in_progress"}),
        json!({"event_type":"step.start","index":0,"step":{"type":"thought"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":"Plan"}}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"synthetic-thought"}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"step.start","index":1,"step":{"type":"function_call","id":"c1","name":"read","arguments":{},"signature":"synthetic-call"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":"{\"path\":"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":"\"a\"}"}}),
        json!({"event_type":"step.stop","index":1}),
        json!({"event_type":"interaction.completed","interaction":{"status":"requires_action","usage":{
            "total_input_tokens":10,"total_cached_tokens":4,"total_output_tokens":5,
            "total_thought_tokens":7,"total_tool_use_tokens":0,"total_tokens":22}}}),
    ]
}
fn decode_interaction(
    frames: &[serde_json::Value],
    done: bool,
) -> Result<morphiecore::protocol::DecodedResponse, String> {
    use morphiecore::protocol::{interactions::GenerationSseDecoder, stream::SseLimits};
    let mut d = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
    )
    .unwrap();
    let mut wire = frames
        .iter()
        .map(|f| {
            format!(
                "event: {}\ndata: {f}\n\n",
                f["event_type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    if done {
        wire.push_str("event: done\ndata: [DONE]\n\n");
    }
    for chunk in wire.as_bytes().chunks(7) {
        let mut rest = chunk;
        while !rest.is_empty() {
            let (n, _) = d.consume(rest).map_err(|e| e.to_string())?;
            rest = &rest[n..];
        }
    }
    d.finish().map_err(|e| e.to_string())
}

#[test]
fn audit_step_errors_and_unrepresented_lifecycle_facts_cannot_complete() {
    let base = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","model":"gemini-synthetic"}}),
        json!({"event_type":"step.start","index":0,"step":{"type":"model_output","content":[{"type":"text","text":"A"}]}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"interaction.completed","interaction":{"id":"i","model":"gemini-synthetic","status":"completed"}}),
    ];
    assert!(decode_interaction(&base, true).is_ok());
    let mut error = base.clone();
    error[1]["step"]["error"] = json!({"code":7,"message":"denied"});
    assert!(
        decode_interaction(&error, true)
            .unwrap_err()
            .contains("Interactions model_output error")
    );
    let mut cited = base.clone();
    cited[1]["step"]["content"][0]["annotations"] = json!([{
        "type":"url_citation","start_index":0,"end_index":1,"url":"https://example.test/source"}]);
    assert!(
        decode_interaction(&cited, true)
            .unwrap_err()
            .contains("Interactions annotations")
    );
    let mut cited_thought = interaction_frames();
    cited_thought[3]["delta"]["content"]["annotations"] =
        cited[1]["step"]["content"][0]["annotations"].clone();
    assert!(
        decode_interaction(&cited_thought, true)
            .unwrap_err()
            .contains("Interactions annotations")
    );
    cited_thought[3]["delta"]["content"]["annotations"] = json!([]);
    assert!(decode_interaction(&cited_thought, true).is_ok());
    for (key, value) in [
        (
            "steps",
            json!([{"type":"model_output","content":[{"type":"text","text":"B"}]}]),
        ),
        ("continuation_token", json!("synthetic-token")),
        ("service_tier", json!("flex")),
    ] {
        let mut unsupported = base.clone();
        unsupported[3]["interaction"][key] = value.clone();
        assert!(
            decode_interaction(&unsupported, true)
                .unwrap_err()
                .contains(key),
            "{key}"
        );
        unsupported = base.clone();
        unsupported[0]["interaction"][key] = value;
        assert!(
            decode_interaction(&unsupported, true)
                .unwrap_err()
                .contains(key),
            "{key}"
        );
    }
}

#[test]
fn step_stream_preserves_usage_call_arguments_and_both_signatures() {
    let decoded = decode_interaction(&interaction_frames(), true).unwrap();
    assert!(decoded.metadata.id.is_none());
    assert_eq!(
        decoded.semantic.progress(),
        InteractionProgress::AwaitingToolResults
    );
    let items = decoded.semantic.items();
    assert_eq!(items.len(), 3);
    assert!(matches!(&items[0].1, Item::Reasoning(r)
        if matches!(r.parts.as_slice(),[(_,ReasoningContent::Summary(t))] if t.as_str()=="Plan")));
    assert!(matches!(&items[1].1, Item::ToolCall(c)
        if c.call_id.as_str()=="c1" && c.arguments.as_structured().unwrap().value()==&json!({"path":"a"})));
    for (index, expected) in [
        (
            0,
            json!({"anchor":"thought","signature":"synthetic-thought"}),
        ),
        (
            2,
            json!({"anchor":"call","call":"c1","signature":"synthetic-call"}),
        ),
    ] {
        let Item::Reasoning(r) = &items[index].1 else {
            panic!()
        };
        let value = r.replay.as_ref().unwrap();
        assert_eq!(value.format(), ReplayFormat::GoogleInteractionsV1Thought);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(value.replay_token().unwrap()).unwrap(),
            expected
        );
    }
    let usage = decoded.semantic.usage().unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.reasoning_tokens,
            usage.cached_input_tokens,
            usage.total_tokens
        ),
        (Some(10), Some(12), Some(7), Some(4), Some(22))
    );
}

#[test]
fn step_stream_rejects_missing_call_identity_and_unfinished_or_late_data() {
    let truncated = vec![
        json!({"event_type":"interaction.created","interaction":{"object":"interaction","model":"gemini-synthetic"}}),
        json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"interaction.completed","interaction":{"status":"incomplete"}}),
    ];
    let decoded = decode_interaction(&truncated, true).unwrap();
    assert_eq!(decoded.semantic.outcome(), Outcome::Incomplete);
    assert_eq!(
        decoded.semantic.details().incomplete,
        Some(IncompleteReason::Unspecified)
    );
    assert!(decoded.semantic.items().is_empty());
    let mut missing = interaction_frames();
    missing[6]["step"].as_object_mut().unwrap().remove("id");
    assert!(decode_interaction(&missing, true).is_err());
    let frames = interaction_frames();
    assert!(decode_interaction(&frames, false).is_err());
    assert!(decode_interaction(&frames[..9], true).is_err());
    let mut late = frames.clone();
    late.push(frames[1].clone());
    assert!(decode_interaction(&late, true).is_err());
    let mut malformed = frames;
    malformed[3]["delta"]["content"]["text"] = json!(42);
    assert!(decode_interaction(&malformed, true).is_err());
}

fn signed_history(call_anchor: &str, format: ReplayFormat) -> (GenerationRequest, FidelityRecords) {
    let source = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            json!({"model":"public","input":[
                {"role":"user","content":"Read."},
                {"type":"function_call","call_id":"c1","name":"read","arguments":"{}"},
                {"type":"function_call_output","call_id":"c1","output":"done"}
            ]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    let mut fidelity = FidelityRecords::default();
    let mut signed = |n, payload: serde_json::Value, parts| {
        let id = ItemId::scoped(LocalScope::new(7), n);
        let r = ReasoningItem {
            parts,
            status: ItemLifecycle::Completed,
            replay: Some(ReplayValue::final_value(
                format,
                Text::new(payload.to_string(), "synthetic signature", MAX_TEXT_BYTES).unwrap(),
            )),
        };
        fidelity
            .record_replay(id, &r, Some(issuer("synthetic-google", "gemini-synthetic")))
            .unwrap();
        (id, Item::Reasoning(r))
    };
    let thought = signed(
        1,
        json!({"anchor":"thought","signature":"synthetic-thought"}),
        vec![(
            PartId::scoped(LocalScope::new(7), 1),
            ReasoningContent::Summary(Text::new("Plan", "summary", 64).unwrap()),
        )],
    );
    let call = signed(
        2,
        json!({"anchor":"call","call":call_anchor,"signature":"synthetic-call"}),
        vec![],
    );
    let mut items = source.task.semantic.items().to_vec();
    // Chat returns opaque carriers ahead of calls; native anchoring must not use position.
    items.splice(1..1, [thought, call]);
    let request = source
        .task
        .semantic
        .with_message_envelopes(vec![])
        .unwrap()
        .with_items(items)
        .unwrap();
    (request, fidelity)
}

#[test]
fn restores_native_signatures_by_anchor_and_drops_the_other_google_format() {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleInteractionsV1Thought);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"thought","summary":[{"type":"text","text":"Plan"}],"signature":"synthetic-thought"},
            {"type":"function_call","id":"c1","name":"read","arguments":{},"signature":"synthetic-call"},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert!(rules.is_empty());
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleGenerateContentPart);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"model_output","content":[{"type":"text","text":"Plan"}]},
            {"type":"function_call","id":"c1","name":"read","arguments":{}},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert_eq!(
        rules,
        [RequestRule::OmitForeignReplay, RequestRule::ReasoningAsText]
    );
}

#[test]
fn rejects_orphaned_or_wrong_target_signatures() {
    let (request, fidelity) =
        signed_history("missing-call", ReplayFormat::GoogleInteractionsV1Thought);
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    assert!(encode_generation_request(&request, &fidelity, &target).is_err());
    let (request, fidelity) = signed_history("c1", ReplayFormat::GoogleInteractionsV1Thought);
    let other = ReplayTarget::new("synthetic-google", "other-model").unwrap();
    assert!(encode_generation_request(&request, &fidelity, &other).is_err());
}

#[test]
fn projects_stateless_steps_tools_and_controls() {
    let schema = json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false});
    let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(json!({
            "model":"public", "instructions":"Be precise.", "max_output_tokens":128,
            "text":{"format":{"type":"json_schema","name":"answer","schema":schema,"strict":true}},
            "reasoning":{"effort":"medium","summary":"auto"},
            "tools":[{"type":"function","name":"read","strict":false,
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "tool_choice":{"type":"allowed_tools","mode":"auto","tools":[{"type":"function","name":"read"}]},
            "input":[{"role":"user","content":"Read."},
                {"type":"function_call","call_id":"c1","name":"read","arguments":"{\"path\":\"a\"}"},
                {"type":"function_call_output","call_id":"c1","output":"done"}]
        }).to_string().as_bytes()).unwrap();
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (wire, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target).unwrap();
    assert_eq!(
        wire,
        json!({
            "model":"gemini-synthetic","store":false,"stream":true,
            "system_instruction":"Be precise.",
            "response_format":{"type":"text","mime_type":"application/json","schema":schema},
            "input":[{"type":"user_input","content":[{"type":"text","text":"Read."}]},
                {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"}},
                {"type":"function_result","call_id":"c1","name":"read","result":"done"}],
            "tools":[{"type":"function","name":"read",
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "generation_config":{"max_output_tokens":128,"thinking_level":"medium",
                "thinking_summaries":"auto","tool_choice":{"allowed_tools":{"mode":"auto","tools":["read"]}}}
        })
    );
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["interactions.omit-schema-name.v1"]
    );
    let parts = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(json!({"model":"public","input":[
            {"role":"system","content":[{"type":"input_text","text":"A"},{"type":"input_text","text":"B"}]},
            {"role":"user","content":"Hi"}]}).to_string().as_bytes())
        .unwrap();
    let (joined, rules) =
        encode_generation_request(&parts.task.semantic, &parts.task.fidelity, &target).unwrap();
    assert_eq!(joined["system_instruction"], "A\n\nB");
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["interactions.join-instruction-parts.v1"]
    );
    let mut endpoint = morphiecore::topology::catalog::GOOGLE_BINDINGS[0].interactions_endpoint();
    assert!(endpoint.encode_request_with_rules(&request).is_err());
    endpoint.representation = morphiecore::protocol::interactions::TargetCaps {
        semantics: morphiecore::protocol::interactions::representation_contract(),
        ..Default::default()
    }
    .into();
    let prepared = endpoint.encode_request_with_rules(&request).unwrap().0;
    assert_eq!(prepared["response_format"], wire["response_format"]);
    let mut controlled = request.task.semantic.settings().clone();
    controlled.controls.seed = Some(0);
    controlled.controls.stop_sequences =
        vec![morphiecore::semantic::value::Text::new("END", "fixture", 64).unwrap()];
    let controlled = request
        .task
        .semantic
        .clone()
        .with_settings(controlled)
        .unwrap();
    let (wire, _) =
        encode_generation_request(&controlled, &request.task.fidelity, &target).unwrap();
    assert_eq!(
        wire["generation_config"],
        json!({
            "max_output_tokens":128,"thinking_level":"medium","thinking_summaries":"auto",
            "tool_choice":{"allowed_tools":{"mode":"auto","tools":["read"]}},
            "seed":0,"stop_sequences":["END"]
        })
    );
    assert!(
        morphiecore::protocol::interactions::semantic_contract()
            .check(&controlled)
            .is_err()
    );
    assert!(
        matches!(request.task.semantic.output(), OutputConstraint::JsonSchema { schema: s, strict: Some(true), .. } if s.root() == &schema)
    );
    let mut settings = request.task.semantic.settings().clone();
    let morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonSchema {
        schema, ..
    }) = &mut settings.text.format
    else {
        panic!()
    };
    *schema = json!({"type":"string","pattern":"^protected$"}).into();
    let invalid = request
        .task
        .semantic
        .clone()
        .with_settings(settings)
        .unwrap();
    assert!(encode_generation_request(&invalid, &request.task.fidelity, &target).is_err());
}

#[test]
fn rejects_unrepresentable_critical_tool_semantics() {
    let advisory = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            br#"{"model":"public","input":"Read.","text":{"format":{"type":"text"},"verbosity":"low"}}"#,
        )
        .unwrap();
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (wire, rules) =
        encode_generation_request(&advisory.task.semantic, &advisory.task.fidelity, &target)
            .unwrap();
    assert!(wire.get("response_format").is_none());
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["interactions.omit-verbosity.v1"]
    );
    assert_eq!(
        advisory.task.semantic.text_options().verbosity,
        morphiecore::semantic::value::Presence::Value(Verbosity::Low)
    );
    let mut settings = advisory.task.semantic.settings().clone();
    settings.text.format =
        morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonObject);
    let constrained = advisory
        .task
        .semantic
        .clone()
        .with_settings(settings)
        .unwrap();
    assert!(encode_generation_request(&constrained, &advisory.task.fidelity, &target).is_err());
    let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(
            json!({
                "model":"public","input":"Read.",
                "tools":[{"type":"function","name":"read","strict":true,
                    "parameters":{"type":"object","properties":{},"additionalProperties":false}}]
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(
        encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
        )
        .is_err()
    );
}

#[test]
fn media_become_v1_content_and_video_has_no_carrier() {
    let decode = |profile, body: serde_json::Value| {
        Adapter::new(profile, Dialect::Standard, None)
            .decode_request(body.to_string().as_bytes())
            .unwrap()
    };
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let request = decode(
        Profile::Responses,
        json!({"model":"public","tools":[{"type":"function","name":"shot","strict":false,"parameters":{"type":"object"}}],
            "input":[{"role":"user","content":[
                {"type":"input_image","image_url":"https://example.invalid/a.png"},
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID"},
                {"type":"input_text","text":"Compare."}]},
            {"type":"function_call","call_id":"c1","name":"shot","arguments":"{}"},
            {"type":"function_call_output","call_id":"c1","output":[
                {"type":"input_image","image_url":"data:image/png;base64,BAUG"}]}]}),
    );
    let (wire, rules) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[
                {"type":"image","uri":"https://example.invalid/a.png"},
                {"type":"document","data":"AQID","mime_type":"application/pdf"},
                {"type":"text","text":"Compare."}]},
            {"type":"function_call","id":"c1","name":"shot","arguments":{}},
            {"type":"function_result","call_id":"c1","name":"shot","result":[
                {"type":"image","data":"BAUG","mime_type":"image/png"}]}])
    );
    assert!(rules.is_empty());
    let mut named = request.clone();
    let mut items = named.task.semantic.items().to_vec();
    let Item::Message(message) = &mut items[0].1 else {
        panic!()
    };
    let ContentPart::Resource(resource) = &mut message.parts[1].content else {
        panic!()
    };
    let ResourceDescription::File(file) = &mut resource.description else {
        panic!()
    };
    file.filename = Some(Text::new("a.pdf", "test", 32).unwrap());
    named.task.semantic = named.task.semantic.with_items(items).unwrap();
    assert!(
        encode_generation_request(&named.task.semantic, &named.task.fidelity, &target).is_err()
    );
    let video = decode(
        Profile::Chat,
        json!({"model":"public","messages":[{"role":"user","content":[
            {"type":"video_url","video_url":{"url":"https://example.invalid/a.mp4"}}]}]}),
    );
    assert!(
        encode_generation_request(&video.task.semantic, &video.task.fidelity, &target).is_err()
    );
    // System instructions are text, and other targets may downsize an oversized image.
    assert_eq!(
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .decode_request(
                json!({"model":"public","input":[
            {"role":"developer","content":[{"type":"input_image","image_url":"data:image/png;base64,AQID"}]},
            {"role":"user","content":"Describe."}]})
                    .to_string()
                    .as_bytes()
            )
            .unwrap_err(),
        morphiecore::protocol::CodecError::Unsupported("instruction content".into())
    );
    let strict = morphiecore::protocol::anthropic::client::decode_client_request(
        json!({"model":"public","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                "transformations":{"oversized_image":"error"}}]}]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert!(
        encode_generation_request(&strict.task.semantic, &strict.task.fidelity, &target).is_err()
    );
    for tool in [false, true] {
        let uri = "https://generativelanguage.googleapis.com/v1beta/files/synthetic";
        let input = if tool {
            json!([
                {"type":"function_call","call_id":"c1","name":"shot","arguments":"{}"},
                {"type":"function_call_output","call_id":"c1","output":[
                    {"type":"input_image","image_url":uri}]}
            ])
        } else {
            json!([{"role":"user","content":[{"type":"input_image","image_url":uri}]}])
        };
        let private = decode(Profile::Responses, json!({"model":"public","input":input}));
        assert!(
            encode_generation_request(&private.task.semantic, &private.task.fidelity, &target)
                .is_err()
        );
    }
}
