//! Ordered Messages history rewrites selected by the request dispositions.
use crate::protocol::failure::CarrierReason;
use crate::{protocol::CodecError, semantic::task::generation::*};

/// The level in effect for the reply being requested, by the documented rules: a change
/// directly after a user turn with new input sets that turn's reply; any other change
/// waits for the next user turn with new input. Tool-result-only turns are not new input.
/// https://platform.claude.com/docs/en/build-with-claude/effort#change-effort-mid-conversation-beta
pub(crate) fn flatten_effort(
    initial: Option<EffortControl>,
    items: &[(ItemId, Item)],
) -> Result<Option<EffortControl>, CodecError> {
    let mut active = initial;
    let mut pending = None;
    // `Some(new_input)` while inside a user turn.
    let mut user_turn = None;
    for (_, item) in items {
        match item {
            Item::ConfigurationUpdate(update) => {
                let effort = match update {
                    ConfigurationUpdate {
                        activation: None,
                        effort: Some(effort),
                    } if effort.scope == EffortScope::Response => *effort,
                    _ => {
                        return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                    }
                };
                if user_turn == Some(true) {
                    active = Some(effort);
                    pending = None;
                } else {
                    pending = Some(effort);
                }
            }
            Item::Message(message) if message.role == MessageRole::User => {
                user_turn = Some(true);
                if let Some(effort) = pending.take() {
                    active = Some(effort);
                }
            }
            Item::ToolResult(_) | Item::CustomResult(_) => {
                user_turn.get_or_insert(false);
            }
            Item::Instruction(_) | Item::ToolChange(_) => {}
            _ => user_turn = None,
        }
    }
    Ok(active)
}
