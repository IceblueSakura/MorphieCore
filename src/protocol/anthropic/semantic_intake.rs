//! Explicit block/container intake and Provider/model compatibility association.
use super::{Block, Media, ResultBlock, ResultContent, Role};
use crate::protocol::native::ReplayTarget;
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
    pub(super) resources: ResourceTable,
    /// Parts whose image rejects downsizing; the caller records them as fidelity.
    pub(super) strict_images: Vec<PartId>,
}
impl Intake {
    pub(super) fn new(scope: LocalScope) -> Self {
        Self {
            scope,
            items: vec![],
            groups: vec![],
            resources: ResourceTable::default(),
            strict_images: vec![],
        }
    }
    fn resource(&mut self, part: PartId, media: &Media) -> Result<ResourceUse, CodecError> {
        if matches!(
            media,
            Media::Image {
                reject_oversized: true,
                ..
            }
        ) {
            self.strict_images.push(part);
        }
        let resource = media.resource()?;
        let id = ResourceId::scoped(self.scope, self.resources.len() as u64 + 1);
        self.resources = std::mem::take(&mut self.resources).insert(id, resource.location)?;
        Ok(ResourceUse {
            id,
            description: resource.description,
        })
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
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
            Block::Media(_) if role != Role::User => {
                return Err(CodecError::Invalid("Anthropic block role"));
            }
            Block::Media(media) => Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    id: part,
                    content: ContentPart::Resource(self.resource(part, media)?),
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
            Block::RedactedThinking(data) => Item::Reasoning(ReasoningItem {
                parts: vec![],
                status: ItemLifecycle::Completed,
                replay: Some(ReplayValue::final_value(
                    ReplayFormat::AnthropicMessagesRedactedThinking,
                    text(data)?,
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
                    ResultContent::Text(s) => ToolOutput::Text(s.clone()),
                    ResultContent::Blocks(parts) => {
                        // Parts occupy a separate scoped range, independent of item counters.
                        let base = (self.items.len() as u64 + 1) * (MAX_ITEMS as u64 + 1);
                        ToolOutput::Parts(
                            parts
                                .iter()
                                .enumerate()
                                .map(|(i, block)| {
                                    let part = PartId::scoped(self.scope, base + i as u64);
                                    Ok((
                                        part,
                                        match block {
                                            ResultBlock::Text(s) => InputPart::Text(text(s)?),
                                            ResultBlock::Media(m) => {
                                                InputPart::Resource(self.resource(part, m)?)
                                            }
                                        },
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
    owners: &[(ItemId, Item)],
    target: &ReplayTarget,
) -> Result<(), CodecError> {
    for (id, item) in owners {
        if let Item::Reasoning(r) = item {
            fidelity.record_replay(*id, r, Some(target.origin()))?;
        }
    }
    Ok(())
}
