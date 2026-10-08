//! In-process proofs of explicitly selected history dependencies, never payload stores.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryDependency {
    MessageGroup(ItemId),
    MessageEnvelope(GroupId),
    ReplayGroup(GroupId),
    PrefixThrough(ItemId),
    /// A source-declared ordered selection, not inferred group membership.
    Owners(Vec<ItemId>),
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettingsField {
    ConfigurationRevision,
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
        HistoryDependency::MessageEnvelope(owner) => {
            let group = request
                .message_envelopes()
                .iter()
                .find(|g| g.id() == *owner)
                .ok_or(GenerationError::InvalidDependency)?;
            group
                .members()
                .iter()
                .map(|id| {
                    request
                        .items()
                        .iter()
                        .find(|(owner, _)| owner == id)
                        .ok_or(GenerationError::InvalidDependency)
                })
                .collect::<Result<_, _>>()?
        }
        HistoryDependency::MessageGroup(owner) => request
            .message_groups()
            .find(|g| g.owner() == *owner)
            .ok_or(GenerationError::InvalidDependency)?
            .items()
            .into_iter()
            .collect(),
        HistoryDependency::ReplayGroup(owner) => request
            .replay_group_views()
            .find(|group| group.declaration().id() == *owner)
            .ok_or(GenerationError::InvalidDependency)?
            .items()
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
    for group in request.message_envelopes() {
        if matches!(scope, HistoryDependency::MessageEnvelope(id) if *id == group.id())
            || group
                .members()
                .iter()
                .any(|id| items.iter().any(|(owner, _)| owner == id))
            || group
                .before()
                .is_some_and(|id| items.iter().any(|(owner, _)| *owner == id))
        {
            write!(writer, "{group:?}").map_err(|_| GenerationError::Limit)?;
        }
    }
    if let HistoryDependency::ReplayGroup(owner) = scope {
        let group = request
            .replay_groups()
            .iter()
            .find(|group| group.id() == *owner)
            .ok_or(GenerationError::InvalidDependency)?;
        write!(writer, "{group:?}").map_err(|_| GenerationError::Limit)?;
    }
    for (owner, _) in &items {
        write!(writer, "{:?}", request.call_derivations().get(owner))
            .map_err(|_| GenerationError::Limit)?;
        write!(writer, "{:?}", request.message_owners().get(owner))
            .map_err(|_| GenerationError::Limit)?;
    }
    let s = request.settings();
    let pinned_configuration = request.configuration_revision().is_some()
        && settings.includes(SettingsField::ConfigurationRevision);
    for field in [
        SettingsField::ConfigurationRevision,
        SettingsField::Instructions,
        SettingsField::Controls,
        SettingsField::Tools,
        SettingsField::ToolChoice,
        SettingsField::ParallelTools,
        SettingsField::Text,
        SettingsField::Reasoning,
        SettingsField::Audio,
    ] {
        if !settings.includes(field)
            || pinned_configuration && field != SettingsField::ConfigurationRevision
        {
            continue;
        }
        let result = match field {
            SettingsField::ConfigurationRevision => {
                // A local ID cannot vouch for independently reconstructed settings.
                // Pin the immutable contents once; avoid hashing all fields twice.
                write!(
                    writer,
                    "{field:?}:{:?}:{:?}",
                    request.configuration_revision(),
                    request.configuration_revision().map(|_| s)
                )
            }
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
        // Public Debug redacts bindings. Hash the original immutable authority,
        // not the redacted view or a lookup in the current tool definitions.
        if let Some(binding) = super::configuration::item_binding(item) {
            write!(
                writer,
                "{:?}:{:?}:{:?}",
                binding.snapshot().revision(),
                binding.reference(),
                binding.snapshot().settings()
            )
            .map_err(|_| GenerationError::Limit)?;
        }
        match item {
            Item::ProviderTool(observed) => {
                if let Some(value) = &observed.replay {
                    writer.hash.update(value.fingerprint());
                }
                if let ProviderOperation::Reported {
                    action: Some(action),
                    ..
                } = &observed.operation
                {
                    action
                        .write_dependency(&mut writer)
                        .map_err(|_| GenerationError::Limit)?;
                }
                write!(
                    writer,
                    "{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
                    observed.source,
                    observed.operation,
                    observed.progress,
                    observed.execution,
                    observed.output,
                    observed.artifact_status
                )
                .map_err(|_| GenerationError::Limit)?;
                if let Some(ToolOutput::Parts(parts)) = &observed.output {
                    for (_, part) in parts {
                        if let ToolResultPart::Resource(resource) = part {
                            writer
                                .hash
                                .update(resource.fingerprint(request.resources())?);
                        }
                    }
                }
            }
            Item::Reasoning(r) => {
                if let Some(value) = &r.replay {
                    writer.hash.update(value.fingerprint());
                }
            }
            Item::Message(m) => {
                for part in &m.parts {
                    if let Some(value) = &part.replay {
                        writer.hash.update(value.fingerprint());
                    }
                    match &part.content {
                        ContentPart::Text(t) => {
                            for a in t.annotations() {
                                writer.hash.update(a.fingerprint());
                            }
                        }
                        ContentPart::Resource(resource) => writer
                            .hash
                            .update(resource.fingerprint(request.resources())?),
                        ContentPart::Audio(audio) => writer.hash.update(audio.fingerprint()),
                        ContentPart::AudioReference(reference) => {
                            writer.hash.update(reference.fingerprint())
                        }
                        _ => {}
                    }
                }
            }
            Item::ToolCall(call) => {
                if let Some(value) = &call.context.replay {
                    writer.hash.update(value.fingerprint());
                }
            }
            Item::ToolResult(result) | Item::CustomResult(result) => {
                if let ToolOutput::Parts(parts) = &result.output {
                    for (_, part) in parts {
                        if let ToolResultPart::Resource(resource) = part {
                            writer
                                .hash
                                .update(resource.fingerprint(request.resources())?);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(writer.hash.finalize().into())
}
pub(super) fn part_dependency(
    request: &GenerationRequest,
    item: ItemId,
    part: PartId,
) -> Result<[u8; 32], GenerationError> {
    let Some((_, Item::Message(message))) = request.items().iter().find(|(id, _)| *id == item)
    else {
        return Err(GenerationError::InvalidReplay);
    };
    let part = message
        .parts
        .iter()
        .find(|p| p.id == part)
        .ok_or(GenerationError::InvalidReplay)?;
    let mut writer = HashWriter {
        hash: Sha256::new(),
        remaining: MAX_TOTAL_BYTES * 8,
    };
    write!(
        writer,
        "{item:?}:{:?}:{:?}:{:?}:{part:?}",
        message.role, message.phase, message.status
    )
    .map_err(|_| GenerationError::Limit)?;
    if let Some(value) = &part.replay {
        writer.hash.update(value.fingerprint());
    }
    if let ContentPart::Text(t) = &part.content {
        for a in t.annotations() {
            writer.hash.update(a.fingerprint());
        }
    }
    if let ContentPart::Resource(resource) = &part.content {
        writer
            .hash
            .update(resource.fingerprint(request.resources())?);
    }
    Ok(writer.hash.finalize().into())
}
pub(super) fn location_dependency(hash: &mut Sha256, location: &ResourceLocation) {
    let values: &[&str] = match location {
        ResourceLocation::Url(value) | ResourceLocation::OpaqueReference(value) => {
            &[value.as_str()]
        }
        ResourceLocation::NamespacedReference { namespace, id } => {
            &[namespace.as_str(), id.as_str()]
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
}
