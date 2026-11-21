//! One consuming state transition; rejected streams cannot resume.
mod items;
use super::snapshot::outcome;
use crate::task::generation::*;
use crate::value::Text;
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
        StreamEvent::ToolBatchSnapshot { item, batch } => {
            if !matches!(state.open_item(item)?.kind, ItemKind::ToolBatch(_)) {
                return Err(EventError::Lifecycle);
            }
            let old = match &state.open_item(item)?.kind {
                ItemKind::ToolBatch(old) => old.validate(true)?,
                _ => unreachable!("checked batch kind"),
            };
            state.charge(batch.validate(true)?.saturating_sub(old))?;
            state.open_item(item)?.kind = ItemKind::ToolBatch(batch);
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
                logprobs: None,
                annotation_bytes: 0,
                logprob_bytes: 0,
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
                .saturating_add(target.metadata_bytes())
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
                // Validate and size only the appended entries; the accumulated
                // prefix was already checked when it arrived.
                validate_logprobs(&logprobs)?;
                let p = state.part(item, part)?;
                let known = p.logprobs.as_ref().map(Vec::len);
                if known.unwrap_or(0).saturating_add(logprobs.len()) > MAX_ITEMS {
                    return Err(EventError::Limit);
                }
                let appended = logprob_cost(&logprobs)?;
                let logprob_bytes = match known {
                    None => appended,
                    // Join two JSON arrays: drop one bracket pair, add a separator.
                    Some(n) => (p.logprob_bytes + appended - 2).saturating_add(usize::from(n > 0)),
                };
                let total = p.annotation_bytes.saturating_add(logprob_bytes);
                if total > MAX_TEXT_BYTES || p.data.bytes()?.saturating_add(total) > MAX_TEXT_BYTES
                {
                    return Err(EventError::Limit);
                }
                let added = logprob_bytes - p.logprob_bytes;
                state.charge(added)?;
                let p = state.open_part(item, part)?;
                p.logprob_bytes = logprob_bytes;
                p.logprobs.get_or_insert_default().extend(logprobs);
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
                || !compatible_logprobs(p.logprobs.as_deref().unwrap_or_default(), &logprobs)
            {
                return Err(EventError::Lifecycle);
            }
            validate_logprobs(&logprobs)?;
            let logprob_bytes = logprob_cost(&logprobs)?;
            let total = p.annotation_bytes.saturating_add(logprob_bytes);
            if total > MAX_TEXT_BYTES || p.data.bytes()?.saturating_add(total) > MAX_TEXT_BYTES {
                return Err(EventError::Limit);
            }
            let old = p.logprob_bytes;
            state.bytes -= old;
            state.charge(logprob_bytes)?;
            let p = state.open_part(item, part)?;
            p.logprobs = Some(logprobs);
            p.logprob_bytes = logprob_bytes;
        }
        StreamEvent::AnnotationAdded {
            item,
            part,
            annotation,
        } => {
            annotation.check_source(&state.resources)?;
            if state.annotation_count >= MAX_ITEMS {
                return Err(EventError::Limit);
            }
            let p = state.open_part(item, part)?;
            if p.kind != PartKind::Text {
                return Err(EventError::Lifecycle);
            }
            if p.metadata_finished {
                return Err(EventError::Lifecycle);
            }
            let annotation_bytes = p
                .annotation_bytes
                .checked_add(annotation.bytes())
                .filter(|n| *n <= MAX_TEXT_BYTES)
                .ok_or(EventError::Limit)?;
            let total = annotation_bytes.saturating_add(p.logprob_bytes);
            if total > MAX_TEXT_BYTES || p.data.bytes()?.saturating_add(total) > MAX_TEXT_BYTES {
                return Err(EventError::Limit);
            }
            let added = annotation_bytes - p.annotation_bytes;
            state.charge(added)?;
            state.annotation_count += 1;
            let p = state.open_part(item, part)?;
            p.annotations.push(annotation);
            p.annotation_bytes = annotation_bytes;
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
            let known = state.part(item, part)?.annotations.len();
            if state
                .annotation_count
                .saturating_add(annotations.len().saturating_sub(known))
                > MAX_ITEMS
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
                p.logprobs.as_deref().unwrap_or_default(),
                logprobs.as_deref().unwrap_or_default(),
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
            let annotation_bytes = annotation_cost(content.annotations())?;
            let logprob_bytes = content.logprobs().as_deref().map_or(Ok(0), logprob_cost)?;
            if annotation_bytes.saturating_add(logprob_bytes) > MAX_TEXT_BYTES {
                return Err(EventError::Limit);
            }
            let old_bytes = p.metadata_bytes();
            // `starts_with` above guarantees the replacement only appends.
            let added = annotations.len() - p.annotations.len();
            state.bytes -= old_bytes;
            state.charge(annotation_bytes.saturating_add(logprob_bytes))?;
            state.annotation_count += added;
            let p = state.open_part(item, part)?;
            p.annotations = annotations;
            p.logprobs = logprobs;
            p.annotation_bytes = annotation_bytes;
            p.logprob_bytes = logprob_bytes;
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
            crate::task::generation::history::envelope::MessageEnvelopes::new(&values, groups)?;
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
            // Statusless declaration batches have no partial wire carrier.
            // An overall incomplete outcome cannot turn an opener into a final batch.
            if state.items.iter().any(|i| {
                (terminal == StreamTerminal::Completed
                    || terminal != StreamTerminal::Error
                        && matches!(i.kind, ItemKind::ToolBatch(_)))
                    && i.status != Some(ItemLifecycle::Completed)
            }) {
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
fn annotation_cost(annotations: &[Annotation]) -> Result<usize, EventError> {
    annotations.iter().try_fold(0usize, |n, a| {
        n.checked_add(a.bytes())
            .filter(|n| *n <= MAX_TEXT_BYTES)
            .ok_or(EventError::Limit)
    })
}
/// Serialized size of a logprob array, including its brackets.
fn logprob_cost(probs: &[Logprob]) -> Result<usize, EventError> {
    crate::value::json_size(&probs, MAX_TEXT_BYTES).map_err(|_| EventError::Limit)
}
