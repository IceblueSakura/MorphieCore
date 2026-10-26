//! Item identity, result snapshots and closure within the consuming stream reducer.
use crate::semantic::task::generation::*;
use crate::semantic::value::Text;

pub(super) fn start_item(
    state: &mut StreamState,
    item: ItemId,
    kind: ItemKind,
    replay: Option<ItemReplay>,
) -> Result<(), EventError> {
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
        charge_tool_result(state, result, None)?;
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
                        .checked_sub(value.resolve(&state.resources)?.source_view().validate()?)
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
                } => value.source == observed.source && reported_alias(value) == Some(alias),
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
        if !nonempty_within(call_id.as_str(), MAX_ID_BYTES) {
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
            if !nonempty_within(domain.source.as_str(), MAX_ID_BYTES) {
                return Err(EventError::Limit);
            }
            state.charge(domain.source.as_str().len())?;
        }
        if !nonempty_within(name.as_str(), MAX_NAME_BYTES) {
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
        if !nonempty_within(call_id.as_str(), MAX_ID_BYTES) {
            return Err(EventError::Limit);
        }
        if state
            .items
            .iter()
            .any(|i| matches!(&i.kind, ItemKind::ProgramOutput { call_id: c, .. } if c == call_id))
        {
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
    Ok(())
}

pub(super) fn update_tool_result(
    state: &mut StreamState,
    item: ItemId,
    output: ToolOutput,
) -> Result<(), EventError> {
    let owner = state.open_item(item)?;
    let (ItemKind::ToolResult(old) | ItemKind::CustomResult(old)) = &owner.kind else {
        return Err(EventError::Lifecycle);
    };
    let old = old.clone();
    if old.output != output {
        let mut next = old.clone();
        next.output = output;
        charge_tool_result(state, &next, Some(&old))?;
        let owner = state.open_item(item)?;
        let (ItemKind::ToolResult(result) | ItemKind::CustomResult(result)) = &mut owner.kind
        else {
            unreachable!("checked result kind");
        };
        *result = next;
    }
    Ok(())
}

pub(super) fn finish_item(
    state: &mut StreamState,
    item: ItemId,
    status: ItemLifecycle,
    replay: Option<ItemReplay>,
) -> Result<(), EventError> {
    if status == ItemLifecycle::InProgress {
        return Err(EventError::Lifecycle);
    }
    let owner = state.open_item(item)?;
    if replay.is_none()
        && owner.replay.as_ref().is_some_and(|old| {
            matches!(owner.kind, ItemKind::ToolCall { .. }) || old.value.replay_token().is_some()
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
    Ok(())
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
        true,
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
fn reported_alias(observation: &ProviderToolObservation) -> Option<&Text> {
    match &observation.operation {
        ProviderOperation::Reported { alias, .. } => alias.as_ref(),
        _ => None,
    }
}
