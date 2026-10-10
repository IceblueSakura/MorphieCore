//! Explicit block/container intake and trusted in-process dependency association.
use super::{Block, Role, TextContent as WireText, target::ReplayTarget};
use crate::{
    protocol::{CodecError, fidelity::FidelityRecords},
    semantic::{task::generation::*, value::Text},
};
pub(super) fn text(s: &str) -> Result<Text, CodecError> {
    Text::allowing_empty(s, "native text", MAX_TEXT_BYTES).map_err(|_| CodecError::Limit)
}
pub(super) struct Intake {
    scope: LocalScope,
    pub(super) items: Vec<(ItemId, Item)>,
    pub(super) groups: Vec<MessageEnvelope>,
}
impl Intake {
    pub(super) fn new(scope: LocalScope) -> Self {
        Self {
            scope,
            items: vec![],
            groups: vec![],
        }
    }
    fn block(&mut self, block: &Block, role: Role) -> Result<ItemId, CodecError> {
        if self.items.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let id = ItemId::scoped(self.scope, self.items.len() as u64 + 1);
        let part = PartId::scoped(self.scope, id.get() * (MAX_ITEMS as u64 + 1));
        let item = match block {
            Block::Text(s) => Item::Message(Message {
                role: if role == Role::User {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                parts: vec![Part {
                    id: part,
                    content: ContentPart::Text(text(s)?.into()),
                    replay: None,
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
            Block::Thinking {
                thinking,
                signature,
            } => Item::Reasoning(ReasoningItem {
                parts: if thinking.is_empty() {
                    vec![]
                } else {
                    vec![(part, ReasoningContent::Summary(text(thinking)?))]
                },
                status: ItemLifecycle::Completed,
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::AnthropicMessagesThinking,
                    text(signature)?,
                )),
            }),
            Block::ToolUse {
                id: call,
                name,
                input,
                ..
            } => Item::ToolCall(ToolCall {
                call_id: text(call)?,
                name: text(name)?,
                arguments: ToolArguments::Structured(input.clone()),
                status: ItemLifecycle::Completed,
                context: Default::default(),
            }),
            Block::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Item::ToolResult(ToolResult {
                call_id: text(tool_use_id)?,
                output: match content {
                    WireText::Text(s) => ToolOutput::Text(s.clone()),
                    WireText::Blocks(parts) => {
                        // Parts occupy a separate scoped range, independent of item counters.
                        let base = (self.items.len() as u64 + 1) * (MAX_ITEMS as u64 + 1);
                        ToolOutput::Parts(
                            parts
                                .iter()
                                .enumerate()
                                .map(|(i, s)| {
                                    Ok((
                                        PartId::scoped(self.scope, base + i as u64),
                                        ToolResultPart::Text(text(s)?),
                                    ))
                                })
                                .collect::<Result<_, CodecError>>()?,
                        )
                    }
                },
                is_error: is_error.value().copied(),
                execution: None,
                status: None,
                context: Default::default(),
            }),
        };
        self.items.push((id, item));
        Ok(id)
    }
    pub(super) fn message(&mut self, role: Role, blocks: &[Block]) -> Result<(), CodecError> {
        let members = blocks
            .iter()
            .map(|b| self.block(b, role))
            .collect::<Result<Vec<_>, _>>()?;
        let id = GroupId::new(self.scope, self.groups.len() as u64 + 1);
        let role = if role == Role::User {
            MessageEnvelopeRole::User
        } else {
            MessageEnvelopeRole::Assistant
        };
        self.groups.push(if members.is_empty() {
            MessageEnvelope::empty(id, role, None)
        } else {
            MessageEnvelope::new(id, role, members)?
        });
        Ok(())
    }
}
pub(super) fn bind(
    fidelity: &mut FidelityRecords,
    request: &GenerationRequest,
    owners: &[(ItemId, Item)],
    target: &ReplayTarget,
) -> Result<(), CodecError> {
    for (id, item) in owners {
        if let Item::Reasoning(r) = item {
            fidelity.record_replay(*id, r, Some(target.origin()))?;
            let proof = RequestDependencyProof::capture(
                request,
                HistoryDependency::PrefixThrough(*id),
                SettingsDependency::only(SettingsField::Instructions)
                    .union(SettingsDependency::only(SettingsField::Tools)),
            )?;
            fidelity.bind_replay_dependency(*id, proof, request)?;
        }
    }
    Ok(())
}
