//! Target-only projection of native Messages facts onto the OpenAI client carriers.
use crate::{
    protocol::{CodecError, DecodedResponse, openai::Profile},
    semantic::task::generation::*,
};
mod events;
use crate::lowering::generation::LossEvidence;
pub(crate) use events::EventProjection;

fn envelope_loss(profile: Profile, direction: &'static str) -> LossEvidence {
    LossEvidence {
        rule: match profile {
            Profile::Responses => "responses.omit-message-envelopes.v1",
            Profile::Chat => "chat.regroup-native-message-envelopes.v1",
        },
        field: "generation.message_envelopes",
        direction,
        action: if profile == Profile::Responses {
            "omit"
        } else {
            "regroup"
        },
    }
}
fn usage_loss(
    before: Option<Usage>,
    after: Option<Usage>,
    direction: &'static str,
) -> Option<LossEvidence> {
    (before != after).then_some(LossEvidence {
        rule: "openai.native-usage-view.v1",
        field: "generation.usage",
        direction,
        action: if after.is_some() { "project" } else { "omit" },
    })
}

fn client_usage(usage: Usage) -> Result<Option<Usage>, CodecError> {
    // Gemini already reports cache-inclusive input and its own total.
    if usage.input_relation == InputTokenRelation::IncludesCache {
        return Ok(Some(usage));
    }
    let derived = usage.derive(UsageFormula::InputPlusCacheReadAndWrite)?;
    Ok(
        if let (Some(input), Some(output)) = (derived, usage.output_tokens) {
            let input = input.tokens();
            let total = input.checked_add(output).ok_or(CodecError::Limit)?;
            Some(Usage {
                cached_input_tokens: usage.cached_input_tokens,
                input_cache_write_tokens: usage.input_cache_write_tokens,
                ..Usage::operation(input, output, total)
            })
        } else {
            None
        },
    )
}

/// A signature-only Gemini item names the part it belongs to, so Chat may carry it
/// ahead of the message whatever its observed position. Readable reasoning keeps its order.
pub(super) fn anchored(item: &ReasoningItem) -> bool {
    item.parts.is_empty()
        && item.replay.as_ref().is_some_and(|value| {
            let format = match crate::protocol::replay_envelope::open(value.as_str()) {
                Some((_, sealed, _)) => sealed,
                None => value.format(),
            };
            matches!(
                format,
                ReplayFormat::GoogleGenerateContentPart | ReplayFormat::GoogleInteractionsV1Thought
            )
        })
}

/// The caller supplies a private delivery copy; the native observation stays intact.
/// Container boundaries are omitted, object arguments are serialized exactly, and
/// stop progress is represented by OpenAI outcome plus the preserved tool calls.
pub(crate) fn project(
    decoded: &mut DecodedResponse,
    profile: Profile,
) -> Result<Vec<LossEvidence>, CodecError> {
    let source = &decoded.semantic;
    match source.progress() {
        InteractionProgress::Unreported | InteractionProgress::TurnFinished => {}
        InteractionProgress::AwaitingToolResults
            if source
                .items()
                .iter()
                .any(|(_, item)| matches!(item, Item::ToolCall(_))) => {}
        _ => return Err(CodecError::Unsupported("native progress projection".into())),
    }
    let mut items = source.items().to_vec();
    for (_, item) in &mut items {
        if let Item::ToolCall(call) = item {
            let ToolArguments::Structured(value) = &call.arguments else {
                return Err(CodecError::Invalid("native tool arguments"));
            };
            call.arguments = ToolArguments::Raw(
                serde_json::to_string(value.value())
                    .map_err(|_| CodecError::Invalid("native tool arguments"))?,
            );
        }
    }
    // Build a target representation rather than editing an observed call under
    // its source dependency receipts. JSON serialization above preserves its value.
    let mut projected =
        GenerationResponse::from_resources(items, source.outcome(), source.resources().clone())?
            .with_details(source.details().clone())?
            .with_safety_reports(source.safety_reports().to_vec())?
            // Derivation requires all disjoint input categories. Missing is not zero.
            .with_usage(source.usage().map(client_usage).transpose()?.flatten())?;
    if profile == Profile::Chat {
        let status = if source.outcome() == Outcome::Incomplete {
            ItemLifecycle::Incomplete
        } else {
            ItemLifecycle::Completed
        };
        let mut reasoning = vec![];
        let mut parts = vec![];
        let mut calls = vec![];
        let mut owner = None;
        for (id, item) in projected.items() {
            match item {
                Item::Reasoning(r) if anchored(r) => reasoning.push((*id, item.clone())),
                // Consecutive blocks such as thinking then redacted thinking keep their order.
                Item::Reasoning(_) if parts.is_empty() && calls.is_empty() => {
                    reasoning.push((*id, item.clone()))
                }
                Item::Message(message)
                    if calls.is_empty() && message.role == MessageRole::Assistant =>
                {
                    owner.get_or_insert(*id);
                    parts.extend(message.parts.clone());
                }
                Item::ToolCall(call) => {
                    let mut call = call.clone();
                    call.status = status;
                    calls.push((*id, Item::ToolCall(call)));
                }
                _ => return Err(CodecError::Unsupported("native block order on Chat".into())),
            }
        }
        let owner = match owner {
            Some(owner) => owner,
            None => ItemId::scoped(
                projected
                    .items()
                    .first()
                    .map_or(LocalScope::new(3), |(id, _)| id.scope()),
                projected
                    .items()
                    .iter()
                    .map(|(id, _)| id.get())
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(CodecError::Limit)?,
            ),
        };
        let message = Item::Message(Message {
            role: MessageRole::Assistant,
            parts,
            status,
            phase: None,
        });
        let mut members = vec![owner];
        members.extend(calls.iter().map(|(id, _)| *id));
        reasoning.push((owner, message));
        reasoning.extend(calls);
        projected = GenerationResponse::from_resources(
            reasoning,
            projected.outcome(),
            projected.resources().clone(),
        )?
        .with_details(projected.details().clone())?
        .with_safety_reports(projected.safety_reports().to_vec())?
        .with_usage(projected.usage())?
        .with_message_envelopes(vec![MessageEnvelope::new(
            GroupId::new(owner.scope(), owner.get()),
            MessageEnvelopeRole::Assistant,
            members,
        )?])?;
    }
    let mut losses = vec![];
    if source.message_envelopes() != projected.message_envelopes() {
        losses.push(envelope_loss(profile, "response.encode"));
    }
    losses.extend(usage_loss(
        source.usage(),
        projected.usage(),
        "response.encode",
    ));
    decoded.semantic = projected;
    decoded.fidelity.retain_owners(decoded.semantic.items());
    Ok(losses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{anthropic, replay_envelope};
    const BODY: &[u8] = br#"{"id":"native","type":"message","role":"assistant","model":"model","content":[{"type":"thinking","thinking":"plan","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"n":9007199254740993.125}}],"stop_reason":"tool_use","usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7}}"#;

    fn fixture() -> DecodedResponse {
        let profile = anthropic::Profile::AdaptiveTextTools;
        let target = crate::protocol::native::ReplayTarget::new("provider", "model").unwrap();
        profile
            .decode_generation_response(BODY, LocalScope::new(3), &target)
            .unwrap()
    }
    #[test]
    fn projection_keeps_native_observation_and_derives_only_the_client_view() {
        let source = fixture();
        let before = source.clone();
        let safety_settings = GenerationSettings {
            safety: vec![SafetySetting {
                category: SafetyCategory::Harassment,
                threshold: SafetyThreshold::MediumAndAbove,
            }],
            ..Default::default()
        };
        let terminal = StreamEvent::Terminal {
            terminal: StreamTerminal::Incomplete,
            details: TerminalDetails {
                incomplete: Some(IncompleteReason::MaxOutputTokens),
                ..Default::default()
            },
        };
        for profile in [Profile::Responses, Profile::Chat] {
            let mut client = source.clone();
            crate::execution::replay::seal_for_client(
                &mut client,
                &crate::execution::replay::CLIENT_ORIGIN,
            )
            .unwrap();
            project(&mut client, profile).unwrap();
            client.metadata.created = Some(1.into());
            let adapter = crate::protocol::openai::Adapter::new(
                profile,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            );
            let contract = crate::protocol::openai::TargetCaps {
                replay_origin: Some(crate::execution::replay::CLIENT_ORIGIN.clone()),
                ..crate::protocol::openai::TargetCaps::full()
            };
            let wire = adapter.encode_response(&client, &contract).unwrap();
            let client_adapter =
                crate::adapter::ClientAdapter::openai(adapter.clone(), contract.clone());
            let mut attempt = crate::execution::Attempt::new(
                crate::adapter::UpstreamAdapter::AnthropicMessages {
                    profile: anthropic::Profile::AdaptiveTextTools,
                    target: crate::protocol::native::ReplayTarget::new("provider", "model")
                        .unwrap(),
                },
                4096,
                crate::protocol::stream::SseLimits::default(),
            );
            attempt.begin(200, "application/json").unwrap();
            attempt.push(BODY).unwrap();
            attempt.finish().unwrap();
            let mut delivery = crate::execution::ResponseDelivery::new(
                client_adapter.clone(),
                "public",
                Default::default(),
                Default::default(),
                crate::protocol::openai::stream::Obfuscation::Disabled,
            );
            delivery.encode_json(&attempt).unwrap();
            assert!(
                delivery
                    .losses()
                    .iter()
                    .any(|loss| loss.field == "generation.message_envelopes")
            );
            assert!(
                delivery
                    .losses()
                    .iter()
                    .any(|loss| loss.field == "generation.usage")
            );
            let mut pending = crate::execution::ResponseDelivery::new(
                client_adapter.clone(),
                "public",
                Default::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: crate::semantic::value::Presence::Value(false),
                    ..Default::default()
                },
                crate::protocol::openai::stream::Obfuscation::Disabled,
            );
            let (owner, call) = source
                .semantic
                .items()
                .iter()
                .find_map(|(id, item)| match item {
                    Item::ToolCall(call) => Some((*id, call)),
                    _ => None,
                })
                .unwrap();
            let opened = pending
                .prepare_events(
                    &attempt,
                    &[
                        StreamEvent::Started,
                        StreamEvent::ItemStarted {
                            item: owner,
                            kind: ItemKind::ToolCall {
                                format: ArgumentFormat::Json,
                                call_id: call.call_id.clone(),
                                name: call.name.clone(),
                                context: call.context.clone(),
                            },
                            replay: None,
                        },
                    ],
                )
                .unwrap()
                .unwrap();
            let arguments = pending
                .prepare_events(
                    &attempt,
                    &[
                        StreamEvent::PartStarted {
                            item: owner,
                            part: PartId::new(20),
                            kind: PartKind::StructuredArguments,
                        },
                        StreamEvent::Delta {
                            item: owner,
                            part: PartId::new(20),
                            fragment: "{\"n\":9007199254740993.125}".into(),
                            logprobs: vec![],
                        },
                    ],
                )
                .unwrap()
                .unwrap();
            assert!(arguments.events.iter().any(|event| matches!(
                event,
                StreamEvent::PartStarted {
                    kind: PartKind::Arguments,
                    ..
                }
            )));
            assert!(pending.lifecycle().may_retry_or_fallback());
            assert!(pending.commit().is_err(), "preparation is not rendering");
            let mut frames = pending.encode_prepared(opened).unwrap();
            frames.extend(pending.encode_prepared(arguments).unwrap());
            let text = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
            assert!(text.contains(r#"{\"n\":9007199254740993.125}"#));
            pending.cancel();
            let mut metadata = source.metadata.clone();
            metadata.created = Some(1.into());
            let mut encoder = client_adapter
                .stream_encoder(
                    metadata.clone(),
                    Default::default(),
                    crate::semantic::context::StreamOptions {
                        include_obfuscation: crate::semantic::value::Presence::Value(false),
                        ..Default::default()
                    },
                    crate::protocol::openai::stream::Obfuscation::Disabled,
                )
                .unwrap();
            encoder
                .project_native(&[
                    StreamEvent::Usage(source.semantic.usage().unwrap()),
                    StreamEvent::MessageEnvelope(source.semantic.message_envelopes()[0].clone()),
                ])
                .unwrap();
            assert!(
                encoder
                    .losses()
                    .iter()
                    .any(|loss| loss.field == "generation.message_envelopes"
                        && loss.direction == "event.encode")
            );
            assert!(
                encoder.losses().iter().any(
                    |loss| loss.field == "generation.usage" && loss.direction == "event.encode"
                )
            );
            encoder
                .encode(&StreamEvent::Started, &source.fidelity)
                .unwrap();
            let mut reported = metadata.clone();
            reported.context.settings = Some(safety_settings.clone());
            assert!(encoder.update_metadata(reported).is_err());
            assert!(encoder.encode(&terminal, &source.fidelity).is_err());
            assert!(encoder.finish().is_err());
            if profile == Profile::Chat {
                let mut failed = client_adapter
                    .stream_encoder(
                        metadata,
                        Default::default(),
                        crate::semantic::context::StreamOptions {
                            include_obfuscation: crate::semantic::value::Presence::Value(false),
                            ..Default::default()
                        },
                        crate::protocol::openai::stream::Obfuscation::Disabled,
                    )
                    .unwrap();
                failed
                    .project_native(&[StreamEvent::Usage(source.semantic.usage().unwrap())])
                    .unwrap();
                let before = failed.losses();
                assert!(
                    failed
                        .project_native(&[
                            StreamEvent::ItemStarted {
                                item: ItemId::new(1),
                                replay: None,
                                kind: ItemKind::ToolCall {
                                    format: ArgumentFormat::Json,
                                    call_id: crate::semantic::value::Text::new("c", "test", 128)
                                        .unwrap(),
                                    name: crate::semantic::value::Text::new("lookup", "test", 128)
                                        .unwrap(),
                                    context: Default::default()
                                }
                            },
                            StreamEvent::ItemStarted {
                                item: ItemId::new(2),
                                replay: None,
                                kind: ItemKind::Message { phase: None }
                            }
                        ])
                        .is_err()
                );
                assert_eq!(failed.losses(), before);
            }
            let args = if profile == Profile::Responses {
                &wire["output"][1]["arguments"]
            } else {
                &wire["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            };
            assert_eq!(args, "{\"n\":9007199254740993.125}");
            assert_eq!(wire["usage"]["total_tokens"], 23);
            assert_eq!(client.semantic.progress(), InteractionProgress::Unreported);
            assert_eq!(source, before);
            assert_eq!(source.metadata.created, None);
            assert_eq!(source.semantic.usage().unwrap().total_tokens, None);
            let Item::Reasoning(r) = &client.semantic.items()[0].1 else {
                panic!()
            };
            assert_eq!(
                replay_envelope::open(r.replay.as_ref().unwrap().as_str())
                    .unwrap()
                    .2
                    .as_str(),
                "synthetic-signature"
            );
        }
        let mut metadata = source.metadata.clone();
        let mut messages = crate::adapter::ClientAdapter::Messages
            .stream_encoder(
                metadata.clone(),
                Default::default(),
                Default::default(),
                crate::protocol::openai::stream::Obfuscation::Disabled,
            )
            .unwrap();
        messages
            .encode(&StreamEvent::Started, &source.fidelity)
            .unwrap();
        messages
            .encode(
                &StreamEvent::Usage(source.semantic.usage().unwrap()),
                &source.fidelity,
            )
            .unwrap();
        metadata.context.settings = Some(safety_settings);
        assert!(messages.update_metadata(metadata).is_err());
        assert!(messages.encode(&terminal, &source.fidelity).is_err());
        assert!(messages.finish().is_err());
        // A later report must not become visible in an earlier pending batch.
        let adapter = crate::protocol::openai::Adapter::new(
            Profile::Chat,
            crate::protocol::openai::Dialect::Standard,
            None,
        );
        let mut attempt = crate::execution::Attempt::new(adapter.clone(), 4096, Default::default());
        attempt.begin(200, "text/event-stream").unwrap();
        let mut delivery = crate::execution::ResponseDelivery::new(
            crate::adapter::ClientAdapter::openai(
                adapter,
                crate::protocol::openai::TargetCaps::full(),
            ),
            "public",
            Default::default(),
            crate::semantic::context::StreamOptions {
                include_obfuscation: crate::semantic::value::Presence::Value(false),
                ..Default::default()
            },
            crate::protocol::openai::stream::Obfuscation::Disabled,
        );
        let mut batches = vec![];
        for (fragment, fingerprint) in [("first", None), ("second", Some("later"))] {
            let mut chunk = serde_json::json!({
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
            let frames = delivery.encode_prepared(batch).unwrap();
            let wire = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
            let values: Vec<serde_json::Value> = wire
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
        let policy = crate::execution::delivery::DeltaCoalescing {
            max_delay_ms: 10,
            max_pending_bytes: 16384,
        };
        for protocol in [
            crate::adapter::ClientProtocol::Chat,
            crate::adapter::ClientProtocol::Responses,
            crate::adapter::ClientProtocol::Messages,
        ] {
            let source = crate::protocol::openai::Adapter::new(
                Profile::Chat,
                crate::protocol::openai::Dialect::Standard,
                None,
            );
            let client = match protocol {
                crate::adapter::ClientProtocol::Messages => crate::adapter::ClientAdapter::Messages,
                _ => crate::adapter::ClientAdapter::openai(
                    crate::protocol::openai::Adapter::new(
                        if protocol == crate::adapter::ClientProtocol::Chat {
                            Profile::Chat
                        } else {
                            Profile::Responses
                        },
                        crate::protocol::openai::Dialect::Standard,
                        None,
                    ),
                    crate::protocol::openai::TargetCaps::full(),
                ),
            };
            let mut attempt = crate::execution::Attempt::new(source, 65536, Default::default());
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = crate::execution::ResponseDelivery::new(
                client,
                "public",
                Default::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: crate::semantic::value::Presence::Value(false),
                    ..Default::default()
                },
                crate::protocol::openai::stream::Obfuscation::Disabled,
            )
            .with_coalescing(Some(policy));
            let now = std::time::Instant::now();
            let mut output = vec![];
            for (index, text) in ["first", "\"", "\\", "\n雪"].into_iter().enumerate() {
                let chunk = serde_json::json!({
                    "id":"reported","object":"chat.completion.chunk","created":1,"model":"model",
                    "choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]
                });
                let (_, events) = attempt
                    .push(format!("data: {chunk}\n\n").as_bytes())
                    .unwrap();
                let frames = delivery
                    .coalesce_events(
                        &attempt,
                        &events,
                        now + std::time::Duration::from_millis(index as u64),
                    )
                    .unwrap();
                if index == 0 {
                    assert!(!frames.is_empty(), "first content is never held");
                } else {
                    assert!(frames.is_empty());
                    assert_eq!(
                        delivery.flush_deadline(),
                        Some(now + std::time::Duration::from_millis(11))
                    );
                }
                output.extend(frames);
            }
            assert!(delivery.coalescing_peak_bytes() <= policy.max_pending_bytes);
            output.extend(delivery.flush_pending().unwrap());
            assert!(delivery.flush_deadline().is_none());
            let wire = String::from_utf8(output.into_iter().flatten().collect()).unwrap();
            let fragments: Vec<String> = wire
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter_map(|value| {
                    value
                        .pointer("/choices/0/delta/content")
                        .or_else(|| {
                            (value["type"] == "response.output_text.delta").then(|| &value["delta"])
                        })
                        .or_else(|| {
                            (value["delta"]["type"] == "text_delta")
                                .then(|| &value["delta"]["text"])
                        })
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                })
                .filter(|text| !text.is_empty())
                .collect();
            assert_eq!(fragments, ["first", "\"\\\n雪"]);
            let now = now + std::time::Duration::from_millis(20);
            let mut last_events = vec![];
            for (text, fingerprint) in [
                ("pending", None),
                ("", None),
                ("before", None),
                ("after", Some("later")),
            ] {
                let mut chunk = serde_json::json!({
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
                let frames = delivery.coalesce_events(&attempt, &events, now).unwrap();
                if text.is_empty() || fingerprint.is_some() {
                    assert!(!frames.is_empty(), "barriers drain the preceding run");
                    if protocol == crate::adapter::ClientProtocol::Chat && fingerprint.is_some() {
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
            if protocol == crate::adapter::ClientProtocol::Responses {
                let mut other = crate::execution::Attempt::new(
                    crate::protocol::openai::Adapter::new(
                        Profile::Chat,
                        crate::protocol::openai::Dialect::Standard,
                        None,
                    ),
                    4096,
                    Default::default(),
                );
                other.begin(200, "text/event-stream").unwrap();
                let (_, events) = other.push(
                    br#"data: {"id":"reported","object":"chat.completion.chunk","created":1,"model":"model","choices":[{"index":0,"delta":{"content":"other"},"finish_reason":null}]}

"#,
                ).unwrap();
                assert!(delivery.coalesce_events(&other, &events, now).is_err());
                assert!(delivery.flush_pending().is_err());
            }
            delivery.cancel();
            assert!(delivery.flush_pending().is_err());
        }
        // Escape expansion, maximum envelope labels and padding must not turn
        // independently valid chunks into a newly oversized merged frame.
        for max_pending_bytes in [16384, 1024] {
            let policy = crate::execution::delivery::DeltaCoalescing {
                max_pending_bytes,
                ..policy
            };
            let mut attempt = crate::execution::Attempt::new(
                crate::protocol::openai::Adapter::new(
                    Profile::Chat,
                    crate::protocol::openai::Dialect::Standard,
                    None,
                ),
                65536,
                Default::default(),
            );
            attempt.begin(200, "text/event-stream").unwrap();
            let mut delivery = crate::execution::ResponseDelivery::new(
                crate::adapter::ClientAdapter::openai(
                    crate::protocol::openai::Adapter::new(
                        Profile::Chat,
                        crate::protocol::openai::Dialect::Standard,
                        None,
                    ),
                    crate::protocol::openai::TargetCaps::full(),
                ),
                &"\\".repeat(256),
                crate::protocol::stream::SseLimits {
                    max_event_bytes: 8192,
                    ..Default::default()
                },
                Default::default(),
                crate::protocol::openai::stream::Obfuscation::Seeded([7; 32]),
            )
            .with_coalescing(Some(policy));
            let mut output = vec![];
            let now = std::time::Instant::now();
            for fragment in
                std::iter::once("first".to_owned()).chain(std::iter::repeat_n("\"".repeat(400), 10))
            {
                let chunk = serde_json::json!({
                    "id":"r".repeat(256),"object":"chat.completion.chunk","created":1,"model":"model",
                    "system_fingerprint":"\"".repeat(256),
                    "choices":[{"index":0,"delta":{"content":fragment},"finish_reason":null}]
                });
                let (_, events) = attempt
                    .push(format!("data: {chunk}\n\n").as_bytes())
                    .unwrap();
                output.extend(delivery.coalesce_events(&attempt, &events, now).unwrap());
            }
            output.extend(delivery.flush_pending().unwrap());
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
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
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
}
