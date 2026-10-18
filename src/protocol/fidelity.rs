//! Bounded representation records. Only surviving semantic owners may reuse them.
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
/// Only the tolerant `drop_block` behavior is admitted; no target receives it.
pub(crate) const BLOCK_BINDING_OMISSION: Normalization = Normalization::Omitted {
    rule: "messages.omit-block-binding.v1",
    field: "generation.reasoning.block_binding",
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FidelityRecords {
    response_item_ids: BTreeMap<ItemId, Text>,
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
        value.validate_attachment()?;
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
            return Err(CodecError::Invalid("part replay rebinding"));
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
                return Err(CodecError::Invalid("replay owner rebinding"));
            }
            if old.final_value && !final_value {
                return Err(CodecError::Invalid("opaque replay phase"));
            }
            if old.origin.is_some() && old.origin != origin {
                return Err(CodecError::Invalid("replay origin rebinding"));
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
                return Err(CodecError::Invalid("native replay dependency"));
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
                return Err(CodecError::Invalid("native part replay dependency"));
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
            return Err(CodecError::Invalid("native replay origin"));
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
                return Err(CodecError::Invalid("native replay dependency"));
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
                return Err(CodecError::Invalid("call replay dependency"));
            }
        }
        for ((owner, part), binding) in &self.part_replay {
            if !parts
                .get(&(*owner, *part))
                .is_some_and(|(status, ordinal, p)| {
                    self.part_replay_matches(*owner, *status, *ordinal, p, binding.origin.as_ref())
                })
            {
                return Err(CodecError::Invalid("part replay dependency"));
            }
        }
        Ok(())
    }
    pub fn remove_replay(&mut self, owner: ItemId) {
        self.item_replay.remove(&owner);
    }
    /// Bind origin only at a trusted decode boundary, never from business JSON.
    pub fn bind_replay_origin(&mut self, origin: &ReplayOrigin) -> Result<(), CodecError> {
        if self
            .item_replay
            .values()
            .chain(self.part_replay.values())
            .any(|r| r.origin.as_ref().is_some_and(|old| old != origin))
        {
            return Err(CodecError::Invalid("replay origin rebinding"));
        }
        for replay in self
            .item_replay
            .values_mut()
            .chain(self.part_replay.values_mut())
        {
            replay.origin = Some(origin.clone());
        }
        Ok(())
    }
    /// Partial snapshots are checked but cannot bind response-wide facts.
    pub(crate) fn capture_response_extras(
        &mut self,
        protocol: Profile,
        adaptation: &Adaptation,
        o: &Map<String, Value>,
        semantic: Option<&GenerationResponse>,
    ) -> Result<(), CodecError> {
        let mut values = BTreeMap::new();
        let mut used = 0usize;
        for name in declared_response_extras(protocol, adaptation) {
            if let Some(value) = o.get(*name) {
                used = used.saturating_add(
                    crate::semantic::value::json_size(value, RESPONSE_EXTRAS_BUDGET)
                        .map_err(|_| CodecError::Limit)?,
                );
                if used > RESPONSE_EXTRAS_BUDGET {
                    return Err(CodecError::Limit);
                }
                values.insert((*name).into(), value.clone());
            }
        }
        if let Some(semantic) = semantic {
            self.response_extras = if values.is_empty() {
                None
            } else {
                Some(ResponseExtras {
                    protocol,
                    profile: adaptation.profile_id,
                    origin: adaptation
                        .scope
                        .clone()
                        .ok_or(CodecError::Invalid("unbound response extras"))?,
                    response_id: Text::new(
                        o.get("id")
                            .and_then(Value::as_str)
                            .ok_or(CodecError::Invalid("response identity"))?,
                        "response identity",
                        256,
                    )
                    .map_err(|_| CodecError::Limit)?,
                    dependency: response_dependency(semantic),
                    values,
                })
            };
        }
        Ok(())
    }
    pub(crate) fn projected_response_extras(
        &self,
        protocol: Profile,
        adaptation: &Adaptation,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Option<&BTreeMap<String, Value>> {
        self.response_extras
            .as_ref()
            .filter(|r| {
                r.protocol == protocol
                    && r.profile == adaptation.profile_id
                    && Some(&r.origin) == adaptation.scope.as_ref()
                    && r.response_id.as_str() == response_id
                    && r.dependency == response_dependency(semantic)
                    && adaptation.rules.response_extras
            })
            .map(|r| &r.values)
    }
    pub(crate) fn capture_routing_extras(
        &mut self,
        protocol: Profile,
        adaptation: &Adaptation,
        values: crate::protocol::openai::dialect::adapter_shapes::Extras,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Result<(), CodecError> {
        crate::protocol::openai::dialect::adapter_shapes::check_budget(&values)?;
        self.routing_extras = if values.is_empty() {
            None
        } else {
            if !(adaptation.rules.routing_extras
                || adaptation.rules.chat_stop_diagnostics
                || adaptation.rules.reported_request_id
                || adaptation.rules.responses_product_accounting
                || adaptation.rules.responses_access_programs
                || adaptation.rules.responses_zero_tool_accounting
                || adaptation.rules.responses_inactive_reports
                || adaptation.rules.responses_usage_attribution
                || adaptation.rules.responses_context_accounting)
            {
                return Err(CodecError::Unsupported("routing extras".into()));
            }
            Some(ResponseExtras {
                protocol,
                profile: adaptation.profile_id,
                origin: adaptation
                    .scope
                    .clone()
                    .ok_or(CodecError::Invalid("unbound routing extras"))?,
                response_id: Text::new(response_id, "response identity", 256)
                    .map_err(|_| CodecError::Limit)?,
                dependency: response_dependency(semantic),
                values,
            })
        };
        Ok(())
    }
    pub(crate) fn projected_routing_extras(
        &self,
        protocol: Profile,
        adaptation: &Adaptation,
        semantic: &GenerationResponse,
        response_id: &str,
    ) -> Option<&BTreeMap<String, Value>> {
        self.routing_extras
            .as_ref()
            .filter(|r| {
                (adaptation.rules.routing_extras
                    || adaptation.rules.chat_stop_diagnostics
                    || adaptation.rules.reported_request_id
                    || adaptation.rules.responses_product_accounting
                    || adaptation.rules.responses_access_programs
                    || adaptation.rules.responses_zero_tool_accounting
                    || adaptation.rules.responses_inactive_reports
                    || adaptation.rules.responses_usage_attribution
                    || adaptation.rules.responses_context_accounting)
                    && r.protocol == protocol
                    && r.profile == adaptation.profile_id
                    && Some(&r.origin) == adaptation.scope.as_ref()
                    && r.response_id.as_str() == response_id
                    && r.dependency == response_dependency(semantic)
            })
            .map(|r| &r.values)
    }
    pub fn normalizations(&self) -> &std::collections::BTreeSet<Normalization> {
        &self.normalizations
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
    pub(crate) fn copy_response_records(&mut self, source: &Self) {
        self.response_extras = source.response_extras.clone();
        self.routing_extras = source.routing_extras.clone();
        self.normalizations = source.normalizations.clone();
    }
    /// Intake only, after independent native snapshot equality and terminal materialization.
    /// Local source IDs may differ between static and interleaved event parsing.
    pub(crate) fn bind_verified_snapshot_records(
        &mut self,
        source: &Self,
        semantic: &GenerationResponse,
    ) {
        self.copy_response_records(source);
        let dependency = response_dependency(semantic);
        for record in [&mut self.response_extras, &mut self.routing_extras]
            .into_iter()
            .flatten()
        {
            record.dependency = dependency;
        }
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
// This in-process fingerprint is not persisted or exposed as a wire identity.
// Hashing the complete typed response conservatively invalidates extras after edits.
fn response_dependency(response: &GenerationResponse) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(format!("{response:?}").as_bytes());
    for (id, value) in response.resources().iter() {
        hash.update(id.scope().get().to_le_bytes());
        hash.update(id.get().to_le_bytes());
        hash.update(value.fingerprint());
    }
    // Opaque Debug is redacted; bind its bytes separately without formatting them.
    for (id, item) in response.items() {
        let value = match item {
            Item::Reasoning(r) => r.replay.as_ref(),
            Item::Compaction(value) => Some(value),
            Item::ToolCall(call) => call.replay.as_ref(),
            _ => None,
        };
        if let Some(value) = value {
            hash.update(id.scope().get().to_le_bytes());
            hash.update(id.get().to_le_bytes());
            hash.update(value.fingerprint());
        }
    }
    for (_, item) in response.items() {
        if let Item::Message(m) = item {
            for p in &m.parts {
                if let Some(value) = &p.replay {
                    hash.update(value.fingerprint());
                }
                match &p.content {
                    ContentPart::Text(t) => {
                        for a in t.annotations() {
                            hash.update(a.fingerprint());
                        }
                    }
                    ContentPart::Audio(a) => hash.update(a.fingerprint()),
                    ContentPart::AudioReference(a) => hash.update(a.fingerprint()),
                    _ => {}
                }
            }
        }
    }
    hash.finalize().into()
}

// A digest binds replay to its typed owner without retaining a second readable payload.
fn fingerprint(item: &ReasoningItem) -> [u8; 32] {
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

fn part_dependency(
    status: ItemLifecycle,
    ordinal: usize,
    part: &Part,
) -> Result<[u8; 32], CodecError> {
    use sha2::{Digest, Sha256};
    let ContentPart::Text(text) = &part.content else {
        return Err(CodecError::Invalid("part replay owner"));
    };
    let value = part
        .replay
        .as_ref()
        .ok_or(CodecError::Invalid("part replay"))?;
    let mut hash = Sha256::new();
    hash.update(format!("{status:?}").as_bytes());
    hash.update((ordinal as u64).to_le_bytes());
    hash.update(part.id.scope().get().to_le_bytes());
    hash.update(part.id.get().to_le_bytes());
    hash.update(text.as_str().as_bytes());
    hash.update(value.fingerprint());
    Ok(hash.finalize().into())
}
fn call_dependency(call: &ToolCall) -> Result<[u8; 32], CodecError> {
    use sha2::{Digest, Sha256};
    let value = call
        .replay
        .as_ref()
        .ok_or(CodecError::Invalid("call replay"))?;
    let arguments = match &call.arguments {
        ToolArguments::Structured(value) => serde_json::to_string(value.value())
            .map_err(|_| CodecError::Invalid("call arguments"))?,
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
