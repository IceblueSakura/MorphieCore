//! Incremental native semantic mapping over the same strict wire reducer as aggregation.
use crate::protocol::anthropic::messages::sse::MessageSseDecoder;
use crate::protocol::anthropic::messages::{Block, Delta, Event, Message};
use crate::protocol::failure::{RecognitionBasis, SourceFailureEvidence};
use crate::{
    protocol::{
        CodecError, ResponseMetadata,
        fidelity::FidelityRecords,
        native::ReplayTarget,
        stream::{SseError, SseLimits},
    },
    semantic::{task::generation::*, value::Text},
};

pub struct GenerationSseDecoder {
    wire: MessageSseDecoder,
    scope: LocalScope,
    target: ReplayTarget,
    metadata: Option<ResponseMetadata>,
    fidelity: FidelityRecords,
    evidence: crate::lowering::generation::EvidenceDelta,
    part_started: bool,
    json_delta: bool,
    rejected: bool,
}
impl GenerationSseDecoder {
    pub(crate) fn take_intake_evidence(&mut self) -> crate::lowering::generation::EvidenceDelta {
        std::mem::take(&mut self.evidence)
    }
    pub fn new(
        status: u16,
        media: &str,
        limits: SseLimits,
        scope: LocalScope,
        target: ReplayTarget,
    ) -> Result<Self, SseError> {
        Ok(Self {
            wire: MessageSseDecoder::new(status, media, limits)?,
            scope,
            target,
            metadata: None,
            fidelity: Default::default(),
            evidence: Default::default(),
            part_started: false,
            json_delta: false,
            rejected: false,
        })
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.metadata.as_ref()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    fn ids(&self, index: u64) -> (ItemId, PartId) {
        (
            ItemId::scoped(self.scope, index + 1),
            PartId::scoped(self.scope, (index + 1) * (MAX_ITEMS as u64 + 1)),
        )
    }
    pub fn consume(&mut self, bytes: &[u8]) -> Result<(usize, Vec<StreamEvent>), SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        let result = (|| {
            let (used, event) = self.wire.consume_event(bytes)?;
            let events = match event {
                Some(event) => self.map(event)?,
                None => vec![],
            };
            Ok((used, events))
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    fn map(&mut self, event: Event) -> Result<Vec<StreamEvent>, SseError> {
        let mut out = vec![];
        match event {
            Event::MessageStart(message) => {
                if message.model != self.target.model() {
                    return Err(CodecError::invalid().into());
                }
                let usage = crate::protocol::anthropic::messages::accounting::report(
                    &message.usage,
                    UsageStage::Checkpoint,
                );
                self.metadata = Some(ResponseMetadata {
                    delivery: None,
                    storage: None,
                    id: Some(message.id),
                    model: message.model,
                    created: None,
                    context: crate::semantic::context::ResponseContext {
                        reports: crate::semantic::context::ResponseReports::single(
                            crate::semantic::context::ResponseReport {
                                service_class: message.usage.service_class,
                                ..Default::default()
                            },
                        ),
                        ..Default::default()
                    },
                    instruction_fidelity: Default::default(),
                });
                out.push(StreamEvent::Started);
                out.push(StreamEvent::UsageReport(usage));
            }
            Event::BlockStart { index, block } => {
                let (item, part) = self.ids(index);
                self.part_started = false;
                self.json_delta = false;
                let (kind, part_kind, initial) = match block {
                    Block::Text(value) => (
                        ItemKind::Message { phase: None },
                        Some(PartKind::Text),
                        value,
                    ),
                    Block::Thinking { thinking, .. } => (
                        ItemKind::Reasoning,
                        (!thinking.is_empty()).then_some(PartKind::Summary),
                        thinking,
                    ),
                    Block::RedactedThinking(_) => (ItemKind::Reasoning, None, String::new()),
                    Block::ToolUse { id, name, .. } => (
                        ItemKind::ToolCall {
                            format: ArgumentFormat::Json,
                            call_id: text(&id)?,
                            name: text(&name)?,
                            context: Default::default(),
                        },
                        Some(PartKind::StructuredArguments),
                        String::new(),
                    ),
                    _ => {
                        return Err(CodecError::invalid().into());
                    }
                };
                out.push(StreamEvent::ItemStarted {
                    item,
                    kind,
                    replay: None,
                });
                if let Some(kind) = part_kind {
                    out.push(StreamEvent::PartStarted { item, part, kind });
                    self.part_started = true;
                }
                if !initial.is_empty() {
                    out.push(StreamEvent::Delta {
                        item,
                        part,
                        fragment: initial,
                        logprobs: vec![],
                    });
                }
            }
            Event::BlockDelta { index, delta } => {
                let (item, part) = self.ids(index);
                let fragment = match delta {
                    Delta::Signature(_) => return Ok(out),
                    Delta::Thinking(value) => {
                        if value.is_empty() {
                            return Ok(out);
                        }
                        if !self.part_started {
                            out.push(StreamEvent::PartStarted {
                                item,
                                part,
                                kind: PartKind::Summary,
                            });
                            self.part_started = true;
                        }
                        value
                    }
                    Delta::InputJson(value) => {
                        self.json_delta = true;
                        value
                    }
                    Delta::Text(value) => value,
                };
                out.push(StreamEvent::Delta {
                    item,
                    part,
                    fragment,
                    logprobs: vec![],
                });
            }
            Event::BlockStop { index } => {
                let (item, part) = self.ids(index);
                let block =
                    &self.wire.message().ok_or(CodecError::invalid())?.content[index as usize];
                let partial = matches!(block, Block::PartialToolUse { .. });
                if matches!(block, Block::ToolUse { .. }) && !self.json_delta {
                    out.push(StreamEvent::Delta {
                        item,
                        part,
                        fragment: "{}".into(),
                        logprobs: vec![],
                    });
                }
                if self.part_started && !partial {
                    out.push(StreamEvent::ValueFinished { item, part });
                    out.push(StreamEvent::PartFinished { item, part });
                }
                let opaque = match block {
                    Block::Thinking {
                        thinking,
                        signature,
                    } => Some((
                        ReplayFormat::AnthropicMessagesThinking,
                        signature,
                        thinking.as_str(),
                    )),
                    Block::RedactedThinking(data) => {
                        Some((ReplayFormat::AnthropicMessagesRedactedThinking, data, ""))
                    }
                    _ => None,
                };
                let replay = if let Some((format, value, thinking)) = opaque {
                    let replay = ReplayValue::final_value(format, text(value)?);
                    let reasoning = ReasoningItem {
                        parts: if thinking.is_empty() {
                            vec![]
                        } else {
                            vec![(part, ReasoningContent::Summary(text(thinking)?))]
                        },
                        status: ItemLifecycle::Completed,
                        replay: Some(replay.clone()),
                    };
                    self.fidelity
                        .record_replay(item, &reasoning, Some(self.target.origin()))?;
                    Some(ItemReplay {
                        value: replay,
                        origin: Some(self.target.origin()),
                    })
                } else {
                    None
                };
                out.push(StreamEvent::ItemFinished {
                    item,
                    status: if partial {
                        ItemLifecycle::Incomplete
                    } else {
                        ItemLifecycle::Completed
                    },
                    replay,
                });
            }
            Event::MessageStop => {
                let message = self.wire.message().ok_or(CodecError::invalid())?;
                let members: Vec<_> = (0..message.content.len())
                    .map(|n| self.ids(n as u64).0)
                    .collect();
                let group_id = GroupId::new(self.scope, 1);
                let group = if members.is_empty() {
                    MessageEnvelope::empty(group_id, MessageEnvelopeRole::Assistant, None)
                } else {
                    MessageEnvelope::new(group_id, MessageEnvelopeRole::Assistant, members)
                        .map_err(CodecError::from)?
                };
                out.push(StreamEvent::MessageEnvelope(group));
                out.push(StreamEvent::UsageReport(
                    crate::protocol::anthropic::messages::accounting::report(
                        &message.usage,
                        UsageStage::Final,
                    ),
                ));
                let (outcome, progress, details) =
                    crate::protocol::anthropic::messages::terminal::decode(message)?;
                if progress != InteractionProgress::Unreported {
                    out.push(StreamEvent::Progress(progress));
                }
                out.push(StreamEvent::Terminal {
                    terminal: if outcome == Outcome::Incomplete {
                        StreamTerminal::Incomplete
                    } else {
                        StreamTerminal::Completed
                    },
                    details,
                });
            }
            Event::MessageDelta { .. } => {
                let message = self.wire.message().ok_or(CodecError::invalid())?;
                if message.input_transformations {
                    self.evidence.record_input_transformations(
                        crate::lowering::generation::ConversionBoundary::EventDecode,
                    );
                }
                out.push(StreamEvent::UsageReport(
                    crate::protocol::anthropic::messages::accounting::report(
                        &message.usage,
                        UsageStage::Checkpoint,
                    ),
                ));
            }
            Event::Ping => {}
            Event::Error { kind, message } => {
                return Err(CodecError::StreamFailure(SourceFailureEvidence::envelope(
                    super::wire::error_status(&kind),
                    RecognitionBasis::EnvelopeType,
                    None,
                    Some(&kind),
                    Some(&message),
                ))
                .into());
            }
        }
        Ok(out)
    }
    /// The attempt withholds the terminal returned by consume until this succeeds.
    pub fn finish(&mut self) -> Result<Message, SseError> {
        if self.rejected {
            return Err(SseError::Closed);
        }
        let result = self.wire.finish();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
}
fn text(value: &str) -> Result<Text, CodecError> {
    Text::allowing_empty(value, "native event text", MAX_TEXT_BYTES).map_err(|_| CodecError::Limit)
}
