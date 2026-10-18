//! A reported disposition is not a pending-call view or execution authorization.
use crate::semantic::task::generation::{
    Continuation, GenerationError, Item, ItemId, ItemLifecycle, Outcome,
};
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
        let pending =
            crate::semantic::task::generation::history::continuation::pending_results(items);
        let open_call = items.iter().any(|(_, item)| {
            matches!(item, Item::ToolCall(call) if call.status != ItemLifecycle::Completed)
        });
        let valid = match self {
            Self::TurnFinished => matches!(pending, Continuation::Unreported),
            Self::AwaitingToolResults => {
                outcome == Outcome::Completed
                    && matches!(pending, Continuation::ToolResults(_))
                    && !open_call
            }
            Self::NeedsContinuation => !matches!(outcome, Outcome::Failed | Outcome::Cancelled),
            Self::Unreported => true,
        };
        if valid {
            Ok(())
        } else {
            Err(GenerationError::InvalidResponse)
        }
    }
}
