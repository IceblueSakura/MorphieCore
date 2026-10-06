//! A reported disposition is not a pending-call view or execution authorization.
use super::{Continuation, GenerationError, Item, ItemId, ItemLifecycle, Outcome};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InteractionProgress {
    #[default]
    Unreported,
    TurnFinished,
    AwaitingToolResults,
    NeedsContinuation,
}
impl InteractionProgress {
    pub(crate) fn validate(
        self,
        outcome: Outcome,
        items: &[(ItemId, Item)],
    ) -> Result<(), GenerationError> {
        let pending = super::continuation::pending_results(items);
        match self {
            Self::TurnFinished if !matches!(pending, Continuation::Unreported) => Err(GenerationError::InvalidResponse),
            Self::AwaitingToolResults if outcome != Outcome::Completed
                || !matches!(pending, Continuation::ToolResults(_))
                || items.iter().any(|(_, item)| matches!(item, Item::ToolCall(call) if call.status != ItemLifecycle::Completed)) =>
                Err(GenerationError::InvalidResponse),
            Self::NeedsContinuation if matches!(outcome, Outcome::Failed | Outcome::Cancelled) => Err(GenerationError::InvalidResponse),
            _ => Ok(()),
        }
    }
}
