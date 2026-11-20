//! Target-only projection of native Messages facts onto the OpenAI client carriers.
use crate::{CodecError, openai::GenerationApi, semantic::task::generation::*};
mod events;
use crate::conversion::generation::Effect;
pub use events::EventProjection;

fn envelope_loss(
    profile: GenerationApi,
    direction: crate::conversion::generation::ConversionBoundary,
) -> Effect {
    Effect::Field {
        rule: crate::conversion::generation::RuleId::new(match profile {
            GenerationApi::Responses => "responses.omit-message-envelopes.v1",
            GenerationApi::Chat => "chat.regroup-native-message-envelopes.v1",
        }),
        field: crate::conversion::generation::Field::MessageEnvelopes,
        boundary: direction,
        change: if profile == GenerationApi::Responses {
            crate::conversion::generation::Change::Omit
        } else {
            crate::conversion::generation::Change::Approximate
        },
        owner: crate::conversion::generation::Owner::Root,
    }
}
pub fn usage_loss(
    before: Option<Usage>,
    after: Option<Usage>,
    direction: crate::conversion::generation::ConversionBoundary,
) -> Vec<crate::conversion::generation::Effect> {
    use crate::conversion::generation::{Change, Effect, Field};
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

pub fn client_usage(usage: Usage) -> Result<Option<Usage>, CodecError> {
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
    use crate::{DecodedResponse, anthropic};
    fn project(
        decoded: &mut DecodedResponse,
        profile: GenerationApi,
    ) -> Result<Vec<Effect>, CodecError> {
        let client = crate::adapter::ClientAdapter::openai(
            crate::openai::GenerationCodec::new(profile, crate::openai::Dialect::MorphieCore, None),
            crate::openai::TargetContract::full(),
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
        let target = crate::native::ReplayTarget::new("provider", "model").unwrap();
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
                    crate::openai::GenerationCodec::new(
                        GenerationApi::Responses,
                        crate::openai::Dialect::Standard,
                        None,
                    ),
                    crate::openai::TargetContract::full(),
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
                crate::openai::obfuscation::Obfuscation::Disabled,
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
                crate::openai::GenerationCodec::new(
                    GenerationApi::Chat,
                    crate::openai::Dialect::Standard,
                    None,
                ),
                crate::openai::TargetContract::full(),
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
