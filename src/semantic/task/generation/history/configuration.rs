//! Source timing, before any target scope, level or budget conversion.
use crate::semantic::task::generation::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedConfiguration {
    pub activation: Option<ReasoningActivation>,
    pub effort: Option<EffortControl>,
    pub pending: Option<ConfigurationUpdate>,
}

impl GenerationRequest {
    /// One bounded scan of validated source history. Tool feedback alone does not
    /// start a new-input turn; mixed feedback and new input does.
    pub fn resolved_configuration(&self) -> ResolvedConfiguration {
        let mut state = ResolvedConfiguration {
            activation: self.reasoning().activation,
            effort: self.settings().effort,
            pending: None,
        };
        let mut user_turn = None;
        for (_, item) in self.items() {
            match item {
                Item::ConfigurationUpdate(update) => {
                    if update.boundary == ConfigurationBoundary::FollowingResponse
                        || user_turn == Some(true)
                    {
                        state.apply(*update);
                    } else {
                        let pending = state.pending.get_or_insert(ConfigurationUpdate {
                            boundary: ConfigurationBoundary::NewInputTurn,
                            ..Default::default()
                        });
                        if update.activation.is_some() {
                            pending.activation = update.activation;
                        }
                        if update.effort.is_some() {
                            pending.effort = update.effort;
                        }
                    }
                }
                Item::Message(message) if message.role == MessageRole::User => {
                    user_turn = Some(true);
                    if let Some(update) = state.pending.take() {
                        state.apply(update);
                    }
                }
                Item::ToolResult(_) | Item::CustomResult(_) => {
                    user_turn.get_or_insert(false);
                }
                Item::Instruction(_) | Item::ToolChange(_) | Item::ToolBatch(_) => {}
                _ => user_turn = None,
            }
        }
        state
    }
}

impl ResolvedConfiguration {
    fn apply(&mut self, update: ConfigurationUpdate) {
        if let Some(activation) = update.activation {
            self.activation = Some(activation);
            if activation == ReasoningActivation::Disabled
                && self
                    .effort
                    .is_some_and(|e| e.scope == EffortScope::Reasoning)
            {
                self.effort = None;
            }
            if let Some(pending) = &mut self.pending {
                pending.activation = None;
            }
        }
        if let Some(effort) = update.effort {
            self.effort = Some(effort);
            // A named reasoning effort supersedes an earlier off selection;
            // response-wide effort remains independent of thinking activation.
            if effort.scope == EffortScope::Reasoning
                && self.activation == Some(ReasoningActivation::Disabled)
                && update.activation.is_none()
            {
                self.activation = Some(ReasoningActivation::Enabled);
            }
            if let Some(pending) = &mut self.pending {
                pending.effort = None;
            }
        }
        if self
            .pending
            .is_some_and(|p| p.activation.is_none() && p.effort.is_none())
        {
            self.pending = None;
        }
    }
}
