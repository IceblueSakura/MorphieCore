//! Atomic caller-supplied edits and content-free receipts, not a history store.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
pub enum ContextEdit {
    Insert {
        at: usize,
        item: (ItemId, Item),
    },
    Delete(Vec<ItemId>),
    Replace {
        owner: ItemId,
        item: Item,
    },
    Reorder(Vec<ItemId>),
    Summary {
        sources: Vec<ItemId>,
        owner: ItemId,
        part: PartId,
        text: Text,
    },
    MessageEnvelopes(Vec<MessageEnvelope>),
}
impl std::fmt::Debug for ContextEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextEdit([redacted])")
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextChange {
    Inserted(ItemId),
    Deleted(Vec<ItemId>),
    Replaced(ItemId),
    Reordered(Vec<ItemId>),
    Summarized { sources: Vec<ItemId>, owner: ItemId },
    MessageEnvelopes,
}
impl ContextChange {
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Deleted(ids) | Self::Reordered(ids) | Self::Summarized { sources: ids, .. } => {
                1 + ids.len()
            }
            _ => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextStage {
    Selection,
    Edit,
    Association,
}
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("context {stage:?} failed at {index:?}: {error}")]
pub struct ContextError {
    pub stage: ContextStage,
    pub index: Option<usize>,
    pub error: GenerationError,
}
impl ContextError {
    pub(super) fn at(stage: ContextStage, index: Option<usize>, error: GenerationError) -> Self {
        Self {
            stage,
            index,
            error,
        }
    }
}
/// Selection position is a local diagnostic coordinate, never a reported response ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedHistory {
    pub owners: Vec<ItemId>,
    /// Original response disposition, not a recomputation on the selected subset.
    pub progress: Option<InteractionProgress>,
}
#[derive(Clone)]
pub struct ContextBuild {
    pub(super) request: GenerationRequest,
    pub(super) changes: Vec<ContextChange>,
    pub(super) selections: Vec<SelectedHistory>,
}
impl std::fmt::Debug for ContextBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextBuild([redacted])")
    }
}
impl ContextBuild {
    pub fn request(&self) -> &GenerationRequest {
        &self.request
    }
    pub fn into_request(self) -> GenerationRequest {
        self.request
    }
    pub fn changes(&self) -> &[ContextChange] {
        &self.changes
    }
    pub fn selections(&self) -> &[SelectedHistory] {
        &self.selections
    }
    pub fn client_results(&self) -> Continuation<'_> {
        self.request.continuation()
    }
}
pub(super) fn selected<'a>(
    items: &'a [(ItemId, Item)],
    owners: &[ItemId],
) -> Result<impl Iterator<Item = &'a (ItemId, Item)>, GenerationError> {
    if owners.len() > MAX_ITEMS {
        return Err(GenerationError::Limit);
    }
    let wanted: BTreeSet<_> = owners.iter().copied().collect();
    if wanted.len() != owners.len()
        || owners
            .iter()
            .any(|id| !items.iter().any(|(owner, _)| owner == id))
    {
        return Err(GenerationError::InvalidDependency);
    }
    // Selection preserves source order. Reordering is a separate explicit edit.
    Ok(items.iter().filter(move |(id, _)| wanted.contains(id)))
}
fn index(items: &[(ItemId, Item)], owner: ItemId) -> Result<usize, GenerationError> {
    items
        .iter()
        .position(|(id, _)| *id == owner)
        .ok_or(GenerationError::InvalidDependency)
}
fn contains_part(items: &[(ItemId, Item)], part: PartId) -> bool {
    items.iter().any(|(_, item)| match item {
        Item::Message(m) => m.parts.iter().any(|p| p.id == part),
        Item::Instruction(i) => i.parts.iter().any(|p| p.id == part),
        Item::Reasoning(r) => r.parts.iter().any(|(id, _)| *id == part),
        Item::ToolResult(r) | Item::CustomResult(r) => matches!(&r.output,
            ToolOutput::Parts(parts) if parts.iter().any(|p| p.id == part)),
        Item::ProviderTool(p) => matches!(&p.output,
            Some(ToolOutput::Parts(parts)) if parts.iter().any(|p| p.id == part)),
        _ => false,
    })
}

fn apply(
    request: &mut GenerationRequest,
    source: &GenerationRequest,
    edit: ContextEdit,
) -> Result<ContextChange, GenerationError> {
    Ok(match edit {
        ContextEdit::Insert { at, item } => {
            if at > request.items.len() || request.items.len() == MAX_ITEMS {
                return Err(GenerationError::Limit);
            }
            if request
                .items
                .iter()
                .chain(source.items())
                .any(|(id, _)| *id == item.0)
            {
                return Err(GenerationError::DuplicateItemId);
            }
            let owner = item.0;
            request.items.insert(at, item);
            ContextChange::Inserted(owner)
        }
        ContextEdit::Delete(owners) => {
            let _ = selected(&request.items, &owners)?;
            request.items.retain(|(id, _)| !owners.contains(id));
            ContextChange::Deleted(owners)
        }
        ContextEdit::Replace { owner, item } => {
            let position = index(&request.items, owner)?;
            request.items[position].1 = item;
            ContextChange::Replaced(owner)
        }
        ContextEdit::Reorder(order) => {
            if order.len() != request.items.len() {
                return Err(GenerationError::InvalidDependency);
            }
            let _ = selected(&request.items, &order)?;
            request
                .items
                .sort_by_key(|(id, _)| order.iter().position(|next| next == id));
            ContextChange::Reordered(order)
        }
        ContextEdit::Summary {
            sources,
            owner,
            part,
            text,
        } => summarize(request, source, sources, owner, part, text)?,
        ContextEdit::MessageEnvelopes(groups) => {
            request.envelopes =
                crate::semantic::task::generation::history::envelope::MessageEnvelopes::new(
                    &request.items,
                    groups,
                )?;
            ContextChange::MessageEnvelopes
        }
    })
}

/// Replaces a contiguous run of plain completed messages with one user summary.
fn summarize(
    request: &mut GenerationRequest,
    source: &GenerationRequest,
    sources: Vec<ItemId>,
    owner: ItemId,
    part: PartId,
    text: Text,
) -> Result<ContextChange, GenerationError> {
    if sources.is_empty() || text.as_str().len() > MAX_TEXT_BYTES {
        return Err(GenerationError::Limit);
    }
    let values: Vec<_> = selected(&request.items, &sources)?.collect();
    let plain_message = |item: &Item| {
        matches!(item, Item::Message(m)
            if m.phase.is_none()
                && m.status == ItemLifecycle::Completed
                && m.parts
                    .iter()
                    .all(|p| matches!(&p.content, ContentPart::Text(t) if t.is_plain())))
    };
    if source.items.iter().any(|(id, _)| *id == owner)
        || request.items.iter().any(|(id, _)| *id == owner)
        || contains_part(source.items(), part)
        || contains_part(request.items(), part)
        || !values.iter().all(|(_, item)| plain_message(item))
    {
        return Err(GenerationError::InvalidContextTransform);
    }
    let in_sources = |(id, _): &(ItemId, Item)| sources.contains(id);
    let position = request
        .items
        .iter()
        .position(in_sources)
        .ok_or(GenerationError::InvalidDependency)?;
    let last = request
        .items
        .iter()
        .rposition(in_sources)
        .ok_or(GenerationError::InvalidDependency)?;
    if last - position + 1 != sources.len() {
        return Err(GenerationError::InvalidContextTransform);
    }
    let sources: Vec<_> = values.iter().map(|(id, _)| *id).collect();
    request.items.retain(|(id, _)| !sources.contains(id));
    let summary = Message {
        role: MessageRole::User,
        parts: vec![Part::new(part, ContentPart::Text(text.into()))],
        status: ItemLifecycle::Completed,
        phase: None,
    };
    request
        .items
        .insert(position, (owner, Item::Message(summary)));
    Ok(ContextChange::Summarized { sources, owner })
}

impl GenerationRequest {
    pub fn transform(self, edits: Vec<ContextEdit>) -> Result<ContextBuild, ContextError> {
        let fail = |stage, index, error| ContextError::at(stage, index, error);
        if edits.len() > MAX_ITEMS {
            return Err(fail(ContextStage::Edit, None, GenerationError::Limit));
        }
        let source = self.clone();
        let mut request = self;
        let mut changes = Vec::new();
        let mut weight = 0usize;
        for (at, edit) in edits.into_iter().enumerate() {
            let changed = apply(&mut request, &source, edit)
                .map_err(|error| fail(ContextStage::Edit, Some(at), error))?;
            weight = weight.saturating_add(changed.weight());
            if weight > MAX_ITEMS {
                return Err(fail(ContextStage::Edit, Some(at), GenerationError::Limit));
            }
            changes.push(changed);
        }
        request.envelopes = request
            .envelopes
            .edited(&request.items)
            .map_err(|e| fail(ContextStage::Association, None, e))?;
        crate::semantic::task::generation::identity::check_call_edits(
            source.items(),
            request.items(),
        )
        .map_err(|e| fail(ContextStage::Association, None, e))?;
        check_protected_edits(source.items(), request.items())
            .map_err(|e| fail(ContextStage::Association, None, e))?;
        request
            .validate()
            .map_err(|e| fail(ContextStage::Association, None, e))?;
        Ok(ContextBuild {
            request,
            changes,
            selections: vec![],
        })
    }
}

pub(in crate::semantic::task::generation) fn check_protected_edits(
    source: &[(ItemId, Item)],
    final_items: &[(ItemId, Item)],
) -> Result<(), GenerationError> {
    for (old_position, (id, old)) in source.iter().enumerate() {
        let Some((new_position, (_, new))) = final_items
            .iter()
            .enumerate()
            .find(|(_, (next, _))| next == id)
        else {
            continue;
        };
        if std::mem::discriminant(old) != std::mem::discriminant(new) {
            return Err(GenerationError::InvalidContextTransform);
        }
        if let (Item::Instruction(old), Item::Instruction(new)) = (old, new) {
            if old.authority != new.authority {
                return Err(GenerationError::InvalidContextTransform);
            }
            for (other_position, (other, _)) in source.iter().enumerate() {
                if let Some(position) = final_items.iter().position(|(id, _)| id == other)
                    && (other_position < old_position) != (position < new_position)
                {
                    return Err(GenerationError::InvalidContextTransform);
                }
            }
        }
        if let (Item::Message(old), Item::Message(new)) = (old, new)
            && (old.role != new.role || old.phase != new.phase)
        {
            return Err(GenerationError::InvalidContextTransform);
        }
    }
    Ok(())
}
