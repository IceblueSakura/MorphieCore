//! One consuming state transition; rejected streams cannot resume.
mod items;
use super::snapshot::outcome;
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;
pub fn reduce(mut state: StreamState, event: StreamEvent) -> Result<StreamState, EventError> {
    if state.terminal.is_some()
        || state.progress != InteractionProgress::Unreported
            && !matches!(
                event,
                StreamEvent::Usage(_)
                    | StreamEvent::UsageReport(_)
                    | StreamEvent::SafetyReport(_)
                    | StreamEvent::Terminal { .. }
            )
    {
        return Err(EventError::Lifecycle);
    }
    if !state.started
        && !matches!(
            event,
            StreamEvent::Started
                | StreamEvent::Queued
                | StreamEvent::Terminal {
                    terminal: StreamTerminal::Error,
                    ..
                }
        )
    {
        return Err(EventError::Lifecycle);
    }
    match event {
        StreamEvent::Started => {
            if state.started {
                return Err(EventError::Lifecycle);
            }
            state.started = true;
        }
        StreamEvent::Queued => {
            // Queued is one pre-created lifecycle state, never a terminal.
            if state.started || state.queued {
                return Err(EventError::Lifecycle);
            }
            state.queued = true;
        }
        StreamEvent::ResourceDeclared { id, resource } => {
            let bytes = resource
                .source_view()
                .validate()?
                .checked_add(std::mem::size_of::<ResourceId>())
                .ok_or(EventError::Limit)?;
            state.charge(bytes)?;
            state.resources = std::mem::take(&mut state.resources).insert(id, resource)?;
        }
        StreamEvent::ItemStarted { item, kind, replay } => {
            items::start_item(&mut state, item, kind, replay)?
        }
        StreamEvent::ToolResultOutput { item, output } => {
            items::update_tool_result(&mut state, item, output)?
        }
        StreamEvent::PartStarted { item, part, kind } => {
            if state.part_ids.len() >= MAX_ITEMS {
                return Err(EventError::Limit);
            }
            if !state.part_ids.insert(part) {
                return Err(EventError::Identity);
            }
            let owner = state.open_item(item)?;
            let valid = matches!(
                (&owner.kind, kind),
                (
                    ItemKind::Message { .. },
                    PartKind::Text | PartKind::Refusal | PartKind::Audio
                ) | (
                    ItemKind::Reasoning,
                    PartKind::Summary | PartKind::ReasoningText
                ) | (
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        ..
                    },
                    PartKind::Arguments
                ) | (
                    ItemKind::ToolCall {
                        format: ArgumentFormat::Json,
                        ..
                    },
                    PartKind::StructuredArguments
                ) | (ItemKind::CustomCall { .. }, PartKind::CustomInput)
                    | (
                        ItemKind::ProviderResult {
                            format: ProviderResultFormat::Text,
                            ..
                        },
                        PartKind::ResultText
                    )
                    | (
                        ItemKind::ProviderResult {
                            format: ProviderResultFormat::Json,
                            ..
                        },
                        PartKind::ResultJson
                    )
            );
            if !valid
                || matches!(
                    kind,
                    PartKind::Arguments
                        | PartKind::StructuredArguments
                        | PartKind::CustomInput
                        | PartKind::ResultText
                        | PartKind::ResultJson
                ) && !owner.parts.is_empty()
            {
                return Err(EventError::Lifecycle);
            }
            owner.parts.push(StreamPart {
                id: part,
                kind,
                replay: None,
                data: if kind == PartKind::Audio {
                    StreamPartValue::AudioFragments(AudioBuffer::default())
                } else if matches!(kind, PartKind::StructuredArguments | PartKind::ResultJson) {
                    StreamPartValue::JsonFragments(String::new())
                } else {
                    StreamPartValue::Text(String::new())
                },
                value_finished: false,
                finished: false,
                annotations: vec![],
                logprobs: crate::semantic::value::Presence::Absent,
                metadata_bytes: 0,
                metadata_finished: false,
            });
        }
        StreamEvent::Delta {
            item,
            part,
            fragment,
            logprobs,
        } => {
            let target = state.open_part(item, part)?;
            if target.value_finished {
                return Err(EventError::Lifecycle);
            }
            if target
                .data
                .bytes()?
                .saturating_add(fragment.len())
                .saturating_add(target.metadata_bytes)
                > MAX_TEXT_BYTES
            {
                return Err(EventError::Limit);
            }
            state.charge(fragment.len())?;
            state.open_part(item, part)?.data.push(&fragment)?;
            if !logprobs.is_empty() {
                if !matches!(
                    state.part(item, part)?.kind,
                    PartKind::Text | PartKind::Refusal
                ) {
                    return Err(EventError::Lifecycle);
                }
                let mut probs = state
                    .part(item, part)?
                    .logprobs
                    .value()
                    .cloned()
                    .unwrap_or_default();
                probs.extend(logprobs);
                validate_logprobs(&probs)?;
                let probs = crate::semantic::value::Presence::Value(probs);
                let bytes = metadata_cost(&state.part(item, part)?.annotations, &probs)?;
                state.charge(bytes.saturating_sub(state.part(item, part)?.metadata_bytes))?;
                let p = state.open_part(item, part)?;
                if p.data.bytes()?.saturating_add(bytes) > MAX_TEXT_BYTES {
                    return Err(EventError::Limit);
                }
                p.metadata_bytes = bytes;
                p.logprobs = probs;
            }
        }
        StreamEvent::AudioDelta { item, part, update } => {
            let p = state.open_part(item, part)?;
            if p.value_finished || p.kind != PartKind::Audio {
                return Err(EventError::Lifecycle);
            }
            let old = p.data.bytes()?;
            let StreamPartValue::AudioFragments(buffer) = &mut p.data else {
                return Err(EventError::Lifecycle);
            };
            buffer.push(&update)?;
            let new = p.data.bytes()?;
            state.charge(new.saturating_sub(old))?;
        }
        StreamEvent::LogprobsSnapshot {
            item,
            part,
            logprobs,
        } => {
            let p = state.open_part(item, part)?;
            if !matches!(p.kind, PartKind::Text | PartKind::Refusal)
                || p.value_finished
                || !compatible_logprobs(
                    p.logprobs.value().map(Vec::as_slice).unwrap_or_default(),
                    &logprobs,
                )
            {
                return Err(EventError::Lifecycle);
            }
            validate_logprobs(&logprobs)?;
            let value = crate::semantic::value::Presence::Value(logprobs);
            let n = metadata_cost(&p.annotations, &value)?;
            let old = p.metadata_bytes;
            if p.data.bytes()?.saturating_add(n) > MAX_TEXT_BYTES {
                return Err(EventError::Limit);
            }
            state.bytes -= old;
            state.charge(n)?;
            let p = state.open_part(item, part)?;
            p.logprobs = value;
            p.metadata_bytes = n;
        }
        StreamEvent::AnnotationAdded {
            item,
            part,
            annotation,
        } => {
            annotation.check_source(&state.resources)?;
            if state
                .items
                .iter()
                .flat_map(|i| &i.parts)
                .map(|p| p.annotations.len())
                .sum::<usize>()
                >= MAX_ITEMS
            {
                return Err(EventError::Limit);
            }
            let p = state.open_part(item, part)?;
            if p.kind != PartKind::Text {
                return Err(EventError::Lifecycle);
            }
            if p.metadata_finished {
                return Err(EventError::Lifecycle);
            }
            let mut annotations = p.annotations.clone();
            annotations.push(annotation);
            let n = metadata_cost(&annotations, &p.logprobs)?;
            let old = p.metadata_bytes;
            state.charge(n.saturating_sub(old))?;
            let p = state.open_part(item, part)?;
            if p.data.bytes()?.saturating_add(n) > MAX_TEXT_BYTES {
                return Err(EventError::Limit);
            }
            p.annotations = annotations;
            p.metadata_bytes = n;
        }
        StreamEvent::TextMetadata {
            item,
            part,
            annotations,
            logprobs,
        } => {
            for a in &annotations {
                a.check_source(&state.resources)?;
            }
            let total = state
                .items
                .iter()
                .flat_map(|i| &i.parts)
                .map(|p| p.annotations.len())
                .sum::<usize>();
            if total.saturating_add(
                annotations
                    .len()
                    .saturating_sub(state.part(item, part)?.annotations.len()),
            ) > MAX_ITEMS
            {
                return Err(EventError::Limit);
            }
            let p = state.open_part(item, part)?;
            if !matches!(p.kind, PartKind::Text | PartKind::Refusal)
                || p.kind == PartKind::Refusal && !annotations.is_empty()
                || !p.value_finished
                || p.metadata_finished
                || !annotations.starts_with(&p.annotations)
            {
                return Err(EventError::Lifecycle);
            }
            if !compatible_logprobs(
                p.logprobs.value().map(Vec::as_slice).unwrap_or_default(),
                logprobs.value().map(Vec::as_slice).unwrap_or_default(),
            ) {
                return Err(EventError::Lifecycle);
            }
            let content = TextContent::new(
                Text::allowing_empty(
                    p.text().ok_or(EventError::Lifecycle)?,
                    "text",
                    MAX_TEXT_BYTES,
                )
                .map_err(|_| EventError::Limit)?,
                annotations.clone(),
                logprobs.clone(),
            )?;
            let new_bytes = metadata_cost(content.annotations(), content.logprobs())?;
            let old_bytes = p.metadata_bytes;
            state.bytes -= old_bytes;
            state.charge(new_bytes)?;
            let p = state.open_part(item, part)?;
            p.annotations = annotations;
            p.logprobs = logprobs;
            p.metadata_bytes = new_bytes;
            p.metadata_finished = true;
        }
        StreamEvent::ValueFinished { item, part } => {
            let p = state.open_part(item, part)?;
            if p.value_finished {
                return Err(EventError::Lifecycle);
            }
            let old = p.data.bytes()?;
            let value =
                std::mem::replace(&mut p.data, StreamPartValue::Text(String::new())).finish()?;
            state.bytes -= old;
            state.charge(value.bytes()?)?;
            let p = state.open_part(item, part)?;
            p.data = value;
            p.value_finished = true;
        }
        StreamEvent::CompactionProgress { item } => {
            if !matches!(state.open_item(item)?.kind, ItemKind::Compaction) {
                return Err(EventError::Lifecycle);
            }
        }
        StreamEvent::PartFinished { item, part } | StreamEvent::PartInterrupted { item, part } => {
            let interrupted = matches!(event, StreamEvent::PartInterrupted { .. });
            let p = state.open_part(item, part)?;
            if interrupted && p.kind != PartKind::Summary {
                return Err(EventError::Lifecycle);
            }
            if !p.value_finished {
                return Err(EventError::Lifecycle);
            }
            if matches!(p.kind, PartKind::Text | PartKind::Refusal) {
                TextContent::new(
                    Text::allowing_empty(
                        p.text().ok_or(EventError::Lifecycle)?,
                        "text",
                        MAX_TEXT_BYTES,
                    )
                    .map_err(|_| EventError::Limit)?,
                    p.annotations.clone(),
                    p.logprobs.clone(),
                )?;
            }
            p.finished = true;
        }
        StreamEvent::PartReplay { item, part, replay } => {
            replay.value.validate_attachment()?;
            if !matches!(state.item(item)?.kind, ItemKind::Message { .. }) {
                return Err(EventError::Lifecycle);
            }
            let p = state.open_part(item, part)?;
            if !p.value_finished || p.kind != PartKind::Text {
                return Err(EventError::Lifecycle);
            }
            if let Some(old) = &p.replay {
                if old.value.replay_token().is_some() {
                    return Err(EventError::Lifecycle);
                }
                if old.origin != replay.origin || old.value.format() != replay.value.format() {
                    return Err(EventError::Identity);
                }
            }
            let old = p.replay.as_ref().map_or(0, |r| r.value.as_str().len());
            state.bytes -= old;
            state.charge(replay.value.as_str().len())?;
            state.open_part(item, part)?.replay = Some(replay);
        }
        StreamEvent::ItemFinished {
            item,
            status,
            replay,
        } => items::finish_item(&mut state, item, status, replay)?,
        StreamEvent::Progress(progress) => {
            if progress == InteractionProgress::Unreported
                || state.items.iter().any(|item| item.status.is_none())
            {
                return Err(EventError::Lifecycle);
            }
            progress.validate(Outcome::Completed, &snapshot_items(&state)?)?;
            state.progress = progress;
        }
        StreamEvent::MessageEnvelope(group) => {
            if state.message_envelopes.len() >= MAX_ITEMS {
                return Err(EventError::Limit);
            }
            if group.members().iter().any(|id| {
                state
                    .items
                    .iter()
                    .find(|item| item.id == *id)
                    .is_none_or(|item| item.status.is_none())
            }) {
                return Err(EventError::Lifecycle);
            }
            let mut groups = state.message_envelopes.clone();
            groups.push(group.clone());
            let values = snapshot_items(&state)?;
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::new(
                &values, groups,
            )?;
            state.charge(
                std::mem::size_of::<MessageEnvelope>() + std::mem::size_of_val(group.members()),
            )?;
            state.message_envelopes.push(group);
        }
        StreamEvent::SafetyReport(report) => {
            if state.safety_reports.len() >= MAX_ITEMS {
                return Err(EventError::Limit);
            }
            state.charge(report.validate()?)?;
            state.safety_reports.push(report);
        }
        StreamEvent::Usage(usage) => {
            usage.validate()?;
            // The report is final; a second one would contradict it.
            if state.usage.operation().is_some() {
                return Err(EventError::Lifecycle);
            }
            let report = UsageReport::operation(usage);
            let before = state.usage.bytes();
            state.usage.replace(report)?;
            state.charge(state.usage.bytes() - before)?;
        }
        StreamEvent::UsageReport(report) => {
            let before = state.usage.bytes();
            state.usage.replace(report)?;
            let after = state.usage.bytes();
            state.bytes -= before;
            state.charge(after)?;
        }
        StreamEvent::Terminal { terminal, details } => {
            if terminal == StreamTerminal::Completed
                && state
                    .items
                    .iter()
                    .any(|i| i.status != Some(ItemLifecycle::Completed))
            {
                return Err(EventError::Lifecycle);
            }
            let outcome = outcome(terminal);
            if state.progress != InteractionProgress::Unreported {
                state.progress.validate(outcome, &snapshot_items(&state)?)?;
            }
            details.validate(outcome)?;
            state.charge(details.bytes())?;
            state.details = details;
            state.terminal = Some(terminal);
        }
    }
    Ok(state)
}
fn metadata_cost(
    annotations: &[Annotation],
    probs: &crate::semantic::value::Presence<Vec<Logprob>>,
) -> Result<usize, EventError> {
    if annotations.is_empty() && probs.is_absent() {
        return Ok(0);
    }
    let a = annotations.iter().try_fold(0usize, |n, a| {
        n.checked_add(a.bytes())
            .filter(|n| *n <= MAX_TEXT_BYTES)
            .ok_or(EventError::Limit)
    })?;
    let p = probs
        .value()
        .map(|p| {
            crate::semantic::value::json_size(p, MAX_TEXT_BYTES).map_err(|_| EventError::Limit)
        })
        .transpose()?
        .unwrap_or(0);
    let total = a.saturating_add(p);
    if total > MAX_TEXT_BYTES {
        return Err(EventError::Limit);
    }
    Ok(total)
}
