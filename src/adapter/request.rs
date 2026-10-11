//! Protocol-neutral generation input, context validation and replay isolation.
use super::AdapterError;
use crate::{
    lowering::generation::RepresentationError,
    protocol::{CodecError, DecodedRequest, extensions::CustomSections, openai::Profile},
    semantic::{
        context::{ConversationContext, DeliveryIntent, ExecutionHints},
        value::Presence,
    },
};
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub task: DecodedRequest,
    pub model: String,
    pub context: ExecutionHints,
    pub delivery: DeliveryIntent,
    pub extensions: CustomSections,
    pub conversation: Option<ConversationContext>,
    pub(crate) inferred_affinity: Option<crate::protocol::cache::InferredAffinity>,
    pub cache_session: Option<crate::protocol::cache::CacheSession>,
    pub(crate) source: Option<Profile>,
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
            delivery: Default::default(),
            extensions: Default::default(),
            conversation: None,
            cache_session: None,
            inferred_affinity: None,
            ignored_fields: vec![],
            source: None,
            n: Presence::Absent,
        })
    }
    /// Optional caller-owned stability check, never a cache directive or hit claim.
    pub fn capture_cache_prefix(
        &self,
        intent: crate::semantic::cache::CachePrefixIntent,
        scope: &crate::semantic::cache::CachePrefixScope,
    ) -> Result<crate::semantic::cache::CachePrefixProof, AdapterError> {
        self.check_context(true)?;
        Ok(intent
            .capture(&self.task.semantic, self.cache_prefix_context(), scope)
            .map_err(CodecError::from)?)
    }
    pub fn check_cache_prefix(
        &self,
        proof: &crate::semantic::cache::CachePrefixProof,
        scope: &crate::semantic::cache::CachePrefixScope,
    ) -> Result<(), AdapterError> {
        self.check_context(true)?;
        Ok(proof
            .check(&self.task.semantic, self.cache_prefix_context(), scope)
            .map_err(CodecError::from)?)
    }
    fn cache_prefix_context(&self) -> crate::semantic::cache::CachePrefixContext<'_> {
        crate::semantic::cache::CachePrefixContext {
            model: &self.model,
            hints: &self.context,
            grouping: self.cache_session.as_ref().map(|session| session.as_str()),
            conversation: self.conversation.as_ref(),
        }
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
        if self.extensions != CustomSections::default()
            || !standard_context
                && (!self.context.metadata.is_absent()
                    || !self.context.service_tier.is_absent()
                    || !self.context.max_tool_calls.is_absent())
        {
            return Err(RepresentationError::UnmigratedSemantic.into());
        }
        Ok(())
    }
    pub(crate) fn check_grouping(&self) -> Result<(), CodecError> {
        if self.conversation.is_some() && self.cache_session.is_some() {
            return Err(CodecError::Invalid("ambiguous request grouping"));
        }
        Ok(())
    }
}
impl Request {
    /// Candidate-private copy without opaque reasoning issued for another target.
    /// Reasoning items left with neither readable content nor replay are removed.
    /// `None` means the request needs no change for this target.
    pub(crate) fn without_foreign_replay(
        &self,
        target: Option<&crate::semantic::value::ReplayOrigin>,
    ) -> Result<Option<Request>, CodecError> {
        use crate::semantic::task::generation::Item;
        let fidelity = &self.task.fidelity;
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
        let stripped: Vec<_> = self
            .task
            .semantic
            .items()
            .iter()
            .cloned()
            .map(|(id, mut item)| {
                if let Item::Reasoning(r) = &mut item
                    && foreign.contains(&id)
                {
                    r.replay = None;
                }
                (id, item)
            })
            .collect();
        let without_empty: Vec<_> = stripped
            .iter()
            .filter(|(id, item)| {
                !(foreign.contains(id) && matches!(item, Item::Reasoning(r) if r.parts.is_empty()))
            })
            .cloned()
            .collect();
        let semantic = self.task.semantic.clone().with_items(without_empty)?;
        let mut copy = self.clone();
        for id in &foreign {
            copy.task.fidelity.remove_replay(*id);
        }
        copy.task.fidelity.retain_owners(semantic.items());
        copy.task.semantic = semantic;
        Ok(Some(copy))
    }
}
