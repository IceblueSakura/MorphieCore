//! In-process proofs of explicitly selected history dependencies, never payload stores.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryDependency {
    MessageGroup(ItemId),
    PrefixThrough(ItemId),
    /// A source-declared ordered selection, not inferred group membership.
    Owners(Vec<ItemId>),
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettingsField {
    Instructions,
    Controls,
    Tools,
    ToolChoice,
    ParallelTools,
    Text,
    Reasoning,
    Audio,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsDependency {
    None,
    All,
    Fields(BTreeSet<SettingsField>),
}
impl SettingsDependency {
    pub fn only(field: SettingsField) -> Self {
        Self::Fields(BTreeSet::from([field]))
    }
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::All, _) | (_, Self::All) => Self::All,
            (Self::None, other) | (other, Self::None) => other,
            (Self::Fields(mut left), Self::Fields(mut right)) => {
                left.append(&mut right);
                Self::Fields(left)
            }
        }
    }
    fn includes(&self, field: SettingsField) -> bool {
        match self {
            Self::None => false,
            Self::All => true,
            Self::Fields(fields) => fields.contains(&field),
        }
    }
}
/// Not a serialized identity, issuer signature or replay permission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestDependencyProof {
    scope: HistoryDependency,
    settings: SettingsDependency,
    digest: [u8; 32],
}
impl RequestDependencyProof {
    pub fn capture(
        request: &GenerationRequest,
        scope: HistoryDependency,
        settings: SettingsDependency,
    ) -> Result<Self, GenerationError> {
        let digest = dependency(request, &scope, &settings)?;
        Ok(Self {
            scope,
            settings,
            digest,
        })
    }
    pub fn check(&self, request: &GenerationRequest) -> Result<(), GenerationError> {
        if self.digest != dependency(request, &self.scope, &self.settings)? {
            return Err(GenerationError::InvalidDependency);
        }
        Ok(())
    }
}
// Debug is an in-process encoding, not a persistent format. Stream into the digest;
// schema order stays visible and redacted opaque/resource bytes bind separately.
struct HashWriter {
    hash: Sha256,
    remaining: usize,
}
impl std::fmt::Write for HashWriter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.remaining = self
            .remaining
            .checked_sub(value.len())
            .ok_or(std::fmt::Error)?;
        self.hash.update(value.as_bytes());
        Ok(())
    }
}
fn dependency(
    request: &GenerationRequest,
    scope: &HistoryDependency,
    settings: &SettingsDependency,
) -> Result<[u8; 32], GenerationError> {
    request.validate()?;
    let items: Vec<&(ItemId, Item)> = match scope {
        HistoryDependency::MessageGroup(owner) => request
            .message_groups()
            .find(|g| g.owner() == *owner)
            .ok_or(GenerationError::InvalidDependency)?
            .items()
            .iter()
            .collect(),
        HistoryDependency::PrefixThrough(owner) => {
            let end = request
                .items()
                .iter()
                .position(|(id, _)| id == owner)
                .ok_or(GenerationError::InvalidDependency)?;
            request.items()[..=end].iter().collect()
        }
        HistoryDependency::Owners(owners) => {
            if owners.is_empty() {
                return Err(GenerationError::InvalidDependency);
            }
            if owners.len() > MAX_ITEMS {
                return Err(GenerationError::Limit);
            }
            let mut previous = None;
            let mut selected = Vec::with_capacity(owners.len());
            for owner in owners {
                let index = request
                    .items()
                    .iter()
                    .position(|(id, _)| id == owner)
                    .ok_or(GenerationError::InvalidDependency)?;
                if previous.is_some_and(|old| index <= old) {
                    return Err(GenerationError::InvalidDependency);
                }
                previous = Some(index);
                selected.push(&request.items()[index]);
            }
            selected
        }
    };
    let mut writer = HashWriter {
        hash: Sha256::new(),
        remaining: MAX_TOTAL_BYTES * 8,
    };
    write!(writer, "{scope:?}:{settings:?}:{items:?}").map_err(|_| GenerationError::Limit)?;
    let s = request.settings();
    for field in [
        SettingsField::Instructions,
        SettingsField::Controls,
        SettingsField::Tools,
        SettingsField::ToolChoice,
        SettingsField::ParallelTools,
        SettingsField::Text,
        SettingsField::Reasoning,
        SettingsField::Audio,
    ] {
        if !settings.includes(field) {
            continue;
        }
        let result = match field {
            SettingsField::Instructions => write!(writer, "{field:?}:{:?}", s.instructions),
            SettingsField::Controls => write!(writer, "{field:?}:{:?}", s.controls),
            SettingsField::Tools => write!(writer, "{field:?}:{:?}", s.tools),
            SettingsField::ToolChoice => write!(writer, "{field:?}:{:?}", s.tool_choice),
            SettingsField::ParallelTools => write!(writer, "{field:?}:{:?}", s.parallel_tool_calls),
            SettingsField::Text => write!(writer, "{field:?}:{:?}", s.text),
            SettingsField::Reasoning => write!(writer, "{field:?}:{:?}", s.reasoning),
            SettingsField::Audio => {
                write!(writer, "{field:?}:{:?}:{:?}", s.output_modalities, s.audio)
            }
        };
        result.map_err(|_| GenerationError::Limit)?;
    }
    for (id, item) in items {
        writer.hash.update(id.get().to_le_bytes());
        match item {
            Item::Reasoning(r) => {
                if let Some(value) = &r.replay {
                    writer.hash.update(value.fingerprint());
                }
            }
            Item::Message(m) => {
                for part in &m.parts {
                    match &part.content {
                        ContentPart::Resource(resource) => {
                            resource_dependency(&mut writer.hash, resource)
                        }
                        ContentPart::Audio(audio) => writer.hash.update(audio.fingerprint()),
                        ContentPart::AudioReference(reference) => {
                            writer.hash.update(reference.fingerprint())
                        }
                        _ => {}
                    }
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &result.output {
                    for (_, part) in parts {
                        if let ToolResultPart::Resource(resource) = part {
                            resource_dependency(&mut writer.hash, resource);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(writer.hash.finalize().into())
}
fn resource_dependency(hash: &mut Sha256, resource: &Resource) {
    let values: &[&str] = match &resource.location {
        ResourceLocation::Url(value) | ResourceLocation::OpaqueReference(value) => {
            &[value.as_str()]
        }
        ResourceLocation::Inline {
            media_type,
            data_base64,
        } => &[media_type.as_str(), data_base64.as_str()],
    };
    for value in values {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    // Debug retains filename presence, but its private value must bind separately.
    if let ResourceDescription::File(file) = &resource.description
        && let Some(filename) = &file.filename
    {
        hash.update((filename.as_str().len() as u64).to_le_bytes());
        hash.update(filename.as_str().as_bytes());
    }
}
