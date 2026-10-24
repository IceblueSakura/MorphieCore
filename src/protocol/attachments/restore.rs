//! Transactional owner/part reconstruction from verified returned projections.
use super::*;

pub(crate) struct Marker {
    pub(super) start: usize,
    pub(super) end: usize,
    /// `None` when the returned carrier cannot be decoded.
    pub(super) packet: Option<Packet>,
}
impl Marker {
    pub(crate) fn bounded(carried: Carried, start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            packet: match carried {
                Carried::Manifest(value) => Packet::decode(&value).ok(),
                Carried::Unusable => None,
            },
        }
    }
    pub(crate) fn prefix(
        carried: Carried,
        items: &[(ItemId, Item)],
        previous: Option<&Self>,
    ) -> Self {
        let end = items.len();
        let mut start = end;
        let boundary = previous.map_or(0, |marker| marker.end);
        while start > boundary && assistant(&items[start - 1].1) {
            start -= 1;
        }
        Self::bounded(carried, start, end)
    }
}
fn assistant(item: &Item) -> bool {
    matches!(item, Item::Reasoning(_) | Item::ToolCall(_))
        || matches!(item,Item::Message(m) if m.role==MessageRole::Assistant)
}
struct TextPiece<'a> {
    part: &'a Part,
    status: ItemLifecycle,
    phase: Option<Phase>,
}
fn status(observed: ItemLifecycle, described: Status) -> Result<ItemLifecycle, CodecError> {
    let described = described.into();
    if observed != ItemLifecycle::Completed && observed != described {
        return Err(invalid());
    }
    Ok(described)
}
impl TextPiece<'_> {
    fn check(&self, desired: Status, phase: Option<Phase>) -> Result<(), CodecError> {
        status(self.status, desired)?;
        if self.phase.is_some() && self.phase != phase {
            return Err(invalid());
        }
        Ok(())
    }
}
struct TextCursor<'a> {
    parts: Vec<TextPiece<'a>>,
    index: usize,
    offset: usize,
}
impl TextCursor<'_> {
    fn take(
        &mut self,
        len: usize,
        id: PartId,
        status: Status,
        phase: Option<Phase>,
        fidelity: &mut FidelityRecords,
    ) -> Result<Part, CodecError> {
        if let Some(piece) = self.parts.get(self.index) {
            let part = piece.part;
            let ContentPart::Text(text) = &part.content else {
                return Err(unsupported());
            };
            if self.offset == 0 && text.as_str().len() == len {
                piece.check(status, phase)?;
                self.index += 1;
                let mut value = part.clone();
                // Preserve existing metadata only when its entire part survives.
                value.id = id;
                if fidelity.input_text_form(part.id) {
                    fidelity.record_input_text(id)?;
                }
                return Ok(value);
            }
        }
        let mut value = String::with_capacity(len);
        let mut cache = None;
        while value.len() < len {
            let piece = self.parts.get(self.index).ok_or_else(invalid)?;
            piece.check(status, phase)?;
            let part = piece.part;
            let ContentPart::Text(text) = &part.content else {
                return Err(unsupported());
            };
            if !text.is_plain() {
                return Err(CodecError::Unsupported("attachment metadata split".into()));
            }
            let take = (len - value.len()).min(text.as_str().len() - self.offset);
            let end = self.offset + take;
            if !text.as_str().is_char_boundary(end) {
                return Err(invalid());
            }
            value.push_str(&text.as_str()[self.offset..end]);
            self.offset = end;
            if self.offset == text.as_str().len() {
                if part.cache.is_some() {
                    if value.len() != len {
                        return Err(CodecError::Unsupported("attachment cache boundary".into()));
                    }
                    cache = part.cache;
                }
                self.index += 1;
                self.offset = 0;
            }
        }
        let mut part = Part::new(
            id,
            ContentPart::Text(
                Text::allowing_empty(value, "attachment text", MAX_TEXT_BYTES)
                    .map_err(|_| invalid())?
                    .into(),
            ),
        );
        part.cache = cache;
        Ok(part)
    }
    fn finish(&self) -> bool {
        self.index == self.parts.len() && self.offset == 0
    }
}

/// Fresh derived identities for restored owners; skipped values after a fallback are harmless.
pub(super) struct Ids {
    pub(super) item: u64,
    pub(super) part: u64,
    pub(super) group: u64,
}
impl Ids {
    fn next(value: &mut u64) -> Result<u64, CodecError> {
        *value = value.checked_add(1).ok_or(CodecError::Limit)?;
        Ok(*value)
    }
    fn item(&mut self) -> Result<ItemId, CodecError> {
        Ok(ItemId::scoped(
            LocalScope::DERIVED,
            Self::next(&mut self.item)?,
        ))
    }
    fn part(&mut self) -> Result<PartId, CodecError> {
        Ok(PartId::scoped(
            LocalScope::DERIVED,
            Self::next(&mut self.part)?,
        ))
    }
    fn group(&mut self) -> Result<GroupId, CodecError> {
        Ok(GroupId::new(
            LocalScope::DERIVED,
            Self::next(&mut self.group)?,
        ))
    }
}
type Restored = (Vec<(ItemId, Item)>, Vec<MessageEnvelope>);
/// Rebuild one carrier's owners. Any mismatch with the returned visible history fails
/// without touching the caller's records.
pub(super) fn restore_marker(
    previous: &[(ItemId, Item)],
    packet: Packet,
    fidelity: &mut FidelityRecords,
    ids: &mut Ids,
) -> Result<Restored, CodecError> {
    if previous.iter().any(|(_, i)| !assistant(i)) {
        return Err(invalid());
    }
    let mut texts = TextCursor {
        parts: vec![],
        index: 0,
        offset: 0,
    };
    let mut barriers = Vec::new();
    let mut text_bytes = 0usize;
    for (id, item) in previous {
        match item {
            Item::Message(m) => {
                if m.parts
                    .iter()
                    .any(|p| !matches!(p.content, ContentPart::Text(_)) || p.replay.is_some())
                {
                    return Err(unsupported());
                }
                if m.parts.is_empty() && m.status != ItemLifecycle::Completed {
                    return Err(invalid());
                }
                texts.parts.extend(m.parts.iter().map(|part| TextPiece {
                    part,
                    status: m.status,
                    phase: m.phase,
                }));
                for part in &m.parts {
                    let ContentPart::Text(text) = &part.content else {
                        return Err(unsupported());
                    };
                    text_bytes = text_bytes
                        .checked_add(text.as_str().len())
                        .ok_or(CodecError::Limit)?;
                }
            }
            Item::ToolCall(_) | Item::Reasoning(_) => {
                barriers.push((*id, item.clone(), text_bytes))
            }
            _ => return Err(unsupported()),
        }
    }
    let mut barriers = barriers.into_iter();
    let mut consumed_text = 0usize;
    let mut restored = Vec::new();
    for entry in packet.items {
        restored.push(match entry {
            Entry::Message {
                status,
                phase,
                parts,
            } => {
                let phase = phase
                    .as_deref()
                    .map(|p| match p {
                        "commentary" => Ok(Phase::Commentary),
                        "final_answer" => Ok(Phase::FinalAnswer),
                        _ => Err(invalid()),
                    })
                    .transpose()?;
                let owner = ids.item()?;
                let mut content = Vec::new();
                for spec in parts {
                    let mut part = texts.take(spec.bytes, ids.part()?, status, phase, fidelity)?;
                    let ContentPart::Text(text) = &part.content else {
                        return Err(invalid());
                    };
                    if digest(&[text.as_str()]) != spec.hash {
                        return Err(invalid());
                    }
                    consumed_text = consumed_text
                        .checked_add(spec.bytes)
                        .ok_or(CodecError::Limit)?;
                    part.replay = replay(spec.replay.as_deref())?;
                    fidelity.record_part_replay(
                        owner,
                        status.into(),
                        content.len(),
                        &part,
                        None,
                    )?;
                    content.push(part);
                }
                (
                    owner,
                    Item::Message(Message {
                        role: MessageRole::Assistant,
                        status: status.into(),
                        phase,
                        parts: content,
                    }),
                )
            }
            Entry::Call {
                status: described,
                hash,
                replay: sealed,
            } => {
                let (owner, Item::ToolCall(mut call), offset) =
                    barriers.next().ok_or_else(invalid)?
                else {
                    return Err(invalid());
                };
                if offset != consumed_text {
                    return Err(invalid());
                }
                if call.replay.is_some() || call_hash(&call)? != hash {
                    return Err(invalid());
                }
                call.status = status(call.status, described)?;
                call.replay = replay(sealed.as_deref())?;
                fidelity.record_call_replay(owner, &call, None)?;
                (owner, Item::ToolCall(call))
            }
            Entry::Reasoning {
                status: described,
                hash,
                parts,
            } => {
                let (owner, Item::Reasoning(mut reasoning), offset) =
                    barriers.next().ok_or_else(invalid)?
                else {
                    return Err(invalid());
                };
                if offset != consumed_text {
                    return Err(invalid());
                }
                if reasoning_hash(&reasoning) != hash {
                    return Err(invalid());
                }
                let text = reasoning_text(&reasoning);
                let mut offset = 0usize;
                let mut content = Vec::new();
                for spec in parts {
                    let end = offset.checked_add(spec.bytes).ok_or(CodecError::Limit)?;
                    let value = text.get(offset..end).ok_or_else(invalid)?;
                    let value = Text::allowing_empty(value, "attachment reasoning", MAX_TEXT_BYTES)
                        .map_err(|_| invalid())?;
                    content.push((
                        ids.part()?,
                        if spec.summary {
                            ReasoningContent::Summary(value)
                        } else {
                            ReasoningContent::Text(value)
                        },
                    ));
                    offset = end;
                }
                if offset != text.len() {
                    return Err(invalid());
                }
                reasoning.parts = content;
                reasoning.status = status(reasoning.status, described)?;
                fidelity.remove_replay(owner);
                fidelity.record_replay(owner, &reasoning, None)?;
                (owner, Item::Reasoning(reasoning))
            }
        });
    }
    if !texts.finish() || barriers.next().is_some() {
        return Err(invalid());
    }
    let mut groups = Vec::with_capacity(packet.groups.len());
    for group in packet.groups {
        let id = ids.group()?;
        groups.push(if group.members.is_empty() {
            MessageEnvelope::empty(
                id,
                MessageEnvelopeRole::Assistant,
                group.before.map(|n| restored[n].0),
            )
        } else {
            if group.before.is_some() {
                return Err(invalid());
            }
            MessageEnvelope::new(
                id,
                MessageEnvelopeRole::Assistant,
                group.members.iter().map(|n| restored[*n].0).collect(),
            )
            .map_err(|_| invalid())?
        });
    }
    if restored.is_empty() {
        return Err(invalid());
    }
    Ok((restored, groups))
}

/// Restore once, before constructing the validated request. Unchanged visible projection
/// authorizes the original segmentation; this does not prove immutable client edit identity.
/// A carrier that no longer matches is dropped with a receipt: the visible history proceeds
/// without that turn's hidden state instead of rejecting the request.
pub(crate) fn restore(
    items: &mut Vec<(ItemId, Item)>,
    groups: &mut Vec<MessageEnvelope>,
    fidelity: &mut FidelityRecords,
    markers: Vec<Marker>,
) -> Result<(), CodecError> {
    if markers.is_empty() {
        return Ok(());
    }
    let mut ids = Ids {
        item: items.iter().map(|(id, _)| id.get()).max().unwrap_or(0),
        part: items
            .iter()
            .flat_map(|(_, item)| match item {
                Item::Message(m) => m.parts.iter().map(|p| p.id.get()).collect::<Vec<_>>(),
                Item::Reasoning(r) => r.parts.iter().map(|(id, _)| id.get()).collect(),
                _ => vec![],
            })
            .max()
            .unwrap_or(0),
        group: groups.iter().map(|g| g.id().get()).max().unwrap_or(0),
    };
    let mut preceding = 0;
    let mut added_parts = 0usize;
    let mut added_groups = 0usize;
    for marker in &markers {
        if marker.start < preceding || marker.end < marker.start || marker.end > items.len() {
            return Err(invalid());
        }
        preceding = marker.end;
        let Some(packet) = &marker.packet else {
            continue;
        };
        added_groups = added_groups
            .checked_add(packet.groups.len())
            .ok_or(CodecError::Limit)?;
        for entry in &packet.items {
            let count = match entry {
                Entry::Message { parts, .. } => parts.len(),
                Entry::Reasoning { parts, .. } => parts.len(),
                Entry::Call { .. } => 0,
            };
            added_parts = added_parts.checked_add(count).ok_or(CodecError::Limit)?;
        }
        if added_parts > MAX_ITEMS || added_groups > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
    }
    let mut final_items = items.len();
    let mut replacements = Vec::with_capacity(markers.len());
    let mut removed_owners = BTreeMap::new();
    let mut restored_groups = Vec::with_capacity(added_groups);
    for (marker_index, marker) in markers.into_iter().enumerate() {
        let previous = &items[marker.start..marker.end];
        let restored = marker.packet.map(|packet| {
            let mut trial = fidelity.clone();
            restore_marker(previous, packet, &mut trial, &mut ids).map(|value| (value, trial))
        });
        let ((restored, packet_groups), trial) = match restored {
            Some(Ok(value)) => value,
            Some(Err(CodecError::Invalid(_) | CodecError::Unsupported(_))) | None => {
                fidelity.record_omission(crate::protocol::fidelity::REPLAY_MANIFEST_OMISSION);
                continue;
            }
            Some(Err(error)) => return Err(error),
        };
        final_items = final_items
            .checked_sub(previous.len())
            .and_then(|n| n.checked_add(restored.len()))
            .ok_or(CodecError::Limit)?;
        *fidelity = trial;
        for (id, _) in previous {
            removed_owners.insert(*id, (marker_index, restored[0].0));
        }
        restored_groups.extend(packet_groups);
        replacements.push((marker.start, marker.end, restored));
    }
    if final_items > MAX_ITEMS {
        return Err(CodecError::Limit);
    }
    let mut kept = Vec::new();
    for group in groups.drain(..) {
        if let Some((marker, _)) = group.members().iter().find_map(|id| removed_owners.get(id)) {
            if !group
                .members()
                .iter()
                .all(|id| removed_owners.get(id).is_some_and(|(n, _)| n == marker))
            {
                return Err(invalid());
            }
            continue;
        }
        if group.members().is_empty()
            && let Some((_, first)) = group.before().and_then(|id| removed_owners.get(&id))
        {
            kept.push(MessageEnvelope::empty(
                group.id(),
                group.role(),
                Some(*first),
            ));
        } else {
            kept.push(group);
        }
    }
    kept.extend(restored_groups);
    *groups = kept;
    let mut source = std::mem::take(items).into_iter();
    let mut output = Vec::with_capacity(final_items);
    let mut cursor = 0;
    for (start, end, restored) in replacements {
        output.extend(source.by_ref().take(start - cursor));
        for _ in start..end {
            source.next();
        }
        output.extend(restored);
        cursor = end;
    }
    output.extend(source);
    *items = output;
    let positions: BTreeMap<_, _> = items
        .iter()
        .enumerate()
        .map(|(n, (id, _))| (*id, n))
        .collect();
    groups.sort_by_key(|g| {
        g.members()
            .first()
            .or(g.before().as_ref())
            .and_then(|id| positions.get(id))
            .copied()
            .unwrap_or(items.len())
    });
    fidelity.retain_owners(items);
    Ok(())
}
