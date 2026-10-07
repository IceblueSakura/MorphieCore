//! Request-local declarations. Identity, location and declared access conditions are distinct.
use super::{
    GenerationError, LocalScope, MAX_ITEMS, MAX_TEXT_BYTES, MAX_TOTAL_BYTES, ResourceDescription,
    ResourceLocation, ResourceView,
};
use crate::semantic::value::Text;
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourcePurpose {
    Input,
    ToolResult,
    Reasoning,
    Output,
    Reference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceUse {
    pub id: ResourceId,
    pub purpose: ResourcePurpose,
    pub description: ResourceDescription,
}
impl ResourceUse {
    pub fn resolve<'a>(
        &self,
        table: &'a ResourceTable,
    ) -> Result<&'a ResourceDeclaration, GenerationError> {
        table.get(self.id).ok_or(GenerationError::InvalidResource)
    }
    pub fn media<'a>(
        &'a self,
        table: &'a ResourceTable,
    ) -> Result<ResourceView<'a>, GenerationError> {
        match &self.resolve(table)?.body {
            ResourceBody::Media(value) => Ok(ResourceView {
                location: value,
                description: &self.description,
            }),
            ResourceBody::Text(_) => Err(GenerationError::InvalidResource),
        }
    }
    pub(crate) fn fingerprint(&self, table: &ResourceTable) -> Result<[u8; 32], GenerationError> {
        let mut hash = Sha256::new();
        hash.update(self.resolve(table)?.fingerprint());
        hash.update(format!("{self:?}").as_bytes());
        if let ResourceDescription::File(file) = &self.description
            && let Some(name) = &file.filename
        {
            hash.update((name.as_str().len() as u64).to_le_bytes());
            hash.update(name.as_str().as_bytes());
        }
        Ok(hash.finalize().into())
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ResourceConditions {
    /// Non-secret declared compatibility requirement, not a credential or issuer attestation.
    pub scope: Option<Text>,
    /// Opaque non-secret requirement name; pure validation does not resolve permissions.
    pub access: Option<Text>,
    /// Exclusive Unix-second expiry; checking it requires an explicit caller clock.
    pub expires_at: Option<u64>,
    /// None is unreported; an empty list explicitly allows no uses.
    pub purposes: Option<Vec<ResourcePurpose>>,
}
impl std::fmt::Debug for ResourceConditions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResourceConditions([redacted])")
    }
}
impl ResourceConditions {
    fn bytes(&self) -> Result<usize, GenerationError> {
        let mut bytes = std::mem::size_of::<Self>();
        for value in [&self.scope, &self.access].into_iter().flatten() {
            if value.as_str().is_empty()
                || value.as_str().len() > 256
                || value.as_str().chars().any(char::is_control)
            {
                return Err(GenerationError::InvalidResource);
            }
            bytes += value.as_str().len();
        }
        if let Some(purposes) = &self.purposes {
            if purposes.len() > 5 {
                return Err(GenerationError::Limit);
            }
            if purposes.iter().collect::<BTreeSet<_>>().len() != purposes.len() {
                return Err(GenerationError::InvalidResource);
            }
            bytes += purposes.len() * std::mem::size_of::<ResourcePurpose>();
        }
        Ok(bytes)
    }
}
#[derive(Clone, Eq, PartialEq)]
pub enum ResourceBody {
    Text(Text),
    Media(ResourceLocation),
}
impl std::fmt::Debug for ResourceBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResourceBody([redacted])")
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceDeclaration {
    pub body: ResourceBody,
    pub conditions: ResourceConditions,
}
/// Values explicitly supplied by a trusted caller, not a permission grant.
#[derive(Clone, Copy, Default)]
pub struct ResourceTarget<'a> {
    pub scope: Option<&'a str>,
    pub access: Option<&'a str>,
    pub now: Option<u64>,
}
impl ResourceDeclaration {
    pub fn validate(&self) -> Result<usize, GenerationError> {
        let bytes = match &self.body {
            ResourceBody::Text(text) => {
                if text.as_str().len() > MAX_TEXT_BYTES {
                    return Err(GenerationError::Limit);
                }
                text.as_str().len()
            }
            ResourceBody::Media(resource) => resource.source_view().validate()?,
        };
        bytes
            .checked_add(self.conditions.bytes()?)
            .ok_or(GenerationError::Limit)
    }
    pub fn check_target(
        &self,
        purpose: ResourcePurpose,
        target: ResourceTarget<'_>,
    ) -> Result<(), GenerationError> {
        self.validate()?;
        let conditions = &self.conditions;
        if conditions
            .scope
            .as_ref()
            .is_some_and(|v| Some(v.as_str()) != target.scope)
            || conditions
                .access
                .as_ref()
                .is_some_and(|v| Some(v.as_str()) != target.access)
            || conditions
                .expires_at
                .is_some_and(|expires| target.now.is_none_or(|now| now >= expires))
            || conditions
                .purposes
                .as_ref()
                .is_some_and(|values| !values.contains(&purpose))
            || matches!(
                &self.body,
                ResourceBody::Media(
                    ResourceLocation::OpaqueReference(_)
                        | ResourceLocation::NamespacedReference { .. }
                )
            ) && conditions.scope.is_none()
                && purpose != ResourcePurpose::Reference
        {
            return Err(GenerationError::InvalidResource);
        }
        Ok(())
    }
    pub(crate) fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        fn field(hash: &mut Sha256, value: Option<&str>) {
            hash.update([u8::from(value.is_some())]);
            if let Some(value) = value {
                hash.update((value.len() as u64).to_le_bytes());
                hash.update(value.as_bytes());
            }
        }
        match &self.body {
            ResourceBody::Text(value) => {
                hash.update([0]);
                field(&mut hash, Some(value.as_str()));
            }
            ResourceBody::Media(resource) => {
                hash.update([1]);
                hash.update(format!("{resource:?}").as_bytes());
                super::dependency::location_dependency(&mut hash, resource);
            }
        }
        field(&mut hash, self.conditions.scope.as_ref().map(Text::as_str));
        field(&mut hash, self.conditions.access.as_ref().map(Text::as_str));
        hash.update(
            format!(
                "{:?}:{:?}",
                self.conditions.expires_at, self.conditions.purposes
            )
            .as_bytes(),
        );
        hash.finalize().into()
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ResourceTable(BTreeMap<ResourceId, ResourceDeclaration>);
impl std::fmt::Debug for ResourceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceTable")
            .field("entries", &self.len())
            .finish()
    }
}
impl ResourceTable {
    pub(crate) fn used_by(items: &[(super::ItemId, super::Item)]) -> Vec<ResourceId> {
        let mut ids: BTreeSet<_> = Self::content_uses(items).into_iter().collect();
        for (_, item) in items {
            if let super::Item::Message(message) = item {
                for part in &message.parts {
                    if let super::ContentPart::Text(text) = &part.content {
                        ids.extend(text.annotations().iter().map(|a| a.value().source));
                    }
                }
            }
        }
        ids.into_iter().collect()
    }
    fn content_uses(items: &[(super::ItemId, super::Item)]) -> Vec<ResourceId> {
        use super::{ContentPart, Item, ToolOutput, ToolResultPart};
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
        items: &[(super::ItemId, super::Item)],
    ) -> Result<Self, GenerationError> {
        self.select(&Self::used_by(items))
    }
    /// Payloads referenced by items are already charged per use, bounding wire expansion.
    pub(crate) fn uncharged_bytes(
        &self,
        items: &[(super::ItemId, super::Item)],
    ) -> Result<usize, GenerationError> {
        let mut bytes = self.validate()?;
        for id in Self::content_uses(items) {
            let value = self.get(id).ok_or(GenerationError::InvalidResource)?;
            let payload = match &value.body {
                ResourceBody::Media(value) => value.source_view().validate()?,
                ResourceBody::Text(value) => value.as_str().len(),
            };
            bytes = bytes
                .checked_sub(payload)
                .ok_or(GenerationError::InvalidResource)?;
        }
        Ok(bytes)
    }
    pub fn insert(
        mut self,
        id: ResourceId,
        value: ResourceDeclaration,
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
    pub fn new(entries: Vec<(ResourceId, ResourceDeclaration)>) -> Result<Self, GenerationError> {
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
    pub fn get(&self, id: ResourceId) -> Option<&ResourceDeclaration> {
        self.0.get(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (ResourceId, &ResourceDeclaration)> {
        self.0.iter().map(|(id, value)| (*id, value))
    }
    pub fn validate(&self) -> Result<usize, GenerationError> {
        if self.len() > MAX_ITEMS {
            return Err(GenerationError::Limit);
        }
        let mut bytes = self.len() * std::mem::size_of::<ResourceId>();
        let mut decoded = 0usize;
        for value in self.0.values() {
            bytes = bytes
                .checked_add(value.validate()?)
                .ok_or(GenerationError::Limit)?;
            if let ResourceBody::Media(resource) = &value.body {
                let size = resource.source_view().inline_decoded_bytes()?.unwrap_or(0);
                decoded = decoded.checked_add(size).ok_or(GenerationError::Limit)?;
            }
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
