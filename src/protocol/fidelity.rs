//! Bounded representation records. Only surviving semantic owners may reuse them.
mod replay;
mod response_records;
use super::CodecError;
use crate::semantic::{
    task::generation::*,
    value::{ReplayOrigin, Text},
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::protocol::openai::{Profile, dialect::adaptation::Adaptation};
const RESPONSE_EXTRAS_BUDGET: usize = 4096;
/// Outgoing carrier identity, not an upstream-reported fact or persistent key.
pub(crate) fn generated_item_id(owner: ItemId) -> String {
    if owner.scope() == LocalScope::ROOT {
        format!("item_{}", owner.get())
    } else {
        format!("item_{}_{}", owner.scope().get(), owner.get())
    }
}
pub(crate) const RESPONSE_EXTRA_FIELDS: &[&str] =
    &["content_filters", "frequency_penalty", "presence_penalty"];
pub(crate) fn declared_response_extras(
    profile: Profile,
    adaptation: &Adaptation,
) -> &'static [&'static str] {
    if profile == Profile::Responses && adaptation.rules.response_extras {
        RESPONSE_EXTRA_FIELDS
    } else {
        &[]
    }
}

/// Intake audit facts only; never used to restore or change semantic values.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Normalization {
    /// Client assistant-history shorthand, never an upstream annotation report.
    HistoryAnnotationsDefault,
    CacheWriteDefault {
        profile: &'static str,
    },
    ReasoningEffortMapping {
        rule: &'static str,
        input: &'static str,
        output: &'static str,
    },
    /// A selected rule dropped a client control or carrier at intake.
    Omitted {
        rule: &'static str,
        field: &'static str,
    },
}
pub(crate) const PREDICTION_OMISSION: Normalization = Normalization::Omitted {
    rule: "chat.omit-prediction.v1",
    field: "generation.prediction",
};
/// A returned replay manifest no longer matched its visible history.
pub(crate) const REPLAY_MANIFEST_OMISSION: Normalization = Normalization::Omitted {
    rule: "generation.omit-unmatched-replay-manifest.v1",
    field: "generation.replay.attachments",
};
/// Messages reports of upstream input changes; their paths index the upstream request.
pub(crate) const INPUT_TRANSFORMATIONS_OMISSION: Normalization = Normalization::Omitted {
    rule: "messages.omit-input-transformations.v1",
    field: "response.input_transformations",
};
#[derive(Clone, Debug, Eq, PartialEq)]
struct ResponseExtras {
    protocol: Profile,
    profile: &'static str,
    origin: ReplayOrigin,
    response_id: Text,
    dependency: [u8; 32],
    values: BTreeMap<String, Value>,
}

/// A bounded proof of intake association, not a token store or issuer verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayOwnerKind {
    Reasoning,
    Compaction,
    Call,
    Part,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct ReplayBinding {
    kind: ReplayOwnerKind,
    origin: Option<ReplayOrigin>,
    dependency: [u8; 32],
    final_value: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AudioBinding {
    origin: Option<ReplayOrigin>,
    reference: [u8; 32],
    body: Option<[u8; 32]>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct CachedContextBinding {
    reference: CachedContextReference,
    origin: ReplayOrigin,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct CreatorBinding {
    actor: Text,
    dependency: [u8; 32],
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FidelityRecords {
    response_item_ids: BTreeMap<ItemId, Text>,
    reported_creators: BTreeMap<ItemId, CreatorBinding>,
    cache_breakpoints: std::collections::BTreeSet<PartId>,
    /// Messages images that must fail rather than be downsized when oversized.
    strict_image_sizes: std::collections::BTreeSet<PartId>,
    input_text_forms: std::collections::BTreeSet<PartId>,
    item_replay: BTreeMap<ItemId, ReplayBinding>,
    part_replay: BTreeMap<(ItemId, PartId), ReplayBinding>,
    audio: BTreeMap<PartId, AudioBinding>,
    cached_context: Option<CachedContextBinding>,
    response_extras: Option<ResponseExtras>,
    routing_extras: Option<ResponseExtras>,
    normalizations: std::collections::BTreeSet<Normalization>,
}
impl FidelityRecords {
    /// Conservative retained-context accounting for the optional delivery buffer.
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .reported_creators
                .values()
                .map(|report| 512 + report.actor.as_str().len())
                .sum::<usize>()
            + self
                .response_item_ids
                .values()
                .map(|id| 1024 + id.as_str().len())
                .sum::<usize>()
            + (self.cache_breakpoints.len()
                + self.strict_image_sizes.len()
                + self.input_text_forms.len())
                * 512
            + (self.item_replay.len() + self.part_replay.len() + self.audio.len()) * 2048
            + self
                .cached_context
                .as_ref()
                .map_or(0, |binding| binding.reference.bytes() + 512)
            + self.normalizations.len() * 1024
            + [self.response_extras.as_ref(), self.routing_extras.as_ref()]
                .into_iter()
                .flatten()
                .count()
                * (RESPONSE_EXTRAS_BUDGET * 128 + 1024)
    }
    /// Compatibility association only, not resource authorization or target selection.
    pub fn record_cached_context(
        &mut self,
        reference: &CachedContextReference,
        origin: ReplayOrigin,
    ) -> Result<(), CodecError> {
        let binding = CachedContextBinding {
            reference: reference.clone(),
            origin,
        };
        if self
            .cached_context
            .as_ref()
            .is_some_and(|old| old != &binding)
        {
            return Err(CodecError::Invalid("cached context binding"));
        }
        self.cached_context = Some(binding);
        Ok(())
    }
    pub(crate) fn check_cached_context(
        &self,
        reference: &CachedContextReference,
        origin: &ReplayOrigin,
        now: Option<std::time::SystemTime>,
    ) -> Result<(), CodecError> {
        let binding = self
            .cached_context
            .as_ref()
            .ok_or(CodecError::Invalid("cached context binding"))?;
        if &binding.reference != reference || &binding.origin != origin {
            return Err(CodecError::Invalid("cached context binding"));
        }
        if reference.expires_at().is_some() {
            reference.check_at(now.ok_or(CodecError::Invalid("cached context clock"))?)?;
        }
        Ok(())
    }
    /// Trusted intake association, never issuer authentication or a client-supplied scope.
    pub fn record_audio(
        &mut self,
        owner: PartId,
        content: &ContentPart,
        origin: Option<ReplayOrigin>,
    ) -> Result<(), CodecError> {
        let (reference, body) = match content {
            ContentPart::Audio(v) => (v.reference(), Some(v.fingerprint())),
            ContentPart::AudioReference(v) => (v, None),
            _ => return Err(CodecError::Invalid("audio owner")),
        };
        if self.audio.len() >= MAX_ITEMS || self.audio.contains_key(&owner) {
            return Err(CodecError::Invalid("audio binding"));
        }
        self.audio.insert(
            owner,
            AudioBinding {
                origin,
                reference: reference.fingerprint(),
                body,
            },
        );
        Ok(())
    }
    pub fn check_audio(
        &self,
        owner: PartId,
        content: &ContentPart,
        origin: Option<&ReplayOrigin>,
        now: Option<u64>,
    ) -> Result<(), CodecError> {
        let (reference, body) = match content {
            ContentPart::Audio(v) => (v.reference(), Some(v.fingerprint())),
            ContentPart::AudioReference(v) => (v, None),
            _ => return Err(CodecError::Invalid("audio owner")),
        };
        let binding = self
            .audio
            .get(&owner)
            .ok_or(CodecError::Invalid("audio binding"))?;
        if origin.is_none()
            || binding.origin.as_ref() != origin
            || binding.reference != reference.fingerprint()
            || body.is_some() && body != binding.body
        {
            return Err(CodecError::Invalid("audio binding"));
        }
        if reference.expires_at().is_some() {
            reference.check_at(now.ok_or(CodecError::Invalid("audio clock"))?)?;
        }
        Ok(())
    }
    pub fn record_input_text(&mut self, owner: PartId) -> Result<(), CodecError> {
        if self.input_text_forms.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.input_text_forms.insert(owner);
        Ok(())
    }
    pub fn input_text_form(&self, owner: PartId) -> bool {
        self.input_text_forms.contains(&owner)
    }
    pub fn record_cache_breakpoint(&mut self, owner: PartId) -> Result<(), CodecError> {
        if self.cache_breakpoints.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.cache_breakpoints.insert(owner);
        Ok(())
    }
    pub fn cache_breakpoint(&self, owner: PartId) -> bool {
        self.cache_breakpoints.contains(&owner)
    }
    pub fn record_strict_image_size(&mut self, owner: PartId) -> Result<(), CodecError> {
        if self.strict_image_sizes.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.strict_image_sizes.insert(owner);
        Ok(())
    }
    pub fn strict_image_size(&self, owner: PartId) -> bool {
        self.strict_image_sizes.contains(&owner)
    }
    /// Targets without a size-policy carrier must reject, not downsize.
    pub fn has_strict_image_sizes(&self, items: &[(ItemId, Item)]) -> bool {
        self.strict_image_sizes
            .iter()
            .any(|owner| part_exists(items, *owner))
    }
    pub fn record_response_item_id(
        &mut self,
        owner: ItemId,
        value: &str,
    ) -> Result<(), CodecError> {
        if (!self.response_item_ids.contains_key(&owner)
            && self.response_item_ids.len() >= MAX_ITEMS)
            || self.response_item_ids.iter().any(|(id, text)| {
                *id != owner && id.scope() == owner.scope() && text.as_str() == value
            })
        {
            return Err(CodecError::Invalid(
                "duplicate or excessive wire item identities",
            ));
        }
        let value = Text::new(value, "wire item id", 256).map_err(|_| CodecError::Limit)?;
        self.response_item_ids.insert(owner, value);
        Ok(())
    }
    pub fn response_item_id(&self, owner: ItemId) -> Option<&str> {
        self.response_item_ids.get(&owner).map(Text::as_str)
    }
    pub(crate) fn remove_response_item_id(&mut self, owner: ItemId) {
        self.response_item_ids.remove(&owner);
    }
    /// Source scopes may reuse aliases; a single target envelope may not.
    pub(crate) fn check_wire_item_ids(
        &self,
        items: &[(ItemId, Item)],
        response: bool,
    ) -> Result<(), CodecError> {
        let mut aliases = std::collections::BTreeSet::new();
        for (owner, item) in items {
            let generated = response
                || matches!(
                    item,
                    Item::Reasoning(_) | Item::Program(_) | Item::ProgramOutput(_)
                )
                || matches!(item, Item::Instruction(instruction) if instruction.status.is_some());
            let alias = self
                .response_item_id(*owner)
                .map(str::to_owned)
                .or_else(|| generated.then(|| generated_item_id(*owner)));
            if alias.is_some_and(|alias| !aliases.insert(alias)) {
                return Err(CodecError::Invalid("duplicate target wire identity"));
            }
        }
        Ok(())
    }
    pub fn normalizations(&self) -> &std::collections::BTreeSet<Normalization> {
        &self.normalizations
    }
    pub(crate) fn record_history_annotations_default(&mut self) {
        self.normalizations
            .insert(Normalization::HistoryAnnotationsDefault);
    }
    pub(crate) fn record_cache_write_default(&mut self, profile: &'static str) {
        self.normalizations
            .insert(Normalization::CacheWriteDefault { profile });
    }
    pub(crate) fn record_effort_mapping(
        &mut self,
        rule: &'static str,
        input: &'static str,
        output: &'static str,
    ) {
        self.normalizations
            .insert(Normalization::ReasoningEffortMapping {
                rule,
                input,
                output,
            });
    }
    pub(crate) fn record_omission(&mut self, omission: Normalization) {
        self.normalizations.insert(omission);
    }
    pub fn retain_owners(&mut self, items: &[(ItemId, Item)]) {
        let owners: BTreeMap<_, _> = items.iter().map(|(id, item)| (*id, item)).collect();
        let message_parts = message_parts(items);
        let mut parts = std::collections::BTreeSet::new();
        for (_, item) in items {
            let output = match item {
                Item::Instruction(value) => {
                    parts.extend(value.parts.iter().map(|p| p.id));
                    None
                }
                Item::Message(value) => {
                    parts.extend(value.parts.iter().map(|p| p.id));
                    None
                }
                Item::Reasoning(value) => {
                    parts.extend(value.parts.iter().map(|(id, _)| *id));
                    None
                }
                Item::ToolResult(value) | Item::CustomResult(value) => Some(&value.output),
                Item::ProviderTool(value) => value.output.as_ref(),
                _ => None,
            };
            if let Some(ToolOutput::Parts(values)) = output {
                parts.extend(values.iter().map(|p| p.id));
            }
        }
        self.cache_breakpoints.retain(|owner| parts.contains(owner));
        self.strict_image_sizes
            .retain(|owner| parts.contains(owner));
        self.input_text_forms.retain(|owner| parts.contains(owner));
        self.audio.retain(|owner, _| parts.contains(owner));
        self.response_item_ids
            .retain(|id, _| owners.contains_key(id));
        self.reported_creators
            .retain(|id, _| owners.contains_key(id));
        self.item_replay.retain(|id, _| {
            matches!(
                owners.get(id),
                Some(Item::Reasoning(_) | Item::Compaction(_) | Item::ToolCall(_))
            )
        });
        self.part_replay
            .retain(|key, _| message_parts.contains_key(key));
    }
}
fn message_parts(
    items: &[(ItemId, Item)],
) -> BTreeMap<(ItemId, PartId), (ItemLifecycle, usize, &Part)> {
    items
        .iter()
        .filter_map(|(id, item)| match item {
            Item::Message(message) => Some((*id, message)),
            _ => None,
        })
        .flat_map(|(id, message)| {
            message
                .parts
                .iter()
                .enumerate()
                .map(move |(ordinal, part)| ((id, part.id), (message.status, ordinal, part)))
        })
        .collect()
}
fn part_exists(items: &[(ItemId, Item)], owner: PartId) -> bool {
    let listed = |ids: &mut dyn Iterator<Item = PartId>| ids.take(MAX_ITEMS).any(|id| id == owner);
    items.iter().take(MAX_ITEMS).any(|(_, item)| {
        let output = match item {
            Item::Instruction(value) => return listed(&mut value.parts.iter().map(|p| p.id)),
            Item::Message(value) => return listed(&mut value.parts.iter().map(|p| p.id)),
            Item::Reasoning(value) => return listed(&mut value.parts.iter().map(|(id, _)| *id)),
            Item::ToolResult(value) | Item::CustomResult(value) => Some(&value.output),
            Item::ProviderTool(value) => value.output.as_ref(),
            _ => None,
        };
        match output {
            Some(ToolOutput::Parts(parts)) => listed(&mut parts.iter().map(|p| p.id)),
            _ => false,
        }
    })
}
