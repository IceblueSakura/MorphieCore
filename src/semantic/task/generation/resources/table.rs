//! Request-local declarations. A resource's identity and its location are distinct.
use crate::semantic::task::generation::{
    GenerationError, LocalScope, MAX_ITEMS, MAX_TOTAL_BYTES, ResourceDescription, ResourceLocation,
    ResourceView,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResourceId {
    scope: LocalScope,
    value: u64,
}
impl ResourceId {
    pub const fn new(value: u64) -> Self {
        Self::scoped(LocalScope::ROOT, value)
    }
    pub const fn scoped(scope: LocalScope, value: u64) -> Self {
        Self { scope, value }
    }
    pub const fn scope(self) -> LocalScope {
        self.scope
    }
    pub const fn get(self) -> u64 {
        self.value
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceUse {
    pub id: ResourceId,
    pub description: ResourceDescription,
}
impl ResourceUse {
    pub fn resolve<'a>(
        &self,
        table: &'a ResourceTable,
    ) -> Result<&'a ResourceLocation, GenerationError> {
        table.get(self.id).ok_or(GenerationError::InvalidResource)
    }
    pub fn media<'a>(
        &'a self,
        table: &'a ResourceTable,
    ) -> Result<ResourceView<'a>, GenerationError> {
        Ok(ResourceView {
            location: self.resolve(table)?,
            description: &self.description,
        })
    }
}
impl ResourceLocation {
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        let values: &[&str] = match self {
            ResourceLocation::Url(value) => {
                hash.update([0]);
                &[value.as_str()]
            }
            ResourceLocation::Inline {
                media_type,
                data_base64,
            } => {
                hash.update([1]);
                &[media_type.as_str(), data_base64.as_str()]
            }
            ResourceLocation::OpaqueReference(value) => {
                hash.update([2]);
                &[value.as_str()]
            }
            ResourceLocation::NamespacedReference { namespace, id } => {
                hash.update([3]);
                &[namespace.as_str(), id.as_str()]
            }
        };
        for value in values {
            hash.update((value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.finalize().into()
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ResourceTable(BTreeMap<ResourceId, ResourceLocation>);
impl std::fmt::Debug for ResourceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceTable")
            .field("entries", &self.len())
            .finish()
    }
}
impl ResourceTable {
    pub(crate) fn used_by(
        items: &[(
            crate::semantic::task::generation::ItemId,
            crate::semantic::task::generation::Item,
        )],
    ) -> Vec<ResourceId> {
        let mut ids: BTreeSet<_> = Self::content_uses(items).into_iter().collect();
        for (_, item) in items {
            if let crate::semantic::task::generation::Item::Message(message) = item {
                for part in &message.parts {
                    if let crate::semantic::task::generation::ContentPart::Text(text) =
                        &part.content
                    {
                        ids.extend(text.annotations().iter().map(|a| a.value().source));
                    }
                }
            }
        }
        ids.into_iter().collect()
    }
    fn content_uses(
        items: &[(
            crate::semantic::task::generation::ItemId,
            crate::semantic::task::generation::Item,
        )],
    ) -> Vec<ResourceId> {
        use crate::semantic::task::generation::{ContentPart, Item, ToolOutput, ToolResultPart};
        let mut ids = BTreeSet::new();
        for (_, item) in items {
            if let Item::Message(message) = item {
                for part in &message.parts {
                    if let ContentPart::Resource(value) = &part.content {
                        ids.insert(value.id);
                    }
                }
            }
            let output = match item {
                Item::ToolResult(value) | Item::CustomResult(value) => Some(&value.output),
                Item::ProviderTool(value) => value.output.as_ref(),
                _ => None,
            };
            if let Some(ToolOutput::Parts(parts)) = output {
                for (_, part) in parts {
                    if let ToolResultPart::Resource(value) = part {
                        ids.insert(value.id);
                    }
                }
            }
        }
        ids.into_iter().collect()
    }
    pub(crate) fn select_items(
        &self,
        items: &[(
            crate::semantic::task::generation::ItemId,
            crate::semantic::task::generation::Item,
        )],
    ) -> Result<Self, GenerationError> {
        self.select(&Self::used_by(items))
    }
    /// Payloads referenced by items are already charged per use, bounding wire expansion.
    pub(crate) fn uncharged_bytes(
        &self,
        items: &[(
            crate::semantic::task::generation::ItemId,
            crate::semantic::task::generation::Item,
        )],
    ) -> Result<usize, GenerationError> {
        let mut bytes = self.validate()?;
        for id in Self::content_uses(items) {
            let payload = self
                .get(id)
                .ok_or(GenerationError::InvalidResource)?
                .source_view()
                .validate()?;
            bytes = bytes
                .checked_sub(payload)
                .ok_or(GenerationError::InvalidResource)?;
        }
        Ok(bytes)
    }
    pub fn insert(
        mut self,
        id: ResourceId,
        value: ResourceLocation,
    ) -> Result<Self, GenerationError> {
        if self.0.contains_key(&id) {
            return Err(GenerationError::InvalidResource);
        }
        if self.len() == MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        self.0.insert(id, value);
        self.validate()?;
        Ok(self)
    }
    pub fn new(entries: Vec<(ResourceId, ResourceLocation)>) -> Result<Self, GenerationError> {
        if entries.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut table = Self::default();
        for (id, value) in entries {
            if table.0.insert(id, value).is_some() {
                return Err(GenerationError::InvalidResource);
            }
        }
        table.validate()?;
        Ok(table)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, id: ResourceId) -> Option<&ResourceLocation> {
        self.0.get(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (ResourceId, &ResourceLocation)> {
        self.0.iter().map(|(id, value)| (*id, value))
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut bytes = self.len() * std::mem::size_of::<ResourceId>();
        let mut decoded = 0usize;
        for value in self.0.values() {
            let value = value.source_view();
            bytes = bytes
                .checked_add(value.validate()?)
                .ok_or(GenerationError::Limit)?;
            let size = value.inline_decoded_bytes()?.unwrap_or(0);
            decoded = decoded.checked_add(size).ok_or(GenerationError::Limit)?;
            if bytes > MAX_TOTAL_BYTES || decoded > MAX_TOTAL_BYTES {
                return Err(GenerationError::Limit);
            }
        }
        Ok(bytes)
    }
    pub fn merge(mut self, other: &Self) -> Result<Self, GenerationError> {
        for (id, value) in &other.0 {
            if let Some(old) = self.0.get(id) {
                if old != value {
                    return Err(GenerationError::InvalidResource);
                }
            } else {
                if self.len() == MAX_ITEMS {
                    return Err(GenerationError::Limit);
                }
                self.0.insert(*id, value.clone());
            }
        }
        self.validate()?;
        Ok(self)
    }
    pub fn select(&self, ids: &[ResourceId]) -> Result<Self, GenerationError> {
        if ids.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut entries = vec![];
        let mut seen = BTreeSet::new();
        for id in ids {
            let value = self.get(*id).ok_or(GenerationError::InvalidResource)?;
            if seen.insert(*id) {
                entries.push((*id, value.clone()));
            }
        }
        Self::new(entries)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceDependency {
    id: ResourceId,
    fingerprint: [u8; 32],
}
impl ResourceDependency {
    pub fn capture(table: &ResourceTable, id: ResourceId) -> Result<Self, GenerationError> {
        table.validate()?;
        Ok(Self {
            id,
            fingerprint: table
                .get(id)
                .ok_or(GenerationError::InvalidResource)?
                .fingerprint(),
        })
    }
    pub fn check(&self, table: &ResourceTable) -> Result<(), GenerationError> {
        if Self::capture(table, self.id)? != *self {
            return Err(GenerationError::InvalidDependency);
        }
        Ok(())
    }
}
