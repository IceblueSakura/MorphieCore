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
    usage.validate()?;
    // An uncalibrated total relation cannot become OpenAI's exact sum contract.
    if usage.input_relation == InputTokenRelation::IncludesCache {
        return Ok((usage.total_relation == TotalTokenRelation::InputAndOutput
            && usage.output_relation == OutputTokenRelation::IncludesReasoning
            && usage.input_tokens.is_some()
            && usage.output_tokens.is_some()
            && usage.total_tokens.is_some())
        .then_some(usage));
    }
    let derived = usage.derive(UsageFormula::InputPlusCacheReadAndWrite)?;
    Ok(
        if let (Some(input), Some(output)) = (derived, usage.output_tokens) {
            let input = input.tokens();
            let total = input.checked_add(output).ok_or(CodecError::Limit)?;
            Some(Usage {
                cached_input_tokens: usage.cached_input_tokens,
                input_cache_write_tokens: usage.input_cache_write_tokens,
                reasoning_tokens: usage.reasoning_tokens,
                ..Usage::operation(input, output, total)
            })
        } else {
            None
        },
    )
}

/// The caller supplies a private delivery copy; the native observation stays intact.
/// Container boundaries are omitted, object arguments are serialized exactly, and
/// stop progress is represented by OpenAI outcome plus the preserved tool calls.
pub(crate) fn project(
    decoded: &mut DecodedResponse,
    profile: Profile,
) -> Result<Vec<LossEvidence>, CodecError> {
    let source = &decoded.semantic;
    let attached = source
        .items()
        .iter()
        .any(|(_, item)| item.has_replay_attachment())
        || profile == Profile::Chat && crate::protocol::phase::has_phase(source.items());
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
            match &call.arguments {
                ToolArguments::Structured(value) => {
                    call.arguments = ToolArguments::Raw(
                        serde_json::to_string(value.value())
                            .map_err(|_| CodecError::Invalid("native tool arguments"))?,
                    );
                }
                ToolArguments::Raw(raw)
                    if StructuredValue::from_bytes(raw.as_bytes())
                        .is_ok_and(|value| value.value().is_object()) => {}
                _ => return Err(CodecError::Invalid("native tool arguments")),
            }
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
    if attached {
        projected = projected.with_message_envelopes(source.message_envelopes().to_vec())?;
    }
    if profile == Profile::Chat && !attached {
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
        // Chat has one output text value. Coalesce only unannotated text; attachment
        // delivery retains the original layout separately before reaching this view.
        if parts.len() > 1 {
            let mut text = String::new();
            for part in &parts {
                let ContentPart::Text(value) = &part.content else {
                    return Err(CodecError::Unsupported(
                        "native Chat text coalescing".into(),
                    ));
                };
                if !value.is_plain() || part.cache.is_some() || part.replay.is_some() {
                    return Err(CodecError::Unsupported(
                        "native Chat text coalescing".into(),
                    ));
                }
                text.push_str(value.as_str());
            }
            parts[0].content = ContentPart::Text(
                crate::semantic::value::Text::allowing_empty(text, "Chat text", MAX_TEXT_BYTES)
                    .map_err(|_| CodecError::Limit)?
                    .into(),
            );
            parts.truncate(1);
        }
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
    if source.usage_reports().has_unrepresented_details() {
        losses.push(crate::protocol::usage::report_loss("response.encode"));
    }
    let coalesced = profile == Profile::Chat
        && !attached
        && source
            .items()
            .iter()
            .filter_map(|(_, item)| match item {
                Item::Message(message) => Some(message.parts.len()),
                _ => None,
            })
            .sum::<usize>()
            > 1;
    if coalesced || source.message_envelopes() != projected.message_envelopes() {
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
        let profile = anthropic::messages::Profile::AdaptiveTextTools;
        let target = crate::protocol::native::ReplayTarget::new("provider", "model").unwrap();
        profile
            .decode_generation_response(BODY, LocalScope::new(3), &target)
            .unwrap()
    }
    #[test]
    fn reports_without_scalar_views_have_equal_static_and_event_omission() {
        for input in [Some(10), None] {
            let mut counts = Usage::operation(0, 0, 0);
            counts.input_tokens = input;
            counts.output_tokens = None;
            counts.total_tokens = None;
            counts.output_relation = OutputTokenRelation::ExcludesReasoning;
            counts.total_relation = TotalTokenRelation::Unreported;
            let mut report = UsageReport::operation(counts);
            report.accounting = UsageAccounting::SplitGeneration {
                tool_input_tokens: None,
                tool_input_relation: ToolInputRelation::Unreported,
                total_relation: SplitTotalRelation::Unreported,
            };
            if input.is_none() {
                report.details.tool_requests.push(ToolUseCount {
                    kind: ToolUsageKind::WebSearch,
                    count: Some(2),
                });
            }
            report.validate().unwrap();
            let mut reports = UsageReports::default();
            reports.replace(report.clone()).unwrap();
            let mut source = fixture();
            source.semantic = source.semantic.with_usage_reports(reports).unwrap();
            let before = source.clone();
            let mut projected = source.clone();
            let losses = project(&mut projected, Profile::Responses).unwrap();
            assert!(projected.semantic.usage().is_none());
            assert!(
                losses
                    .iter()
                    .any(|r| r.rule == "generation.usage-report-view.v1")
            );
            let mut events = EventProjection::new(Profile::Responses);
            assert!(
                events
                    .project(&[StreamEvent::UsageReport(report)])
                    .unwrap()
                    .is_empty()
            );
            assert!(
                events
                    .losses()
                    .iter()
                    .any(|r| r.rule == "generation.usage-report-view.v1")
            );
            assert_eq!(source, before);
        }
    }
    #[test]
    fn projection_keeps_native_observation_and_derives_only_the_client_view() {
        let source = fixture();
        let before = source.clone();
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
    }

    #[test]
    fn native_stream_preparation_records_losses_and_rejects_unrepresentable_metadata() {
        let source = fixture();
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
            let adapter = crate::protocol::openai::Adapter::new(
                profile,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            );
            let contract = crate::protocol::openai::TargetCaps {
                replay_origin: Some(crate::execution::replay::CLIENT_ORIGIN.clone()),
                ..crate::protocol::openai::TargetCaps::full()
            };
            let client_adapter =
                crate::adapter::ClientAdapter::openai(adapter.clone(), contract.clone());
            let mut attempt = crate::execution::Attempt::new(
                crate::adapter::UpstreamAdapter::AnthropicMessages {
                    profile: anthropic::messages::Profile::AdaptiveTextTools,
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
                crate::protocol::openai::obfuscation::Obfuscation::Disabled,
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
                crate::protocol::openai::obfuscation::Obfuscation::Disabled,
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
                    crate::protocol::openai::obfuscation::Obfuscation::Disabled,
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
            reported
                .context
                .reports
                .latest_mut()
                .expect("report")
                .settings = Some(safety_settings.clone());
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
                        crate::protocol::openai::obfuscation::Obfuscation::Disabled,
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
        }
    }

    #[test]
    fn messages_stream_records_adaptations_and_stays_rejected_after_metadata_error() {
        let source = fixture();
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
        let mut metadata = source.metadata.clone();
        let mut messages = crate::adapter::ClientAdapter::Messages
            .stream_encoder(
                metadata.clone(),
                Default::default(),
                Default::default(),
                crate::protocol::openai::obfuscation::Obfuscation::Disabled,
            )
            .unwrap();
        messages
            .encode(&StreamEvent::Started, &source.fidelity)
            .unwrap();
        messages
            .encode(
                &StreamEvent::ItemStarted {
                    item: ItemId::new(1),
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
                &source.fidelity,
            )
            .unwrap();
        messages
            .encode(
                &StreamEvent::PartStarted {
                    item: ItemId::new(1),
                    part: PartId::new(1),
                    kind: PartKind::Refusal,
                },
                &source.fidelity,
            )
            .unwrap();
        let mut inclusive = Usage::operation(10, 3, 13);
        inclusive.cached_input_tokens = Some(2);
        inclusive.input_cache_write_tokens = Some(0);
        messages
            .encode(&StreamEvent::Usage(inclusive), &source.fidelity)
            .unwrap();
        assert!(
            messages
                .losses()
                .iter()
                .any(|loss| loss.rule == "messages.refusal-as-text.v1")
        );
        assert!(
            messages
                .losses()
                .iter()
                .any(|loss| loss.rule == "messages.usage-view.v1")
        );
        metadata
            .context
            .reports
            .latest_mut()
            .expect("report")
            .settings = Some(safety_settings);
        assert!(messages.update_metadata(metadata).is_err());
        assert!(messages.encode(&terminal, &source.fidelity).is_err());
        assert!(messages.finish().is_err());
    }
}
