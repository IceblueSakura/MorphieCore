//! Incremental representability checks against one immutable output target.
use crate::protocol::openai::lowering::{RepresentationError, TargetCaps};
use crate::{protocol::openai::Profile, semantic::task::generation::*};

pub fn check_event(
    state: &StreamState,
    event: &StreamEvent,
    profile: Profile,
    contract: &TargetCaps,
) -> Result<(), RepresentationError> {
    if matches!(event, StreamEvent::SafetyReport(_)) || !state.safety_reports().is_empty() {
        return Err(RepresentationError::Safety);
    }
    if let StreamEvent::Terminal { details, .. } = event {
        let pending_tools = state.items().iter().any(|item| {
            matches!(
                item.kind,
                ItemKind::ToolCall { .. } | ItemKind::CustomCall { .. } | ItemKind::Program { .. }
            )
        });
        crate::protocol::openai::terminal::check_completion(details, profile, pending_tools)
            .map_err(|_| RepresentationError::Terminal)?;
    }
    if matches!(event, StreamEvent::ItemStarted { replay: Some(replay), .. } | StreamEvent::ItemFinished { replay: Some(replay), .. }
        if !matches!(replay.value.format(), ReplayFormat::ResponsesEncrypted | ReplayFormat::ResponsesCompaction))
    {
        return Err(RepresentationError::ReplayFormat);
    }
    if matches!(event, StreamEvent::ItemStarted { replay: Some(replay), .. } if replay.value.format()==ReplayFormat::ResponsesEncrypted && replay.value.replay_token().is_some())
    {
        return Err(RepresentationError::ReplayPhase);
    }
    match event {
        StreamEvent::AnnotationAdded { annotation, .. } => {
            crate::protocol::openai::content::citations::check(
                annotation,
                state.resources(),
                profile,
            )
            .map_err(|_| RepresentationError::TextMetadata)?;
        }
        StreamEvent::TextMetadata { annotations, .. } => {
            for a in annotations {
                crate::protocol::openai::content::citations::check(a, state.resources(), profile)
                    .map_err(|_| RepresentationError::TextMetadata)?;
            }
        }
        _ => {}
    }
    match event {
        StreamEvent::ResourceDeclared { resource, .. } => {
            if profile != Profile::Responses
                && !crate::protocol::openai::content::citations::context(resource)
            {
                return Err(RepresentationError::UnmigratedSemantic);
            }
        }
        StreamEvent::ToolResultOutput { item, output } => {
            let (ItemKind::ToolResult(result) | ItemKind::CustomResult(result)) =
                &state.item(*item)?.kind
            else {
                return Err(RepresentationError::Tools);
            };
            let mut result = result.clone();
            result.output = output.clone();
            check_result(
                state,
                &result,
                matches!(state.item(*item)?.kind, ItemKind::CustomResult(_)),
                profile,
                contract,
            )?;
        }
        StreamEvent::MessageEnvelope(group) => {
            let items = snapshot_items(state).map_err(|_| RepresentationError::MessageGrouping)?;
            super::message_groups::project(&items, std::slice::from_ref(group), profile)?;
        }
        StreamEvent::Terminal {
            terminal: StreamTerminal::Cancelled,
            ..
        } if profile == Profile::Responses => {
            // The static status exists, but the pinned standard stream union
            // has no cancellation event. Never invent one or report failure/success.
            // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/response_stream_event.py
            return Err(RepresentationError::Terminal);
        }
        StreamEvent::PartStarted {
            kind: PartKind::Audio,
            ..
        }
        | StreamEvent::AudioDelta { .. }
            if profile != Profile::Chat || !contract.semantics.audio_output =>
        {
            return Err(RepresentationError::UnmigratedSemantic);
        }
        StreamEvent::PartStarted {
            kind: PartKind::StructuredArguments | PartKind::ResultText | PartKind::ResultJson,
            ..
        } => return Err(RepresentationError::Tools),
        StreamEvent::Progress(_) => {
            return Err(RepresentationError::InteractionProgress);
        }
        StreamEvent::Usage(usage) => {
            let projected = if profile == Profile::Chat {
                *usage
            } else {
                super::projection::responses_usage(*usage, &contract.adaptation.rules)?.0
            };
            super::check_usage(projected, profile, &contract.adaptation.rules)?;
        }
        StreamEvent::ItemStarted { kind, replay, .. } => {
            match kind {
                ItemKind::Compaction
                    if profile != Profile::Responses || !contract.semantics.compaction =>
                {
                    return Err(RepresentationError::UnmigratedSemantic);
                }
                ItemKind::ToolResult(result) | ItemKind::CustomResult(result) => {
                    check_result(
                        state,
                        result,
                        matches!(kind, ItemKind::CustomResult(_)),
                        profile,
                        contract,
                    )?;
                }
                ItemKind::ProviderTool(_) | ItemKind::ProviderResult { .. } => {
                    return Err(RepresentationError::UnmigratedSemantic);
                }
                ItemKind::ToolCall { context, .. } | ItemKind::CustomCall { context, .. }
                    if context.alias_domain.is_some() =>
                {
                    return Err(RepresentationError::UnmigratedSemantic);
                }
                ItemKind::Message { phase: Some(_) } if profile == Profile::Chat => {
                    return Err(RepresentationError::UnmigratedSemantic);
                }
                ItemKind::ToolCall {
                    format: ArgumentFormat::Json,
                    ..
                } => {
                    return Err(RepresentationError::Tools);
                }
                ItemKind::ToolCall { context, .. }
                    if profile == Profile::Chat && !context.is_direct() =>
                {
                    return Err(RepresentationError::Tools);
                }
                ItemKind::ConfigurationUpdate(update)
                    if profile != Profile::Responses
                        || !contract.semantics.reasoning
                        || super::super::reasoning::effort_label(update).is_err() =>
                {
                    return Err(RepresentationError::Reasoning);
                }
                // Chat reasoning is provisional at event time: its carrier
                // message and single text part are validated at the terminal.
                ItemKind::Reasoning
                    if !contract.semantics.reasoning
                        || (profile == Profile::Chat
                            && !contract.adaptation.rules.readable_reasoning) =>
                {
                    return Err(RepresentationError::Reasoning);
                }
                ItemKind::CustomCall { .. }
                    if profile != Profile::Responses
                        || !contract.semantics.tools
                        || !contract.semantics.custom_tools =>
                {
                    return Err(RepresentationError::Tools);
                }
                ItemKind::Program { .. } | ItemKind::ProgramOutput { .. }
                    if profile != Profile::Responses || !contract.semantics.tools =>
                {
                    return Err(RepresentationError::Tools);
                }
                ItemKind::ToolCall { .. } if !contract.semantics.tools => {
                    return Err(RepresentationError::Tools);
                }
                ItemKind::Message { .. }
                    if profile == Profile::Chat
                        && state.items().iter().any(|i| {
                            matches!(i.kind, ItemKind::Message { .. } | ItemKind::ToolCall { .. })
                        }) =>
                {
                    return Err(RepresentationError::MessageGrouping);
                }
                _ => {}
            }
            if replay
                .as_ref()
                .is_some_and(|r| !r.permits(contract.replay_origin.as_ref()))
            {
                return Err(RepresentationError::ReplayOrigin);
            }
        }
        StreamEvent::ItemFinished { replay, .. }
            if replay.as_ref().is_some_and(|r| {
                !r.permits(contract.replay_origin.as_ref())
                    || profile == Profile::Chat && r.value.replay_token().is_none()
            }) =>
        {
            return Err(RepresentationError::ReplayOrigin);
        }
        StreamEvent::ItemFinished {
            status,
            replay: Some(_),
            ..
        } if profile == Profile::Chat && *status != ItemLifecycle::Completed => {
            return Err(RepresentationError::Terminal);
        }
        StreamEvent::AnnotationAdded { .. }
            if profile == Profile::Chat || !contract.semantics.text_metadata =>
        {
            return Err(RepresentationError::TextMetadata);
        }
        StreamEvent::Queued if profile != Profile::Responses => {
            return Err(RepresentationError::Lifecycle);
        }
        StreamEvent::Delta {
            item,
            part,
            logprobs,
            ..
        } if !logprobs.is_empty() => {
            probability_target(state, *item, *part, logprobs, profile, contract)?;
        }
        StreamEvent::LogprobsSnapshot {
            item,
            part,
            logprobs,
        } => {
            probability_target(state, *item, *part, logprobs, profile, contract)?;
            if profile == Profile::Chat {
                let old = &state.part(*item, *part)?.logprobs;
                if let Some(old) = old.value()
                    && old != logprobs
                {
                    return Err(RepresentationError::TextMetadata);
                }
            }
        }
        StreamEvent::TextMetadata {
            item,
            part,
            annotations,
            logprobs,
        } => {
            if !annotations.is_empty()
                && (profile == Profile::Chat || !contract.semantics.text_metadata)
            {
                return Err(RepresentationError::TextMetadata);
            }
            if !logprobs.is_absent() {
                probability_target(
                    state,
                    *item,
                    *part,
                    logprobs.value().map(Vec::as_slice).unwrap_or_default(),
                    profile,
                    contract,
                )?;
            }
            if profile == Profile::Chat
                && let Some(old) = state.part(*item, *part)?.logprobs.value()
                && logprobs.value() != Some(old)
            {
                // A final snapshot cannot retract or rewrite facts already emitted to Chat.
                return Err(RepresentationError::TextMetadata);
            }
        }
        StreamEvent::PartStarted { item, kind, .. }
            if profile == Profile::Chat
                && matches!(state.item(*item)?.kind, ItemKind::Message { .. })
                && state.item(*item)?.parts.iter().any(|p| {
                    p.kind == *kind
                        || p.kind == PartKind::Audio
                        || p.kind == PartKind::Refusal
                        || *kind == PartKind::Refusal
                        || (p.kind != PartKind::Audio && *kind != PartKind::Audio)
                }) =>
        {
            return Err(RepresentationError::MessageGrouping);
        }
        StreamEvent::Terminal { terminal, details }
            if profile == Profile::Chat
                && (*terminal == StreamTerminal::Incomplete
                    && !matches!(
                        details.incomplete,
                        Some(IncompleteReason::MaxOutputTokens | IncompleteReason::ContentFilter)
                    )
                    || matches!(
                        terminal,
                        StreamTerminal::Failed | StreamTerminal::Cancelled | StreamTerminal::Error
                    )
                    || details.incomplete.as_ref().is_some_and(|r| {
                        !matches!(
                            r,
                            IncompleteReason::MaxOutputTokens | IncompleteReason::ContentFilter
                        )
                    })) =>
        {
            return Err(RepresentationError::Terminal);
        }
        _ => {}
    }
    Ok(())
}
fn check_result(
    state: &StreamState,
    result: &ToolResult,
    custom: bool,
    profile: Profile,
    contract: &TargetCaps,
) -> Result<(), RepresentationError> {
    if profile != Profile::Responses {
        return Err(RepresentationError::Tools);
    }
    let item = if custom {
        Item::CustomResult(result.clone())
    } else {
        Item::ToolResult(result.clone())
    };
    let items = vec![(ItemId::new(1), item)];
    super::text_items(&items, profile, false, state.resources())?;
    let response =
        GenerationResponse::from_resources(items, Outcome::Incomplete, state.resources().clone())
            .map_err(|_| RepresentationError::Tools)?;
    contract.semantics.check_response(&response)?;
    Ok(())
}

fn probability_target(
    state: &StreamState,
    item: ItemId,
    part: PartId,
    logprobs: &[Logprob],
    profile: Profile,
    contract: &TargetCaps,
) -> Result<(), RepresentationError> {
    if !contract.semantics.logprobs
        || !contract.semantics.text_metadata
        || profile == Profile::Responses && state.part(item, part)?.kind == PartKind::Refusal
    {
        return Err(RepresentationError::TextMetadata);
    }
    if profile == Profile::Chat {
        crate::protocol::openai::chat::logprobs::validate(logprobs)
            .map_err(|_| RepresentationError::TextMetadata)?;
    }
    Ok(())
}
