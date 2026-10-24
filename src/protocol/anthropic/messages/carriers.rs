//! Beta Messages carriers a binding may lack, and the named rules that replace them.
//! The rewrites are pure IR edits on one candidate's copy.
use super::RequestRule;
use crate::{
    protocol::CodecError,
    semantic::{task::generation::*, value::Presence},
};
use std::collections::BTreeSet;

/// Beta carriers the trusted binding has verified for its upstream model.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Carriers {
    /// `thinking.block_binding`.
    pub block_binding: bool,
    /// `output_config` on mid-conversation system messages.
    pub message_effort: bool,
    /// `tool_addition` and `tool_removal` blocks.
    pub tool_changes: bool,
}
impl Carriers {
    pub const fn all() -> Self {
        Self {
            block_binding: true,
            message_effort: true,
            tool_changes: true,
        }
    }
}

/// Replace missing carriers by their named rules; `None` keeps the request unchanged.
pub(super) fn project(
    request: &GenerationRequest,
    carriers: Carriers,
    rules: &mut BTreeSet<RequestRule>,
) -> Result<Option<GenerationRequest>, CodecError> {
    let mut settings = request.settings().clone();
    let mut items = request.items().to_vec();
    if !carriers.block_binding {
        match settings.reasoning.replay_mismatch.take() {
            Some(ReplayMismatch::Drop) => {
                rules.insert(RequestRule::OmitBlockBinding);
            }
            // Explicit enforcement is not the account default everywhere.
            Some(ReplayMismatch::Reject) => {
                return Err(CodecError::Unsupported(
                    crate::protocol::failure::CarrierReason::Reasoning,
                ));
            }
            None => {}
        }
    }
    if !carriers.message_effort && items.iter().any(|(_, i)| is_effort(i)) {
        settings.effort = flatten_effort(settings.effort, &items)?;
        items.retain(|(_, item)| !is_effort(item));
        rules.insert(RequestRule::FlattenMessageEffort);
    }
    if !carriers.tool_changes && items.iter().any(|(_, i)| matches!(i, Item::ToolChange(_))) {
        if GenerationRequirements::derive(request).tool_examples {
            return Err(CodecError::Unsupported(
                crate::protocol::failure::CarrierReason::Tools,
            ));
        }
        settings.tools = Some(collapse_tools(request.tools(), &items)?);
        items.retain(|(_, item)| !matches!(item, Item::ToolChange(_)));
        rules.insert(RequestRule::CollapseToolChanges);
    }
    if &settings == request.settings() && items == request.items() {
        return Ok(None);
    }
    Ok(Some(
        request.clone().with_items(items)?.with_settings(settings)?,
    ))
}

fn is_effort(item: &Item) -> bool {
    matches!(item, Item::ConfigurationUpdate(_))
}

/// The level in effect for the reply being requested, by the documented rules: a change
/// directly after a user turn with new input sets that turn's reply; any other change
/// waits for the next user turn with new input. Tool-result-only turns are not new input.
/// https://platform.claude.com/docs/en/build-with-claude/effort#change-effort-mid-conversation-beta
fn flatten_effort(
    initial: Presence<EffortControl>,
    items: &[(ItemId, Item)],
) -> Result<Presence<EffortControl>, CodecError> {
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
                        return Err(CodecError::Unsupported(
                            crate::protocol::failure::CarrierReason::Unavailable,
                        ));
                    }
                };
                if user_turn == Some(true) {
                    active = Presence::Value(effort);
                    pending = None;
                } else {
                    pending = Some(effort);
                }
            }
            Item::Message(message) if message.role == MessageRole::User => {
                user_turn = Some(true);
                if let Some(effort) = pending.take() {
                    active = Presence::Value(effort);
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

/// The tool set available after the last change. Deferred tools never surfaced are
/// withheld; surfaced ones become ordinary definitions.
fn collapse_tools(
    initial: &[ToolDefinition],
    items: &[(ItemId, Item)],
) -> Result<Vec<ToolDefinition>, CodecError> {
    let mut tools: Vec<(ToolDefinition, bool)> = initial
        .iter()
        .map(|tool| {
            let deferred = matches!(tool, ToolDefinition::Function(t) if t.dispatch.defer_loading);
            (tool.clone(), !deferred)
        })
        .collect();
    for (_, item) in items {
        let Item::ToolChange(change) = item else {
            continue;
        };
        let index = tools.iter().position(|(t, _)| t.name() == change.name());
        match (&change.action, index) {
            (ToolChangeAction::AddReference(_), Some(i)) => tools[i].1 = true,
            (ToolChangeAction::Remove(_), Some(i)) => tools[i].1 = false,
            (ToolChangeAction::AddDefinition(tool), Some(i)) => {
                tools[i] = (ToolDefinition::Function((**tool).clone()), true);
            }
            (ToolChangeAction::AddDefinition(tool), None) => {
                tools.push((ToolDefinition::Function((**tool).clone()), true));
            }
            (_, None) => {
                return Err(CodecError::Invalid(
                    crate::protocol::failure::ShapeReason::InvalidValue,
                ));
            }
        }
    }
    Ok(tools
        .into_iter()
        .filter(|(_, available)| *available)
        .map(|(mut tool, _)| {
            if let ToolDefinition::Function(t) = &mut tool {
                t.dispatch.defer_loading = false;
            }
            tool
        })
        .collect())
}
