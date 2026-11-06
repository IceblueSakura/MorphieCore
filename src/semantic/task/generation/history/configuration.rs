//! Source timing, before any target scope, level or budget conversion.
use crate::semantic::task::generation::*;

/// Effective controls, not an update patch. `None` can represent a cleared value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationState {
    pub activation: Option<ReasoningActivation>,
    pub effort: Option<EffortControl>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedConfiguration {
    pub active: ConfigurationState,
    /// Effective controls when the next new-input turn activates pending updates.
    pub pending: Option<ConfigurationState>,
}

impl GenerationRequest {
    /// One bounded scan of validated source history. Tool feedback alone does not
    /// start a new-input turn; mixed feedback and new input do.
    pub fn resolved_configuration(&self) -> ResolvedConfiguration {
        self.configuration_states()
            .last()
            .map_or_else(|| self.initial_configuration(), |(_, state)| state)
    }

    /// Retaining markers is lawful only if history edits preserve their timing,
    /// including earlier pending intervals hidden by a later effective value.
    pub(crate) fn configuration_timing_matches(&self, other: &Self) -> bool {
        self.configuration_states()
            .filter(|(item, _)| matches!(item, Item::ConfigurationUpdate(_)))
            .eq(other
                .configuration_states()
                .filter(|(item, _)| matches!(item, Item::ConfigurationUpdate(_))))
            && self.resolved_configuration() == other.resolved_configuration()
    }

    fn initial_configuration(&self) -> ResolvedConfiguration {
        ResolvedConfiguration {
            active: ConfigurationState {
                activation: self.reasoning().activation,
                effort: self.settings().effort,
            },
            pending: None,
        }
    }

    fn configuration_states(&self) -> impl Iterator<Item = (&Item, ResolvedConfiguration)> {
        self.items().iter().scan(
            (self.initial_configuration(), None),
            |(state, user_turn), (_, item)| {
                match item {
                    Item::ConfigurationUpdate(update) => {
                        if update.boundary == ConfigurationBoundary::FollowingResponse
                            || *user_turn == Some(true)
                        {
                            state.active.apply(*update);
                            if let Some(pending) = &mut state.pending {
                                pending.apply(*update);
                                if *pending == state.active {
                                    state.pending = None;
                                }
                            }
                        } else {
                            state.pending.get_or_insert(state.active).apply(*update);
                        }
                    }
                    Item::Message(message) if message.role == MessageRole::User => {
                        *user_turn = Some(true);
                        if let Some(pending) = state.pending.take() {
                            state.active = pending;
                        }
                    }
                    Item::ToolResult(_) | Item::CustomResult(_) => {
                        user_turn.get_or_insert(false);
                    }
                    Item::Instruction(_) | Item::ToolChange(_) | Item::ToolBatch(_) => {}
                    _ => *user_turn = None,
                }
                Some((item, *state))
            },
        )
    }
}

impl ConfigurationState {
    /// Both active and pending controls compose through the same ordered rule.
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
        }
        if let Some(effort) = update.effort {
            self.effort = Some(effort);
            // A named reasoning effort supersedes off; response-wide effort is independent.
            if effort.scope == EffortScope::Reasoning
                && self.activation == Some(ReasoningActivation::Disabled)
                && update.activation.is_none()
            {
                self.activation = Some(ReasoningActivation::Enabled);
            }
        }
    }
}
