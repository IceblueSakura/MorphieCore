//! Generation input, protocol provenance, context validation and replay isolation.
use crate::{
    protocol::{CodecError, DecodedRequest, failure::CarrierReason},
    semantic::{
        context::{ConversationContext, DeliveryIntent, ExecutionHints, ServicePolicy},
        value::Presence,
    },
};
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationRequestEnvelope {
    pub task: DecodedRequest,
    pub model: String,
    pub context: ExecutionHints,
    pub service_policy: Option<ServicePolicy>,
    pub delivery: DeliveryIntent,
    pub conversation: Option<ConversationContext>,
    pub(crate) inferred_affinity: Option<crate::protocol::cache::InferredAffinity>,
    pub cache_session: Option<crate::protocol::cache::CacheSession>,
    pub(crate) source: Option<ClientProtocol>,
    pub(crate) n: Presence<u64>,
    /// Ignored client request field names, for diagnostics only; never encoded.
    pub(crate) ignored_fields: Vec<String>,
}
impl GenerationRequestEnvelope {
    /// Pure typed entry; model is a public label, not an origin or runtime target.
    pub fn from_generation(
        task: crate::protocol::DecodedRequest,
        model: &str,
    ) -> Result<Self, CodecError> {
        task.semantic.validate()?;
        if model.is_empty()
            || model.len() > 256
            || model.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(CodecError::invalid());
        }
        Ok(Self {
            task,
            model: model.into(),
            context: Default::default(),
            service_policy: None,
            delivery: Default::default(),
            conversation: None,
            cache_session: None,
            inferred_affinity: None,
            ignored_fields: vec![],
            source: None,
            n: Presence::Absent,
        })
    }
    pub(crate) fn check_grouping(&self) -> Result<(), CodecError> {
        if self.conversation.is_some() && self.cache_session.is_some() {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
}
impl GenerationRequestEnvelope {
    /// Transfer visible history, not another model's internal reasoning state.
    /// Demonstrably incompatible reasoning is omitted regardless of item lifecycle;
    /// its opaque value must still be finalized. Unattributed readable reasoning is
    /// not inferred to be foreign. Required context remains.
    pub(crate) fn without_foreign_reasoning(
        &self,
        target: Option<&crate::semantic::value::ReplayOrigin>,
    ) -> Result<Option<GenerationRequestEnvelope>, CodecError> {
        self.without_uncarried_reasoning(target, |_| Ok(false))
    }
    /// Extend visible-history transfer with the selected codec's carrier policy.
    /// Dependency checks and alias pruning apply equally to same-issuer omissions.
    pub(crate) fn without_uncarried_reasoning(
        &self,
        target: Option<&crate::semantic::value::ReplayOrigin>,
        uncarried: impl Fn(
            &crate::semantic::task::generation::ReasoningItem,
        ) -> Result<bool, CodecError>,
    ) -> Result<Option<GenerationRequestEnvelope>, CodecError> {
        use crate::semantic::task::generation::{Item, ItemLifecycle, MessageRole};
        let fidelity = &self.task.fidelity;
        fidelity.check_target_replay(&self.task.semantic, target)?;
        // Compaction replaces context; losing a foreign value is not a safe projection.
        for (id, item) in self.task.semantic.items() {
            if let Item::Compaction(value) = item
                && !fidelity.compaction_matches(*id, value, target)
            {
                return Err(CodecError::Unsupported(CarrierReason::Replay));
            }
        }
        let mut omitted = Vec::new();
        for (id, item) in self.task.semantic.items() {
            if let Item::Reasoning(reasoning) = item {
                let no_carrier = uncarried(reasoning)?;
                if no_carrier
                    || reasoning.replay.is_some()
                        && (target.is_none() || fidelity.replay_origin(*id) != target)
                {
                    omitted.push(*id);
                }
            }
        }
        let mut foreign_parts = Vec::new();
        let mut foreign_calls = Vec::new();
        for (id, item) in self.task.semantic.items() {
            match item {
                Item::Message(message) => {
                    for part in &message.parts {
                        if let Some(value) = &part.replay {
                            let origin = fidelity
                                .part_replay_origin(*id, part.id)
                                .ok_or(CodecError::invalid())?;
                            if Some(origin) != target {
                                if message.status != ItemLifecycle::Completed
                                    || value.replay_token().is_none()
                                {
                                    return Err(CodecError::Unsupported(CarrierReason::Replay));
                                }
                                foreign_parts.push((*id, part.id));
                            }
                        }
                    }
                }
                Item::ToolCall(call) => {
                    if let Some(value) = &call.replay {
                        let origin = fidelity.replay_origin(*id).ok_or(CodecError::invalid())?;
                        if Some(origin) != target {
                            if call.status != ItemLifecycle::Completed
                                || value.replay_token().is_none()
                            {
                                return Err(CodecError::Unsupported(CarrierReason::Replay));
                            }
                            foreign_calls.push(*id);
                        }
                    }
                }
                _ => {}
            }
        }
        if omitted.is_empty() && foreign_parts.is_empty() && foreign_calls.is_empty() {
            return Ok(None);
        }
        if self.task.semantic.items().iter().any(|(id, item)| {
            omitted.contains(id)
                && matches!(item, Item::Reasoning(r)
                    if r.replay.as_ref().is_some_and(|v| v.replay_token().is_none()))
        }) {
            return Err(CodecError::Unsupported(CarrierReason::Reasoning));
        }
        let mut copy = self.clone();
        // Responses item IDs can be paired with discarded reasoning. Forget only
        // call wire aliases in affected assistant runs; semantic call IDs and links
        // stay unchanged. Part/call replay is removed without deleting its visible owner.
        let assistant = |item: &Item| match item {
            Item::Reasoning(_) | Item::ToolCall(_) | Item::CustomCall(_) => true,
            Item::Message(message) => message.role == MessageRole::Assistant,
            _ => false,
        };
        for run in self
            .task
            .semantic
            .items()
            .chunk_by(|a, b| assistant(&a.1) && assistant(&b.1))
        {
            if run.iter().any(|(id, _)| {
                omitted.contains(id)
                    || foreign_calls.contains(id)
                    || foreign_parts.iter().any(|(owner, _)| owner == id)
            }) {
                if run.iter().any(|(id, item)| {
                    !omitted.contains(id)
                        && matches!(item, Item::Reasoning(reasoning) if reasoning.replay.is_some())
                }) && run.iter().any(|(id, item)| {
                    matches!(item, Item::ToolCall(_) | Item::CustomCall(_))
                        && fidelity.response_item_id(*id).is_some()
                }) {
                    return Err(CodecError::Unsupported(CarrierReason::Replay));
                }
                for (id, item) in run {
                    if matches!(item, Item::ToolCall(_) | Item::CustomCall(_)) {
                        copy.task.fidelity.remove_response_item_id(*id);
                    }
                }
            }
        }
        if !omitted.is_empty() {
            copy.task.semantic = copy
                .task
                .semantic
                .retain_items(|id, _| !omitted.contains(&id))?;
        }
        if !foreign_parts.is_empty() || !foreign_calls.is_empty() {
            let mut items = copy.task.semantic.items().to_vec();
            for (id, item) in &mut items {
                match item {
                    Item::Message(message) => {
                        for part in &mut message.parts {
                            if foreign_parts.contains(&(*id, part.id)) {
                                part.replay = None;
                                copy.task.fidelity.remove_part_replay(*id, part.id);
                            }
                        }
                    }
                    Item::ToolCall(call) if foreign_calls.contains(id) => {
                        call.replay = None;
                        copy.task.fidelity.remove_replay(*id);
                    }
                    _ => {}
                }
            }
            copy.task.semantic = copy.task.semantic.with_items(items)?;
        }
        copy.task.fidelity.retain_owners(copy.task.semantic.items());
        Ok(Some(copy))
    }
}

/// Downstream wire identity, independent of the selected upstream endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ClientProtocol {
    Chat,
    Responses,
    Messages,
}
impl ClientProtocol {
    /// Every public Generation client protocol.
    pub const ALL: [Self; 3] = [Self::Chat, Self::Responses, Self::Messages];
}
impl From<crate::protocol::openai::GenerationApi> for ClientProtocol {
    fn from(profile: crate::protocol::openai::GenerationApi) -> Self {
        match profile {
            crate::protocol::openai::GenerationApi::Chat => Self::Chat,
            crate::protocol::openai::GenerationApi::Responses => Self::Responses,
        }
    }
}
