//! Validated Generation request construction and owned history transforms.
use crate::semantic::task::generation::*;
use crate::semantic::value::{Presence, Text};
mod client_managed;
pub use client_managed::ClientManaged;
pub(super) mod context_transform;
pub use context_transform::{
    ContextBuild, ContextChange, ContextEdit, ContextError, ContextStage, SelectedHistory,
};
mod settings;
pub use settings::{GenerationControls, GenerationSettings, Truncation};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationRequest {
    items: Vec<(ItemId, Item)>,
    resources: ResourceTable,
    settings: std::sync::Arc<GenerationSettings>,
    message_owners: crate::semantic::task::generation::history::envelope::MessageEnvelopes,
}
impl GenerationRequest {
    pub fn new(
        items: Vec<(ItemId, Item)>,
        controls: GenerationControls,
    ) -> Result<Self, GenerationError> {
        Self::from_settings(
            items,
            GenerationSettings {
                controls,
                ..Default::default()
            },
        )
    }
    pub fn from_settings(
        items: Vec<(ItemId, Item)>,
        settings: GenerationSettings,
    ) -> Result<Self, GenerationError> {
        Self::from_resources(items, settings, ResourceTable::default())
    }
    pub fn from_resources(
        items: Vec<(ItemId, Item)>,
        settings: GenerationSettings,
        resources: ResourceTable,
    ) -> Result<Self, GenerationError> {
        let r = Self {
            items,
            resources,
            settings: std::sync::Arc::new(settings),
            message_owners: Default::default(),
        };
        r.validate()?;
        Ok(r)
    }
    pub fn validate(&self) -> Result<(), GenerationError> {
        let settings = self.settings.validate()?;
        let items = if self.items.is_empty() && self.settings.instructions.value().is_some() {
            0
        } else {
            crate::semantic::task::generation::validate::items(&self.items, false, &self.resources)?
        };
        let resources = self.resources.uncharged_bytes(&self.items)?;
        let message_owners = self.message_owners.validate(&self.items)?;
        if items
            .saturating_add(settings)
            .saturating_add(resources)
            .saturating_add(message_owners)
            > MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        Ok(())
    }
    pub fn items(&self) -> &[(ItemId, Item)] {
        &self.items
    }
    pub fn resources(&self) -> &ResourceTable {
        &self.resources
    }
    pub fn with_resources(mut self, resources: ResourceTable) -> Result<Self, GenerationError> {
        self.resources = resources;
        self.validate()?;
        Ok(self)
    }
    pub fn settings(&self) -> &GenerationSettings {
        &self.settings
    }
    pub fn message_owners(&self) -> &std::collections::BTreeMap<ItemId, ItemId> {
        self.message_owners.chat()
    }
    pub fn message_envelopes(&self) -> &[MessageEnvelope] {
        self.message_owners.values()
    }
    pub fn with_message_envelopes(
        mut self,
        groups: Vec<MessageEnvelope>,
    ) -> Result<Self, GenerationError> {
        self.message_owners =
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::new(
                &self.items,
                groups,
            )?;
        self.validate()?;
        Ok(self)
    }
    pub fn with_message_owners(
        mut self,
        owners: Vec<(ItemId, ItemId)>,
    ) -> Result<Self, GenerationError> {
        self.message_owners =
            crate::semantic::task::generation::history::envelope::MessageEnvelopes::from_chat(
                &self.items,
                owners,
            )?;
        self.validate()?;
        Ok(self)
    }
    pub fn controls(&self) -> &GenerationControls {
        &self.settings.controls
    }
    pub fn tools(&self) -> &[ToolDefinition] {
        self.settings.tools.as_deref().unwrap_or_default()
    }
    pub fn tools_present(&self) -> bool {
        self.settings.tools.is_some()
    }
    pub fn tool_choice(&self) -> Option<&ToolChoice> {
        self.settings.tool_choice.as_ref()
    }
    pub fn parallel_tool_calls(&self) -> Option<bool> {
        self.settings.parallel_tool_calls
    }
    pub fn output(&self) -> &OutputConstraint {
        self.settings.output()
    }
    pub fn text_options(&self) -> &TextOptions {
        &self.settings.text
    }
    pub fn instructions(&self) -> &Presence<Text> {
        &self.settings.instructions
    }
    pub fn reasoning(&self) -> &ReasoningRequest {
        &self.settings.reasoning
    }
    pub fn with_settings(mut self, settings: GenerationSettings) -> Result<Self, GenerationError> {
        settings.validate()?;
        self.settings = std::sync::Arc::new(settings);
        self.validate()?;
        Ok(self)
    }
    pub fn with_tool_settings(
        self,
        tools: Option<Vec<ToolDefinition>>,
        choice: Option<ToolChoice>,
        parallel: Option<bool>,
    ) -> Result<Self, GenerationError> {
        let mut settings = self.settings().clone();
        settings.tools = tools;
        settings.tool_choice = choice;
        settings.parallel_tool_calls = parallel;
        self.with_settings(settings)
    }
    pub fn with_tools(
        self,
        tools: Vec<ToolDefinition>,
        choice: ToolChoice,
    ) -> Result<Self, GenerationError> {
        let parallel = self.parallel_tool_calls();
        self.with_tool_settings(Some(tools), Some(choice), parallel)
    }
    pub fn with_output(self, output: OutputConstraint) -> Result<Self, GenerationError> {
        let mut settings = self.settings().clone();
        settings.text.presence = true;
        settings.text.format = Presence::Value(output);
        self.with_settings(settings)
    }
    pub fn with_reasoning(self, reasoning: ReasoningRequest) -> Result<Self, GenerationError> {
        let mut settings = self.settings().clone();
        settings.reasoning = reasoning;
        self.with_settings(settings)
    }
    pub fn with_items(mut self, items: Vec<(ItemId, Item)>) -> Result<Self, GenerationError> {
        if items.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        self.message_owners = self.message_owners.edited(&items)?;
        let source = std::mem::replace(&mut self.items, items);
        self.validate()?;
        crate::semantic::task::generation::identity::check_call_edits(&source, &self.items)?;
        context_transform::check_protected_edits(&source, &self.items)?;
        Ok(self)
    }
    /// Candidate copy: listed reasoning becomes assistant text under a fresh derived ID,
    /// since an item's kind is protected under its ID; envelope membership follows the
    /// new ID. Listed reasoning without non-blank text is removed.
    pub fn with_reasoning_as_text(mut self, owners: &[ItemId]) -> Result<Self, GenerationError> {
        let mut items = Vec::with_capacity(self.items.len());
        for (position, (id, item)) in self.items.iter().enumerate() {
            let reasoning = match item {
                Item::Reasoning(reasoning) if owners.contains(id) => reasoning,
                _ => {
                    items.push((*id, item.clone()));
                    continue;
                }
            };
            let parts: Vec<_> = reasoning
                .parts
                .iter()
                .filter_map(|(part, content)| {
                    let (ReasoningContent::Summary(text) | ReasoningContent::Text(text)) = content;
                    (!text.as_str().trim().is_empty()).then(|| Part {
                        id: *part,
                        content: ContentPart::Text(text.clone().into()),
                    })
                })
                .collect();
            if parts.is_empty() {
                continue;
            }
            let fresh = ItemId::scoped(LocalScope::DERIVED, position as u64);
            self.message_owners.revise(*id, fresh);
            items.push((
                fresh,
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    parts,
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            ));
        }
        self.with_items(items)
    }
    pub fn retain_items(
        self,
        mut keep: impl FnMut(ItemId, &Item) -> bool,
    ) -> Result<Self, GenerationError> {
        let items = self
            .items
            .iter()
            .filter(|(id, i)| keep(*id, i))
            .cloned()
            .collect();
        self.with_items(items)
    }
}
