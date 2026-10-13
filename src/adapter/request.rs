//! Protocol-neutral generation input, context validation and replay isolation.
use super::AdapterError;
use crate::{
    lowering::generation::RepresentationError,
    protocol::{CodecError, DecodedRequest, openai::Profile},
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
                || !self.context.service_tier.is_absent()
                || !self.context.max_tool_calls.is_absent())
        {
            return Err(RepresentationError::UnmigratedSemantic.into());
        }
        Ok(())
    }
    /// OpenAI reasoning has an effort only: Messages' thinking mode, display and
    /// budget are dropped, and a budget becomes the nearest effort when none is
    /// given. The bool says whether a budget was mapped.
    pub(crate) fn without_extended_reasoning(&self) -> Result<Option<(Self, bool)>, CodecError> {
        let settings = self.task.semantic.settings();
        let reasoning = &settings.reasoning;
        if !reasoning.requires_extended_controls() {
            return Ok(None);
        }
        let mut settings = settings.clone();
        let mapped = reasoning.effort.is_absent() && reasoning.budget_as_effort().is_some();
        if let (true, Some(effort)) = (mapped, reasoning.budget_as_effort()) {
            settings.reasoning.effort = Presence::Value(effort);
        }
        settings.reasoning.mode = Presence::Absent;
        settings.reasoning.display = Presence::Absent;
        settings.reasoning.budget_tokens = None;
        let mut copy = self.clone();
        copy.task.semantic = copy.task.semantic.with_settings(settings)?;
        Ok(Some((copy, mapped)))
    }
    /// Efforts other than `low`, `high` and `max` move to the nearest of those.
    pub(crate) fn with_effort_levels(&self) -> Result<Option<Self>, CodecError> {
        use crate::semantic::task::generation::ReasoningEffort as E;
        let settings = self.task.semantic.settings();
        let leveled = match settings.reasoning.effort.value() {
            Some(E::None | E::Minimal) => E::Low,
            Some(E::Medium) => E::High,
            Some(E::XHigh) => E::Max,
            _ => return Ok(None),
        };
        let mut settings = settings.clone();
        settings.reasoning.effort = Presence::Value(leveled);
        let mut copy = self.clone();
        copy.task.semantic = copy.task.semantic.with_settings(settings)?;
        Ok(Some(copy))
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
    /// Candidate-private copy without opaque reasoning issued for another target.
    /// Reasoning items left with neither readable content nor replay are removed;
    /// with `as_text` the readable rest becomes assistant text. The flag reports
    /// that conversion. `None` means the request needs no change for this target.
    pub(crate) fn without_foreign_replay(
        &self,
        target: Option<&crate::semantic::value::ReplayOrigin>,
        as_text: bool,
    ) -> Result<Option<(Request, bool)>, CodecError> {
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
        let readable = stripped.iter().any(|(id, item)| {
            foreign.contains(id) && matches!(item, Item::Reasoning(r) if !r.parts.is_empty())
        });
        let without_empty: Vec<_> = stripped
            .into_iter()
            .filter(|(id, item)| {
                !(foreign.contains(id) && matches!(item, Item::Reasoning(r) if r.parts.is_empty()))
            })
            .collect();
        let mut semantic = self.task.semantic.clone().with_items(without_empty)?;
        if as_text && readable {
            semantic = semantic.with_reasoning_as_text(&foreign)?;
        }
        let mut copy = self.clone();
        for id in &foreign {
            copy.task.fidelity.remove_replay(*id);
        }
        copy.task.fidelity.retain_owners(semantic.items());
        copy.task.semantic = semantic;
        Ok(Some((copy, as_text && readable)))
    }
}
