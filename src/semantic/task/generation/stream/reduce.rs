//! One consuming state transition; rejected streams cannot resume.
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
            if state.items.len() >= MAX_ITEMS {
                return Err(EventError::Limit);
            }
            if state.items.iter().any(|i| i.id == item) {
                return Err(EventError::Identity);
            }
            if let ItemKind::ToolResult(result) | ItemKind::CustomResult(result) = &kind {
                if result
                    .status
                    .is_some_and(|s| s != ItemLifecycle::InProgress)
                {
                    return Err(EventError::Lifecycle);
                }
                if let Some(call_id) = &result.call_id
                    && state.items.iter().any(|old| {
                        matches!(&old.kind, ItemKind::ToolResult(r) | ItemKind::CustomResult(r)
                            if r.context.alias_domain == result.context.alias_domain
                                && r.call_id.as_ref() == Some(call_id))
                    })
                {
                    return Err(EventError::Identity);
                }
                charge_tool_result(&mut state, result, None)?;
            }
            if let ItemKind::ProviderTool(observed)
            | ItemKind::ProviderResult {
                observation: observed,
                ..
            } = &kind
            {
                let mut bytes = 0;
                if matches!(kind, ItemKind::ProviderResult { .. }) {
                    crate::semantic::task::generation::validate::provider_result_header(
                        observed,
                        &mut bytes,
                        &mut state.part_ids,
                        &state.resources,
                    )?;
                } else {
                    crate::semantic::task::generation::validate::provider_observation(
                        observed,
                        &mut bytes,
                        &mut state.part_ids,
                        &state.resources,
                        &mut state.file_decoded_bytes,
                    )?;
                }
                // Declarations prepay each body once; repeated uses still charge expansion.
                if let Some(ToolOutput::Parts(parts)) = &observed.output {
                    for part in parts {
                        if let InputPart::Resource(value) = &part.content
                            && state.used_resources.insert(value.id)
                        {
                            bytes = bytes
                                .checked_sub(
                                    value.resolve(&state.resources)?.source_view().validate()?,
                                )
                                .ok_or(EventError::Limit)?;
                        }
                    }
                }
                state.charge(bytes)?;
                if let ProviderOperation::Reported {
                    alias: Some(alias), ..
                } = &observed.operation
                    && state.items.iter().any(|old| match &old.kind {
                        ItemKind::ProviderTool(value)
                        | ItemKind::ProviderResult {
                            observation: value, ..
                        } => {
                            value.source == observed.source && reported_alias(value) == Some(alias)
                        }
                        _ => {
                            old.kind.alias_domain() == Some(&observed.source)
                                && old.kind.call_id() == Some(alias)
                        }
                    })
                {
                    return Err(EventError::Identity);
                }
            }
            if let Some(call_id) = kind.call_id() {
                if call_id.as_str().is_empty() || call_id.as_str().len() > 256 {
                    return Err(EventError::Limit);
                }
                if state.items.iter().any(|i| {
                    let aliased = match &i.kind {
                        ItemKind::ProviderTool(observed)
                        | ItemKind::ProviderResult {
                            observation: observed,
                            ..
                        } => {
                            Some(&observed.source) == kind.alias_domain()
                                && reported_alias(observed) == Some(call_id)
                        }
                        _ => false,
                    };
                    aliased
                        || i.kind.alias_domain() == kind.alias_domain()
                            && i.kind.call_id().is_some_and(|id| id == call_id)
                }) {
                    return Err(EventError::Identity);
                }
                state.charge(call_id.as_str().len())?;
            }
            if let Some((_, name)) = kind.call() {
                if let Some(domain) = kind.alias_domain() {
                    if domain.source.as_str().is_empty() || domain.source.as_str().len() > 256 {
                        return Err(EventError::Limit);
                    }
                    state.charge(domain.source.as_str().len())?;
                }
                if name.as_str().is_empty() || name.as_str().len() > 128 {
                    return Err(EventError::Limit);
                }
                state.charge(name.as_str().len())?;
            }
            if let ItemKind::Program {
                code, fingerprint, ..
            } = &kind
            {
                state.charge(code.len() + fingerprint.len())?;
            }
            if let ItemKind::ProgramOutput { call_id, result } = &kind {
                if call_id.as_str().is_empty() || call_id.as_str().len() > 256 {
                    return Err(EventError::Limit);
                }
                if state.items.iter().any(|i| {
                    matches!(&i.kind, ItemKind::ProgramOutput { call_id: c, .. } if c == call_id)
                }) {
                    return Err(EventError::Identity);
                }
                state.charge(call_id.as_str().len() + result.len())?;
            }
            if matches!(kind, ItemKind::Compaction) && replay.is_none() {
                return Err(EventError::Lifecycle);
            }
            if let ItemKind::ConfigurationUpdate(update) = &kind {
                update.validate().map_err(|_| EventError::Lifecycle)?;
            }
            if let Some(r) = &replay {
                r.validate(&kind)?;
                state.charge(r.value.as_str().len())?;
            }
            state.items.push(StreamItem {
                id: item,
                kind,
                parts: vec![],
                status: None,
                replay,
            });
        }
        StreamEvent::ToolResultOutput { item, output } => {
            let owner = state.open_item(item)?;
            let (ItemKind::ToolResult(old) | ItemKind::CustomResult(old)) = &owner.kind else {
                return Err(EventError::Lifecycle);
            };
            let old = old.clone();
            if old.output != output {
                let mut next = old.clone();
                next.output = output;
                charge_tool_result(&mut state, &next, Some(&old))?;
                let owner = state.open_item(item)?;
                let (ItemKind::ToolResult(result) | ItemKind::CustomResult(result)) =
                    &mut owner.kind
                else {
                    unreachable!("checked result kind");
                };
                *result = next;
            }
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
                let bytes = metadata_cost(
                    &state.part(item, part)?.annotations,
                    &crate::semantic::value::Presence::Value(probs.clone()),
                )?;
                state.charge(bytes.saturating_sub(state.part(item, part)?.metadata_bytes))?;
                let p = state.open_part(item, part)?;
                if p.data.bytes()?.saturating_add(bytes) > MAX_TEXT_BYTES {
                    return Err(EventError::Limit);
                }
                p.metadata_bytes = bytes;
                p.logprobs = crate::semantic::value::Presence::Value(probs);
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
            if p.kind != PartKind::Text || p.annotations.len() >= MAX_ITEMS {
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
            let old = p.logprobs.value().cloned().unwrap_or_default();
            let next = logprobs.value().cloned().unwrap_or_default();
            if !compatible_logprobs(&old, &next) {
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
        StreamEvent::PartFinished { item, part } => {
            let p = state.open_part(item, part)?;
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
        } => {
            if status == ItemLifecycle::InProgress {
                return Err(EventError::Lifecycle);
            }
            let owner = state.open_item(item)?;
            if replay.is_none()
                && owner.replay.as_ref().is_some_and(|old| {
                    matches!(owner.kind, ItemKind::ToolCall { .. })
                        || old.value.replay_token().is_some()
                })
            {
                return Err(EventError::Lifecycle);
            }
            if matches!(owner.kind, ItemKind::ProviderResult { .. })
                && (owner.parts.len() != 1 || owner.parts.iter().any(|part| !part.finished))
            {
                return Err(EventError::Lifecycle);
            }
            if status == ItemLifecycle::Completed
                && owner.parts.iter().any(|p| {
                    !p.finished
                        || p.replay
                            .as_ref()
                            .is_some_and(|r| r.value.replay_token().is_none())
                })
            {
                return Err(EventError::Lifecycle);
            }
            if owner.kind.call().is_some() && owner.parts.len() != 1 {
                return Err(EventError::Lifecycle);
            }
            if matches!(owner.kind, ItemKind::Compaction)
                && (replay.is_none() || status != ItemLifecycle::Completed)
            {
                return Err(EventError::Lifecycle);
            }
            if let Some(r) = &replay {
                r.validate(&owner.kind)?;
                if r.value.replay_token().is_none()
                    && (status == ItemLifecycle::Completed
                        || !matches!(owner.kind, ItemKind::ToolCall { .. })
                        || owner
                            .replay
                            .as_ref()
                            .is_some_and(|old| old.value.replay_token().is_some()))
                {
                    return Err(EventError::Lifecycle);
                }
                if owner.replay.as_ref().is_some_and(|old| {
                    old.origin != r.origin
                        || old.value.format() != r.value.format()
                        || old.value.replay_token().is_some() && old.value != r.value
                }) {
                    return Err(EventError::Identity);
                }
            }
            let old = owner.replay.as_ref().map_or(0, |r| r.value.as_str().len());
            state.bytes -= old;
            state.charge(replay.as_ref().map_or(0, |r| r.value.as_str().len()))?;
            let owner = state.open_item(item)?;
            owner.replay = replay;
            owner.status = Some(status);
        }
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
fn charge_tool_result(
    state: &mut StreamState,
    result: &ToolResult,
    previous: Option<&ToolResult>,
) -> Result<(), EventError> {
    let mut parts = state.part_ids.clone();
    if let Some(ToolResult {
        output: ToolOutput::Parts(old),
        ..
    }) = previous
    {
        for part in old {
            parts.remove(&part.id);
        }
    }
    let mut bytes = 0;
    crate::semantic::task::generation::validate::tool_result(
        result,
        &mut bytes,
        &mut parts,
        &state.resources,
        &mut state.file_decoded_bytes,
    )?;
    state.part_ids.extend(parts);
    if state.part_ids.len() > MAX_ITEMS {
        return Err(EventError::Limit);
    }
    if let ToolOutput::Parts(parts) = &result.output {
        for part in parts {
            if let InputPart::Resource(value) = &part.content
                && state.used_resources.insert(value.id)
            {
                bytes = bytes
                    .checked_sub(value.resolve(&state.resources)?.source_view().validate()?)
                    .ok_or(EventError::Limit)?;
            }
        }
    }
    // Snapshot updates consume a bounded cumulative value budget, like deltas.
    state.charge(bytes)
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
fn reported_alias(observation: &ProviderToolObservation) -> Option<&Text> {
    match &observation.operation {
        ProviderOperation::Reported { alias, .. } => alias.as_ref(),
        _ => None,
    }
}
