//! Delivery snapshots, coalescing barriers and bounded wire output.
use super::*;
use crate::{
    protocol::openai::{Adapter, Dialect, Profile, TargetCaps},
    semantic::value::Presence,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn coalesce_events(
    delivery: &mut ResponseDelivery,
    attempt: &Attempt,
    events: &[StreamEvent],
    now: Instant,
) -> Result<Vec<Bytes>, AttemptError> {
    let mut frames = vec![];
    for batch in delivery.queue_events(attempt, events, now)? {
        frames.extend(
            delivery
                .encode_prepared(batch)
                .map(|batch| batch.into_bytes())?,
        );
    }
    Ok(frames)
}

#[test]
fn finalized_json_still_needs_handoff_and_cannot_resume_after_rejection() {
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 4096, Default::default());
    attempt.begin(200, "application/json").unwrap();
    attempt.push(br#"{"id":"r","object":"response","created_at":1,"model":"m","status":"completed","output":[]}"#).unwrap();
    attempt.finish().unwrap();
    for reject in [false, true] {
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(adapter.clone(), TargetCaps::full()),
            "public",
            Default::default(),
            StreamOptions::default(),
            Obfuscation::Disabled,
        );
        let body = delivery.encode_json(&attempt).unwrap();
        assert_eq!(
            delivery.lifecycle().state(),
            crate::execution::DeliveryState::Uncommitted
        );
        assert_eq!(delivery.snapshot(&attempt).unwrap().1, body);
        assert!(matches!(
            delivery.complete(&attempt),
            Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::DeliveryNotComplete
            ))
        ));
        if reject {
            assert!(matches!(
                delivery.encode_json(&attempt),
                Err(AttemptError::Protocol(
                    crate::execution::attempt::ProtocolReason::DeliveryClosed
                ))
            ));
            assert!(matches!(
                delivery.commit(),
                Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::NoDeliverableOutput
                ))
            ));
            assert!(matches!(
                delivery.complete(&attempt),
                Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::DeliveryNotComplete
                ))
            ));
        } else {
            delivery.commit().unwrap();
            delivery.complete(&attempt).unwrap();
            assert_eq!(
                delivery.lifecycle().state(),
                crate::execution::DeliveryState::Terminal
            );
        }
    }
}

#[test]
fn prepared_batches_keep_the_metadata_snapshot_from_their_intake() {
    // A later report must not become visible in an earlier pending batch.
    let adapter = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 4096, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::openai(adapter, TargetCaps::full()),
        "public",
        Default::default(),
        StreamOptions {
            include_obfuscation: Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    let mut batches = vec![];
    for (fragment, fingerprint) in [("first", None), ("second", Some("later"))] {
        let mut chunk = json!({
            "id":"reported", "object":"chat.completion.chunk", "created":1, "model":"model",
            "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
        });
        if let Some(fingerprint) = fingerprint {
            chunk["system_fingerprint"] = fingerprint.into();
        }
        let (_, events) = attempt
            .push(format!("data: {chunk}\n\n").as_bytes())
            .unwrap();
        batches.push(delivery.prepare_events(&attempt, &events).unwrap().unwrap());
    }
    for (index, batch) in batches.into_iter().enumerate() {
        let frames = delivery
            .encode_prepared(batch)
            .map(|batch| batch.into_bytes())
            .unwrap();
        let wire = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
        let values: Vec<Value> = wire
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(!values.is_empty());
        if index == 0 {
            assert!(values.iter().all(|v| v.get("system_fingerprint").is_none()));
            assert!(
                values
                    .iter()
                    .any(|v| v["choices"][0]["delta"]["content"] == "first")
            );
        } else {
            assert!(values.iter().all(|v| v["system_fingerprint"] == "later"));
            assert!(
                values
                    .iter()
                    .any(|v| v["choices"][0]["delta"]["content"] == "second")
            );
        }
    }
    delivery.cancel();
}

#[test]
fn lifecycle_reports_freeze_opening_batches_and_charge_both_observations() {
    use crate::semantic::context::{ReportPhase, ServiceClass};
    let mut adapter = Adapter::new(Profile::Responses, Dialect::OpenRouter, None);
    adapter.adaptation.rules.responses_lifecycle_reports = true;
    let mut attempt = Attempt::new(adapter, 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut delivery = ResponseDelivery::new(
        ClientAdapter::openai(
            Adapter::new(Profile::Responses, Dialect::MorphieCore, None),
            TargetCaps::full(),
        ),
        "public",
        Default::default(),
        StreamOptions {
            include_obfuscation: Presence::Value(false),
            ..Default::default()
        },
        Obfuscation::Disabled,
    );
    let opening = json!({"type":"response.created","sequence_number":0,"response":{"id":"reported","object":"response",
        "created_at":1,"model":"model","status":"in_progress","output":[],
        "reasoning":{"summary":"auto"},"service_tier":"auto"}});
    let (_, events) = attempt
        .push(format!("data: {opening}\n\n").as_bytes())
        .unwrap();
    let prepared = delivery.prepare_events(&attempt, &events).unwrap().unwrap();
    let opening_bytes = prepared.context.retained_bytes().unwrap();
    let terminal = json!({"type":"response.completed","sequence_number":1,"response":{"id":"reported","object":"response",
        "created_at":1,"model":"model","status":"completed","output":[],
        "reasoning":{"summary":"detailed","context":"all_turns","mode":"standard"},"service_tier":"default"}});
    let (_, events) = attempt
        .push(format!("data: {terminal}\n\n").as_bytes())
        .unwrap();
    assert!(events.is_empty(), "terminal remains EOF-gated");
    attempt.finish().unwrap();
    assert_eq!(
        prepared
            .context
            .metadata
            .context
            .reports
            .at(ReportPhase::Terminal),
        None
    );
    assert_eq!(
        prepared.context.metadata.context.service_class(),
        &Presence::Value(ServiceClass::ConfiguredProcessing)
    );
    let wire: Vec<_> = delivery
        .encode_prepared(prepared)
        .map(|batch| batch.into_bytes())
        .unwrap()
        .into_iter()
        .flatten()
        .collect();
    let opening: Value = serde_json::from_str(
        std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(opening["response"]["service_tier"], "auto");
    let frames = delivery
        .finish_stream(&attempt)
        .map(|batch| batch.into_bytes())
        .unwrap();
    let wire: Vec<_> = frames.into_iter().flatten().collect();
    let terminal: Value = serde_json::from_str(
        std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["response"]["service_tier"], "default");
    let retained = delivery
        .prepared_source
        .as_ref()
        .unwrap()
        .context
        .retained_bytes()
        .unwrap();
    assert!(
        retained > opening_bytes,
        "both retained reports count against coalescing"
    );
}

#[test]
fn classified_failure_after_opening_has_no_invented_terminal_reports() {
    // A classified failure after a native Responses opening has no upstream
    // terminal report. A standard error must not invent a failed Response.
    let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let mut attempt = Attempt::new(adapter.clone(), 65536, Default::default());
    attempt.begin(200, "text/event-stream").unwrap();
    let mut failure_delivery = ResponseDelivery::new(
        ClientAdapter::openai(adapter, TargetCaps::full()),
        "public",
        Default::default(),
        StreamOptions::default(),
        Obfuscation::Disabled,
    );
    let opening = json!({"type":"response.created","sequence_number":0,"response":{
        "id":"failed-report","object":"response","created_at":1,"model":"m",
        "status":"in_progress","output":[],"reasoning":{"summary":"auto"},"service_tier":"auto"}});
    let (_, events) = attempt
        .push(format!("data: {opening}\n\n").as_bytes())
        .unwrap();
    failure_delivery
        .encode_events(&attempt, &events)
        .map(|batch| batch.into_bytes())
        .unwrap();
    let frames = failure_delivery.fail_stream(&json!({"error":{
        "code":"context_length_exceeded","type":"invalid_request_error","message":"Context limit"}})).map(|batch|batch.into_bytes()).unwrap();
    let bytes: Vec<_> = frames.into_iter().flatten().collect();
    let value: Value = serde_json::from_str(
        std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("data: "))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["type"], "error");
    assert_eq!(value["sequence_number"], 2);
    assert_eq!(value["code"], "context_length_exceeded");
    assert_eq!(value["message"], "Context limit");
    assert!(value.get("response").is_none());
    assert!(
        failure_delivery
            .fail_stream(&json!({"error":{}}))
            .map(|batch| batch.into_bytes())
            .is_err()
    );
}

#[test]
fn classified_response_errors_obey_remaining_budgets_and_poison_failed_delivery() {
    let error = json!({"error":{
        "code":"server_is_overloaded","message":"x".repeat(4096),"param":null
    }});
    let make = |limits, started| {
        let adapter = Adapter::new(Profile::Responses, Dialect::Standard, None);
        let mut attempt = Attempt::new(adapter.clone(), 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(adapter, TargetCaps::full()),
            "public",
            limits,
            StreamOptions::default(),
            Obfuscation::Disabled,
        );
        let prefix = if started {
            let (_, events) = attempt.push(br#"data: {"type":"response.created","sequence_number":0,"response":{"id":"r","object":"response","created_at":1,"model":"m","status":"in_progress","output":[]}}

"#).unwrap();
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .unwrap()
        } else {
            vec![]
        };
        (
            attempt,
            delivery,
            prefix.iter().map(Bytes::len).sum::<usize>(),
        )
    };
    for started in [false, true] {
        let (_, mut reference, prefix_bytes) = make(SseLimits::default(), started);
        let error_bytes: usize = reference
            .fail_stream(&error)
            .map(|batch| batch.into_bytes())
            .unwrap()
            .iter()
            .map(Bytes::len)
            .sum();
        let event_count = if started { 3 } else { 1 };
        for limits in [
            SseLimits {
                max_events: event_count,
                ..Default::default()
            },
            SseLimits {
                max_wire_bytes: prefix_bytes + error_bytes,
                ..Default::default()
            },
            SseLimits {
                max_event_bytes: error_bytes,
                ..Default::default()
            },
        ] {
            let (_, mut delivery, _) = make(limits, started);
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_ok(),
                "exact fit"
            );
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_err(),
                "closed"
            );
        }
        let mut exceeded = vec![
            SseLimits {
                max_wire_bytes: prefix_bytes + error_bytes - 1,
                ..Default::default()
            },
            SseLimits {
                max_event_bytes: error_bytes - 1,
                ..Default::default()
            },
        ];
        if started {
            exceeded.push(SseLimits {
                max_events: event_count - 1,
                ..Default::default()
            });
        }
        for limits in exceeded {
            let (attempt, mut delivery, _) = make(limits, started);
            assert!(
                delivery
                    .fail_stream(&error)
                    .map(|batch| batch.into_bytes())
                    .is_err(),
                "budget must reject"
            );
            assert!(
                delivery
                    .fail_stream(&json!({"error":{"message":"small"}}))
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(
                delivery
                    .encode_events(&attempt, &[StreamEvent::Started])
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(
                delivery
                    .finish_stream(&attempt)
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
            assert!(delivery.commit().is_err());
        }
    }
}

#[test]
fn coalescing_preserves_first_content_barriers_and_attempt_isolation() {
    let policy = DeltaCoalescing {
        max_delay_ms: 10,
        max_pending_bytes: 16384,
    };
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        let source = Adapter::new(Profile::Chat, Dialect::Standard, None);
        let client = match protocol {
            ClientProtocol::Messages => ClientAdapter::Messages,
            _ => ClientAdapter::openai(
                Adapter::new(
                    if protocol == ClientProtocol::Chat {
                        Profile::Chat
                    } else {
                        Profile::Responses
                    },
                    Dialect::Standard,
                    None,
                ),
                TargetCaps::full(),
            ),
        };
        let mut attempt = Attempt::new(source, 65536, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            client,
            "public",
            Default::default(),
            StreamOptions {
                include_obfuscation: Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        )
        .with_coalescing(Some(policy));
        let now = Instant::now();
        let mut output = vec![];
        for (index, text) in ["first", "\"", "\\", "\n雪"].into_iter().enumerate() {
            let chunk = json!({
                "id":"reported","object":"chat.completion.chunk","created":1,"model":"model",
                "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]
            });
            let (_, events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            let frames = coalesce_events(
                &mut delivery,
                &attempt,
                &events,
                now + Duration::from_millis(index as u64),
            )
            .unwrap();
            if index == 0 {
                assert!(!frames.is_empty(), "first content is never held");
            } else {
                assert!(frames.is_empty());
                assert_eq!(
                    delivery.flush_deadline(),
                    Some(now + Duration::from_millis(11))
                );
            }
            output.extend(frames);
        }
        assert!(delivery.coalescing_peak_bytes() <= policy.max_pending_bytes);
        output.extend(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        assert!(delivery.flush_deadline().is_none());
        let wire = String::from_utf8(output.into_iter().flatten().collect()).unwrap();
        let fragments: Vec<String> = wire
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| {
                value
                    .pointer("/choices/0/delta/content")
                    .or_else(|| {
                        (value["type"] == "response.output_text.delta").then(|| &value["delta"])
                    })
                    .or_else(|| {
                        (value["delta"]["type"] == "text_delta").then(|| &value["delta"]["text"])
                    })
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
            .filter(|text| !text.is_empty())
            .collect();
        assert_eq!(fragments, ["first", "\"\\\n雪"]);
        let now = now + Duration::from_millis(20);
        let mut last_events = vec![];
        for (text, fingerprint) in [
            ("pending", None),
            ("", None),
            ("before", None),
            ("after", Some("later")),
        ] {
            let mut chunk = json!({
                "id":"reported","object":"chat.completion.chunk","created":1,"model":"model",
                "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]
            });
            if let Some(fingerprint) = fingerprint {
                chunk["system_fingerprint"] = fingerprint.into();
            }
            let (_, mut events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            if text.is_empty() {
                // Empty semantic deltas are barriers even when a source codec
                // normally normalizes away an empty native content field.
                let StreamEvent::Delta { item, part, .. } = &last_events[0] else {
                    panic!()
                };
                events = vec![StreamEvent::Delta {
                    item: *item,
                    part: *part,
                    fragment: String::new(),
                    logprobs: vec![],
                }];
            } else {
                last_events = events.clone();
            }
            let frames = coalesce_events(&mut delivery, &attempt, &events, now).unwrap();
            if text.is_empty() || fingerprint.is_some() {
                assert!(!frames.is_empty(), "barriers drain the preceding run");
                if protocol == ClientProtocol::Chat && fingerprint.is_some() {
                    assert!(
                        !String::from_utf8(frames.into_iter().flatten().collect())
                            .unwrap()
                            .contains("later")
                    );
                }
            } else {
                assert!(frames.is_empty());
            }
        }
        assert!(
            delivery.flush_deadline().is_some(),
            "new context starts a distinct run"
        );
        if protocol == ClientProtocol::Responses {
            let mut other = Attempt::new(
                Adapter::new(Profile::Chat, Dialect::Standard, None),
                4096,
                Default::default(),
            );
            other.begin(200, "text/event-stream").unwrap();
            let (_, events) = other.push(
                br#"data: {"id":"reported","object":"chat.completion.chunk","created":1,"model":"model","choices":[{"index":0,"delta":{"content":"other"},"finish_reason":null}]}

"#,
            ).unwrap();
            assert!(coalesce_events(&mut delivery, &other, &events, now).is_err());
            assert!(
                delivery
                    .flush_pending()
                    .map(|batch| batch.into_bytes())
                    .is_err()
            );
        }
        delivery.cancel();
        assert!(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .is_err()
        );
    }
}

#[test]
fn coalescing_respects_escaped_frame_and_pending_memory_limits() {
    let policy = DeltaCoalescing {
        max_delay_ms: 10,
        max_pending_bytes: 16384,
    };
    // Escape expansion, maximum envelope labels and padding must not turn
    // independently valid chunks into a newly oversized merged frame.
    for max_pending_bytes in [16384, 1024] {
        let policy = DeltaCoalescing {
            max_pending_bytes,
            ..policy
        };
        let mut attempt = Attempt::new(
            Adapter::new(Profile::Chat, Dialect::Standard, None),
            65536,
            Default::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = ResponseDelivery::new(
            ClientAdapter::openai(
                Adapter::new(Profile::Chat, Dialect::Standard, None),
                TargetCaps::full(),
            ),
            &"\\".repeat(256),
            SseLimits {
                max_event_bytes: 8192,
                ..Default::default()
            },
            Default::default(),
            Obfuscation::Seeded([7; 32]),
        )
        .with_coalescing(Some(policy));
        let mut output = vec![];
        let now = Instant::now();
        for fragment in
            std::iter::once("first".to_owned()).chain(std::iter::repeat_n("\"".repeat(400), 10))
        {
            let chunk = json!({
                "id":"r".repeat(256),"object":"chat.completion.chunk","created":1,"model":"model",
                "system_fingerprint":"\"".repeat(256),
                "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
            });
            let (_, events) = attempt
                .push(format!("data: {chunk}\n\n").as_bytes())
                .unwrap();
            output.extend(coalesce_events(&mut delivery, &attempt, &events, now).unwrap());
        }
        output.extend(
            delivery
                .flush_pending()
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        assert!(output.iter().all(|frame| frame.len() <= 8192));
        assert!(delivery.coalescing_peak_bytes() <= policy.max_pending_bytes);
        if max_pending_bytes == 1024 {
            assert_eq!(
                delivery.coalescing_peak_bytes(),
                0,
                "oversized contexts bypass buffering"
            );
        }
        let text: String = output
            .iter()
            .flat_map(|frame| std::str::from_utf8(frame).unwrap().lines())
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter_map(|v| {
                v["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(text, format!("first{}", "\"".repeat(4000)));
        delivery.cancel();
    }
}

#[test]
fn assessment_only_safety_reports_are_omitted_and_blocking_reports_reject() {
    use crate::adapter::{ClientProtocol, UpstreamAdapter};
    use crate::protocol::native::ReplayTarget;
    let wire = |blocked: bool| {
        let body = json!({"responseId":"r","modelVersion":"fixture-model","candidates":[{
            "content":{"role":"model","parts":[{"text":"OK"}]},"finishReason":"MAX_TOKENS",
            "safetyRatings":[{"category":"HARM_CATEGORY_HARASSMENT","probability":"NEGLIGIBLE",
                "blocked":blocked}]}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1,
            "totalTokenCount":4}});
        format!("data: {body}\n\n")
    };
    let run = |protocol: ClientProtocol, blocked: bool, stream: bool| {
        let mut attempt = Attempt::new(
            UpstreamAdapter::GoogleGenerateContent {
                target: ReplayTarget::new("fixture-google", "fixture-model").unwrap(),
            },
            1 << 20,
            Default::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let client = match protocol {
            ClientProtocol::Messages => ClientAdapter::Messages,
            _ => ClientAdapter::openai(
                Adapter::new(
                    if protocol == ClientProtocol::Chat {
                        Profile::Chat
                    } else {
                        Profile::Responses
                    },
                    Dialect::Standard,
                    None,
                ),
                TargetCaps::full(),
            ),
        };
        let mut delivery = ResponseDelivery::new(
            client,
            "public",
            Default::default(),
            StreamOptions {
                include_obfuscation: Presence::Value(false),
                ..Default::default()
            },
            Obfuscation::Disabled,
        );
        let (_, events) = attempt.push(wire(blocked).as_bytes()).unwrap();
        let body = if stream {
            delivery
                .encode_events(&attempt, &events)
                .map(|batch| batch.into_bytes())
                .and_then(|frames| {
                    delivery.commit()?;
                    attempt.finish()?;
                    Ok(frames
                        .into_iter()
                        .chain(
                            delivery
                                .finish_stream(&attempt)
                                .map(|batch| batch.into_bytes())?,
                        )
                        .collect::<Vec<_>>()
                        .concat())
                })
        } else {
            attempt.finish().unwrap();
            delivery.encode_json(&attempt)
        };
        (body, delivery.effects().to_vec())
    };
    for protocol in [
        ClientProtocol::Chat,
        ClientProtocol::Responses,
        ClientProtocol::Messages,
    ] {
        for stream in [false, true] {
            let (body, losses) = run(protocol, false, stream);
            let body = String::from_utf8(body.unwrap()).unwrap();
            assert!(
                body.contains("OK") && !body.contains("HARASSMENT"),
                "{protocol:?}"
            );
            assert!(
                losses
                    .iter()
                    .any(|loss| loss.rule() == "generation.omit-safety-assessment.v1"
                        && loss.field() == "response.safety_reports"),
                "{protocol:?} {stream}"
            );
            // Only the block flag differs from the delivered case above.
            let error = format!("{:?}", run(protocol, true, stream).0.unwrap_err());
            assert!(
                error.contains("Safety")
                    || error.contains("safety report")
                    || error.contains("event target representation"),
                "{protocol:?} {stream}: {error}"
            );
        }
    }
}
