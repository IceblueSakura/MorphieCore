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
fn audit_explicit_signed_part_boundaries_survive_intake_and_replay() {
    let observed = decode(&wire(&[
        json!({"responseId":"parts","modelVersion":"gemini-synthetic",
        "candidates":[{"content":{"role":"model","parts":[
            {"text":"A"},{"text":"B","thoughtSignature":"U0lH"}]},"finishReason":"STOP"}]}),
    ]))
    .unwrap();
    let texts: Vec<_> = observed
        .semantic
        .items()
        .iter()
        .filter_map(|(_, item)| match item {
            Item::Message(m) => Some(
                m.parts
                    .iter()
                    .filter_map(|p| match &p.content {
                        ContentPart::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["A", "B"]);
    let history = GenerationRequest::new(
        observed.semantic.items().to_vec(),
        GenerationControls::default(),
    )
    .unwrap()
    .with_message_envelopes(observed.semantic.message_envelopes().to_vec())
    .unwrap();
    let (body, _) =
        encode_generation_request(&history, &observed.fidelity, &target(), None).unwrap();
    assert_eq!(
        body["contents"][0]["parts"],
        json!([
            {"text":"A"},{"text":"B","thoughtSignature":"U0lH"}
        ])
    );
}

#[test]
fn audit_citation_reports_are_not_successfully_erased() {
    let mut value = json!({"responseId":"cited","modelVersion":"gemini-synthetic",
        "candidates":[{"content":{"role":"model","parts":[{"text":"A"}]},"finishReason":"STOP"}]});
    assert!(decode(&wire(&[value.clone()])).is_ok());
    value["candidates"][0]["citationMetadata"] = json!({"citationSources":[{
        "startIndex":0,"endIndex":1,"uri":"https://example.test/source","license":"MIT"}]});
    assert!(
        decode(&wire(&[value]))
            .unwrap_err()
            .contains("Gemini citationMetadata")
    );
}

#[test]
fn source_usage_keeps_tool_input_modalities_and_default_zero_provenance() {
    let mut frame = json!({"responseId":"usage","modelVersion":"gemini-synthetic",
        "candidates":[{"content":{"role":"model","parts":[{"text":"A"}]},"finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":100,"toolUsePromptTokenCount":30,
            "candidatesTokenCount":20,"thoughtsTokenCount":7,"cachedContentTokenCount":40,"totalTokenCount":157,
            "promptTokensDetails":[{"modality":"TEXT","tokenCount":60},{"modality":"IMAGE","tokenCount":20},
                {"modality":"AUDIO","tokenCount":10},{"modality":"VIDEO","tokenCount":10}],
            "toolUsePromptTokensDetails":[{"modality":"TEXT","tokenCount":12},{"modality":"AUDIO","tokenCount":18}],
            "candidatesTokensDetails":[{"modality":"TEXT","tokenCount":15},{"modality":"AUDIO","tokenCount":5}],
            "cacheTokensDetails":[{"modality":"TEXT","tokenCount":25},{"modality":"IMAGE","tokenCount":15}]}});
    let decoded = decode(&wire(&[frame.clone()])).unwrap();
    let report = decoded.semantic.usage_reports().operation().unwrap();
    assert_eq!(
        (
            report.counts.input_tokens,
            report.tool_input_tokens(),
            report.counts.output_tokens,
            report.counts.reasoning_tokens,
            report.counts.total_tokens
        ),
        (Some(100), Some(30), Some(20), Some(7), Some(157))
    );
    assert_eq!(report.details.modalities.len(), 10);
    assert!(report.details.defaulted.is_empty());
    let summary = decoded.semantic.usage().unwrap();
    assert_eq!(
        (
            summary.input_tokens,
            summary.output_tokens,
            summary.total_tokens
        ),
        (Some(130), Some(27), Some(157))
    );
    assert_eq!(
        (
            summary.input_text_tokens,
            summary.input_audio_tokens,
            summary.output_text_tokens
        ),
        (Some(72), Some(28), Some(15))
    );
    assert_eq!(summary.input_image_tokens, None);
    frame["usageMetadata"]["totalTokenCount"] = json!(127);
    assert!(decode(&wire(&[frame.clone()])).is_err());
    frame["usageMetadata"] = json!({});
    let defaulted = decode(&wire(&[frame.clone()])).unwrap();
    assert_eq!(
        defaulted
            .semantic
            .usage_reports()
            .operation()
            .unwrap()
            .details
            .defaulted
            .len(),
        6
    );
    frame.as_object_mut().unwrap().remove("usageMetadata");
    assert!(
        decode(&wire(&[frame]))
            .unwrap()
            .semantic
            .usage_reports()
            .operation()
            .is_none()
    );
}

#[test]
fn cross_format_reasoning_is_removed_before_grouping_but_partial_values_reject() {
    use morphiecore::protocol::{fidelity::FidelityRecords, replay_envelope::issuer};
    let source = client(
        Profile::Responses,
        json!({"model":"m","input":[
        {"role":"user","content":"first"},{"role":"user","content":"next"}]}),
    );
    let owner = ItemId::scoped(LocalScope::new(19), 1);
    for (status, partial) in [
        (ItemLifecycle::Completed, false),
        (ItemLifecycle::Completed, true),
        (ItemLifecycle::Incomplete, false),
    ] {
        let value = Text::new("opaque", "synthetic", 64).unwrap();
        let reasoning = ReasoningItem {
            status,
            parts: vec![(
                PartId::scoped(LocalScope::new(19), 1),
                ReasoningContent::Summary(Text::new("Plan", "synthetic", 64).unwrap()),
            )],
            replay: Some(if partial {
                ReplayValue::partial(ReplayFormat::GoogleInteractionsV1Thought, value)
            } else {
                ReplayValue::final_value(ReplayFormat::GoogleInteractionsV1Thought, value)
            }),
        };
        let mut fidelity = FidelityRecords::default();
        fidelity
            .record_replay(
                owner,
                &reasoning,
                Some(issuer("synthetic-google", "gemini-synthetic")),
            )
            .unwrap();
        let mut items = source.task.semantic.items().to_vec();
        items.insert(1, (owner, Item::Reasoning(reasoning)));
        let request = source.task.semantic.clone().with_items(items).unwrap();
        let result = encode_generation_request(&request, &fidelity, &target(), None);
        if status == ItemLifecycle::Completed && !partial {
            let (body, rules) = result.unwrap();
            assert_eq!(
                body["contents"],
                json!([{"role":"user","parts":[{"text":"first"},{"text":"next"}]}])
            );
            assert_eq!(
                rules,
                [morphiecore::protocol::google::RequestRule::TransferVisibleHistory]
            );
            assert_eq!(request.items().len(), 3);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("partial foreign reasoning")
            );
        }
    }
}

#[test]
fn stream_maps_thoughts_text_calls_and_owned_signatures() {
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
    let (body, _) = encode_generation_request(&history, &empty.fidelity, &target(), None).unwrap();
    assert_eq!(
        body["contents"][0]["parts"],
        json!([{"text":"","thoughtSignature":"U0lH"}])
    );
    let missing = client(
        Profile::Chat,
        json!({"model":"public","messages":[{"role":"user","content":"continue"}]}),
    )
    .task
    .semantic;
    assert!(encode_generation_request(&missing, &empty.fidelity, &target(), None).is_err());

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
    let recited = decode(&wire(&[json!({
        "responseId":"recited","modelVersion":"gemini-synthetic",
        "candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"partial"}]},
            "finishReason":"RECITATION"}],
        "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}
    })]))
    .unwrap();
    assert_eq!(
        recited.semantic.details().incomplete,
        Some(IncompleteReason::Recitation)
    );
    let observed = decode(&wire(&frames())).unwrap();
    assert_eq!(observed.metadata.id.as_deref(), Some("resp-1"));
    assert_eq!(
        observed.semantic.details().completion,
        Some(CompletionCause::NaturalOrStopSequence)
    );
    assert!(
        Adapter::new(Profile::Chat, Dialect::MorphieCore, None)
            .encode_response(
                &observed,
                &morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
    );
    assert!(
        Adapter::new(Profile::Responses, Dialect::MorphieCore, None)
            .encode_response(
                &observed,
                &morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
    );
    assert_eq!(observed.metadata.created, None);
    let items = observed.semantic.items();
    assert_eq!(items.len(), 4);
    assert!(matches!(&items[0].1, Item::Reasoning(r)
        if matches!(r.parts.as_slice(), [(_, ReasoningContent::Summary(t))] if t.as_str() == "Plan more")));
    assert_eq!(signature(&items[0].1), json!({"anchor":"thought"}));
    assert!(matches!(&items[1].1, Item::Message(m)
        if m.role == MessageRole::Assistant
            && matches!(&m.parts[0].content, ContentPart::Text(t) if t.as_str() == "Reading.")));
    assert!(matches!(&items[2].1, Item::ToolCall(call)
        if call.replay.as_ref().is_some_and(|r|r.format()==ReplayFormat::GoogleGenerateContentSignature&&r.replay_token()==Some("U0lH"))));
    assert!(matches!(&items[3].1, Item::ToolCall(call) if call.replay.is_none()));
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
    let (encoded, _) = encode_generation_request(
        &next_request(&observed),
        &observed.fidelity,
        &target(),
        None,
    )
    .unwrap();
    assert_eq!(
        &encoded["contents"][1]["parts"].as_array().unwrap()[..2],
        &[
            json!({"thought":true,"text":"Plan","thoughtSignature":"U0lHMQ=="}),
            json!({"thought":true,"text":" more","thoughtSignature":"U0lHMg=="}),
        ]
    );
    let blocked = wire(&[json!({
        "responseId":"blocked","modelVersion":"gemini-synthetic",
        "promptFeedback":{"blockReason":"SAFETY","safetyRatings":[{
            "category":"HARM_CATEGORY_HARASSMENT","probability":"HIGH","blocked":true
        }]},"candidates":[]
    })]);
    let expected = SafetyReport {
        subject: SafetySubject::Input,
        ratings: vec![SafetyRating {
            category: SafetyCategory::Harassment,
            probability: Some(SafetyProbability::High),
            blocked: Some(true),
        }],
        reason: Some(SafetyBlockReason::Safety),
        message: None,
    };
    let mut decoder = GenerationSseDecoder::new(
        200,
        "text/event-stream",
        SseLimits::default(),
        LocalScope::new(2),
        target(),
    )
    .unwrap();
    let (_, events) = decoder.consume(&blocked).unwrap();
    assert!(events.contains(&StreamEvent::SafetyReport(expected.clone())));
    let denied = decoder.finish().unwrap();
    assert!(denied.semantic.items().is_empty());
    assert_eq!(denied.semantic.outcome(), Outcome::Incomplete);
    assert_eq!(
        denied.semantic.details().incomplete,
        Some(IncompleteReason::ContentFilter)
    );
    assert_eq!(
        denied.semantic.safety_reports(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        denied
            .semantic
            .clone()
            .with_items(vec![])
            .unwrap()
            .safety_reports(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        morphiecore::protocol::openai::lowering::lower_response(
            &denied.semantic,
            &denied.fidelity,
            &denied.metadata,
            Profile::Responses,
            morphiecore::protocol::openai::TargetCaps::full()
        )
        .err()
        .unwrap(),
        morphiecore::lowering::generation::RepresentationError::Safety
    );
    let mut metadata = denied.metadata.clone();
    metadata.created = Some(1.into());
    let mut openai =
        morphiecore::protocol::openai::events::EventEncoder::new(Profile::Responses, metadata)
            .unwrap();
    openai
        .encode(&StreamEvent::Started, &denied.fidelity)
        .unwrap();
    assert!(
        openai
            .encode(
                &StreamEvent::SafetyReport(expected.clone()),
                &denied.fidelity
            )
            .is_err()
    );
    assert!(
        openai
            .encode(events.last().unwrap(), &denied.fidelity)
            .is_err()
    );
    let mut messages =
        morphiecore::protocol::anthropic::client_events::ClientEventEncoder::new("r", "m");
    messages.encode(&StreamEvent::Started).unwrap();
    assert!(
        messages
            .encode(&StreamEvent::SafetyReport(expected.clone()))
            .is_err()
    );
    assert!(messages.encode(events.last().unwrap()).is_err());
    assert!(messages.finish().is_err());
    assert!(
        denied
            .semantic
            .clone()
            .with_safety_reports(vec![expected.clone(); MAX_ITEMS + 1])
            .is_err()
    );
    let mut reports_state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
    for _ in 0..MAX_ITEMS {
        reports_state = reduce(reports_state, StreamEvent::SafetyReport(expected.clone())).unwrap();
    }
    assert!(matches!(
        reduce(reports_state, StreamEvent::SafetyReport(expected.clone())),
        Err(EventError::Limit)
    ));
    {
        let mut report = expected.clone();
        report.message = Some(Text::new("assessment", "fixture", 64).unwrap());
        let report_bytes = std::mem::size_of::<SafetyReport>()
            + std::mem::size_of::<SafetyRating>()
            + "assessment".len();
        assert_eq!(report.validate().unwrap(), report_bytes);
        // Each text part and the report fit alone; together they reach the shared limit.
        let items = |tail_bytes| {
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    status: ItemLifecycle::Completed,
                    phase: None,
                    parts: [MAX_TEXT_BYTES, tail_bytes]
                        .into_iter()
                        .enumerate()
                        .map(|(index, bytes)| Part {
                            replay: None,
                            id: PartId::new(index as u64 + 1),
                            cache: None,
                            content: ContentPart::Text(
                                Text::new("x".repeat(bytes), "fixture", MAX_TEXT_BYTES)
                                    .unwrap()
                                    .into(),
                            ),
                        })
                        .collect(),
                }),
            )]
        };
        let tail_bytes = MAX_TOTAL_BYTES - MAX_TEXT_BYTES - report_bytes;
        let report_only = GenerationResponse::new(vec![], Outcome::Completed)
            .unwrap()
            .with_safety_reports(vec![report.clone()])
            .unwrap();
        let at_limit = report_only.with_items(items(tail_bytes)).unwrap();
        assert_eq!(at_limit.safety_reports(), std::slice::from_ref(&report));
        let item = ItemId::new(1);
        let mut state = reduce(StreamState::new(), StreamEvent::Started).unwrap();
        state = reduce(
            state,
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
        )
        .unwrap();
        let Item::Message(message) = &at_limit.items()[0].1 else {
            panic!("message")
        };
        for part in &message.parts {
            state = reduce(
                state,
                StreamEvent::PartStarted {
                    item,
                    part: part.id,
                    kind: PartKind::Text,
                },
            )
            .unwrap();
            let ContentPart::Text(text) = &part.content else {
                panic!("text")
            };
            state = reduce(
                state,
                StreamEvent::Delta {
                    item,
                    part: part.id,
                    fragment: text.as_str().to_owned(),
                    logprobs: vec![],
                },
            )
            .unwrap();
        }
        let content_only =
            GenerationResponse::new(items(tail_bytes + 1), Outcome::Completed).unwrap();
        assert!(matches!(
            at_limit.with_items(content_only.items().to_vec()),
            Err(GenerationError::Limit)
        ));
        assert!(matches!(
            content_only.with_safety_reports(vec![report.clone()]),
            Err(GenerationError::Limit)
        ));

        let with_report = reduce(state.clone(), StreamEvent::SafetyReport(report.clone())).unwrap();
        assert_eq!(with_report.safety_reports(), std::slice::from_ref(&report));
        let extra = StreamEvent::Delta {
            item,
            part: PartId::new(2),
            fragment: "x".into(),
            logprobs: vec![],
        };
        assert!(matches!(
            reduce(with_report, extra.clone()),
            Err(EventError::Limit)
        ));
        let content_only = reduce(state, extra).unwrap();
        assert!(matches!(
            reduce(content_only, StreamEvent::SafetyReport(report)),
            Err(EventError::Limit)
        ));
    }
    let mut private_report = expected;
    private_report.message = Some(Text::new("private-assessment-detail", "fixture", 64).unwrap());
    assert!(!format!("{private_report:?}").contains("private-assessment-detail"));

    let mut reports = frames();
    reports[0]["candidates"][0]["safetyRatings"] = json!([{
        "category":"HARM_CATEGORY_HARASSMENT","probability":"LOW","blocked":false
    }]);
    reports[3]["candidates"][0]["safetyRatings"] = json!([{
        "category":"HARM_CATEGORY_HARASSMENT","probability":"MEDIUM"
    }, {}]);
    let evaluated = decode(&wire(&reports)).unwrap();
    assert_eq!(
        evaluated.semantic.safety_reports(),
        &[
            SafetyReport {
                subject: SafetySubject::Output,
                ratings: vec![SafetyRating {
                    category: SafetyCategory::Harassment,
                    probability: Some(SafetyProbability::Low),
                    blocked: Some(false)
                }],
                reason: None,
                message: None
            },
            SafetyReport {
                subject: SafetySubject::Output,
                ratings: vec![
                    SafetyRating {
                        category: SafetyCategory::Harassment,
                        probability: Some(SafetyProbability::Medium),
                        blocked: Some(false)
                    },
                    SafetyRating {
                        category: SafetyCategory::Unspecified,
                        probability: Some(SafetyProbability::Unspecified),
                        blocked: Some(false)
                    },
                ],
                reason: None,
                message: None
            },
        ]
    );
    assert_eq!(evaluated.semantic.outcome(), Outcome::Completed);
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
    let mut safety = frames();
    safety[0]["candidates"][0]["safetyRatings"] = json!([{
        "category":"HARM_CATEGORY_HARASSMENT","probability":7,"blocked":false
    }]);
    let mut blocked_with_output = frames();
    blocked_with_output[0]["promptFeedback"] = json!({"blockReason":"SAFETY"});
    let mut drift = frames();
    drift[1]["responseId"] = json!("different-response");
    drift[1]["modelVersion"] = json!("different-model");
    let mut malformed_identity = frames();
    malformed_identity[1]["modelVersion"] = json!(7);
    for bytes in [
        wire(&missing_id),
        wire(&late),
        wire(unfinished),
        wire(&known_type),
        wire(&probabilities),
        wire(&safety),
        wire(&blocked_with_output),
        wire(&drift),
        wire(&malformed_identity),
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
                cache: None,
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
    let (body, _) = encode_generation_request(&history, &tail.fidelity, &target(), None).unwrap();
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
    assert!(encode_generation_request(&edited, &tail.fidelity, &target(), None).is_err());

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
    let (body, rules) =
        encode_generation_request(&request, &observed.fidelity, &target(), None).unwrap();
    assert_eq!(body, expected);
    assert!(rules.is_empty());
    // A function without a parameter schema keeps that shape.
    let mut bare = request.settings().clone();
    bare.tools = Some(vec![ToolDefinition::Function(FunctionTool {
        name: Text::new("read", "fixture", 64).unwrap(),
        description: None,
        parameters: None,
        strict: FunctionStrictness::Omitted(StrictDefault::NonStrict),
        output_schema: None,
        dispatch: Default::default(),
        cache: None,
        eager_input: None,
    })]);
    let bare = request.clone().with_settings(bare).unwrap();
    let (body, _) = encode_generation_request(&bare, &observed.fidelity, &target(), None).unwrap();
    assert_eq!(
        body["tools"],
        json!([{"functionDeclarations":[{"name":"read"}]}])
    );
    let mut scalar = bare.settings().clone();
    let Some(ToolDefinition::Function(tool)) = scalar.tools.as_mut().and_then(|v| v.first_mut())
    else {
        panic!("function");
    };
    tool.parameters = Some(json!({"type":"string"}).into());
    let scalar = bare.with_settings(scalar).unwrap();
    assert!(matches!(
        encode_generation_request(&scalar, &observed.fidelity, &target(), None),
        Err(morphiecore::protocol::CodecError::Unsupported(reason))
            if reason == "Gemini semantic carrier"
    ));
    let other = ReplayTarget::new("synthetic-google", "gemini-other").unwrap();
    assert!(encode_generation_request(&request, &observed.fidelity, &other, None).is_err());
    let mut settings = request.settings().clone();
    settings.text = TextOptions {
        presence: morphiecore::semantic::value::Presence::Value(()),
        format: morphiecore::semantic::value::Presence::Value(OutputConstraint::Text),
        verbosity: morphiecore::semantic::value::Presence::Value(Verbosity::Low),
    };
    let advisory = request.clone().with_settings(settings.clone()).unwrap();
    let (body, rules) =
        encode_generation_request(&advisory, &observed.fidelity, &target(), None).unwrap();
    assert_eq!(body, expected);
    assert_eq!(
        rules.iter().map(|r| r.name()).collect::<Vec<_>>(),
        ["gemini.omit-verbosity.v1"]
    );
    assert_eq!(advisory.settings(), &settings);
    settings.text.format =
        morphiecore::semantic::value::Presence::Value(OutputConstraint::JsonObject);
    let constrained = request.clone().with_settings(settings).unwrap();
    assert!(encode_generation_request(&constrained, &observed.fidelity, &target(), None).is_err());
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
    let (body, rules) = encode_generation_request(
        &source.task.semantic,
        &source.task.fidelity,
        &target(),
        None,
    )
    .unwrap();
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
    let mut controlled = source.task.semantic.settings().clone();
    controlled.controls.top_k = Some(4);
    controlled.controls.seed = Some(0);
    controlled.controls.stop_sequences = vec![Text::new("END", "fixture", 64).unwrap()];
    controlled.controls = controlled
        .controls
        .with_presence_penalty(0.25)
        .unwrap()
        .with_frequency_penalty(-0.5)
        .unwrap();
    let controlled = source
        .task
        .semantic
        .clone()
        .with_settings(controlled.clone())
        .unwrap();
    let (wire, _) =
        encode_generation_request(&controlled, &source.task.fidelity, &target(), None).unwrap();
    assert_eq!(
        wire["generationConfig"],
        json!({
            "responseMimeType":"application/json","responseJsonSchema":schema,
            "responseLogprobs":true,"logprobs":1,
            "topK":4,"seed":0,"stopSequences":["END"],"presencePenalty":0.25,"frequencyPenalty":-0.5
        })
    );
    assert!(
        morphiecore::protocol::google::semantic_contract()
            .check(&controlled)
            .is_err()
    );
    assert!(
        morphiecore::protocol::interactions::encode_generation_request(
            &controlled,
            &source.task.fidelity,
            &target()
        )
        .is_err()
    );
    let mut invalid_controls = controlled.settings().clone();
    invalid_controls.controls.seed = Some(i64::from(i32::MAX) + 1);
    let out_of_range = controlled
        .clone()
        .with_settings(invalid_controls.clone())
        .unwrap();
    assert!(
        encode_generation_request(&out_of_range, &source.task.fidelity, &target(), None).is_err()
    );
    assert_eq!(out_of_range.controls().seed, Some(i64::from(i32::MAX) + 1));
    invalid_controls.controls.seed = Some(i64::from(i32::MIN));
    let signed = controlled
        .clone()
        .with_settings(invalid_controls.clone())
        .unwrap();
    assert_eq!(
        encode_generation_request(&signed, &source.task.fidelity, &target(), None)
            .unwrap()
            .0["generationConfig"]["seed"],
        i32::MIN
    );
    invalid_controls.controls.top_k = Some(0);
    assert!(controlled.clone().with_settings(invalid_controls).is_err());
    assert!(
        GenerationControls::default()
            .with_presence_penalty(f64::NAN)
            .is_err()
    );
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
    assert!(encode_generation_request(&invalid, &source.task.fidelity, &target(), None).is_err());

    let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(10);
    let expiry = now + std::time::Duration::from_nanos(1);
    let reference = CachedContextReference::new(
        Text::new("cachedContents/example", "fixture", 256).unwrap(),
        Some(expiry),
    )
    .unwrap();
    let mut cached = client(
        Profile::Responses,
        json!({"model":"public","input":"question","max_output_tokens":16}),
    );
    cached.task.semantic = cached
        .task
        .semantic
        .with_cached_context(Some(reference.clone()))
        .unwrap();
    assert!(
        encode_generation_request(
            &cached.task.semantic,
            &cached.task.fidelity,
            &target(),
            Some(now)
        )
        .is_err()
    );
    cached
        .task
        .fidelity
        .record_cached_context(
            &reference,
            morphiecore::protocol::replay_envelope::issuer("synthetic-google", "gemini-synthetic"),
        )
        .unwrap();
    let observed = cached.clone();
    let (wire, rules) = encode_generation_request(
        &cached.task.semantic,
        &cached.task.fidelity,
        &target(),
        Some(now),
    )
    .unwrap();
    assert_eq!(
        wire,
        json!({
            "cachedContent":"cachedContents/example",
            "contents":[{"role":"user","parts":[{"text":"question"}]}],
            "generationConfig":{"maxOutputTokens":16}
        })
    );
    assert!(rules.is_empty());
    assert_eq!(cached, observed);
    assert!(
        encode_generation_request(
            &cached.task.semantic,
            &cached.task.fidelity,
            &target(),
            None
        )
        .is_err()
    );
    assert!(
        encode_generation_request(
            &cached.task.semantic,
            &cached.task.fidelity,
            &target(),
            Some(expiry)
        )
        .is_err()
    );
    assert!(
        encode_generation_request(
            &cached.task.semantic,
            &cached.task.fidelity,
            &ReplayTarget::new("other", "gemini-synthetic").unwrap(),
            Some(now)
        )
        .is_err()
    );
    let changed = cached
        .task
        .semantic
        .clone()
        .with_cached_context(Some(
            CachedContextReference::new(
                Text::new("cachedContents/other", "fixture", 256).unwrap(),
                Some(expiry),
            )
            .unwrap(),
        ))
        .unwrap();
    assert!(
        encode_generation_request(&changed, &cached.task.fidelity, &target(), Some(now)).is_err()
    );
    let unreported_expiry = CachedContextReference::new(
        Text::new("cachedContents/example", "fixture", 256).unwrap(),
        None,
    )
    .unwrap();
    let no_expiry = cached
        .task
        .semantic
        .clone()
        .with_cached_context(Some(unreported_expiry.clone()))
        .unwrap();
    assert!(encode_generation_request(&no_expiry, &cached.task.fidelity, &target(), None).is_err());
    let mut unreported = morphiecore::protocol::fidelity::FidelityRecords::default();
    unreported
        .record_cached_context(
            &unreported_expiry,
            morphiecore::protocol::replay_envelope::issuer("synthetic-google", "gemini-synthetic"),
        )
        .unwrap();
    assert_eq!(
        encode_generation_request(&no_expiry, &unreported, &target(), None)
            .unwrap()
            .0,
        wire
    );
    let mut settings = cached.task.semantic.settings().clone();
    settings.instructions = morphiecore::semantic::value::Presence::Value(
        Text::new("replacement instruction", "fixture", 256).unwrap(),
    );
    assert!(
        encode_generation_request(
            &cached
                .task
                .semantic
                .clone()
                .with_settings(settings)
                .unwrap(),
            &cached.task.fidelity,
            &target(),
            Some(now)
        )
        .is_err()
    );
    assert!(
        morphiecore::protocol::google::semantic_contract()
            .check(&cached.task.semantic)
            .is_err()
    );
    assert!(
        morphiecore::protocol::google::representation_contract()
            .check(&cached.task.semantic)
            .is_ok()
    );
    assert!(
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_request(
                &cached,
                "upstream",
                &morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
    );
    assert!(
        morphiecore::protocol::anthropic::Profile::AdaptiveTextTools
            .encode_generation_request(&cached.task.semantic, &cached.task.fidelity, &target())
            .is_err()
    );
    assert!(
        morphiecore::protocol::interactions::encode_generation_request(
            &cached.task.semantic,
            &cached.task.fidelity,
            &target()
        )
        .is_err()
    );
    let mut policy = client(
        Profile::Responses,
        json!({"model":"public","input":"question","max_output_tokens":16}),
    );
    let mut settings = policy.task.semantic.settings().clone();
    settings.safety = vec![
        SafetySetting {
            category: SafetyCategory::Harassment,
            threshold: SafetyThreshold::MediumAndAbove,
        },
        SafetySetting {
            category: SafetyCategory::HateSpeech,
            threshold: SafetyThreshold::Disabled,
        },
        SafetySetting {
            category: SafetyCategory::DangerousContent,
            threshold: SafetyThreshold::BlockNone,
        },
    ];
    policy.task.semantic = policy
        .task
        .semantic
        .with_settings(settings.clone())
        .unwrap();
    assert_eq!(
        encode_generation_request(
            &policy.task.semantic,
            &policy.task.fidelity,
            &target(),
            None
        )
        .unwrap()
        .0,
        json!({
            "contents":[{"role":"user","parts":[{"text":"question"}]}],
            "generationConfig":{"maxOutputTokens":16},
            "safetySettings":[
                {"category":"HARM_CATEGORY_HARASSMENT","threshold":"BLOCK_MEDIUM_AND_ABOVE"},
                {"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"OFF"},
                {"category":"HARM_CATEGORY_DANGEROUS_CONTENT","threshold":"BLOCK_NONE"}
            ]
        })
    );
    assert!(
        morphiecore::protocol::google::semantic_contract()
            .check(&policy.task.semantic)
            .is_err()
    );
    assert!(
        morphiecore::protocol::google::representation_contract()
            .check(&policy.task.semantic)
            .is_ok()
    );
    assert!(
        Adapter::new(Profile::Responses, Dialect::Standard, None)
            .encode_request(
                &policy,
                "upstream",
                &morphiecore::protocol::openai::TargetCaps::full()
            )
            .is_err()
    );
    assert!(
        morphiecore::protocol::anthropic::Profile::AdaptiveTextTools
            .encode_generation_request(&policy.task.semantic, &policy.task.fidelity, &target())
            .is_err()
    );
    assert!(
        morphiecore::protocol::interactions::encode_generation_request(
            &policy.task.semantic,
            &policy.task.fidelity,
            &target()
        )
        .is_err()
    );
    settings.safety.push(settings.safety[0]);
    assert!(
        policy
            .task
            .semantic
            .clone()
            .with_settings(settings)
            .is_err()
    );
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
    let (body, rules) = encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &target(),
        None,
    )
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
            encode_generation_request(
                &invalid.task.semantic,
                &invalid.task.fidelity,
                &target(),
                None
            )
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
    let (body, rules) = encode_generation_request(
        &request.task.semantic,
        &request.task.fidelity,
        &target(),
        None,
    )
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
        encode_generation_request(
            &detailed.task.semantic,
            &detailed.task.fidelity,
            &target(),
            None
        )
        .is_err()
    );
    let named = client(
        Profile::Responses,
        json!({"model":"gemini","input":[
        {"role":"user","content":[{"type":"input_file","filename":"report.pdf",
        "file_data":"data:application/pdf;base64,AQID"}]}]}),
    );
    assert!(
        encode_generation_request(&named.task.semantic, &named.task.fidelity, &target(), None)
            .is_err()
    );
    // FunctionResponsePart has no fileData.
    let remote = client(Profile::Responses, input("https://example.invalid/a.png"));
    assert!(
        encode_generation_request(
            &remote.task.semantic,
            &remote.task.fidelity,
            &target(),
            None
        )
        .is_err()
    );
    let unsupported = client(Profile::Responses, input("data:image/gif;base64,AQID"));
    assert!(
        encode_generation_request(
            &unsupported.task.semantic,
            &unsupported.task.fidelity,
            &target(),
            None,
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
        encode_generation_request(&pdf.task.semantic, &pdf.task.fidelity, &target(), None).unwrap();
    assert_eq!(
        body["contents"][1]["parts"][0]["functionResponse"]["parts"],
        json!([{"inlineData":{"mimeType":"application/pdf","data":"BAUG"}}])
    );
    let video = part(json!({"type":"input_video","video_url":"data:video/mp4;base64,BAUG"}));
    assert!(
        encode_generation_request(&video.task.semantic, &video.task.fidelity, &target(), None)
            .is_err()
    );
}

#[test]
fn image_sizes_remain_strict() {
    let messages = |oversized: &str| {
        let raw = json!({"model":"gemini","max_tokens":8,"messages":[{"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID"},
                "transformations":{"oversized_image":oversized}}]}]});
        morphiecore::protocol::anthropic::client::decode_client_request(raw.to_string().as_bytes())
            .unwrap()
    };
    for (request, ok) in [(messages("downsize"), true), (messages("error"), false)] {
        let result = encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &target(),
            None,
        );
        assert_eq!(result.is_ok(), ok);
    }
}

#[test]
fn request_maps_tool_choice_effort_and_leading_instructions() {
    let encode = |extra: Value| {
        let mut body = json!({"model":"gemini","messages":[
            {"role":"developer","content":"Be brief."},
            {"role":"user","content":"go"}],
            "tools":[{"type":"function","function":{"name":"read","parameters":{"type":"object"}}}]});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let request = client(Profile::Chat, body);
        let (wire, rules) = encode_generation_request(
            &request.task.semantic,
            &request.task.fidelity,
            &target(),
            None,
        )
        .unwrap();
        (wire, rules.iter().map(|r| r.name()).collect::<Vec<_>>())
    };
    let (wire, rules) = encode(json!({}));
    assert_eq!(
        wire["systemInstruction"],
        json!({"parts":[{"text":"Be brief."}]})
    );
    assert_eq!(
        wire["contents"],
        json!([{"role":"user","parts":[{"text":"go"}]}])
    );
    assert_eq!(rules, ["gemini.developer-as-system.v1"]);
    assert!(wire.get("toolConfig").is_none());
    for (choice, config) in [
        (json!("auto"), json!({"mode":"AUTO"})),
        (json!("none"), json!({"mode":"NONE"})),
        (json!("required"), json!({"mode":"ANY"})),
        (
            json!({"type":"function","function":{"name":"read"}}),
            json!({"mode":"ANY","allowedFunctionNames":["read"]}),
        ),
    ] {
        let (wire, _) = encode(json!({"tool_choice": choice}));
        assert_eq!(
            wire["toolConfig"],
            json!({"functionCallingConfig": config}),
            "{choice}"
        );
    }
    for (effort, level) in [
        ("minimal", "MINIMAL"),
        ("low", "LOW"),
        ("medium", "MEDIUM"),
        ("high", "HIGH"),
    ] {
        let (wire, _) = encode(json!({"reasoning_effort": effort}));
        assert_eq!(
            wire["generationConfig"],
            json!({"thinkingConfig":{"thinkingLevel":level}}),
            "{effort}"
        );
    }
}
