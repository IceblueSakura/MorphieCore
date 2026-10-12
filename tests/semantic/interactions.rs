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
        Some(IncompleteReason::MaxOutputTokens)
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
    let request = Adapter::new(Profile::Responses, Dialect::Standard, None)
        .decode_request(json!({
            "model":"public", "instructions":"Be precise.", "max_output_tokens":128,
            "reasoning":{"effort":"medium","summary":"auto"},
            "tools":[{"type":"function","name":"read","strict":false,
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "tool_choice":"required",
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
            "input":[{"type":"user_input","content":[{"type":"text","text":"Read."}]},
                {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"}},
                {"type":"function_result","call_id":"c1","name":"read","result":"done"}],
            "tools":[{"type":"function","name":"read",
                "parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "generation_config":{"max_output_tokens":128,"thinking_level":"medium",
                "thinking_summaries":"auto","tool_choice":"any"}
        })
    );
    assert!(rules.is_empty());
}

#[test]
fn rejects_unrepresentable_critical_tool_semantics() {
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
                {"type":"input_file","file_data":"data:application/pdf;base64,AQID","filename":"a.pdf"},
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
    assert_eq!(rules, [RequestRule::OmitMediaOptions]);
    let video = decode(
        Profile::Chat,
        json!({"model":"public","messages":[{"role":"user","content":[
            {"type":"video_url","video_url":{"url":"https://example.invalid/a.mp4"}}]}]}),
    );
    assert!(
        encode_generation_request(&video.task.semantic, &video.task.fidelity, &target).is_err()
    );
}
