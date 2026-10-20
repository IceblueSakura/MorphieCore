//! Explicit block/container intake and Provider/model compatibility association.
use crate::protocol::anthropic::messages::{
    Block, Media, ResultBlock, ResultContent, Role, wire::request::text_cache_eligible,
};
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
    /// A mid-conversation system message becomes one ordered, text-only instruction.
    pub(super) fn instruction(
        &mut self,
        parts: &[super::CacheMarked<String>],
    ) -> Result<(), CodecError> {
        if parts.is_empty() {
            return Err(CodecError::Invalid("Anthropic empty system message"));
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let id = ItemId::scoped(self.scope, self.items.len() as u64 + 1);
        let base = id.get() * (MAX_ITEMS as u64 + 1);
        let parts = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                Ok(Part {
                    replay: None,
                    id: PartId::scoped(self.scope, base + i as u64),
                    content: text(&part.content)?,
                    cache: part.cache,
                })
            })
            .collect::<Result<_, CodecError>>()?;
        self.items.push((
            id,
            Item::Instruction(Instruction {
                authority: InstructionAuthority::System,
                parts,
                status: None,
            }),
        ));
        Ok(())
    }
    fn push(&mut self, item: Item) -> Result<(), CodecError> {
        if self.items.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let id = ItemId::scoped(self.scope, self.items.len() as u64 + 1);
        self.items.push((id, item));
        Ok(())
    }
    /// One mid-conversation system message, in block order: text runs become
    /// instructions, tool-change blocks ordered changes, and a per-message effort an
    /// ordered configuration update after them.
    pub(super) fn system_message(
        &mut self,
        blocks: &[super::CacheMarked<Block>],
        effort: Option<super::Effort>,
    ) -> Result<(), CodecError> {
        if blocks.is_empty() && effort.is_none() {
            return Err(CodecError::Invalid("Anthropic empty system message"));
        }
        let mut text_run = vec![];
        for block in blocks {
            let action = match &block.content {
                Block::Text(s) => {
                    text_run.push(super::CacheMarked {
                        content: s.clone(),
                        cache: block.cache,
                    });
                    continue;
                }
                Block::ToolAddition(super::ToolTarget::Reference(name)) => {
                    ToolChangeAction::AddReference(text(name)?)
                }
                Block::ToolAddition(super::ToolTarget::Definition(tool)) => {
                    ToolChangeAction::AddDefinition(Box::new(function_tool(tool)?))
                }
                Block::ToolRemoval(name) => ToolChangeAction::Remove(text(name)?),
                _ => return Err(CodecError::Invalid("Anthropic block role")),
            };
            if !text_run.is_empty() {
                self.instruction(&std::mem::take(&mut text_run))?;
            }
            self.push(Item::ToolChange(ToolChange {
                action,
                cache: block.cache,
            }))?;
        }
        if !text_run.is_empty() {
            self.instruction(&text_run)?;
        }
        if let Some(effort) = effort {
            self.push(Item::ConfigurationUpdate(ConfigurationUpdate {
                activation: None,
                effort: Some(EffortControl {
                    scope: EffortScope::Response,
                    level: super::semantic::effort_level(effort),
                }),
            }))?;
        }
        Ok(())
    }
    fn block(&mut self, block: &Block, role: Role) -> Result<ItemId, CodecError> {
        if role == Role::System
            || !block.permits(role)
                && !(role == Role::Assistant && matches!(block, Block::PartialToolUse { .. }))
        {
            return Err(CodecError::Invalid("Anthropic block role"));
        }
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
                    replay: None,
                    id: part,
                    content: ContentPart::Text(text(s)?.into()),
                    cache: None,
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
            Block::Media(media) => Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    replay: None,
                    id: part,
                    content: ContentPart::Resource(self.resource(part, media)?),
                    cache: None,
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
                replay: None,
                call_id: text(call)?,
                cache: None,
                name: text(name)?,
                arguments: ToolArguments::Structured(input.clone()),
                status: ItemLifecycle::Completed,
                context: Default::default(),
            }),
            Block::PartialToolUse {
                id: call,
                name,
                input,
                ..
            } => Item::ToolCall(ToolCall {
                replay: None,
                call_id: text(call)?,
                cache: None,
                name: text(name)?,
                arguments: ToolArguments::StructuredPartial(input.clone()),
                status: ItemLifecycle::Incomplete,
                context: Default::default(),
            }),
            Block::ToolAddition(_) | Block::ToolRemoval(_) => {
                return Err(CodecError::Invalid("Anthropic block role"));
            }
            Block::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Item::ToolResult(ToolResult {
                call_id: Some(text(tool_use_id)?),
                cache: None,
                name: None,
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
                                    Ok(Part {
                                        replay: None,
                                        id: part,
                                        cache: block.cache,
                                        content: match &block.content {
                                            ResultBlock::Text(s) => InputPart::Text(text(s)?),
                                            ResultBlock::Media(m) => {
                                                InputPart::Resource(self.resource(part, m)?)
                                            }
                                        },
                                    })
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
        self.envelope(role, members)
    }
    pub(super) fn request_message(
        &mut self,
        role: Role,
        blocks: &[super::CacheMarked<Block>],
    ) -> Result<(), CodecError> {
        let mut members = Vec::new();
        for block in blocks {
            let id = self.block(&block.content, role)?;
            let item = &mut self.items.last_mut().expect("inserted block").1;
            match item {
                Item::Message(m) => m.parts[0].cache = block.cache,
                Item::ToolCall(c) => c.cache = block.cache,
                Item::ToolResult(r) => r.cache = block.cache,
                _ if block.cache.is_some() => {
                    return Err(CodecError::Unsupported("Anthropic cache owner".into()));
                }
                _ => {}
            }
            members.push(id);
        }
        self.envelope(role, members)
    }
    fn envelope(&mut self, role: Role, members: Vec<ItemId>) -> Result<(), CodecError> {
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
/// One native tool definition; deferred loading and eager input keep their owners.
pub(super) fn function_tool(tool: &super::Tool) -> Result<FunctionTool, CodecError> {
    Ok(FunctionTool {
        name: text(&tool.name)?,
        description: tool.description.value().cloned(),
        parameters: Some(tool.input_schema.clone()),
        strict: tool
            .strict
            .value()
            .map_or(FunctionStrictness::Omitted(StrictDefault::NonStrict), |b| {
                FunctionStrictness::Explicit(*b)
            }),
        output_schema: None,
        dispatch: ToolDispatch {
            defer_loading: tool.defer_loading,
            ..Default::default()
        },
        cache: tool.cache_control.value().copied(),
        eager_input: tool.eager_input_streaming.value().copied(),
    })
}
pub(super) fn validate_cache(request: &GenerationRequest) -> Result<(), CodecError> {
    let mut markers = request
        .tools()
        .iter()
        .filter_map(|t| match t {
            ToolDefinition::Function(t) => t.cache,
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut last = request.tools().last().and_then(|tool| match tool {
        ToolDefinition::Function(tool) => tool.cache,
        _ => None,
    });
    if request
        .settings()
        .instructions
        .value()
        .is_some_and(|text| !text.as_str().is_empty())
    {
        last = None;
    }
    for (_, item) in request.items() {
        match item {
            Item::Instruction(i) => {
                for part in &i.parts {
                    if !text_cache_eligible(part.content.as_str(), part.cache)? {
                        continue;
                    }
                    markers.extend(part.cache);
                    last = part.cache;
                }
            }
            Item::Message(m) => {
                for part in &m.parts {
                    if let ContentPart::Text(text) = &part.content
                        && !text_cache_eligible(text.as_str(), part.cache)?
                    {
                        continue;
                    }
                    markers.extend(part.cache);
                    last = part.cache;
                }
            }
            Item::ToolCall(c) => {
                markers.extend(c.cache);
                last = c.cache;
            }
            Item::ToolResult(r) => {
                if let ToolOutput::Parts(parts) = &r.output {
                    for part in parts {
                        if let InputPart::Text(text) = &part.content {
                            text_cache_eligible(text.as_str(), part.cache)?;
                        }
                        markers.extend(part.cache);
                    }
                }
                markers.extend(r.cache);
                last = r.cache;
            }
            // The marker sits on the block or inside the definition, never both.
            Item::ToolChange(change) => {
                let definition = match &change.action {
                    ToolChangeAction::AddDefinition(tool) => tool.cache,
                    _ => None,
                };
                if change.cache.is_some() && definition.is_some() {
                    return Err(CodecError::Invalid("Anthropic tool change cache"));
                }
                let cache = change.cache.or(definition);
                markers.extend(cache);
                last = cache;
            }
            _ => {}
        }
    }
    super::request::validate_automatic_cache(
        &mut markers,
        last,
        request.settings().automatic_prefix_cache,
    )
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
