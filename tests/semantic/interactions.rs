//! Independent Interactions request expectations; no native encoder builds the oracle.
use morphiecore::protocol::{
    google::interactions::encode_generation_request,
    native::ReplayTarget,
    openai::{Adapter, Dialect, Profile},
};
use morphiecore::{
    protocol::{
        fidelity::FidelityRecords, google::interactions::RequestRule, replay_envelope::issuer,
    },
    semantic::{
        task::generation::*,
        value::{Presence, Text},
    },
};
use serde_json::json;

fn interaction_frames() -> Vec<serde_json::Value> {
    vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","status":"in_progress","object":"interaction","model":"gemini-synthetic"}}),
        json!({"event_type":"interaction.status_update","interaction_id":"i","status":"in_progress"}),
        json!({"event_type":"step.start","index":0,"step":{"type":"thought"}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_summary","content":{"type":"text","text":"Plan"}}}),
        json!({"event_type":"step.delta","index":0,"delta":{"type":"thought_signature","signature":"synthetic-thought"}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"step.start","index":1,"step":{"type":"function_call","id":"c1","name":"read","arguments":{}}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":"{\"path\":"}}),
        json!({"event_type":"step.delta","index":1,"delta":{"type":"arguments_delta","arguments":"\"a\"}"}}),
        json!({"event_type":"step.stop","index":1}),
        json!({"event_type":"interaction.completed","interaction":{"id":"i","status":"requires_action","usage":{
            "total_input_tokens":10,"total_cached_tokens":4,"total_output_tokens":5,
            "total_thought_tokens":7,"total_tool_use_tokens":0,"total_tokens":22}}}),
    ]
}
fn decode_interaction(
    frames: &[serde_json::Value],
    done: bool,
) -> Result<morphiecore::protocol::DecodedResponse, String> {
    use morphiecore::protocol::{google::interactions::GenerationSseDecoder, stream::SseLimits};
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
fn empty_chat_carrier_is_not_an_empty_text_part() {
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    for (content, count, projected) in [
        (json!(null), 0, json!([])),
        (json!(""), 1, json!([{"type":"text","text":""}])),
    ] {
        let request = json!({"model":"m","messages":[
            {"role":"user","content":"Read."},
            {"role":"assistant","content":content,"tool_calls":[
                {"id":"c","type":"function","function":{"name":"read","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"c","content":"ok"}]});
        let decoded = adapter
            .decode_request(request.to_string().as_bytes())
            .unwrap()
            .task;
        let message = decoded
            .semantic
            .items()
            .iter()
            .find_map(|(_, item)| match item {
                Item::Message(message) if message.role == MessageRole::Assistant => Some(message),
                _ => None,
            })
            .unwrap();
        assert_eq!(message.parts.len(), count);
        if count == 1 {
            assert!(
                matches!(&message.parts[0].content, ContentPart::Text(text) if text.as_str().is_empty())
            );
        }
        let (wire, _) =
            encode_generation_request(&decoded.semantic, &decoded.fidelity, &target).unwrap();
        assert_eq!(
            wire["input"],
            json!([
                {"type":"user_input","content":[{"type":"text","text":"Read."}]},
                {"type":"model_output","content":projected},
                {"type":"function_call","id":"c","name":"read","arguments":{}},
                {"type":"function_result","call_id":"c","name":"read","result":"ok"},
            ])
        );
    }
}

#[test]
fn empty_assistant_envelopes_cannot_disappear_behind_complete_member_coverage() {
    let user = ItemId::new(1);
    let source = GenerationRequest::new(
        vec![(
            user,
            Item::Message(Message {
                participant: None,
                role: MessageRole::User,
                status: ItemLifecycle::Completed,
                phase: None,
                parts: vec![Part::new(
                    PartId::new(1),
                    ContentPart::Text(Text::new("hello", "fixture", 64).unwrap().into()),
                )],
            }),
        )],
        Default::default(),
    )
    .unwrap();
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    for before in [None, Some(user)] {
        let request = source
            .clone()
            .with_message_envelopes(vec![
                MessageEnvelope::new(
                    GroupId::new(LocalScope::ROOT, 1),
                    MessageEnvelopeRole::User,
                    vec![user],
                )
                .unwrap(),
                MessageEnvelope::empty(
                    GroupId::new(LocalScope::ROOT, 2),
                    MessageEnvelopeRole::Assistant,
                    before,
                ),
            ])
            .unwrap();
        request.validate().unwrap();
        assert_eq!(
            encode_generation_request(&request, &FidelityRecords::default(), &target),
            Err(morphiecore::protocol::CodecError::Unsupported(
                "Interactions semantic carrier".into()
            ))
        );
    }
}

#[test]
fn lifecycle_distinguishes_optional_fields_from_invalid_or_unsupported_values() {
    let frames = interaction_frames();
    assert!(decode_interaction(&frames, true).is_ok());
    for index in [0, 1, frames.len() - 1] {
        let key = if index == 1 { "interaction_id" } else { "id" };
        for value in [None, Some(json!(null)), Some(json!(17)), Some(json!(""))] {
            let identity_omission = value.is_none();
            let mut changed = frames.clone();
            let owner = if index == 1 {
                changed[index].as_object_mut().unwrap()
            } else {
                changed[index]["interaction"].as_object_mut().unwrap()
            };
            match value {
                Some(value) => {
                    owner.insert(key.into(), value);
                }
                None => {
                    owner.remove(key);
                }
            }
            if identity_omission {
                assert!(decode_interaction(&changed, true).is_ok());
                continue;
            }
            assert!(
                decode_interaction(&changed, true)
                    .unwrap_err()
                    .contains(key)
            );
        }
        if index != 0 {
            let mut changed = frames.clone();
            if index == 1 {
                changed[index][key] = json!("foreign");
                changed[index]["usage"] = json!({"total_tokens":999});
            } else {
                changed[index]["interaction"][key] = json!("foreign");
            }
            assert!(
                decode_interaction(&changed, true)
                    .unwrap_err()
                    .contains("interaction identity changed")
            );
        }
        for value in [
            None,
            Some(json!(null)),
            Some(json!(17)),
            Some(json!("invented")),
        ] {
            let opening_omission = index == 0 && value.is_none();
            let mut changed = frames.clone();
            let owner = if index == 1 {
                changed[index].as_object_mut().unwrap()
            } else {
                changed[index]["interaction"].as_object_mut().unwrap()
            };
            match value {
                Some(value) => {
                    owner.insert("status".into(), value);
                }
                None => {
                    owner.remove("status");
                }
            }
            if opening_omission {
                assert!(decode_interaction(&changed, true).is_ok());
                continue;
            }
            let error = decode_interaction(&changed, true).unwrap_err();
            assert!(
                error.contains("invalid") && error.contains("status"),
                "{error}"
            );
        }
    }
    let mut unsupported = frames.clone();
    unsupported[0]["interaction"]["status"] = json!("failed");
    assert!(
        decode_interaction(&unsupported, true)
            .unwrap_err()
            .contains("unsupported")
    );
    unsupported = frames.clone();
    unsupported[1]["status"] = json!("queued");
    assert!(
        decode_interaction(&unsupported, true)
            .unwrap_err()
            .contains("unsupported")
    );
    unsupported = frames;
    unsupported[1]["event_id"] = json!(17);
    assert!(
        decode_interaction(&unsupported, true)
            .unwrap_err()
            .contains("invalid event_id")
    );
}

#[test]
fn lifecycle_identity_binds_first_observation_without_inventing_status() {
    use morphiecore::protocol::{google::interactions::GenerationSseDecoder, stream::SseLimits};
    let original = interaction_frames();
    let expected = decode_interaction(&original, true).unwrap();
    let mut frames = original;
    for key in ["id", "status"] {
        frames[0]["interaction"]
            .as_object_mut()
            .unwrap()
            .remove(key);
    }
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
    )
    .unwrap();
    let (_, events) = decoder
        .consume(format!("data: {}\n\n", frames[0]).as_bytes())
        .unwrap();
    assert_eq!(events, [StreamEvent::Started]);
    assert_eq!(decoder.metadata().unwrap().id, None);
    decoder
        .consume(format!("data: {}\n\n", frames[1]).as_bytes())
        .unwrap();
    assert_eq!(decoder.metadata().unwrap().id.as_deref(), Some("i"));
    for frame in &frames[2..] {
        decoder
            .consume(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    decoder.consume(b"event: done\ndata: [DONE]\n\n").unwrap();
    assert_eq!(decoder.finish().unwrap(), expected);
    let mut first_at_terminal = frames.clone();
    first_at_terminal.remove(1);
    assert_eq!(
        decode_interaction(&first_at_terminal, true).unwrap(),
        expected
    );
    let mut absent = frames.clone();
    absent[1].as_object_mut().unwrap().remove("interaction_id");
    let last = absent.len() - 1;
    absent[last]["interaction"]
        .as_object_mut()
        .unwrap()
        .remove("id");
    let unreported = decode_interaction(&absent, true).unwrap();
    assert_eq!(unreported.metadata.id, None);
    assert_eq!(unreported.semantic, expected.semantic);
    assert_eq!(unreported.fidelity, expected.fidelity);
    let mut intermittent = frames.clone();
    intermittent.insert(
        2,
        json!({"event_type":"interaction.status_update","status":"in_progress"}),
    );
    let last = intermittent.len() - 1;
    intermittent[last]["interaction"]
        .as_object_mut()
        .unwrap()
        .remove("id");
    assert_eq!(decode_interaction(&intermittent, true).unwrap(), expected);
    let mut contradictory = frames.clone();
    contradictory[1]["interaction_id"] = json!("first");
    assert!(
        decode_interaction(&contradictory, true)
            .unwrap_err()
            .contains("identity changed")
    );
    let mut overlong = frames.clone();
    overlong[1]["interaction_id"] = json!("x".repeat(257));
    assert!(
        decode_interaction(&overlong, true)
            .unwrap_err()
            .contains("interaction_id")
    );
    for (key, value) in [
        ("id", json!(null)),
        ("id", json!(17)),
        ("id", json!("")),
        ("status", json!(null)),
        ("status", json!(17)),
        ("status", json!("invented")),
    ] {
        let mut malformed = frames.clone();
        malformed[0]["interaction"][key] = value;
        assert!(
            decode_interaction(&malformed, true)
                .unwrap_err()
                .contains(key)
        );
    }
}

#[test]
fn status_observations_neither_finish_nor_override_the_final_envelope() {
    use morphiecore::protocol::{google::interactions::GenerationSseDecoder, stream::SseLimits};
    for status in [
        "in_progress",
        "requires_action",
        "completed",
        "failed",
        "cancelled",
        "incomplete",
    ] {
        let mut frames = interaction_frames();
        frames[1]["status"] = json!(status);
        frames[1]["usage"] = json!({"total_tokens":7});
        let mut decoder = GenerationSseDecoder::new(
            200,
            "text/event-stream",
            SseLimits::default(),
            LocalScope::new(2),
            ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
        )
        .unwrap();
        decoder
            .consume(format!("data: {}\n\n", frames[0]).as_bytes())
            .unwrap();
        let (_, observed) = decoder
            .consume(format!("data: {}\n\n", frames[1]).as_bytes())
            .unwrap();
        assert!(
            matches!(observed.as_slice(), [StreamEvent::UsageReport(report)]
            if report.stage == UsageStage::Checkpoint && report.counts.total_tokens == Some(7))
        );
        assert!(
            decoder.finish().is_err(),
            "{status} is not a final envelope"
        );
        let decoded = decode_interaction(&frames, true).unwrap();
        assert_eq!(decoded.semantic.outcome(), Outcome::Completed);
        assert_eq!(
            decoded.semantic.progress(),
            InteractionProgress::AwaitingToolResults
        );
        // A checkpoint cannot make a final envelope with pending calls valid.
        frames.last_mut().unwrap()["interaction"]["status"] = json!("completed");
        assert!(
            decode_interaction(&frames, true)
                .unwrap_err()
                .contains("Interactions terminal status")
        );
    }
    // Source types do not establish an irreversible status-transition graph.
    let mut frames = interaction_frames();
    frames[1]["status"] = json!("requires_action");
    let resumed = json!({"event_type":"interaction.status_update",
        "interaction_id":"i","status":"in_progress"});
    frames.insert(2, resumed.clone());
    // Remove call steps, leaving the completed thought and no pending tools.
    frames.drain(7..11);
    frames.last_mut().unwrap()["interaction"]["status"] = json!("completed");
    let decoded = decode_interaction(&frames, true).unwrap();
    assert_eq!(
        decoded.semantic.progress(),
        InteractionProgress::TurnFinished
    );
    frames.push(resumed);
    assert!(
        decode_interaction(&frames, true)
            .unwrap_err()
            .contains("event after completion")
    );
}

#[test]
fn continuation_is_rejected_at_each_native_owner_and_poisoning_is_absorbing() {
    use morphiecore::protocol::{
        google::interactions::GenerationSseDecoder,
        stream::{SseError, SseLimits},
    };
    let frames = interaction_frames();
    for index in [0, 1, frames.len() - 1] {
        for (value, accepted) in [
            (json!(null), true),
            (json!("checkpoint"), false),
            (json!(7), false),
        ] {
            let mut changed = frames.clone();
            if index == 1 {
                changed[index]["continuation_token"] = value;
            } else {
                changed[index]["interaction"]["continuation_token"] = value;
            }
            if accepted {
                assert!(decode_interaction(&changed, true).is_ok());
            } else {
                assert!(
                    decode_interaction(&changed, true)
                        .unwrap_err()
                        .contains("continuation_token")
                );
            }
        }
    }
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
    )
    .unwrap();
    let opening = format!("data: {}\n\n", frames[0]);
    decoder.consume(opening.as_bytes()).unwrap();
    let foreign = json!({"event_type":"interaction.status_update","interaction_id":"foreign",
        "status":"in_progress","usage":{"total_tokens":999}});
    let foreign = format!("data: {foreign}\n\n");
    assert!(
        decoder
            .consume(foreign.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("interaction identity changed")
    );
    assert!(matches!(
        decoder.consume(opening.as_bytes()),
        Err(SseError::Closed)
    ));
    assert!(matches!(decoder.finish(), Err(SseError::Closed)));
}

#[test]
fn audit_step_errors_and_unrepresented_lifecycle_facts_cannot_complete() {
    let base = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","status":"in_progress","model":"gemini-synthetic"}}),
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
fn service_reports_preserve_matching_v1_phases_without_inference_or_capacity_aliases() {
    use morphiecore::semantic::context::{ReportPhase, ServiceClass};
    let base = interaction_frames();
    let expected = decode_interaction(&base, true).unwrap();
    assert!(expected.metadata.context.service_class().is_absent());
    for (native, class) in [
        ("standard", ServiceClass::StandardProcessing),
        ("flex", ServiceClass::FlexibleProcessing),
        ("priority", ServiceClass::PriorityProcessing),
    ] {
        let mut frames = base.clone();
        frames[0]["interaction"]["service_tier"] = json!(native);
        frames.last_mut().unwrap()["interaction"]["service_tier"] = json!(native);
        let reported = decode_interaction(&frames, true).unwrap();
        for phase in [ReportPhase::Opening, ReportPhase::Terminal] {
            assert_eq!(
                reported
                    .metadata
                    .context
                    .reports
                    .at(phase)
                    .unwrap()
                    .service_class,
                Presence::Value(class)
            );
        }
        assert_eq!(reported.semantic, expected.semantic);
        let mut drift = frames.clone();
        drift.last_mut().unwrap()["interaction"]["service_tier"] = json!(if native == "standard" {
            "priority"
        } else {
            "standard"
        });
        assert!(
            decode_interaction(&drift, true)
                .unwrap_err()
                .contains("service_tier changed")
        );
        let mut late = frames.clone();
        late[0]["interaction"]
            .as_object_mut()
            .unwrap()
            .remove("service_tier");
        let reported = decode_interaction(&late, true).unwrap();
        assert!(
            reported
                .metadata
                .context
                .reports
                .at(ReportPhase::Opening)
                .is_none()
        );
        assert_eq!(
            reported
                .metadata
                .context
                .reports
                .at(ReportPhase::Terminal)
                .unwrap()
                .service_class,
            Presence::Value(class)
        );
        let mut erased = frames.clone();
        erased.last_mut().unwrap()["interaction"]
            .as_object_mut()
            .unwrap()
            .remove("service_tier");
        let opening_only = decode_interaction(&erased, true).unwrap();
        assert_eq!(
            opening_only
                .metadata
                .context
                .reports
                .at(ReportPhase::Opening)
                .unwrap()
                .service_class,
            Presence::Value(class)
        );
        assert!(
            opening_only
                .metadata
                .context
                .reports
                .at(ReportPhase::Terminal)
                .unwrap()
                .service_class
                .is_absent()
        );
    }
    for value in [
        serde_json::Value::Null,
        json!(false),
        json!("deferred"),
        json!("unspecified"),
        json!("unknown"),
    ] {
        for phase in [0, base.len() - 1] {
            let mut frames = base.clone();
            frames[phase]["interaction"]["service_tier"] = value.clone();
            assert!(decode_interaction(&frames, true).is_err());
        }
    }
    let mut wrong_owner = base;
    wrong_owner[1]["service_tier"] = json!("standard");
    assert!(
        decode_interaction(&wrong_owner, true)
            .unwrap_err()
            .contains("Interactions status service_tier")
    );
}

#[test]
fn lifecycle_snapshots_compare_source_ordinals_without_republishing_content() {
    use morphiecore::protocol::{google::interactions::GenerationSseDecoder, stream::SseLimits};
    let mut frames = interaction_frames();
    for frame in &mut frames {
        if let Some(index) = frame.get_mut("index") {
            *index = json!(index.as_u64().unwrap() + 1);
        }
    }
    frames.splice(
        2..2,
        [
            json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
            json!({"event_type":"step.stop","index":0}),
        ],
    );
    frames[0]["interaction"]["steps"] = json!([]);
    let original = decode_interaction(&frames, true).unwrap();
    let steps = json!([
        {"type":"model_output","content":[]},
        {"type":"thought","summary":[{"type":"text","text":"Plan"}],"signature":"synthetic-thought"},
        {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"}}
    ]);
    let last = frames.len() - 1;
    frames[last]["interaction"]["steps"] = steps.clone();
    let matched = decode_interaction(&frames, true).unwrap();
    assert_eq!(matched, original);
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
    )
    .unwrap();
    let mut emitted = vec![];
    for frame in &frames {
        emitted.extend(
            decoder
                .consume(format!("data: {frame}\n\n").as_bytes())
                .unwrap()
                .1,
        );
    }
    decoder.consume(b"event: done\ndata: [DONE]\n\n").unwrap();
    assert_eq!(decoder.finish().unwrap(), original);
    assert_eq!(
        emitted
            .iter()
            .filter(|e| matches!(e, StreamEvent::ItemStarted { .. }))
            .count(),
        2
    );
    assert_eq!(
        emitted
            .iter()
            .filter(|e| matches!(e, StreamEvent::ItemFinished { .. }))
            .count(),
        2
    );
    assert_eq!(
        emitted
            .iter()
            .filter(|e| matches!(e, StreamEvent::Delta { fragment, .. } if fragment == "Plan"))
            .count(),
        1
    );
    assert_eq!(
        emitted
            .iter()
            .filter(|e| matches!(e, StreamEvent::Terminal { .. }))
            .count(),
        1
    );
    for supplied in [
        json!([{"type":"model_output"},{"type":"thought"},
            {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"},"future":17}]),
        json!(null),
    ] {
        let mut partial = frames.clone();
        partial[last]["interaction"]["steps"] = supplied;
        assert_eq!(decode_interaction(&partial, true).unwrap(), original);
    }
    let change = |index: usize, field: &str, value: serde_json::Value| {
        let mut snapshot = steps.clone();
        snapshot[index][field] = value;
        snapshot
    };
    for (snapshot, expected) in [
        (json!([]), "snapshot steps"),
        (json!(17), "Interactions steps"),
        (
            change(0, "content", json!([{"type":"text","text":"new"}])),
            "snapshot steps",
        ),
        (
            change(1, "summary", json!([{"type":"text","text":"changed"}])),
            "snapshot steps",
        ),
        (change(1, "signature", json!("changed")), "snapshot steps"),
        (change(1, "error", json!({"code":7})), "model_output error"),
        (
            change(
                1,
                "summary",
                json!([{"type":"text","text":"P"},{"type":"text","text":"lan"}]),
            ),
            "snapshot multipart",
        ),
        (change(2, "id", json!("foreign")), "snapshot steps"),
        (change(2, "name", json!("other")), "snapshot steps"),
        (
            change(2, "arguments", json!({"path":"b"})),
            "snapshot steps",
        ),
        (change(2, "signature", json!("new")), "snapshot steps"),
        (change(0, "type", json!("thought")), "snapshot steps"),
    ] {
        let mut changed = frames.clone();
        changed[last]["interaction"]["steps"] = snapshot;
        let error = decode_interaction(&changed, true).unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
    let mut missing = frames.clone();
    missing[last]["interaction"]["steps"][2]
        .as_object_mut()
        .unwrap()
        .remove("arguments");
    assert!(
        decode_interaction(&missing, true)
            .unwrap_err()
            .contains("arguments")
    );
    let mut signature_only_in_snapshot = frames.clone();
    signature_only_in_snapshot.remove(6);
    assert!(
        decode_interaction(&signature_only_in_snapshot, true)
            .unwrap_err()
            .contains("snapshot steps")
    );
    let mut multipart = frames.clone();
    multipart[4]["step"]["summary"] =
        json!([{"type":"text","text":"A"},{"type":"text","text":"B"}]);
    assert!(
        decode_interaction(&multipart, true)
            .unwrap_err()
            .contains("multipart step content")
    );
    let mut introduced = frames;
    introduced[6]["delta"]["signature"] = json!("other");
    assert!(
        decode_interaction(&introduced, true)
            .unwrap_err()
            .contains("snapshot steps")
    );
}

#[test]
fn empty_source_steps_are_bounded_even_without_semantic_members() {
    use morphiecore::protocol::{
        google::interactions::GenerationSseDecoder,
        stream::{SseError, SseLimits},
    };
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap(),
    )
    .unwrap();
    decoder
        .consume(format!("data: {}\n\n", interaction_frames()[0]).as_bytes())
        .unwrap();
    for index in 0..MAX_ITEMS {
        let start = json!({"event_type":"step.start","index":index,"step":{"type":"model_output"}});
        assert!(
            decoder
                .consume(format!("data: {start}\n\n").as_bytes())
                .unwrap()
                .1
                .is_empty()
        );
        let stop = json!({"event_type":"step.stop","index":index});
        assert!(
            decoder
                .consume(format!("data: {stop}\n\n").as_bytes())
                .unwrap()
                .1
                .is_empty()
        );
    }
    let excess =
        json!({"event_type":"step.start","index":MAX_ITEMS,"step":{"type":"model_output"}});
    assert!(matches!(
        decoder.consume(format!("data: {excess}\n\n").as_bytes()),
        Err(SseError::Limit)
    ));
    assert!(decoder.finish().is_err());
}

#[test]
fn step_stream_preserves_usage_call_arguments_and_thought_signature() {
    let decoded = decode_interaction(&interaction_frames(), true).unwrap();
    let mut placeholder = interaction_frames();
    placeholder[2]["step"]["signature"] = json!("");
    assert_eq!(decode_interaction(&placeholder, false).unwrap(), decoded);
    for invalid in [json!(null), json!(42)] {
        let mut malformed = placeholder.clone();
        malformed[2]["step"]["signature"] = invalid;
        assert!(
            decode_interaction(&malformed, true)
                .unwrap_err()
                .contains("signature")
        );
    }
    let mut duplicate = placeholder;
    duplicate.insert(5, duplicate[4].clone());
    assert!(
        decode_interaction(&duplicate, true)
            .unwrap_err()
            .contains("duplicate step signature")
    );
    assert_eq!(decoded.metadata.id.as_deref(), Some("i"));
    assert_eq!(
        decoded.semantic.progress(),
        InteractionProgress::AwaitingToolResults
    );
    let items = decoded.semantic.items();
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0].1, Item::Reasoning(r)
        if matches!(r.parts.as_slice(),[(_,ReasoningContent::Summary(t))] if t.as_str()=="Plan")));
    assert!(matches!(&items[1].1, Item::ToolCall(c)
        if c.call_id.as_str()=="c1" && c.arguments.as_structured().unwrap().value()==&json!({"path":"a"})));
    let Item::Reasoning(r) = &items[0].1 else {
        panic!("thought")
    };
    let value = r.replay.as_ref().unwrap();
    assert_eq!(value.format(), ReplayFormat::GoogleInteractionsV1Thought);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value.replay_token().unwrap()).unwrap(),
        json!({"anchor":"thought","signature":"synthetic-thought"})
    );
    let mut signed = interaction_frames();
    signed[6]["step"]["signature"] = json!("synthetic-call");
    let signed = decode_interaction(&signed, true).unwrap();
    assert_eq!(signed.semantic.items().len(), 2);
    let Item::ToolCall(call) = &signed.semantic.items()[1].1 else {
        panic!("call")
    };
    let value = call.replay.as_ref().unwrap();
    assert_eq!(
        value.format(),
        ReplayFormat::GoogleInteractionsV1FunctionCall
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value.replay_token().unwrap()).unwrap(),
        json!({"anchor":"function_call","signature":"synthetic-call"})
    );
    let mut full = interaction_frames();
    full[6]["step"]["signature"] = json!("synthetic-call");
    full[6]["step"]["arguments"] = json!({"path":"a"});
    full.drain(7..9);
    assert_eq!(decode_interaction(&full, true).unwrap(), signed);
    let mut snapshots = interaction_frames();
    snapshots[6]["step"]["signature"] = json!("synthetic-call");
    let last = snapshots.len() - 1;
    snapshots[last]["interaction"]["steps"] = json!([
        {"type":"thought","signature":"synthetic-thought"},
        {"type":"function_call","id":"c1","name":"read","arguments":{"path":"a"},"signature":"synthetic-call"}
    ]);
    assert_eq!(decode_interaction(&snapshots, true).unwrap(), signed);
    snapshots[last]["interaction"]["steps"][1]["signature"] = json!("different");
    assert!(
        decode_interaction(&snapshots, true)
            .unwrap_err()
            .contains("snapshot steps")
    );
    for signature in [json!(null), json!(17), json!("")] {
        let mut malformed = interaction_frames();
        malformed[6]["step"]["signature"] = signature;
        assert!(
            decode_interaction(&malformed, true)
                .unwrap_err()
                .contains("signature")
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
fn scoped_usage_keeps_uncalibrated_tool_counts_and_replaces_operation_snapshots() {
    let usage = |total| {
        json!({"total_input_tokens":total,"total_output_tokens":0,
        "total_thought_tokens":0,"total_tool_use_tokens":0,"total_tokens":total})
    };
    let frames = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","status":"in_progress","model":"gemini-synthetic"}}),
        json!({"event_type":"interaction.status_update","interaction_id":"i","status":"in_progress","usage":usage(50)}),
        json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
        json!({"event_type":"step.stop","index":0,"usage":usage(100),"step_usage":usage(100)}),
        json!({"event_type":"step.start","index":1,"step":{"type":"model_output"}}),
        json!({"event_type":"step.stop","index":1,"usage":usage(220),"step_usage":usage(120)}),
        json!({"event_type":"interaction.completed","interaction":{"id":"i","status":"completed"}}),
    ];
    let observed = decode_interaction(&frames, true).unwrap();
    assert_eq!(observed.semantic.usage().unwrap().total_tokens, Some(220));
    assert_eq!(
        observed
            .semantic
            .usage_reports()
            .reports()
            .map(|r| (r.scope, r.stage, r.counts.total_tokens))
            .collect::<Vec<_>>(),
        [
            (UsageScope::Operation, UsageStage::Final, Some(220)),
            (UsageScope::Step(0), UsageStage::Final, Some(100)),
            (UsageScope::Step(1), UsageStage::Final, Some(120)),
        ]
    );
    for total in [125, 157] {
        let mut frames = frames.clone();
        frames[6]["interaction"]["usage"] = json!({"total_input_tokens":100,"total_tool_use_tokens":30,
            "total_output_tokens":20,"total_thought_tokens":7,"total_tokens":total,
            "input_tokens_by_modality":[{"modality":"text","tokens":60}],
            "grounding_tool_count":[{"type":"google_search","count":2}]});
        let decoded = decode_interaction(&frames, true).unwrap();
        let report = decoded.semantic.usage_reports().operation().unwrap();
        assert_eq!(
            (
                report.counts.input_tokens,
                report.tool_input_tokens(),
                report.counts.output_tokens
            ),
            (Some(100), Some(30), Some(20))
        );
        assert_eq!(report.details.tool_requests[0].count, Some(2));
        assert_eq!(report.details.modalities[0].tokens, Some(60));
        assert_eq!(decoded.semantic.usage().unwrap().input_tokens, None);
        assert_eq!(
            decoded.semantic.usage().unwrap().total_relation,
            TotalTokenRelation::Unreported
        );
        frames[6]["interaction"]["usage"]
            .as_object_mut()
            .unwrap()
            .remove("total_thought_tokens");
        let decoded = decode_interaction(&frames, true).unwrap();
        assert_eq!(
            decoded
                .semantic
                .usage_reports()
                .operation()
                .unwrap()
                .counts
                .output_tokens,
            Some(20)
        );
        assert_eq!(decoded.semantic.usage().unwrap().output_tokens, None);
    }
}

#[test]
fn step_stream_rejects_missing_call_identity_and_unfinished_or_late_data() {
    let opening = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","status":"in_progress","model":"gemini-synthetic","usage":{"total_tokens":7}}}),
        json!({"event_type":"interaction.completed","interaction":{"id":"i","status":"completed"}}),
    ];
    let observed = decode_interaction(&opening, true).unwrap();
    assert_eq!(
        observed
            .semantic
            .usage_reports()
            .operation()
            .unwrap()
            .counts
            .total_tokens,
        Some(7)
    );
    let mut invalid_usage = opening.clone();
    invalid_usage[0]["interaction"]["usage"] = json!({"total_input_tokens":-1});
    assert!(
        decode_interaction(&invalid_usage, true)
            .unwrap_err()
            .contains("total_input_tokens")
    );
    for entry in [
        json!({"modality":"text"}),
        json!({"modality":"text","tokens":null}),
        json!({"tokens":3}),
    ] {
        let mut frames = opening.clone();
        frames[0]["interaction"]["usage"]["input_tokens_by_modality"] = json!([entry.clone()]);
        let decoded = decode_interaction(&frames, true).unwrap();
        let part = &decoded
            .semantic
            .usage_reports()
            .operation()
            .unwrap()
            .details
            .modalities[0];
        assert_eq!(
            part.tokens,
            entry.get("tokens").and_then(serde_json::Value::as_u64)
        );
        assert_eq!(
            part.modality,
            entry.get("modality").map(|_| TokenModality::Text)
        );
        assert!(!part.defaulted);
    }
    let truncated = vec![
        json!({"event_type":"interaction.created","interaction":{"id":"i","status":"in_progress","object":"interaction","model":"gemini-synthetic"}}),
        json!({"event_type":"step.start","index":0,"step":{"type":"model_output"}}),
        json!({"event_type":"step.stop","index":0}),
        json!({"event_type":"interaction.completed","interaction":{"id":"i","status":"incomplete"}}),
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
    assert_eq!(
        decode_interaction(&frames, false).unwrap(),
        decode_interaction(&frames, true).unwrap()
    );
    assert!(decode_interaction(&frames[..9], true).is_err());
    let mut late = frames.clone();
    late.push(frames[1].clone());
    assert!(decode_interaction(&late, true).is_err());
    let mut malformed = frames;
    malformed[3]["delta"]["content"]["text"] = json!(42);
    assert!(decode_interaction(&malformed, true).is_err());
}

fn signed_history(format: ReplayFormat) -> (GenerationRequest, FidelityRecords) {
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
    let mut items = source.task.semantic.items().to_vec();
    items.insert(1, thought);
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
fn function_call_signatures_are_native_and_dependency_bound() {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let origin = issuer("synthetic-google", "gemini-synthetic");
    let history = |payload: serde_json::Value| {
        let (request, mut fidelity) = signed_history(ReplayFormat::GoogleInteractionsV1Thought);
        let mut items = request.items().to_vec();
        let (id, Item::ToolCall(call)) = &mut items[2] else {
            panic!("call")
        };
        call.replay = Some(ReplayValue::final_value(
            ReplayFormat::GoogleInteractionsV1FunctionCall,
            Text::new(payload.to_string(), "call signature", MAX_TEXT_BYTES).unwrap(),
        ));
        fidelity
            .record_call_replay(*id, call, Some(origin.clone()))
            .unwrap();
        let (owner, call) = (*id, call.clone());
        (request.with_items(items).unwrap(), fidelity, owner, call)
    };
    let (request, fidelity, owner, call) =
        history(json!({"anchor":"function_call","signature":"synthetic-call"}));
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
    for field in ["id", "name", "arguments", "context", "status", "signature"] {
        let mut edited = call.clone();
        match field {
            "id" => edited.call_id = Text::new("different", "id", 64).unwrap(),
            "name" => edited.name = Text::new("other", "name", 64).unwrap(),
            "arguments" => edited.arguments = ToolArguments::Raw("{\"changed\":true}".into()),
            "context" => edited.context.async_call = true,
            "status" => edited.status = ItemLifecycle::Incomplete,
            "signature" => {
                edited.replay = Some(ReplayValue::final_value(
                    ReplayFormat::GoogleInteractionsV1FunctionCall,
                    Text::new(
                        "{\"anchor\":\"function_call\",\"signature\":\"changed\"}",
                        "fixture",
                        256,
                    )
                    .unwrap(),
                ))
            }
            _ => unreachable!(),
        }
        assert!(
            !fidelity.call_replay_matches(owner, &edited, Some(&origin)),
            "{field}"
        );
    }
    for other in [
        ReplayTarget::new("synthetic-google", "other-model").unwrap(),
        ReplayTarget::new("other-deployment", "gemini-synthetic").unwrap(),
    ] {
        assert!(encode_generation_request(&request, &fidelity, &other).is_err());
    }
    let (foreign, receipt) =
        morphiecore::protocol::google::generate_content::encode_generation_request(
            &request, &fidelity, &target, None,
        )
        .unwrap();
    assert!(!foreign.to_string().contains("synthetic-call"));
    assert!(receipt.contains(
        &morphiecore::protocol::google::generate_content::RequestRule::TransferVisibleHistory
    ));
    for payload in [
        json!({"anchor":"thought","signature":"synthetic-call"}),
        json!({"anchor":"function_call"}),
        json!({"anchor":"function_call","signature":null}),
        json!({"anchor":"function_call","signature":""}),
        json!({"anchor":"function_call","signature":"x","unexpected":true}),
    ] {
        let (request, fidelity, _, _) = history(payload);
        assert!(
            encode_generation_request(&request, &fidelity, &target)
                .unwrap_err()
                .to_string()
                .contains("signature payload")
        );
    }
}

#[test]
fn restores_native_signatures_by_anchor_and_drops_the_other_google_format() {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let (request, fidelity) = signed_history(ReplayFormat::GoogleInteractionsV1Thought);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"thought","summary":[{"type":"text","text":"Plan"}],"signature":"synthetic-thought"},
            {"type":"function_call","id":"c1","name":"read","arguments":{}},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert!(rules.is_empty());
    let (request, fidelity) = signed_history(ReplayFormat::GoogleGenerateContentPart);
    let (wire, rules) = encode_generation_request(&request, &fidelity, &target).unwrap();
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"Read."}]},
            {"type":"function_call","id":"c1","name":"read","arguments":{}},
            {"type":"function_result","call_id":"c1","name":"read","result":"done"}
        ])
    );
    assert_eq!(rules, [RequestRule::TransferVisibleHistory]);
}

#[test]
fn rejects_orphaned_or_wrong_target_signatures() {
    let (request, mut fidelity) = signed_history(ReplayFormat::GoogleInteractionsV1Thought);
    let mut items = request.items().to_vec();
    let (id, Item::Reasoning(reasoning)) = &mut items[1] else {
        panic!("thought")
    };
    reasoning.replay = Some(ReplayValue::final_value(
        ReplayFormat::GoogleInteractionsV1Thought,
        Text::new(
            json!({"anchor":"call","call":"c1","signature":"invented"}).to_string(),
            "synthetic",
            1024,
        )
        .unwrap(),
    ));
    fidelity
        .record_replay(
            *id,
            reasoning,
            Some(issuer("synthetic-google", "gemini-synthetic")),
        )
        .unwrap();
    let request = request.with_items(items).unwrap();
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    assert!(
        encode_generation_request(&request, &fidelity, &target)
            .unwrap_err()
            .to_string()
            .contains("signature payload")
    );
    let (request, fidelity) = signed_history(ReplayFormat::GoogleInteractionsV1Thought);
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
    endpoint.representation = morphiecore::protocol::google::interactions::TargetCaps {
        semantics: morphiecore::protocol::google::interactions::representation_contract(),
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
        morphiecore::protocol::google::interactions::semantic_contract()
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
    for (schema, supported) in [
        (
            json!({"type":"array","items":{"type":"string"},"minItems":1}),
            true,
        ),
        (
            json!({"type":"array","items":{"type":"string","minLength":1}}),
            false,
        ),
        (
            json!({"type":"array","items":{"type":"string"},"uniqueItems":true}),
            false,
        ),
    ] {
        let mut settings = request.task.semantic.settings().clone();
        let Presence::Value(OutputConstraint::JsonSchema { schema: slot, .. }) =
            &mut settings.text.format
        else {
            panic!("schema")
        };
        *slot = schema.clone().into();
        let semantic = request
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .unwrap();
        let encoded = encode_generation_request(&semantic, &request.task.fidelity, &target);
        assert_eq!(encoded.is_ok(), supported);
        if supported {
            assert_eq!(encoded.unwrap().0["response_format"]["schema"], schema);
        }
    }
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
    let strict = morphiecore::protocol::anthropic::messages::client::decode_client_request(
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

#[test]
fn projects_instructions_tool_choice_effort_summaries_and_result_errors() {
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let encode = |profile, body: serde_json::Value| {
        let request = Adapter::new(profile, Dialect::Standard, None)
            .decode_request(body.to_string().as_bytes())
            .unwrap();
        let (wire, rules) =
            encode_generation_request(&request.task.semantic, &request.task.fidelity, &target)
                .unwrap();
        (wire, rules.iter().map(|r| r.name()).collect::<Vec<_>>())
    };
    let chat = |extra: serde_json::Value| {
        let mut body = json!({"model":"public","messages":[
            {"role":"developer","content":"Be brief."},
            {"role":"user","content":"go"},
            {"role":"assistant","content":"Reading."}],
            "tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}]});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        encode(Profile::Chat, body)
    };
    let (wire, rules) = chat(json!({}));
    assert_eq!(wire["system_instruction"], "Be brief.");
    assert_eq!(
        wire["input"],
        json!([
            {"type":"user_input","content":[{"type":"text","text":"go"}]},
            {"type":"model_output","content":[{"type":"text","text":"Reading."}]}
        ])
    );
    assert_eq!(rules, ["interactions.developer-as-system.v1"]);
    for (choice, wire_choice) in [
        (json!("auto"), json!("auto")),
        (json!("none"), json!("none")),
        (json!("required"), json!("any")),
        (
            json!({"type":"function","function":{"name":"read"}}),
            json!({"allowed_tools":{"mode":"any","tools":["read"]}}),
        ),
    ] {
        let (wire, _) = chat(json!({"tool_choice": choice}));
        assert_eq!(
            wire["generation_config"],
            json!({"tool_choice": wire_choice}),
            "{choice}"
        );
    }
    for effort in ["minimal", "low", "medium", "high"] {
        let (wire, _) = chat(json!({"reasoning_effort": effort}));
        assert_eq!(
            wire["generation_config"],
            json!({"thinking_level": effort}),
            "{effort}"
        );
    }
    // One summary mode: requested concision is approximated, not dropped.
    let (wire, rules) = encode(
        Profile::Responses,
        json!({"model":"public","input":"go","reasoning":{"summary":"concise"}}),
    );
    assert_eq!(
        wire["generation_config"],
        json!({"thinking_summaries":"auto"})
    );
    assert_eq!(rules, ["interactions.summarize-reasoning.v1"]);

    // Messages tool errors keep their flag on the result step.
    let request = morphiecore::protocol::anthropic::messages::client::decode_client_request(
        json!({"model":"public","max_tokens":64,"messages":[
            {"role":"user","content":"go"},
            {"role":"assistant","content":[{"type":"tool_use","id":"c1","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1","content":"missing","is_error":true}]}]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    let (wire, _) =
        encode_generation_request(&request.task.semantic, &request.task.fidelity, &target).unwrap();
    assert_eq!(
        wire["input"][2],
        json!({"type":"function_result","call_id":"c1","name":"read","result":"missing","is_error":true})
    );
}

/// A result error flag is not an execution report in either Google API.
#[test]
fn tool_result_execution_reports_require_a_real_carrier() {
    let text = |value: &str| Text::new(value, "fixture", 64).unwrap();
    let request = |execution, is_error| {
        GenerationRequest::new(
            vec![
                (
                    ItemId::new(1),
                    Item::Message(Message {
                        participant: None,
                        role: MessageRole::User,
                        status: ItemLifecycle::Completed,
                        phase: None,
                        parts: vec![Part::new(
                            PartId::new(1),
                            ContentPart::Text(text("go").into()),
                        )],
                    }),
                ),
                (
                    ItemId::new(2),
                    Item::ToolCall(ToolCall {
                        replay: None,
                        call_id: text("c1"),
                        cache: None,
                        name: text("read"),
                        arguments: "{}".into(),
                        status: ItemLifecycle::Completed,
                        context: CallContext::default(),
                    }),
                ),
                (
                    ItemId::new(3),
                    Item::ToolResult(ToolResult {
                        is_error,
                        cache: None,
                        call_id: Some(text("c1")),
                        name: None,
                        output: "out".into(),
                        execution,
                        status: None,
                        context: CallContext::default(),
                    }),
                ),
            ],
            GenerationControls::default(),
        )
        .unwrap()
    };
    let target = ReplayTarget::new("synthetic-google", "gemini-synthetic").unwrap();
    let fidelity = FidelityRecords::default();
    let encode = |request: &GenerationRequest| {
        let interactions = encode_generation_request(request, &fidelity, &target)
            .map(|(wire, _)| wire["input"][2].clone());
        let generate = morphiecore::protocol::google::generate_content::encode_generation_request(
            request, &fidelity, &target, None,
        )
        .map(|(wire, _)| wire["contents"][2]["parts"][0]["functionResponse"]["response"].clone());
        (interactions, generate)
    };
    for is_error in [None, Some(false), Some(true)] {
        let source = request(None, is_error);
        let (interactions, generate) = encode(&source);
        let key = if is_error == Some(true) {
            "error"
        } else {
            "output"
        };
        assert_eq!(generate.unwrap(), json!({key: "out"}));
        let interactions = interactions.unwrap();
        assert_eq!(interactions["result"], "out");
        assert_eq!(
            interactions.get("is_error"),
            is_error.map(|e| json!(e)).as_ref()
        );
        assert!(
            morphiecore::protocol::google::generate_content::semantic_contract()
                .check(&source)
                .is_ok()
        );
        assert!(
            morphiecore::protocol::google::interactions::semantic_contract()
                .check(&source)
                .is_ok()
        );
    }
    // IR validation already rejects success reported as an error.
    for (execution, is_error) in [
        (ToolExecution::Succeeded, None),
        (ToolExecution::Succeeded, Some(false)),
        (ToolExecution::NotExecuted, Some(true)),
        (ToolExecution::Failed { code: None }, Some(true)),
        (ToolExecution::NotExecuted, None),
        (ToolExecution::NotExecuted, Some(false)),
        (
            ToolExecution::Failed {
                code: Some(text("E1")),
            },
            Some(true),
        ),
        (ToolExecution::Cancelled, Some(true)),
        (ToolExecution::Unknown, None),
    ] {
        let source = request(Some(execution.clone()), is_error);
        let original = source.clone();
        let (interactions, generate) = encode(&source);
        assert!(
            matches!(&generate, Err(morphiecore::protocol::CodecError::Unsupported(r)) if r == "Gemini semantic carrier"),
            "{execution:?}"
        );
        assert!(
            matches!(&interactions, Err(morphiecore::protocol::CodecError::Unsupported(r)) if r == "Interactions semantic carrier"),
            "{execution:?}"
        );
        for contract in [
            morphiecore::protocol::google::generate_content::semantic_contract(),
            morphiecore::protocol::google::interactions::semantic_contract(),
        ] {
            assert_eq!(
                contract.check(&source),
                Err(GenerationFeature::ToolExecutionReport)
            );
        }
        assert_eq!(source, original);
    }
}
