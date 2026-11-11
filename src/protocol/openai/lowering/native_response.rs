//! Target-only projection of native Messages facts onto the OpenAI client carriers.
use crate::{
    protocol::{CodecError, openai::GenerationApi},
    semantic::task::generation::*,
};
mod events;
use crate::protocol::conversion::generation::Effect;
pub(crate) use events::EventProjection;

fn envelope_loss(
    profile: GenerationApi,
    direction: crate::protocol::conversion::generation::ConversionBoundary,
) -> Effect {
    Effect::Field {
        rule: crate::protocol::conversion::generation::RuleId::new(match profile {
            GenerationApi::Responses => "responses.omit-message-envelopes.v1",
            GenerationApi::Chat => "chat.regroup-native-message-envelopes.v1",
        }),
        field: crate::protocol::conversion::generation::Field::MessageEnvelopes,
        boundary: direction,
        change: if profile == GenerationApi::Responses {
            crate::protocol::conversion::generation::Change::Omit
        } else {
            crate::protocol::conversion::generation::Change::Approximate
        },
        owner: crate::protocol::conversion::generation::Owner::Root,
    }
}
pub(crate) fn usage_loss(
    before: Option<Usage>,
    after: Option<Usage>,
    direction: crate::protocol::conversion::generation::ConversionBoundary,
) -> Vec<crate::protocol::conversion::generation::Effect> {
    use crate::protocol::conversion::generation::{Change, Effect, Field};
    if before == after {
        return vec![];
    }
    let Some(after) = after else {
        return vec![Effect::new(
            "openai.omit-native-usage-view.v1",
            Field::UsageReports,
            direction,
            Change::Omit,
        )];
    };
    let Some(before) = before else {
        return vec![];
    };
    let mut effects = Vec::new();
    for (field, input, output, relation_changed) in [
        (
            Field::InputTokens,
            before.input_tokens,
            after.input_tokens,
            before.input_relation != after.input_relation,
        ),
        (
            Field::OutputTokens,
            before.output_tokens,
            after.output_tokens,
            before.output_relation != after.output_relation,
        ),
        (
            Field::TotalTokens,
            before.total_tokens,
            after.total_tokens,
            before.total_relation != after.total_relation,
        ),
        (
            Field::CachedTokens,
            before.cached_input_tokens,
            after.cached_input_tokens,
            false,
        ),
        (
            Field::CacheWriteTokens,
            before.input_cache_write_tokens,
            after.input_cache_write_tokens,
            false,
        ),
        (
            Field::ReasoningTokens,
            before.reasoning_tokens,
            after.reasoning_tokens,
            false,
        ),
        (
            Field::InputTextTokens,
            before.input_text_tokens,
            after.input_text_tokens,
            false,
        ),
        (
            Field::InputImageTokens,
            before.input_image_tokens,
            after.input_image_tokens,
            false,
        ),
        (
            Field::InputAudioTokens,
            before.input_audio_tokens,
            after.input_audio_tokens,
            false,
        ),
    ] {
        if input != output || relation_changed {
            let change = if output.is_none() {
                Change::Omit
            } else {
                Change::Derive
            };
            effects.push(Effect::new(
                "openai.native-usage-view.v1",
                field,
                direction,
                change,
            ));
        }
    }
    effects
}

pub(crate) fn client_usage(usage: Usage) -> Result<Option<Usage>, CodecError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{DecodedResponse, anthropic, replay_envelope};
    fn project(
        decoded: &mut DecodedResponse,
        profile: GenerationApi,
    ) -> Result<Vec<Effect>, CodecError> {
        let client = crate::adapter::ClientAdapter::openai(
            crate::protocol::openai::GenerationCodec::new(
                profile,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            ),
            crate::protocol::openai::TargetContract::full(),
        );
        let mut projection = crate::adapter::ClientProjection::new(client, true, false);
        crate::adapter::emit_response_events(decoded, |event| {
            projection.project(&[event]).map(|_| ())
        })?;
        decoded.semantic = projection.snapshot()?;
        decoded.fidelity.retain_owners(decoded.semantic.items());
        Ok(projection.take_evidence().iter().cloned().collect())
    }
    const BODY: &[u8] = br#"{"id":"native","type":"message","role":"assistant","model":"model","content":[{"type":"thinking","thinking":"plan","signature":"synthetic-signature"},{"type":"tool_use","id":"c","name":"lookup","input":{"n":9007199254740993.125}}],"stop_reason":"tool_use","usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":7}}"#;

    fn fixture() -> DecodedResponse {
        let profile = anthropic::messages::Profile::AdaptiveTextTools;
        let target = crate::protocol::native::ReplayTarget::new("provider", "model").unwrap();
        profile
            .decode_generation_response(BODY, LocalScope::new(3), &target)
            .unwrap()
    }

    #[test]
    fn native_pending_custom_call_keeps_its_identity_and_input() {
        let mut source = fixture();
        let text = |s| crate::semantic::value::Text::new(s, "fixture", 64).unwrap();
        let call = (
            ItemId::new(9),
            Item::CustomCall(CustomCall {
                call_id: text("custom-call"),
                name: text("sql"),
                input: "  SELECT 1;\n".into(),
                context: Default::default(),
            }),
        );
        source.semantic = GenerationResponse::new(vec![call.clone()], Outcome::Completed)
            .unwrap()
            .with_progress(InteractionProgress::AwaitingToolResults)
            .unwrap();
        let before = source.clone();
        let mut projected = source.clone();
        project(&mut projected, GenerationApi::Chat).unwrap();
        assert_eq!(projected.semantic.items().last(), Some(&call));
        assert_eq!(
            projected.semantic.progress(),
            InteractionProgress::Unreported
        );
        assert_eq!(source, before);
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
            let losses = project(&mut projected, GenerationApi::Responses).unwrap();
            assert!(projected.semantic.usage().is_none());
            assert!(
                losses
                    .iter()
                    .any(|r| r.rule() == "generation.usage-report-view.v1")
            );
            let mut events = crate::adapter::ClientProjection::new(
                crate::adapter::ClientAdapter::openai(
                    crate::protocol::openai::GenerationCodec::new(
                        GenerationApi::Responses,
                        crate::protocol::openai::Dialect::Standard,
                        None,
                    ),
                    crate::protocol::openai::TargetContract::full(),
                ),
                true,
                true,
            );
            assert!(
                events
                    .project(&[StreamEvent::UsageReport(report)])
                    .unwrap()
                    .is_empty()
            );
            assert!(
                events
                    .take_evidence()
                    .iter()
                    .any(|r| r.rule() == "generation.usage-report-view.v1")
            );
            assert_eq!(source, before);
        }
    }
    #[test]
    fn projection_keeps_native_observation_and_derives_only_the_client_view() {
        let source = fixture();
        let before = source.clone();
        for profile in [GenerationApi::Responses, GenerationApi::Chat] {
            let mut client = source.clone();
            crate::execution::replay::seal_for_client(
                &mut client,
                &crate::protocol::replay_envelope::CLIENT_ORIGIN,
            )
            .unwrap();
            project(&mut client, profile).unwrap();
            client.metadata.created = Some(1.into());
            let adapter = crate::protocol::openai::GenerationCodec::new(
                profile,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            );
            let contract = crate::protocol::openai::TargetContract {
                replay_origin: Some(crate::protocol::replay_envelope::CLIENT_ORIGIN.clone()),
                ..crate::protocol::openai::TargetContract::full()
            };
            let wire = adapter.encode_response(&client, &contract).unwrap();
            let args = if profile == GenerationApi::Responses {
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
        for profile in [GenerationApi::Responses, GenerationApi::Chat] {
            let adapter = crate::protocol::openai::GenerationCodec::new(
                profile,
                crate::protocol::openai::Dialect::MorphieCore,
                None,
            );
            let contract = crate::protocol::openai::TargetContract {
                replay_origin: Some(crate::protocol::replay_envelope::CLIENT_ORIGIN.clone()),
                ..crate::protocol::openai::TargetContract::full()
            };
            let client_adapter =
                crate::adapter::ClientAdapter::openai(adapter.clone(), contract.clone());
            let mut attempt = crate::execution::Attempt::new(
                crate::adapter::UpstreamAdapter::AnthropicMessages {
                    profile: anthropic::messages::Profile::AdaptiveTextTools,
                    dialect: crate::protocol::anthropic::messages::Dialect::Standard,
                    target: crate::protocol::native::ReplayTarget::new("provider", "model")
                        .unwrap(),
                },
                4096,
                crate::protocol::stream::SseLimits::default(),
            );
            attempt.begin(200, "text/event-stream").unwrap();
            for frame in [
                serde_json::json!({"type":"message_start","message":{"id":"native","type":"message","role":"assistant","model":"model","content":[],"stop_reason":null,"usage":{"input_tokens":11,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"output_tokens":0}}}),
                serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
                serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}),
                serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"synthetic-signature"}}),
                serde_json::json!({"type":"content_block_stop","index":0}),
                serde_json::json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"c","name":"lookup","input":{}}}),
                serde_json::json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":r#"{"n":9007199254740993.125}"#}}),
                serde_json::json!({"type":"content_block_stop","index":1}),
                serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}),
                serde_json::json!({"type":"message_stop"}),
            ] {
                attempt
                    .push(
                        format!(
                            "event: {}\ndata: {frame}\n\n",
                            frame["type"].as_str().unwrap()
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
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
                    .effects()
                    .iter()
                    .any(|loss| loss.field() == "generation.message_envelopes")
            );
            assert!(
                !delivery
                    .effects()
                    .iter()
                    .any(|effect| effect.semantic_field()
                        == crate::protocol::conversion::generation::Field::InputTokens
                        && effect.change()
                            == crate::protocol::conversion::generation::Change::Derive)
            );
            let mut pending = crate::execution::ResponseDelivery::new(
                client_adapter.clone(),
                "public",
                Default::default(),
                crate::semantic::context::StreamOptions {
                    include_obfuscation: Some(false),
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
            let mut frames = pending
                .encode_prepared(opened)
                .map(|batch| batch.into_bytes())
                .unwrap();
            frames.extend(
                pending
                    .encode_prepared(arguments)
                    .map(|batch| batch.into_bytes())
                    .unwrap(),
            );
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
                        include_obfuscation: Some(false),
                        ..Default::default()
                    },
                    crate::protocol::openai::obfuscation::Obfuscation::Disabled,
                )
                .unwrap();
            let mut projected = source.clone();
            let effects = project(&mut projected, profile).unwrap();
            assert!(
                effects
                    .iter()
                    .any(|loss| loss.field() == "generation.message_envelopes"
                        && loss.boundary().as_str() == "event.project")
            );
            let usage = projected.semantic.usage().unwrap();
            assert_eq!(usage.input_tokens, Some(16));
            assert_eq!(usage.output_tokens, Some(7));
            assert_eq!(usage.total_tokens, Some(23));
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
            if profile == GenerationApi::Chat {
                let mut failed = EventProjection::new(profile);
                failed
                    .project(&[StreamEvent::Usage(source.semantic.usage().unwrap())])
                    .unwrap();
                let before = failed.effects().to_vec();
                assert!(
                    failed
                        .project(&[
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
                assert_eq!(failed.effects(), before);
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
        let mut projection = crate::adapter::ClientProjection::new(
            crate::adapter::ClientAdapter::Messages,
            false,
            true,
        );
        projection.metadata(&metadata).unwrap();
        let mut messages = crate::adapter::ClientAdapter::Messages
            .stream_encoder(
                metadata.clone(),
                Default::default(),
                Default::default(),
                crate::protocol::openai::obfuscation::Obfuscation::Disabled,
            )
            .unwrap();
        for event in [
            StreamEvent::Started,
            StreamEvent::ItemStarted {
                item: ItemId::new(1),
                kind: ItemKind::Message { phase: None },
                replay: None,
            },
            StreamEvent::PartStarted {
                item: ItemId::new(1),
                part: PartId::new(1),
                kind: PartKind::Refusal,
            },
        ] {
            for event in projection.project(&[event]).unwrap() {
                messages.encode(&event, &source.fidelity).unwrap();
            }
        }
        let mut inclusive = Usage::operation(10, 3, 13);
        inclusive.cached_input_tokens = Some(2);
        inclusive.input_cache_write_tokens = Some(0);
        let events = projection
            .project(&[StreamEvent::Usage(inclusive)])
            .unwrap();
        let report = events
            .iter()
            .find_map(|event| match event {
                StreamEvent::UsageReport(report) => Some(report),
                _ => None,
            })
            .unwrap();
        assert_eq!(report.counts.input_tokens, Some(8));
        assert_eq!(report.counts.output_tokens, Some(3));
        assert_eq!(
            report.counts.input_relation,
            InputTokenRelation::ExcludesCacheReadAndWrite
        );
        assert_eq!(report.counts.total_tokens, None);
        for event in events {
            messages.encode(&event, &source.fidelity).unwrap();
        }
        let effects = projection.take_evidence();
        assert!(
            effects
                .iter()
                .any(|loss| loss.rule() == "messages.refusal-as-text.v1")
        );
        metadata
            .context
            .reports
            .latest_mut()
            .expect("report")
            .settings = Some(safety_settings);
        for client in [
            crate::adapter::ClientAdapter::Messages,
            crate::adapter::ClientAdapter::openai(
                crate::protocol::openai::GenerationCodec::new(
                    GenerationApi::Chat,
                    crate::protocol::openai::Dialect::Standard,
                    None,
                ),
                crate::protocol::openai::TargetContract::full(),
            ),
        ] {
            let mut projection = crate::adapter::ClientProjection::new(client, false, true);
            assert!(
                projection.metadata(&metadata).is_err(),
                "the common owner must reject safety before incidental settings omission"
            );
        }
        assert!(messages.update_metadata(metadata).is_err());
        assert!(messages.encode(&terminal, &source.fidelity).is_err());
        assert!(messages.finish().is_err());
    }
}
