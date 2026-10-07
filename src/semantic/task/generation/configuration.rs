//! Immutable caller-declared revisions; neither labels nor bindings grant execution.
use super::*;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConfigurationId {
    scope: LocalScope,
    value: u64,
}
impl ConfigurationId {
    pub const fn new(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}

/// A complete immutable configuration, shared by its historical references.
/// Revisions are unique only within caller-supplied scopes, not globally allocated.
#[derive(Clone)]
pub struct ConfigurationSnapshot {
    pub(super) revision: ConfigurationId,
    pub(super) settings: Arc<GenerationSettings>,
}
impl std::fmt::Debug for ConfigurationSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConfigurationSnapshot([redacted])")
    }
}
impl PartialEq for ConfigurationSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision && same_settings(&self.settings, &other.settings)
    }
}
impl Eq for ConfigurationSnapshot {}
impl ConfigurationSnapshot {
    pub fn new(
        revision: ConfigurationId,
        settings: GenerationSettings,
    ) -> Result<Self, GenerationError> {
        if settings
            .validate()?
            .saturating_add(std::mem::size_of::<ConfigurationId>())
            > MAX_TOTAL_BYTES
        {
            return Err(GenerationError::Limit);
        }
        Ok(Self {
            revision,
            settings: Arc::new(settings),
        })
    }
    pub const fn revision(&self) -> ConfigurationId {
        self.revision
    }
    pub fn settings(&self) -> &GenerationSettings {
        &self.settings
    }
    pub(super) fn check_revision(
        &self,
        revision: ConfigurationId,
        settings: &GenerationSettings,
    ) -> Result<(), GenerationError> {
        if self.revision == revision && !same_settings(&self.settings, settings) {
            return Err(GenerationError::ConfigurationRevisionConflict);
        }
        Ok(())
    }
}

/// The only definition association on a call. The definition body is borrowed
/// from the immutable original snapshot, never copied or resolved in current tools.
#[derive(Clone, Eq, PartialEq)]
pub struct ToolDefinitionBinding {
    snapshot: ConfigurationSnapshot,
    reference: ToolReference,
}
impl std::fmt::Debug for ToolDefinitionBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToolDefinitionBinding([redacted])")
    }
}
impl ToolDefinitionBinding {
    pub fn new(
        snapshot: ConfigurationSnapshot,
        reference: ToolReference,
    ) -> Result<Self, GenerationError> {
        let binding = Self {
            snapshot,
            reference,
        };
        if binding.resolve().is_none() {
            return Err(GenerationError::InvalidToolBinding);
        }
        Ok(binding)
    }
    pub fn snapshot(&self) -> &ConfigurationSnapshot {
        &self.snapshot
    }
    pub fn reference(&self) -> &ToolReference {
        &self.reference
    }
    pub fn definition(&self) -> &ToolDefinition {
        self.resolve()
            .expect("immutable validated definition binding")
    }
    fn resolve(&self) -> Option<&ToolDefinition> {
        let tools = self.snapshot.settings.tools.as_deref().unwrap_or_default();
        let leaves = if let Some(namespace) = &self.reference.namespace {
            tools.iter().find_map(|tool| match tool {
                ToolDefinition::Namespace(group) if &group.name == namespace => {
                    Some(group.tools.as_slice())
                }
                _ => None,
            })?
        } else {
            tools
        };
        leaves.iter().find(|tool| {
            tool.kind() == Some(self.reference.kind) && tool.name() == &self.reference.name
        })
    }
    pub(super) fn check_call(
        &self,
        kind: ToolKind,
        name: &crate::semantic::value::Text,
        namespace: Option<&crate::semantic::value::Text>,
    ) -> Result<(), GenerationError> {
        if self.reference.kind != kind
            || &self.reference.name != name
            || self.reference.namespace.as_ref() != namespace
        {
            return Err(GenerationError::InvalidToolBinding);
        }
        Ok(())
    }
    /// Conservatively charge each retained reference, even if allocation is shared.
    pub(super) fn bytes(&self) -> Result<usize, GenerationError> {
        Ok(self.snapshot.settings.validate()?
            + std::mem::size_of::<ConfigurationId>()
            + self.reference.name.as_str().len()
            + self
                .reference
                .namespace
                .as_ref()
                .map_or(0, |s| s.as_str().len()))
    }
}

pub(super) fn item_binding(item: &Item) -> Option<&ToolDefinitionBinding> {
    match item {
        Item::ToolCall(call) => call.context.definition.as_ref(),
        Item::CustomCall(call) => call.context.definition.as_ref(),
        _ => None,
    }
}

pub(super) fn validate_bindings(items: &[(ItemId, Item)]) -> Result<usize, GenerationError> {
    let mut revisions: BTreeMap<ConfigurationId, &ConfigurationSnapshot> = BTreeMap::new();
    let mut bytes = 0usize;
    for (_, item) in items {
        let (context, kind, name) = match item {
            Item::ToolCall(call) => (&call.context, ToolKind::Function, &call.name),
            Item::CustomCall(call) => (&call.context, ToolKind::Custom, &call.name),
            Item::ToolResult(result) | Item::CustomResult(result)
                if result.context.definition.is_some() =>
            {
                return Err(GenerationError::InvalidToolBinding);
            }
            _ => continue,
        };
        if let Some(binding) = &context.definition {
            binding.check_call(kind, name, context.namespace.as_ref())?;
            let snapshot = binding.snapshot();
            if let Some(old) = revisions.insert(snapshot.revision(), snapshot) {
                snapshot.check_revision(old.revision(), old.settings())?;
            }
            bytes = bytes.saturating_add(binding.bytes()?);
            if bytes > MAX_TOTAL_BYTES {
                return Err(GenerationError::Limit);
            }
        }
    }
    Ok(bytes)
}

/// Serde Value equality ignores object order; configuration identity must not.
/// Values have passed their independent depth/node limits before this comparison.
pub(super) fn same_settings(a: &GenerationSettings, b: &GenerationSettings) -> bool {
    fn schema(a: Option<&SchemaDocument>, b: Option<&SchemaDocument>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => a.same_authority(b),
            (None, None) => true,
            _ => false,
        }
    }
    if a != b {
        return false;
    }
    if let (
        OutputConstraint::JsonSchema { schema: a, .. },
        OutputConstraint::JsonSchema { schema: b, .. },
    ) = (a.output(), b.output())
        && !a.same_authority(b)
    {
        return false;
    }
    a.tools
        .as_deref()
        .unwrap_or_default()
        .iter()
        .flat_map(ToolDefinition::leaves)
        .zip(
            b.tools
                .as_deref()
                .unwrap_or_default()
                .iter()
                .flat_map(ToolDefinition::leaves),
        )
        .all(|(a, b)| match (a, b) {
            (ToolDefinition::Function(a), ToolDefinition::Function(b)) => {
                schema(a.parameters.as_ref(), b.parameters.as_ref())
                    && schema(a.output_schema.as_ref(), b.output_schema.as_ref())
            }
            _ => true,
        })
}
