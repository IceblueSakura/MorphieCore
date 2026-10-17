//! Generation input, protocol provenance, context validation and replay isolation.
use super::{AdapterError, ClientProtocol};
use crate::{
    lowering::generation::RepresentationError,
    protocol::{CodecError, DecodedRequest},
    semantic::{
        context::{ConversationContext, DeliveryIntent, ExecutionHints, ServicePolicy},
        value::Presence,
    },
};
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub task: DecodedRequest,
    pub model: String,
    pub context: ExecutionHints,
    pub service_policy: Presence<ServicePolicy>,
    pub delivery: DeliveryIntent,
    pub conversation: Option<ConversationContext>,
    pub(crate) inferred_affinity: Option<crate::protocol::cache::InferredAffinity>,
    pub cache_session: Option<crate::protocol::cache::CacheSession>,
    pub(crate) source: Option<ClientProtocol>,
    pub(crate) n: Presence<u64>,
    /// Ignored client request field names, for diagnostics only; never encoded.
    pub(crate) ignored_fields: Vec<String>,
}
impl Request {
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
            return Err(CodecError::Invalid("model label"));
        }
        Ok(Self {
            task,
            model: model.into(),
            context: Default::default(),
            service_policy: Presence::Absent,
            delivery: Default::default(),
            conversation: None,
            cache_session: None,
            inferred_affinity: None,
            ignored_fields: vec![],
            source: None,
            n: Presence::Absent,
        })
    }
    pub fn check_semantic(
        &self,
        contract: &crate::semantic::task::generation::GenerationSemanticContract,
    ) -> Result<crate::semantic::task::generation::GenerationRequirements, AdapterError> {
        self.task.semantic.validate().map_err(CodecError::from)?;
        contract
            .check(&self.task.semantic)
            .map_err(|e| RepresentationError::Admission(e).into())
    }
    pub fn check_context(&self, standard_context: bool) -> Result<(), AdapterError> {
        self.check_grouping()?;
        self.context.validate().map_err(CodecError::from)?;
        self.delivery.validate().map_err(CodecError::from)?;
        if !standard_context
            && (!self.context.metadata.is_absent()
                || !self.service_policy.is_absent()
                || !self.context.max_tool_calls.is_absent())
        {
            return Err(RepresentationError::UnmigratedSemantic.into());
        }
        Ok(())
    }
    /// Clear tool result error flags; true when any was set.
    pub(crate) fn drop_tool_error_flags(&mut self) -> Result<bool, CodecError> {
        use crate::semantic::task::generation::Item;
        let mut items = self.task.semantic.items().to_vec();
        let mut dropped = false;
        for (_, item) in &mut items {
            if let Item::ToolResult(result) = item
                && result.is_error.take().is_some()
            {
                dropped = true;
            }
        }
        if dropped {
            self.task.semantic = self.task.semantic.clone().with_items(items)?;
        }
        Ok(dropped)
    }
    pub(crate) fn check_grouping(&self) -> Result<(), CodecError> {
        if self.conversation.is_some() && self.cache_session.is_some() {
            return Err(CodecError::Invalid("ambiguous request grouping"));
        }
        Ok(())
    }
}
impl Request {
    /// Transfer visible history, not another model's internal reasoning state.
    /// Only complete, demonstrably incompatible reasoning is omitted. Unattributed
    /// readable reasoning is not inferred to be foreign. Required context remains.
    pub(crate) fn without_foreign_reasoning(
        &self,
        target: Option<&crate::semantic::value::ReplayOrigin>,
    ) -> Result<Option<Request>, CodecError> {
        use crate::semantic::task::generation::{Item, ItemLifecycle, MessageRole};
        let fidelity = &self.task.fidelity;
        fidelity.check_target_replay(&self.task.semantic, target)?;
        // Compaction replaces context; losing a foreign value is not a safe projection.
        for (id, item) in self.task.semantic.items() {
            if let Item::Compaction(value) = item
                && !fidelity.compaction_matches(*id, value, target)
            {
                return Err(CodecError::Unsupported(
                    "incompatible compaction replay".into(),
                ));
            }
        }
        let foreign: Vec<_> = self
            .task
            .semantic
            .items()
            .iter()
            .filter(|(id, item)| {
                matches!(item, Item::Reasoning(r) if r.replay.is_some())
                    && (target.is_none() || fidelity.replay_origin(*id) != target)
            })
            .map(|(id, _)| *id)
            .collect();
        if foreign.is_empty() {
            return Ok(None);
        }
        if self.task.semantic.items().iter().any(|(id, item)| {
            foreign.contains(id)
                && matches!(item, Item::Reasoning(r) if r.status != ItemLifecycle::Completed
                    || r.replay.as_ref().is_some_and(|v| v.replay_token().is_none()))
        }) {
            return Err(CodecError::Unsupported("partial foreign reasoning".into()));
        }
        let mut copy = self.clone();
        // Responses item IDs can be paired with discarded reasoning. Forget only
        // call wire aliases in affected assistant runs; semantic call IDs and links
        // stay unchanged. This also covers trailing Google signature carriers.
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
            if run.iter().any(|(id, _)| foreign.contains(id)) {
                if run.iter().any(|(id, item)| {
                    !foreign.contains(id)
                        && matches!(item, Item::Reasoning(reasoning) if reasoning.replay.is_some())
                }) && run.iter().any(|(id, item)| {
                    matches!(item, Item::ToolCall(_) | Item::CustomCall(_))
                        && fidelity.response_item_id(*id).is_some()
                }) {
                    return Err(CodecError::Unsupported(
                        "mixed replay call dependencies".into(),
                    ));
                }
                for (id, item) in run {
                    if matches!(item, Item::ToolCall(_) | Item::CustomCall(_)) {
                        copy.task.fidelity.remove_response_item_id(*id);
                    }
                }
            }
        }
        let semantic = self
            .task
            .semantic
            .clone()
            .retain_items(|id, _| !foreign.contains(&id))?;
        copy.task.fidelity.retain_owners(semantic.items());
        copy.task.semantic = semantic;
        Ok(Some(copy))
    }
}
