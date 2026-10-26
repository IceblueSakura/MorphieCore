//! Owner-bound replay associations and dependency fingerprints.
use super::*;

impl FidelityRecords {
    /// Capture at trusted decode/event boundaries, never to bless a transformed value.
    pub fn record_replay(
        &mut self,
        owner: ItemId,
        semantic: &ReasoningItem,
        origin: Option<ReplayOrigin>,
    ) -> Result<(), CodecError> {
        let Some(value) = &semantic.replay else {
            self.remove_replay(owner);
            return Ok(());
        };
        value.validate_reasoning()?;
        self.record_opaque(
            owner,
            value,
            fingerprint(semantic),
            origin,
            ReplayOwnerKind::Reasoning,
        )
    }
    pub fn record_compaction(
        &mut self,
        owner: ItemId,
        value: &ReplayValue,
        origin: Option<ReplayOrigin>,
    ) -> Result<(), CodecError> {
        value.validate_compaction()?;
        self.record_opaque(
            owner,
            value,
            value.fingerprint(),
            origin,
            ReplayOwnerKind::Compaction,
        )
    }
    pub fn record_call_replay(
        &mut self,
        owner: ItemId,
        call: &ToolCall,
        origin: Option<ReplayOrigin>,
    ) -> Result<(), CodecError> {
        let Some(value) = &call.replay else {
            self.remove_replay(owner);
            return Ok(());
        };
        value.validate_call_attachment()?;
        self.record_opaque(
            owner,
            value,
            call_dependency(call)?,
            origin,
            ReplayOwnerKind::Call,
        )
    }
    pub fn record_part_replay(
        &mut self,
        owner: ItemId,
        status: ItemLifecycle,
        ordinal: usize,
        part: &Part,
        origin: Option<ReplayOrigin>,
    ) -> Result<(), CodecError> {
        let key = (owner, part.id);
        let Some(value) = &part.replay else {
            self.part_replay.remove(&key);
            return Ok(());
        };
        value.validate_attachment()?;
        let dependency = part_dependency(status, ordinal, part)?;
        if !self.part_replay.contains_key(&key) && self.part_replay.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let final_value = value.replay_token().is_some();
        if self.part_replay.get(&key).is_some_and(|old| {
            old.final_value && !final_value || old.origin.is_some() && old.origin != origin
        }) {
            return Err(CodecError::invalid());
        }
        self.part_replay.insert(
            key,
            ReplayBinding {
                kind: ReplayOwnerKind::Part,
                origin,
                dependency,
                final_value,
            },
        );
        Ok(())
    }
    pub fn part_replay_origin(&self, owner: ItemId, part: PartId) -> Option<&ReplayOrigin> {
        self.part_replay.get(&(owner, part))?.origin.as_ref()
    }
    pub fn remove_part_replay(&mut self, owner: ItemId, part: PartId) {
        self.part_replay.remove(&(owner, part));
    }
    pub fn part_replay_matches(
        &self,
        owner: ItemId,
        status: ItemLifecycle,
        ordinal: usize,
        part: &Part,
        target: Option<&ReplayOrigin>,
    ) -> bool {
        self.part_replay
            .get(&(owner, part.id))
            .is_some_and(|binding| {
                binding.origin.is_some()
                    && binding.origin.as_ref() == target
                    && part_dependency(status, ordinal, part)
                        .is_ok_and(|dependency| dependency == binding.dependency)
            })
    }
    pub fn call_replay_matches(
        &self,
        owner: ItemId,
        call: &ToolCall,
        target: Option<&ReplayOrigin>,
    ) -> bool {
        self.item_replay.get(&owner).is_some_and(|binding| {
            binding.kind == ReplayOwnerKind::Call
                && binding.origin.is_some()
                && binding.origin.as_ref() == target
                && call_dependency(call).is_ok_and(|dependency| dependency == binding.dependency)
        })
    }
    fn record_opaque(
        &mut self,
        owner: ItemId,
        value: &ReplayValue,
        dependency: [u8; 32],
        origin: Option<ReplayOrigin>,
        kind: ReplayOwnerKind,
    ) -> Result<(), CodecError> {
        if !self.item_replay.contains_key(&owner) && self.item_replay.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let final_value = value.replay_token().is_some();
        if let Some(old) = self.item_replay.get(&owner) {
            if old.kind != kind {
                return Err(CodecError::invalid());
            }
            if old.final_value && !final_value {
                return Err(CodecError::invalid());
            }
            if old.origin.is_some() && old.origin != origin {
                return Err(CodecError::invalid());
            }
        }
        self.item_replay.insert(
            owner,
            ReplayBinding {
                kind,
                origin,
                dependency,
                final_value,
            },
        );
        Ok(())
    }
    pub fn compaction_matches(
        &self,
        owner: ItemId,
        value: &ReplayValue,
        target: Option<&ReplayOrigin>,
    ) -> bool {
        self.item_replay.get(&owner).is_some_and(|binding| {
            binding.origin.is_some()
                && binding.origin.as_ref() == target
                && binding.dependency == value.fingerprint()
        })
    }
    pub fn replay_matches(
        &self,
        owner: ItemId,
        semantic: &ReasoningItem,
        target: Option<&ReplayOrigin>,
    ) -> bool {
        self.item_replay.get(&owner).is_some_and(|binding| {
            binding.origin.is_some()
                && binding.origin.as_ref() == target
                && binding.dependency == fingerprint(semantic)
        })
    }
    /// The owner must also still carry the same value in the final history.
    pub fn replay_matches_request(
        &self,
        owner: ItemId,
        semantic: &ReasoningItem,
        target: Option<&ReplayOrigin>,
        request: &GenerationRequest,
    ) -> bool {
        self.replay_matches(owner, semantic, target)
            && request.items().iter().any(|(id, item)| {
                *id == owner && matches!(item, Item::Reasoning(value) if value == semantic)
            })
    }
    /// Selected native history must not lose a required owner or its value.
    pub(crate) fn check_target_replay(
        &self,
        request: &GenerationRequest,
        target: Option<&ReplayOrigin>,
    ) -> Result<(), CodecError> {
        let owners: BTreeMap<_, _> = request
            .items()
            .iter()
            .map(|(id, item)| (*id, item))
            .collect();
        let parts = message_parts(request.items());
        for (owner, binding) in &self.item_replay {
            if target.is_none() || binding.origin.as_ref() != target {
                continue;
            }
            let valid = match owners.get(owner) {
                Some(Item::Reasoning(value)) => self.replay_matches(*owner, value, target),
                Some(Item::Compaction(value)) => self.compaction_matches(*owner, value, target),
                Some(Item::ToolCall(call)) => self.call_replay_matches(*owner, call, target),
                _ => false,
            };
            if !valid {
                return Err(CodecError::invalid());
            }
        }
        for ((owner, part), binding) in &self.part_replay {
            if target.is_none() || binding.origin.as_ref() != target {
                continue;
            }
            let valid = parts
                .get(&(*owner, *part))
                .is_some_and(|(status, ordinal, p)| {
                    self.part_replay_matches(*owner, *status, *ordinal, p, target)
                });
            if !valid {
                return Err(CodecError::invalid());
            }
        }
        Ok(())
    }
    /// Selected native history must not lose a required owner or its value.
    pub(crate) fn require_native_replay(
        &self,
        request: &GenerationRequest,
        target: &ReplayOrigin,
    ) -> Result<(), CodecError> {
        self.check_target_replay(request, Some(target))?;
        if self
            .item_replay
            .values()
            .chain(self.part_replay.values())
            .any(|binding| binding.origin.as_ref() != Some(target))
        {
            return Err(CodecError::invalid());
        }
        for (owner, item) in request.items() {
            let valid = match item {
                Item::Reasoning(r) if r.replay.is_some() => {
                    self.replay_matches(*owner, r, Some(target))
                }
                Item::Compaction(value) => self.compaction_matches(*owner, value, Some(target)),
                Item::ToolCall(call) if call.replay.is_some() => {
                    self.call_replay_matches(*owner, call, Some(target))
                }
                Item::Message(message) => message.parts.iter().enumerate().all(|(ordinal, p)| {
                    p.replay.is_none()
                        || self.part_replay_matches(
                            *owner,
                            message.status,
                            ordinal,
                            p,
                            Some(target),
                        )
                }),
                _ => true,
            };
            if !valid {
                return Err(CodecError::invalid());
            }
        }
        Ok(())
    }
    /// Compatibility label associated at intake, not proof of issuer authenticity.
    pub fn replay_origin(&self, owner: ItemId) -> Option<&ReplayOrigin> {
        self.item_replay.get(&owner)?.origin.as_ref()
    }
    pub(crate) fn is_call_replay(&self, owner: ItemId) -> bool {
        self.item_replay
            .get(&owner)
            .is_some_and(|binding| binding.kind == ReplayOwnerKind::Call)
    }
    /// Sealing cannot silently erase a recorded attachment or its owner.
    pub(crate) fn check_attachment_bindings(
        &self,
        items: &[(ItemId, Item)],
    ) -> Result<(), CodecError> {
        let owners: BTreeMap<_, _> = items.iter().map(|(id, item)| (*id, item)).collect();
        let parts = message_parts(items);
        for (owner, binding) in &self.item_replay {
            if binding.kind == ReplayOwnerKind::Call
                && !owners.get(owner).is_some_and(|item| {
                    matches!(item,Item::ToolCall(call)
                    if self.call_replay_matches(*owner,call,binding.origin.as_ref()))
                })
            {
                return Err(CodecError::invalid());
            }
        }
        for ((owner, part), binding) in &self.part_replay {
            if !parts
                .get(&(*owner, *part))
                .is_some_and(|(status, ordinal, p)| {
                    self.part_replay_matches(*owner, *status, *ordinal, p, binding.origin.as_ref())
                })
            {
                return Err(CodecError::invalid());
            }
        }
        Ok(())
    }
    pub fn remove_replay(&mut self, owner: ItemId) {
        self.item_replay.remove(&owner);
    }
    /// Bind origin only at a trusted decode boundary, never from business JSON.
    pub fn bind_replay_origin(&mut self, origin: &ReplayOrigin) -> Result<(), CodecError> {
        self.bind_origins(origin, None)
    }
    /// A typed retained prefix keeps its original owner-local associations.
    pub(crate) fn bind_replay_origin_for_items(
        &mut self,
        origin: &ReplayOrigin,
        items: &[(ItemId, Item)],
    ) -> Result<(), CodecError> {
        let owners = items.iter().map(|(id, _)| *id).collect();
        self.bind_origins(origin, Some(&owners))
    }
    fn bind_origins(
        &mut self,
        origin: &ReplayOrigin,
        owners: Option<&std::collections::BTreeSet<ItemId>>,
    ) -> Result<(), CodecError> {
        if self
            .item_replay
            .iter()
            .chain(self.part_replay.iter().map(|((owner, _), r)| (owner, r)))
            .filter(|(owner, _)| owners.is_none_or(|owners| owners.contains(owner)))
            .any(|(_, r)| r.origin.as_ref().is_some_and(|old| old != origin))
        {
            return Err(CodecError::invalid());
        }
        for replay in self
            .item_replay
            .iter_mut()
            .chain(
                self.part_replay
                    .iter_mut()
                    .map(|((owner, _), r)| (owner, r)),
            )
            .filter(|(owner, _)| owners.is_none_or(|owners| owners.contains(owner)))
        {
            replay.1.origin = Some(origin.clone());
        }
        Ok(())
    }
}

// A digest binds replay to its typed owner without retaining a second readable payload.
pub(super) fn fingerprint(item: &ReasoningItem) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update([match item.status {
        ItemLifecycle::Completed => 0,
        ItemLifecycle::Incomplete => 1,
        ItemLifecycle::InProgress => 2,
    }]);
    match &item.replay {
        None => hash.update([0]),
        Some(value) => {
            hash.update([1]);
            hash.update(value.fingerprint());
        }
    }
    for (id, part) in &item.parts {
        hash.update(id.scope().get().to_le_bytes());
        hash.update(id.get().to_le_bytes());
        let (kind, text) = match part {
            ReasoningContent::Summary(t) => (0, t),
            ReasoningContent::Text(t) => (1, t),
        };
        hash.update([kind]);
        hash.update((text.as_str().len() as u64).to_le_bytes());
        hash.update(text.as_str().as_bytes());
    }
    hash.finalize().into()
}

pub(super) fn part_dependency(
    status: ItemLifecycle,
    ordinal: usize,
    part: &Part,
) -> Result<[u8; 32], CodecError> {
    use sha2::{Digest, Sha256};
    let ContentPart::Text(text) = &part.content else {
        return Err(CodecError::invalid());
    };
    let value = part.replay.as_ref().ok_or(CodecError::invalid())?;
    let mut hash = Sha256::new();
    hash.update(format!("{status:?}").as_bytes());
    hash.update((ordinal as u64).to_le_bytes());
    hash.update(part.id.scope().get().to_le_bytes());
    hash.update(part.id.get().to_le_bytes());
    hash.update(text.as_str().as_bytes());
    hash.update(value.fingerprint());
    Ok(hash.finalize().into())
}
pub(super) fn call_dependency(call: &ToolCall) -> Result<[u8; 32], CodecError> {
    use sha2::{Digest, Sha256};
    let value = call.replay.as_ref().ok_or(CodecError::invalid())?;
    let arguments = match &call.arguments {
        ToolArguments::Structured(value) => {
            serde_json::to_string(value.value()).map_err(|_| CodecError::invalid())?
        }
        ToolArguments::Raw(raw) => {
            crate::semantic::task::generation::StructuredValue::from_bytes(raw.as_bytes())
                .ok()
                .and_then(|v| serde_json::to_string(v.value()).ok())
                .unwrap_or_else(|| raw.clone())
        }
        ToolArguments::StructuredPartial(raw) => raw.clone(),
    };
    let mut hash = Sha256::new();
    for text in [
        call.call_id.as_str(),
        call.name.as_str(),
        arguments.as_str(),
    ] {
        hash.update((text.len() as u64).to_le_bytes());
        hash.update(text.as_bytes());
    }
    hash.update(format!("{:?}{:?}", call.status, call.context).as_bytes());
    hash.update(value.fingerprint());
    Ok(hash.finalize().into())
}
