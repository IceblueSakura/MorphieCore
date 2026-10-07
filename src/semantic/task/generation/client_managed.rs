//! Consuming, bounded selection into one final request, not a session/history store.
use super::*;

#[derive(Clone)]
pub struct ClientManaged {
    request: GenerationRequest,
    selections: Vec<SelectedHistory>,
    changes: Vec<ContextChange>,
    continuations: Vec<ProviderContinuationRequirement>,
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
        Ok(Self::from_parts(std::sync::Arc::new(settings), None))
    }
    pub fn from_configuration(snapshot: ConfigurationSnapshot) -> Result<Self, GenerationError> {
        Ok(Self::from_parts(snapshot.settings, Some(snapshot.revision)))
    }
    fn from_parts(
        settings: std::sync::Arc<GenerationSettings>,
        configuration_revision: Option<ConfigurationId>,
    ) -> Self {
        Self {
            selections: vec![],
            changes: vec![],
            continuations: vec![],
            request: GenerationRequest {
                items: vec![],
                resources: ResourceTable::default(),
                settings,
                configuration_revision,
                replay_groups: vec![],
                call_derivations: Default::default(),
                message_owners: Default::default(),
            },
        }
    }
    pub fn select_request(mut self, source: &GenerationRequest) -> Result<Self, GenerationError> {
        source.validate()?;
        self.reserve_selection(source.items(), source.replay_groups())?;
        self.request.resources = self
            .request
            .resources
            .merge(&source.resources().select_items(source.items())?)?;
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
        self.record_selection(source.items(), None)?;
        Ok(self)
    }
    pub fn select_response(mut self, source: &GenerationResponse) -> Result<Self, GenerationError> {
        self.reserve_selection(source.items(), source.replay_groups())?;
        self.request.resources = self
            .request
            .resources
            .merge(&source.resources().select_items(source.items())?)?;
        self.request.items.extend_from_slice(source.items());
        self.request
            .replay_groups
            .extend_from_slice(source.replay_groups());
        self.merge_owners(source.message_owners())?;
        self.request.validate()?;
        self.record_selection(source.items(), Some(source.progress()))?;
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
        self.finish(dependencies)
            .map(ContextBuild::into_request)
            .map_err(|e| e.error)
    }
    pub fn finish(
        self,
        dependencies: &[RequestDependencyProof],
    ) -> Result<ContextBuild, ContextError> {
        self.request
            .validate()
            .map_err(|e| ContextError::at(ContextStage::Association, None, e))?;
        super::context_transform::check_dependencies(&self.request, dependencies)?;
        for (index, requirement) in self.continuations.iter().enumerate() {
            requirement
                .check(&self.request)
                .map_err(|e| ContextError::at(ContextStage::Continuation, Some(index), e))?;
        }
        Ok(ContextBuild {
            request: self.request,
            changes: self.changes,
            selections: self.selections,
            continuations: self.continuations,
        })
    }
    pub fn require_provider_continuation(
        mut self,
        requirement: ProviderContinuationRequirement,
    ) -> Result<Self, GenerationError> {
        if self.continuations.len() == MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        if self
            .continuations
            .iter()
            .any(|old| old.owner() == requirement.owner())
        {
            return Err(GenerationError::InvalidDependency);
        }
        self.continuations.push(requirement);
        Ok(self)
    }
    fn record_selection(
        &mut self,
        items: &[(ItemId, Item)],
        progress: Option<InteractionProgress>,
    ) -> Result<(), GenerationError> {
        if self.selections.len() == MAX_ITEMS
            || self
                .selections
                .iter()
                .map(|s| s.owners.len())
                .sum::<usize>()
                .saturating_add(items.len())
                > MAX_ITEMS
        {
            return Err(GenerationError::Limit);
        }
        self.selections.push(SelectedHistory {
            owners: items.iter().map(|(id, _)| *id).collect(),
            progress,
        });
        Ok(())
    }
    pub fn select_request_items(
        self,
        source: &GenerationRequest,
        owners: &[ItemId],
    ) -> Result<Self, ContextError> {
        if owners.is_empty() {
            return Ok(self);
        }
        let select = || -> Result<GenerationRequest, GenerationError> {
            let items = super::context_transform::selected(source.items(), owners)?;
            let mut selected = source.clone();
            selected
                .replay_groups
                .retain(|g| g.members().iter().any(|id| owners.contains(id)));
            selected.with_items(items)
        };
        let selected = select().map_err(|e| ContextError::at(ContextStage::Selection, None, e))?;
        self.select_request(&selected)
            .map_err(|e| ContextError::at(ContextStage::Selection, None, e))
    }
    pub fn select_response_items(
        mut self,
        source: &GenerationResponse,
        owners: &[ItemId],
    ) -> Result<Self, ContextError> {
        if owners.is_empty() {
            return Ok(self);
        }
        let selection = |e| ContextError::at(ContextStage::Selection, None, e);
        let items =
            super::context_transform::selected(source.items(), owners).map_err(selection)?;
        let groups: Vec<_> = source
            .replay_groups()
            .iter()
            .filter(|g| g.members().iter().any(|id| owners.contains(id)))
            .cloned()
            .collect();
        self.reserve_selection(&items, &groups).map_err(selection)?;
        self.request.resources = self
            .request
            .resources
            .merge(&source.resources().select_items(&items).map_err(selection)?)
            .map_err(selection)?;
        self.request.items.extend_from_slice(&items);
        self.request.replay_groups.extend(groups);
        let ownership = source
            .message_owners()
            .iter()
            .filter(|(id, _)| owners.contains(id))
            .map(|(id, parent)| (*id, *parent))
            .collect();
        self.merge_owners(&ownership).map_err(selection)?;
        self.request
            .validate()
            .map_err(|e| ContextError::at(ContextStage::Association, None, e))?;
        // Selection never edits the response or its item/operation-scoped usage.
        self.record_selection(&items, Some(source.progress()))
            .map_err(selection)?;
        Ok(self)
    }
    pub fn edit(mut self, edits: Vec<ContextEdit>) -> Result<Self, ContextError> {
        let edited = self.request.transform(edits, &[])?;
        let weight: usize = self
            .changes
            .iter()
            .chain(&edited.changes)
            .map(ContextChange::weight)
            .sum();
        if weight > MAX_ITEMS {
            return Err(ContextError::at(
                ContextStage::Edit,
                None,
                GenerationError::Limit,
            ));
        }
        self.request = edited.request;
        self.changes.extend(edited.changes);
        Ok(self)
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
