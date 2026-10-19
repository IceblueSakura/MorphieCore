//! Outgoing Chat chunks: message ownership, probabilities, deltas and finish reasons.
use crate::protocol::openai::events::*;
use serde_json::json;
impl EventEncoder {
    fn chunk(&self, mut delta: Value, finish: Value) -> Value {
        if self.contract.adaptation.rules.reasoning_alias {
            crate::protocol::openai::dialect::adapter_shapes::encode_message(&mut delta);
        }
        let created = self
            .metadata
            .created
            .as_ref()
            .expect("validated Chat timestamp");
        let mut value = json!({
            "id": self.metadata.id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": self.metadata.model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": null,
        });
        put_presence(
            value.as_object_mut().expect("object"),
            "system_fingerprint",
            &self.metadata.context.system_fingerprint,
            |v| json!(v),
        );
        crate::protocol::openai::service::write_class(
            value.as_object_mut().expect("object"),
            &self.metadata.context.service_class,
            Profile::Chat,
        )
        .expect("validated reported service");
        value
    }
    fn call_index(&self, item: ItemId) -> Result<usize, CodecError> {
        self.state()?
            .items()
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::ToolCall { .. }))
            .position(|i| i.id == item)
            .ok_or(CodecError::Invalid("call index"))
    }
    /// Compute before reduction: aggregate snapshots must not replay emitted probabilities.
    pub(in crate::protocol::openai::events) fn chat_probabilities(
        &self,
        event: &StreamEvent,
    ) -> Result<Option<Value>, CodecError> {
        let (item, part, probs) = match event {
            StreamEvent::Delta {
                item,
                part,
                logprobs,
                ..
            } if !logprobs.is_empty() => (
                *item,
                *part,
                crate::semantic::value::Presence::Value(logprobs.clone()),
            ),
            StreamEvent::LogprobsSnapshot {
                item,
                part,
                logprobs,
            } => (
                *item,
                *part,
                crate::semantic::value::Presence::Value(logprobs.clone()),
            ),
            StreamEvent::TextMetadata {
                item,
                part,
                logprobs,
                ..
            } => (*item, *part, logprobs.clone()),
            _ => return Ok(None),
        };
        let p = self.state()?.part(item, part)?;
        if !matches!(event, StreamEvent::Delta { .. }) && (probs.is_absent() || p.logprobs == probs)
        {
            return Ok(None);
        }
        crate::protocol::openai::chat::logprobs::carrier(p.kind, &probs)
    }
    /// Position of a reasoning detail in the message: earlier items' parts and replay
    /// values, then this item's parts before `part`, or all of them for its replay.
    fn detail_index(&self, item: ItemId, part: Option<PartId>) -> Result<usize, CodecError> {
        let mut index = 0;
        for i in self.state()?.items() {
            if !matches!(i.kind, ItemKind::Reasoning) {
                continue;
            }
            if i.id == item {
                return Ok(index
                    + match part {
                        Some(part) => i
                            .parts
                            .iter()
                            .position(|p| p.id == part)
                            .ok_or(CodecError::Invalid("part identity"))?,
                        None => i.parts.len(),
                    });
            }
            index += i.parts.len() + usize::from(i.replay.is_some());
        }
        Err(CodecError::Invalid("item identity"))
    }
    /// Readable deltas carry both the view and the detail, as the static message does.
    fn chat_delta(
        &mut self,
        item: ItemId,
        part: PartId,
        fragment: &str,
        structured: bool,
    ) -> Result<Value, CodecError> {
        Ok(match self.state()?.part(item, part)?.kind {
            PartKind::Text => json!({ "content": fragment }),
            PartKind::Refusal => json!({ "refusal": fragment }),
            PartKind::Arguments => {
                let call = json!({
                    "index": self.call_index(item)?,
                    "function": {"arguments": fragment},
                });
                json!({ "tool_calls": [call] })
            }
            PartKind::ReasoningText if !structured => json!({ "reasoning_content": fragment }),
            PartKind::ReasoningText => {
                let detail = json!({
                    "type": "reasoning.text",
                    "text": fragment,
                    "format": "openai-responses-v1",
                    "index": self.detail_index(item, Some(part))?,
                });
                json!({ "reasoning_content": fragment, "reasoning_details": [detail] })
            }
            PartKind::Summary if structured => {
                let detail = json!({
                    "type": "reasoning.summary",
                    "summary": fragment,
                    "format": "openai-responses-v1",
                    "index": self.detail_index(item, Some(part))?,
                });
                json!({ "reasoning_content": fragment, "reasoning_details": [detail] })
            }
            _ => return Err(CodecError::Unsupported("Chat reasoning".into())),
        })
    }
    fn chat_terminal(&mut self) -> Result<Vec<Value>, CodecError> {
        let response = materialize(self.state()?)?;
        let target = crate::protocol::openai::lowering::lower_response(
            &response,
            &self.fidelity,
            &self.metadata,
            Profile::Chat,
            self.contract.clone(),
        )
        .map_err(|_| CodecError::Unsupported("Chat terminal".into()))?;
        let finish = crate::protocol::openai::terminal::chat_finish(&response)?;
        let mut chunks = vec![];
        // The owned replay manifest precedes the finishing chunk of its message.
        if let Some(carrier) = self.attachment.take() {
            let detail = crate::protocol::attachments::chat_detail(carrier.as_str());
            chunks.push(self.chunk(json!({ "reasoning_details": [detail] }), Value::Null));
        }
        chunks.push(self.chunk(json!({}), json!(finish)));
        if let Some(usage) = target.semantic().usage() {
            let mut v = if self.contract.adaptation.rules.repeated_finish_usage {
                self.chunk(json!({}), json!(finish))
            } else {
                let mut v = self.chunk(json!({}), Value::Null);
                v["choices"] = json!([]);
                v
            };
            v["usage"] = crate::protocol::openai::static_response::encode_usage(
                usage,
                Profile::Chat,
                &self.contract.adaptation.rules,
            );
            chunks.push(v);
        }
        let last = chunks.last_mut().expect("terminal chunk");
        crate::protocol::openai::responses::envelope::write_response_extras(
            &self.fidelity,
            Profile::Chat,
            &self.contract.adaptation,
            target.semantic(),
            self.metadata.required_id()?,
            last.as_object_mut().expect("object"),
        );
        for mut stage in target.projection().iter().cloned() {
            stage.direction =
                crate::protocol::openai::lowering::projection::ProjectionDirection::Event;
            if !self.projection.contains(&stage) {
                self.projection.push(stage);
            }
        }
        Ok(chunks)
    }
    pub(in crate::protocol::openai::events) fn chat(
        &mut self,
        event: &StreamEvent,
        probabilities: Option<Value>,
    ) -> Result<Vec<Value>, CodecError> {
        let structured = self.contract.adaptation.rules.structured_chat_reasoning;
        let mut chunks = match event {
            StreamEvent::Started => vec![self.chunk(json!({"role": "assistant"}), Value::Null)],
            StreamEvent::AudioDelta { update, .. } => {
                crate::protocol::openai::chat::audio::write_update(
                    update,
                    &mut self.chat_audio_pending,
                )
                .into_iter()
                .map(|v| self.chunk(json!({ "audio": v }), Value::Null))
                .collect()
            }
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::ToolCall { call_id, name, .. },
                ..
            } => {
                let call = json!({
                    "index": self.call_index(*item)?,
                    "id": call_id.as_str(),
                    "type": "function",
                    "function": {"name": name.as_str(), "arguments": ""},
                });
                vec![self.chunk(json!({ "tool_calls": [call] }), Value::Null)]
            }
            StreamEvent::PartStarted {
                kind: PartKind::Text,
                ..
            } => vec![self.chunk(json!({"content": ""}), Value::Null)],
            StreamEvent::PartStarted {
                kind: PartKind::Refusal,
                ..
            } => vec![self.chunk(json!({"refusal": ""}), Value::Null)],
            // Some consumers open a reasoning block per delta, so no empty opener.
            StreamEvent::ItemFinished {
                item,
                replay: Some(replay),
                ..
            } if structured => {
                let id = self
                    .fidelity
                    .response_item_id(*item)
                    .map(str::to_owned)
                    .unwrap_or_else(|| crate::protocol::fidelity::generated_item_id(*item));
                let detail = json!({
                    "type": "reasoning.encrypted",
                    "data": replay.value.as_str(),
                    "format": "openai-responses-v1",
                    "index": self.detail_index(*item, None)?,
                    "id": id,
                });
                vec![self.chunk(json!({ "reasoning_details": [detail] }), Value::Null)]
            }
            StreamEvent::Delta {
                item,
                part,
                fragment,
                ..
            } => {
                let delta = self.chat_delta(*item, *part, fragment, structured)?;
                vec![self.chunk(delta, Value::Null)]
            }
            StreamEvent::Terminal { .. } => self.chat_terminal()?,
            _ => vec![],
        };
        if let Some(probabilities) = probabilities {
            if chunks.is_empty() {
                chunks.push(self.chunk(json!({}), Value::Null));
            }
            chunks[0]["choices"][0]["logprobs"] = probabilities;
        }
        Ok(chunks)
    }
}
