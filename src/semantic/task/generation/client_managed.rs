//! Consuming, bounded selection into one final request, not a session/history store.
use super::*;

#[derive(Clone)]
pub struct ClientManaged {
    request: GenerationRequest,
}
impl std::fmt::Debug for ClientManaged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientManaged([redacted])")
    }
}
impl ClientManaged {
    /// Each build receives the complete current configuration explicitly.
    pub fn new(settings: GenerationSettings) -> Result<Self, GenerationError> {
        settings.validate()?;
        Ok(Self {
            request: GenerationRequest {
                items: vec![],
                settings,
                replay_groups: vec![],
                call_derivations: Default::default(),
                message_owners: Default::default(),
            },
        })
    }
    pub fn select_request(mut self, source: &GenerationRequest) -> Result<Self, GenerationError> {
        source.validate()?;
        self.reserve_selection(source.items(), source.replay_groups())?;
        self.request.items.extend_from_slice(source.items());
        self.request
            .replay_groups
            .extend_from_slice(source.replay_groups());
        self.merge_owners(source.message_owners())?;
        for (owner, ancestor) in source.call_derivations() {
            if self
                .request
                .call_derivations
                .insert(*owner, *ancestor)
                .is_some()
            {
                return Err(GenerationError::InvalidDependency);
            }
        }
        self.request.validate()?;
        Ok(self)
    }
    pub fn select_response(mut self, source: &GenerationResponse) -> Result<Self, GenerationError> {
        self.reserve_selection(source.items(), source.replay_groups())?;
        self.request.items.extend_from_slice(source.items());
        self.request
            .replay_groups
            .extend_from_slice(source.replay_groups());
        self.merge_owners(source.message_owners())?;
        self.request.validate()?;
        Ok(self)
    }
    /// Explicit new input/results; edits of selected history use validated request APIs.
    pub fn append_items(mut self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        self.reserve_selection(&items, &[])?;
        self.request.items.extend(items);
        self.request.validate()?;
        Ok(self)
    }
    pub fn build(
        self,
        dependencies: &[RequestDependencyProof],
    ) -> Result<GenerationRequest, GenerationError> {
        if dependencies.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        self.request.validate()?;
        for dependency in dependencies {
            dependency.check(&self.request)?;
        }
        Ok(self.request)
    }
    fn reserve_selection(
        &self,
        items: &[(ItemId, Item)],
        groups: &[ReplayGroup],
    ) -> Result<(), GenerationError> {
        if items.len().saturating_add(self.request.items.len()) > MAX_ITEMS
            || groups
                .len()
                .saturating_add(self.request.replay_groups.len())
                > MAX_ITEMS
        {
            return Err(GenerationError::Limit);
        }
        // Reject overlapping selected identities rather than renumbering or deduplicating.
        if items
            .iter()
            .any(|(id, _)| self.request.items.iter().any(|(old, _)| old == id))
        {
            return Err(GenerationError::DuplicateItemId);
        }
        Ok(())
    }
    fn merge_owners(
        &mut self,
        owners: &std::collections::BTreeMap<ItemId, ItemId>,
    ) -> Result<(), GenerationError> {
        for (member, parent) in owners {
            if self
                .request
                .message_owners
                .insert(*member, *parent)
                .is_some()
            {
                return Err(GenerationError::InvalidMessageGroup);
            }
        }
        Ok(())
    }
}
