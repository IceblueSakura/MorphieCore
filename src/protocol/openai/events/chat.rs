//! Chat chunks use a single candidate and an explicit framing-level DONE boundary.
use super::*;
use serde_json::json;
impl EventDecoder {
    pub(super) fn chat(&mut self, o: &Map<String, Value>) -> Result<Vec<StreamEvent>, CodecError> {
        fields(
            o,
            &[
                "id",
                "object",
                "created",
                "model",
                "choices",
                "usage",
                "system_fingerprint",
                "service_tier",
            ],
        )?;
        if string(o, "object")? != "chat.completion.chunk" {
            return Err(CodecError::Invalid("chunk object"));
        }
        let mut out = vec![];
        if self.metadata.is_none() {
            self.emit(StreamEvent::Started, &mut out)?;
        }
        self.observe_metadata(o)?;
        let choices = o
            .get("choices")
            .and_then(Value::as_array)
            .ok_or(CodecError::Invalid("choices"))?;
        if self.chat_pending.is_some() {
            if !choices.is_empty() {
                if !self.adaptation.rules.repeated_finish_usage || choices.len() != 1 {
                    return Err(CodecError::Invalid("chunk after finish"));
                }
                let choice = object(&choices[0])?;
                fields(choice, &["index", "delta", "finish_reason", "logprobs"])?;
                if choice.get("index").and_then(Value::as_u64) != Some(0)
                    || choice.get("finish_reason").and_then(Value::as_str)
                        != self.chat_finish_reason.as_deref()
                    || choice.get("logprobs").is_some_and(|v| !v.is_null())
                {
                    return Err(CodecError::Invalid("repeated finish"));
                }
                let delta = object(choice.get("delta").ok_or(CodecError::Invalid("delta"))?)?;
                fields(delta, &["role", "content"])?;
                if delta
                    .get("role")
                    .is_some_and(|v| !v.is_null() && v.as_str() != Some("assistant"))
                    || delta
                        .get("content")
                        .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
                {
                    return Err(CodecError::Invalid("content after finish"));
                }
            }
            let usage = super::super::static_response::usage(
                o.get("usage"),
                Profile::Chat,
                &self.adaptation,
                &mut self.fidelity,
            )?
            .ok_or(CodecError::Invalid("usage tail"))?;
            self.emit(StreamEvent::Usage(usage), &mut out)?;
            return Ok(out);
        }
        if choices.len() != 1 {
            return Err(CodecError::Unsupported("candidate count".into()));
        }
        let choice = object(&choices[0])?;
        fields(choice, &["index", "delta", "finish_reason", "logprobs"])?;
        // Usage belongs to the terminal: either this finish chunk or a trailing
        // empty-choices chunk. Anything earlier is rejected, never guessed.
        let terminal_usage = o.get("usage").filter(|v| !v.is_null());
        if terminal_usage.is_some()
            && choice
                .get("finish_reason")
                .filter(|v| !v.is_null())
                .is_none()
        {
            return Err(CodecError::Invalid("early usage"));
        }
        if choice.get("index").and_then(Value::as_u64) != Some(0) {
            return Err(CodecError::Unsupported("candidate".into()));
        }
        let probabilities = super::super::chat_logprobs::read(choice.get("logprobs"))?;
        let delta = object(choice.get("delta").ok_or(CodecError::Invalid("delta"))?)?;
        fields(
            delta,
            &[
                "role",
                "content",
                "refusal",
                "tool_calls",
                "reasoning_content",
                "reasoning",
                "reasoning_details",
                "audio",
            ],
        )?;
        if let Some(role) = delta.get("role").filter(|v| !v.is_null())
            && role.as_str() != Some("assistant")
        {
            return Err(CodecError::Invalid("role"));
        }
        let prior = self.chat_reasoning.and_then(|id| {
            self.state
                .as_ref()?
                .item(id)
                .ok()?
                .parts
                .first()
                .map(|p| p.kind)
        });
        let details = super::super::chat_reasoning::parse(delta, &self.adaptation, prior)?;
        if details.is_none() && delta.contains_key("reasoning") {
            return Err(CodecError::Unsupported("reasoning alias".into()));
        }
        let kind = details
            .as_ref()
            .and_then(|d| d.part.as_ref().map(|(k, _)| *k))
            .unwrap_or(PartKind::ReasoningText);
        let fragment = if let Some((_, s)) = details.as_ref().and_then(|d| d.part.as_ref()) {
            Some(s.as_str())
        } else {
            delta
                .get("reasoning_content")
                .filter(|v| !v.is_null())
                .map(|v| v.as_str().ok_or(CodecError::Invalid("reasoning delta")))
                .transpose()?
        };
        if let Some(fragment) = fragment
            && (!fragment.is_empty() || details.as_ref().is_some_and(|d| d.part.is_some()))
        {
            let item = self.reasoning_owner(&mut out)?;
            let part = match self
                .state()?
                .item(item)?
                .parts
                .first()
                .map(|p| (p.id, p.kind))
            {
                Some((part, old)) if old == kind => part,
                Some(_) => return Err(CodecError::Unsupported("mixed Chat reasoning".into())),
                None => {
                    let part = self.allocate_part()?;
                    self.emit(StreamEvent::PartStarted { item, part, kind }, &mut out)?;
                    part
                }
            };
            self.emit(
                StreamEvent::Delta {
                    item,
                    part,
                    fragment: fragment.into(),
                    logprobs: vec![],
                },
                &mut out,
            )?;
        }
        if let Some((wire_id, data)) = details.and_then(|d| d.responses_encrypted) {
            if self.chat_replay.is_some() {
                return Err(CodecError::Invalid("duplicate reasoning replay"));
            }
            let item = self.reasoning_owner(&mut out)?;
            self.fidelity.record_response_item_id(item, &wire_id)?;
            self.chat_replay = Some(ReasoningReplay {
                value: ReplayValue::final_value(
                    ReplayFormat::ResponsesEncrypted,
                    text(&data, "encrypted reasoning", MAX_TEXT_BYTES)?,
                ),
                origin: self.origin.clone(),
            });
            // The complete encrypted detail closes its reasoning owner before
            // the carrier content; never wait for response finish to invent it.
            for part in self.state()?.item(item)?.parts.clone() {
                self.emit(
                    StreamEvent::ValueFinished {
                        item,
                        part: part.id,
                    },
                    &mut out,
                )?;
                self.emit(
                    StreamEvent::PartFinished {
                        item,
                        part: part.id,
                    },
                    &mut out,
                )?;
            }
            self.emit(
                StreamEvent::ItemFinished {
                    item,
                    status: ItemLifecycle::Completed,
                    replay: self.chat_replay.clone(),
                },
                &mut out,
            )?;
        }
        // One Chat candidate owns an assistant message even when it only contains
        // calls. The owner is allocated at the first message-owned signal so
        // readable reasoning keeps its static order ahead of the message.
        let message_owned = delta
            .get("content")
            .and_then(Value::as_str)
            .is_some_and(|fragment| !fragment.is_empty())
            || delta
                .get("refusal")
                .and_then(Value::as_str)
                .is_some_and(|fragment| !fragment.is_empty())
            || delta.get("tool_calls").is_some_and(|v| !v.is_null())
            || delta.get("audio").is_some_and(|v| !v.is_null());
        if message_owned && self.chat_owner.is_none() {
            let id = self.allocate_item()?;
            self.emit(
                StreamEvent::ItemStarted {
                    item: id,
                    kind: ItemKind::Message { phase: None },
                    replay: None,
                },
                &mut out,
            )?;
            self.chat_owner = Some(id);
        }
        let owner = self.chat_owner;
        for (key, kind) in [("content", PartKind::Text), ("refusal", PartKind::Refusal)] {
            if let Some(v) = delta.get(key).filter(|v| !v.is_null()) {
                let fragment = v.as_str().ok_or(CodecError::Invalid("text delta"))?;
                if fragment.is_empty() && self.chat_owner.is_none() {
                    // An opening empty fragment defers the carrier message so
                    // readable reasoning keeps its static order ahead of it.
                    if self.chat_pending_part.is_none() {
                        self.chat_pending_part = Some(kind);
                    }
                    continue;
                }
                let item = owner.ok_or(CodecError::Invalid("message owner"))?;
                let existing = self
                    .state()?
                    .item(item)?
                    .parts
                    .iter()
                    .find(|p| p.kind != PartKind::Audio)
                    .map(|p| (p.id, p.kind));
                if existing.is_none()
                    && self
                        .state()?
                        .item(item)?
                        .parts
                        .iter()
                        .any(|p| p.kind == PartKind::Audio)
                {
                    return Err(CodecError::Unsupported(
                        "late text or refusal after audio".into(),
                    ));
                }
                let part = if let Some((part, old_kind)) = existing {
                    if kind != old_kind {
                        return Err(CodecError::Unsupported("mixed Chat text/refusal".into()));
                    }
                    part
                } else {
                    let id = self.allocate_part()?;
                    self.emit(
                        StreamEvent::PartStarted {
                            item,
                            part: id,
                            kind,
                        },
                        &mut out,
                    )?;
                    id
                };
                self.emit(
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: fragment.into(),
                        logprobs: vec![],
                    },
                    &mut out,
                )?;
            }
        }
        if let Some(audio) = delta.get("audio").filter(|v| !v.is_null()) {
            let item = self.chat_owner.ok_or(CodecError::Invalid("audio owner"))?;
            if self
                .state()?
                .item(item)?
                .parts
                .iter()
                .any(|p| p.kind == PartKind::Refusal)
            {
                return Err(CodecError::Invalid("audio with refusal"));
            }
            let existing = self
                .state()?
                .item(item)?
                .parts
                .iter()
                .find(|p| p.kind == PartKind::Audio)
                .map(|p| p.id);
            let part = if let Some(part) = existing {
                part
            } else {
                let part = self.allocate_part()?;
                self.emit(
                    StreamEvent::PartStarted {
                        item,
                        part,
                        kind: PartKind::Audio,
                    },
                    &mut out,
                )?;
                part
            };
            let update = super::super::chat_audio::read_update(audio, &mut self.chat_audio_bytes)?;
            self.emit(StreamEvent::AudioDelta { item, part, update }, &mut out)?;
        }
        for (kind, probs) in probabilities {
            let key = if kind == PartKind::Text {
                "content"
            } else {
                "refusal"
            };
            let existing = self.chat_owner.and_then(|item| {
                self.state()
                    .ok()?
                    .item(item)
                    .ok()?
                    .parts
                    .first()
                    .map(|p| (item, p.id, p.kind))
            });
            let (item, part) = if let Some((item, part, old_kind)) = existing {
                if kind != old_kind {
                    return Err(CodecError::Invalid("probability owner"));
                }
                (item, part)
            } else {
                if delta.get(key).and_then(Value::as_str).is_none()
                    && self.chat_pending_part != Some(kind)
                {
                    return Err(CodecError::Invalid("probability owner"));
                }
                let item = if let Some(item) = self.chat_owner {
                    item
                } else {
                    let item = self.allocate_item()?;
                    self.emit(
                        StreamEvent::ItemStarted {
                            item,
                            kind: ItemKind::Message { phase: None },
                            replay: None,
                        },
                        &mut out,
                    )?;
                    self.chat_owner = Some(item);
                    item
                };
                let part = self.allocate_part()?;
                self.emit(StreamEvent::PartStarted { item, part, kind }, &mut out)?;
                self.chat_pending_part = None;
                (item, part)
            };
            if probs.is_empty() {
                if self.state()?.part(item, part)?.logprobs.is_absent() {
                    self.emit(
                        StreamEvent::LogprobsSnapshot {
                            item,
                            part,
                            logprobs: probs,
                        },
                        &mut out,
                    )?;
                }
            } else {
                self.emit(
                    StreamEvent::Delta {
                        item,
                        part,
                        fragment: String::new(),
                        logprobs: probs,
                    },
                    &mut out,
                )?;
            }
        }
        if let Some(calls) = delta.get("tool_calls").filter(|v| !v.is_null()) {
            for call in calls.as_array().ok_or(CodecError::Invalid("tool calls"))? {
                let call = object(call)?;
                fields(call, &["index", "id", "type", "function"])?;
                let n = index(call, "index")?;
                match call.get("type") {
                    None | Some(Value::Null) => {}
                    Some(Value::String(kind)) if kind == "function" => {}
                    _ => return Err(CodecError::Unsupported("tool kind".into())),
                }
                let empty = Map::new();
                let f = match call.get("function") {
                    None | Some(Value::Null) => &empty,
                    Some(value) => object(value)?,
                };
                fields(f, &["name", "arguments"])?;
                let item = if n == self.chat_calls.len() {
                    let item = self.allocate_item()?;
                    let kind = ItemKind::ToolCall {
                        format: ArgumentFormat::Raw,
                        call_id: text(string(call, "id")?, "call id", 256)?,
                        name: text(string(f, "name")?, "name", 128)?,
                        message: self.chat_owner,
                        context: CallContext::default(),
                    };
                    self.emit(
                        StreamEvent::ItemStarted {
                            item,
                            kind,
                            replay: None,
                        },
                        &mut out,
                    )?;
                    let part = self.allocate_part()?;
                    self.emit(
                        StreamEvent::PartStarted {
                            item,
                            part,
                            kind: PartKind::Arguments,
                        },
                        &mut out,
                    )?;
                    self.chat_calls.push(item);
                    item
                } else {
                    *self
                        .chat_calls
                        .get(n)
                        .ok_or(CodecError::Invalid("tool index"))?
                };
                let ItemKind::ToolCall { call_id, name, .. } = &self.state()?.item(item)?.kind
                else {
                    return Err(CodecError::Invalid("call"));
                };
                // Continuation chunks reuse null for already-announced identity;
                // only conflicting non-null values are rejected.
                let same_identity = |value: Option<&Value>, expected: &str| match value {
                    None | Some(Value::Null) => Ok(()),
                    Some(Value::String(s)) if s == expected => Ok(()),
                    _ => Err(CodecError::Invalid("call identity")),
                };
                same_identity(call.get("id"), call_id.as_str())?;
                same_identity(f.get("name"), name.as_str())?;
                if let Some(v) = f.get("arguments").filter(|v| !v.is_null()) {
                    let fragment = v.as_str().ok_or(CodecError::Invalid("arguments"))?;
                    let part = self.state()?.item(item)?.parts[0].id;
                    self.emit(
                        StreamEvent::Delta {
                            item,
                            part,
                            fragment: fragment.into(),
                            logprobs: vec![],
                        },
                        &mut out,
                    )?;
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").filter(|v| !v.is_null()) {
            let owner = if let Some(id) = self.chat_owner {
                id
            } else {
                let id = self.allocate_item()?;
                self.emit(
                    StreamEvent::ItemStarted {
                        item: id,
                        kind: ItemKind::Message { phase: None },
                        replay: None,
                    },
                    &mut out,
                )?;
                self.chat_owner = Some(id);
                id
            };
            // A deferred opening fragment materializes its empty part only when
            // no other content won the part, matching static `content: ""`.
            if let Some(kind) = self.chat_pending_part.take()
                && self.state()?.item(owner)?.parts.is_empty()
            {
                let part = self.allocate_part()?;
                self.emit(
                    StreamEvent::PartStarted {
                        item: owner,
                        part,
                        kind,
                    },
                    &mut out,
                )?;
            }
            let terminal = match reason.as_str() {
                Some("stop") if self.chat_calls.is_empty() => StreamTerminal::Completed,
                Some("tool_calls") if !self.chat_calls.is_empty() => StreamTerminal::Completed,
                Some("length" | "content_filter") => StreamTerminal::Incomplete,
                _ => return Err(CodecError::Unsupported("finish reason".into())),
            };
            let items = self.state()?.items().to_vec();
            for item in items {
                if item.status.is_some() {
                    continue;
                }
                for part in item.parts {
                    self.emit(
                        StreamEvent::ValueFinished {
                            item: item.id,
                            part: part.id,
                        },
                        &mut out,
                    )?;
                    if part.kind == PartKind::Audio {
                        let StreamPartValue::Audio(audio) =
                            self.state()?.part(item.id, part.id)?.value()
                        else {
                            return Err(CodecError::Invalid("audio value"));
                        };
                        let content = ContentPart::Audio(audio.clone());
                        self.fidelity
                            .record_audio(part.id, &content, self.origin.clone())?;
                    }
                    self.emit(
                        StreamEvent::PartFinished {
                            item: item.id,
                            part: part.id,
                        },
                        &mut out,
                    )?;
                }
                self.emit(
                    StreamEvent::ItemFinished {
                        item: item.id,
                        status: if terminal == StreamTerminal::Completed {
                            ItemLifecycle::Completed
                        } else {
                            ItemLifecycle::Incomplete
                        },
                        replay: if Some(item.id) == self.chat_reasoning {
                            self.chat_replay.clone()
                        } else {
                            None
                        },
                    },
                    &mut out,
                )?;
            }
            self.chat_finish_reason = reason.as_str().map(str::to_owned);
            self.chat_pending = Some((
                terminal,
                if terminal == StreamTerminal::Incomplete {
                    TerminalDetails {
                        error: None,
                        incomplete: Some(if reason.as_str() == Some("content_filter") {
                            IncompleteReason::ContentFilter
                        } else {
                            IncompleteReason::MaxOutputTokens
                        }),
                    }
                } else {
                    TerminalDetails::default()
                },
            ));
            if let Some(value) = terminal_usage {
                let usage = super::super::static_response::usage(
                    Some(value),
                    Profile::Chat,
                    &self.adaptation,
                    &mut self.fidelity,
                )?
                .ok_or(CodecError::Invalid("usage tail"))?;
                self.emit(StreamEvent::Usage(usage), &mut out)?;
            }
        }
        Ok(out)
    }
    fn reasoning_owner(&mut self, out: &mut Vec<StreamEvent>) -> Result<ItemId, CodecError> {
        if let Some(item) = self.chat_reasoning {
            return Ok(item);
        }
        if self.chat_owner.is_some() {
            return Err(CodecError::Unsupported("late Chat reasoning owner".into()));
        }
        let item = self.allocate_item()?;
        self.emit(
            StreamEvent::ItemStarted {
                item,
                kind: ItemKind::Reasoning,
                replay: None,
            },
            out,
        )?;
        self.chat_reasoning = Some(item);
        Ok(item)
    }
}
impl EventEncoder {
    fn chunk(&self, mut delta: Value, finish: Value) -> Value {
        if self.contract.adaptation.rules.reasoning_alias {
            super::super::adapter_shapes::encode_message(&mut delta);
        }
        let mut value = json!({"id":self.metadata.id,"object":"chat.completion.chunk","created":self.metadata.created,"model":self.metadata.model,"choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":null});
        put_presence(
            value.as_object_mut().expect("object"),
            "system_fingerprint",
            &self.metadata.context.system_fingerprint,
            |v| json!(v),
        );
        put_presence(
            value.as_object_mut().expect("object"),
            "service_tier",
            &self.metadata.context.execution.service_tier,
            |v| json!(v),
        );
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
    pub(super) fn chat_probabilities(
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
        super::super::chat_logprobs::carrier(p.kind, &probs)
    }
    pub(super) fn chat(
        &mut self,
        event: &StreamEvent,
        probabilities: Option<Value>,
    ) -> Result<Vec<Value>, CodecError> {
        let mut chunks = match event {
            StreamEvent::Started=>vec![self.chunk(json!({"role":"assistant"}),Value::Null)],
            StreamEvent::AudioDelta{update,..}=>super::super::chat_audio::write_update(update,&mut self.chat_audio_pending).into_iter().map(|v|self.chunk(json!({"audio":v}),Value::Null)).collect(),
            StreamEvent::ItemStarted{item,kind:ItemKind::ToolCall{call_id,name,..},..}=>vec![self.chunk(json!({"tool_calls":[{"index":self.call_index(*item)?,"id":call_id.as_str(),"type":"function","function":{"name":name.as_str(),"arguments":""}}]}),Value::Null)],
            StreamEvent::PartStarted{kind:PartKind::Text,..}=>vec![self.chunk(json!({"content":""}),Value::Null)],
            StreamEvent::PartStarted{kind:PartKind::Refusal,..}=>vec![self.chunk(json!({"refusal":""}),Value::Null)],
            StreamEvent::PartStarted{kind:PartKind::ReasoningText,..}=>vec![self.chunk(json!({"reasoning_content":""}),Value::Null)],
            StreamEvent::PartStarted{kind:PartKind::Summary,..} if self.contract.adaptation.rules.structured_chat_reasoning => vec![self.chunk(json!({"reasoning_details":[{"type":"reasoning.summary","summary":"","format":"openai-responses-v1","index":0}]}),Value::Null)],
            StreamEvent::ItemFinished{item,replay:Some(replay),..} if self.contract.adaptation.rules.structured_chat_reasoning => {
                let state=self.state()?.item(*item)?;
                let id=self.fidelity.response_item_id(*item).map(str::to_owned).unwrap_or_else(||format!("item_{}",item.get()));
                vec![self.chunk(json!({"reasoning_details":[{"type":"reasoning.encrypted","data":replay.value.as_str(),"format":"openai-responses-v1","index":state.parts.len(),"id":id}]}),Value::Null)]
            }
            StreamEvent::Delta{item,part,fragment,..}=>{
                let delta=match self.state()?.part(*item,*part)?.kind{PartKind::Text=>json!({"content":fragment}),PartKind::Refusal=>json!({"refusal":fragment}),PartKind::Arguments=>json!({"tool_calls":[{"index":self.call_index(*item)?,"function":{"arguments":fragment}}]}),PartKind::ReasoningText=>json!({"reasoning_content":fragment}),PartKind::Summary if self.contract.adaptation.rules.structured_chat_reasoning => json!({"reasoning_details":[{"type":"reasoning.summary","summary":fragment,"format":"openai-responses-v1","index":0}]}),_=>return Err(CodecError::Unsupported("Chat reasoning".into()))};vec![self.chunk(delta,Value::Null)]
            }
            StreamEvent::Terminal{..}=>{
                let response=materialize(self.state()?)?;
                crate::lowering::generation::lower_response(&response,&self.fidelity,&self.metadata,Profile::Chat,self.contract.clone()).map_err(|_|CodecError::Unsupported("Chat terminal".into()))?;
                let finish=super::super::terminal::chat_finish(&response)?;
                let mut chunks=vec![self.chunk(json!({}),json!(finish))];
                if let Some(usage)=response.usage(){
                    let mut v=if self.contract.adaptation.rules.repeated_finish_usage {
                        self.chunk(json!({}),json!(finish))
                    } else {
                        let mut v=self.chunk(json!({}),Value::Null);v["choices"]=json!([]);v
                    };
                    v["usage"]=super::super::static_response::encode_usage(usage,Profile::Chat,&self.contract.adaptation.rules);chunks.push(v);
                }
                super::super::envelope::write_response_extras(&self.fidelity,Profile::Chat,&self.contract.adaptation,&response,&self.metadata.id,chunks.last_mut().expect("terminal chunk").as_object_mut().expect("object"));
                chunks
            }
            _=>vec![],
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
