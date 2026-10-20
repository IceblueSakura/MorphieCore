//! Incoming payload driver: state, rejection, metadata and profile dispatch.
mod chat;
mod responses;
use crate::protocol::openai::events::*;
use serde_json::json;

pub struct EventDecoder {
    pub(super) profile: Profile,
    pub(super) adaptation: crate::protocol::openai::dialect::adaptation::Adaptation,
    pub(super) state: Option<StreamState>,
    pub(super) fidelity: FidelityRecords,
    pub(super) metadata: Option<ResponseMetadata>,
    native_reports: Option<crate::semantic::context::ResponseReports>,
    native_instructions: Option<Vec<(ItemId, Item)>>,
    native_configurations: Vec<(usize, ConfigurationUpdate)>,
    pub(super) origin: Option<ReplayOrigin>,
    pub(super) ids: Vec<ItemId>,
    pub(super) next_part: u64,
    pub(super) poisoned: bool,
    sequence: Option<u64>,
    events: usize,
    ignored: usize,
    queued_creation_pending: bool,
    pub(super) chat_pending: Option<(StreamTerminal, TerminalDetails)>,
    pub(super) chat_finish_reason: Option<String>,
    chat_extras: crate::protocol::openai::dialect::adapter_shapes::Extras,
    pub(super) chat_owner: Option<ItemId>,
    pub(super) chat_calls: Vec<ItemId>,
    pub(super) chat_reasoning: Option<ItemId>,
    /// Message position of the open reasoning item's first detail.
    pub(super) chat_detail_base: u64,
    pub(super) chat_pending_part: Option<PartKind>,
    pub(super) chat_audio_bytes: crate::protocol::openai::chat::audio::ChunkBytes,
}
impl EventDecoder {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            adaptation: Default::default(),
            state: Some(StreamState::new()),
            fidelity: FidelityRecords::default(),
            metadata: None,
            native_reports: None,
            native_instructions: None,
            native_configurations: vec![],
            origin: None,
            ids: vec![],
            next_part: 0,
            poisoned: false,
            sequence: None,
            events: 0,
            ignored: 0,
            queued_creation_pending: false,
            chat_pending: None,
            chat_finish_reason: None,
            chat_extras: Default::default(),
            chat_owner: None,
            chat_calls: vec![],
            chat_reasoning: None,
            chat_detail_base: 0,
            chat_pending_part: None,
            chat_audio_bytes: Default::default(),
        }
    }
    /// Unregistered Responses event types skipped so far; names are never kept.
    pub fn ignored_events(&self) -> usize {
        self.ignored
    }
    pub(crate) fn permits_responses_done(&self) -> bool {
        self.profile == Profile::Responses && self.adaptation.rules.responses_done_marker
    }
    pub(crate) fn validate_stream_payload(&self, value: &Value) -> Result<(), CodecError> {
        crate::protocol::openai::responses::envelope::validate_stream_payload_with(
            value,
            &self.adaptation,
            DecodeDirection::Upstream,
        )
    }
    pub(crate) fn profile(&self) -> Profile {
        self.profile
    }
    pub(crate) fn with_adaptation(
        mut self,
        adaptation: crate::protocol::openai::dialect::adaptation::Adaptation,
    ) -> Self {
        self.adaptation = adaptation;
        self
    }
    pub fn with_replay_origin(mut self, origin: ReplayOrigin) -> Self {
        self.origin = Some(origin);
        self
    }
    pub fn push(&mut self, payload: &Value) -> Result<Vec<StreamEvent>, CodecError> {
        if self.poisoned || self.state.as_ref().is_none_or(|s| s.terminal().is_some()) {
            self.poisoned = true;
            return Err(CodecError::Invalid("event after terminal or rejection"));
        }
        self.events += 1;
        let result = (|| {
            if self.events > 1_000_000 {
                return Err(CodecError::Limit);
            }
            bounded(payload)?;
            let (normalized, extras) = if self.profile == Profile::Responses {
                if let Some(snapshot) = payload.get("response")
                    && (self.adaptation.rules.routing_extras
                        || self.adaptation.rules.responses_billing_view
                        || self.adaptation.rules.responses_product_accounting
                        || self.adaptation.rules.responses_access_programs
                        || self.adaptation.rules.responses_zero_tool_accounting
                        || self.adaptation.rules.responses_inactive_reports
                        || self.adaptation.rules.responses_usage_attribution
                        || self.adaptation.rules.responses_context_accounting
                        || self.adaptation.rules.responses_inactive_state
                        || self.adaptation.rules.responses_reasoning_text_shorthand)
                {
                    let (snapshot, extras) =
                        crate::protocol::openai::dialect::adapter_shapes::decode(
                            snapshot,
                            self.profile,
                            &self.adaptation,
                            DecodeDirection::Upstream,
                        )?;
                    let mut mapped = payload.clone();
                    mapped["response"] = snapshot.into_owned();
                    (std::borrow::Cow::Owned(mapped), extras)
                } else if (self.adaptation.rules.responses_reasoning_format
                    || self.adaptation.rules.responses_reasoning_text_shorthand)
                    && payload.get("item").is_some()
                {
                    let mut mapped = payload.clone();
                    if self.adaptation.rules.responses_reasoning_format {
                        crate::protocol::openai::dialect::adapter_shapes::reasoning_marker(
                            &mut mapped["item"],
                        )?;
                    }
                    if self.adaptation.rules.responses_reasoning_text_shorthand {
                        crate::protocol::openai::dialect::adapter_shapes::reasoning_text_shorthand(
                            &mut mapped["item"],
                        )?;
                    }
                    (std::borrow::Cow::Owned(mapped), Default::default())
                } else {
                    (std::borrow::Cow::Borrowed(payload), Default::default())
                }
            } else {
                crate::protocol::openai::dialect::adapter_shapes::decode(
                    payload,
                    self.profile,
                    &self.adaptation,
                    DecodeDirection::Upstream,
                )?
            };
            let o = object(&normalized)?;
            if let Some(padding) = o.get("obfuscation")
                && (self.profile != Profile::Responses
                    || !string(o, "type")?.ends_with(".delta")
                    || !padding.as_str().is_some_and(|s| s.len() <= 4096))
            {
                return Err(CodecError::Invalid("obfuscation"));
            }
            if let Some(n) = o.get("sequence_number") {
                let n = n.as_u64().ok_or(CodecError::Invalid("sequence"))?;
                if self.sequence.is_some_and(|old| n <= old) {
                    return Err(CodecError::Invalid("sequence"));
                }
                self.sequence = Some(n);
            }
            let events = if self.profile == Profile::Responses {
                self.responses(o)?
            } else {
                self.chat(o)?
            };
            if self.profile == Profile::Chat && self.chat_pending.is_some() {
                self.chat_extras.extend(extras);
                crate::protocol::openai::dialect::adapter_shapes::check_budget(&self.chat_extras)?;
            } else if self.profile == Profile::Responses
                && !extras.is_empty()
                && self
                    .state()?
                    .terminal()
                    .is_some_and(|t| t != StreamTerminal::Error)
            {
                let semantic = materialize(self.state()?)?;
                let id = self
                    .metadata
                    .as_ref()
                    .ok_or(CodecError::Invalid("metadata"))?
                    .required_id()?;
                self.fidelity.capture_routing_extras(
                    self.profile,
                    &self.adaptation,
                    extras,
                    &semantic,
                    id,
                )?;
            }
            Ok(events)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Available immediately after response.created; no whole-stream buffering is required.
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        self.metadata.as_ref()
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        end_of_stream(
            self.state
                .as_ref()
                .ok_or(CodecError::Invalid("stream state"))?,
        )?;
        Ok(())
    }
    /// Framing calls this only for a real Chat `[DONE]`, never for transport EOF.
    pub fn done(&mut self) -> Result<Vec<StreamEvent>, CodecError> {
        if self.profile != Profile::Chat || self.poisoned {
            return Err(CodecError::Invalid("DONE"));
        }
        let Some((terminal, details)) = self.chat_pending.take() else {
            self.poisoned = true;
            return Err(CodecError::Invalid("DONE before finish"));
        };
        let result = (|| {
            let mut out = vec![];
            self.emit(StreamEvent::Terminal { terminal, details }, &mut out)?;
            if !self.chat_extras.is_empty() {
                let semantic = materialize(self.state()?)?;
                let id = self
                    .metadata
                    .as_ref()
                    .ok_or(CodecError::Invalid("metadata"))?
                    .required_id()?;
                self.fidelity.capture_routing_extras(
                    self.profile,
                    &self.adaptation,
                    std::mem::take(&mut self.chat_extras),
                    &semantic,
                    id,
                )?;
            }
            Ok(out)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn materialize(&self) -> Result<crate::protocol::openai::DecodedResponse, CodecError> {
        self.finish()?;
        Ok(crate::protocol::openai::DecodedResponse {
            semantic: materialize(self.state.as_ref().ok_or(CodecError::Invalid("state"))?)?,
            fidelity: self.fidelity.clone(),
            metadata: self
                .metadata
                .clone()
                .ok_or(CodecError::Invalid("metadata"))?,
        })
    }
    pub fn into_parts(self) -> Result<(FidelityRecords, ResponseMetadata), CodecError> {
        self.finish()?;
        Ok((
            self.fidelity,
            self.metadata.ok_or(CodecError::Invalid("metadata"))?,
        ))
    }
    pub(super) fn state(&self) -> Result<&StreamState, CodecError> {
        self.state
            .as_ref()
            .ok_or(CodecError::Invalid("rejected stream"))
    }
    pub(super) fn emit(
        &mut self,
        event: StreamEvent,
        out: &mut Vec<StreamEvent>,
    ) -> Result<(), CodecError> {
        self.state = Some(reduce(
            self.state.take().ok_or(CodecError::Invalid("state"))?,
            event.clone(),
        )?);
        if let StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. } =
            &event
        {
            sync_replays(
                self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                &mut self.fidelity,
                Some(*item),
            )?;
        }
        out.push(event);
        Ok(())
    }
    pub(super) fn allocate_item(&mut self) -> Result<ItemId, CodecError> {
        if self.ids.len() >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        let id = ItemId::new(self.ids.len() as u64 + 1);
        self.ids.push(id);
        Ok(id)
    }
    pub(super) fn allocate_part(&mut self) -> Result<PartId, CodecError> {
        if self.next_part >= MAX_ITEMS as u64 {
            return Err(CodecError::Limit);
        }
        self.next_part += 1;
        Ok(PartId::new(self.next_part))
    }
    pub(super) fn observe_metadata(&mut self, o: &Map<String, Value>) -> Result<(), CodecError> {
        if self.profile == Profile::Responses {
            let (native, _) =
                crate::semantic::context::ResponseContext::read(o, DecodeDirection::Upstream)?;
            native.validate()?;
            if let Some(instructions) = native.instruction_messages {
                if self
                    .native_instructions
                    .as_ref()
                    .is_some_and(|old| old != &instructions)
                {
                    return Err(CodecError::Invalid("reported instructions changed"));
                }
                self.native_instructions = Some(instructions);
            }
            self.native_reports = Some(crate::protocol::openai::reports::observe(
                self.native_reports.as_ref(),
                &native.reports,
                self.adaptation.rules.responses_lifecycle_reports,
            )?);
        }
        let mut m = crate::protocol::openai::context::metadata(
            o,
            self.profile,
            &self.adaptation,
            &mut self.fidelity,
        )?;
        if let Some(old) = &self.metadata {
            // Response identity is strict; providers re-stamp scalar facts such
            // as `created` and `system_fingerprint` per chunk, so the first
            // reported value binds and later drift is normalized away.
            if old.id != m.id {
                return Err(CodecError::Invalid("response id changed"));
            }
            if old.model != m.model {
                return Err(CodecError::Invalid("response model changed"));
            }
            if old.created != m.created {
                if !(self.adaptation.rules.chunk_metadata_drift
                    || self.adaptation.rules.chunk_created_drift)
                {
                    return Err(CodecError::Invalid("created changed"));
                }
                m.created = old.created.clone();
            }
            if self.profile == Profile::Responses {
                m.context.reports = crate::protocol::openai::reports::observe(
                    Some(&old.context.reports),
                    &m.context.reports,
                    self.adaptation.rules.responses_lifecycle_reports,
                )?;
            } else if !old.context.service_class().is_absent() {
                if !m.context.service_class().is_absent()
                    && old.context.service_class() != m.context.service_class()
                {
                    return Err(CodecError::Invalid("service tier changed"));
                }
                m.context
                    .reports
                    .latest_mut()
                    .expect("single report")
                    .service_class = old.context.service_class().clone();
            }
            if !old.context.system_fingerprint.is_absent() {
                if !m.context.system_fingerprint.is_absent()
                    && old.context.system_fingerprint != m.context.system_fingerprint
                    && !self.adaptation.rules.chunk_metadata_drift
                {
                    return Err(CodecError::Invalid("fingerprint changed"));
                }
                m.context.system_fingerprint = old.context.system_fingerprint.clone();
            }
        }
        self.metadata = Some(m);
        // Vendor adaptation (ADR 0008) also covers response snapshots in streams.
        let profile = self.profile;
        crate::protocol::openai::responses::envelope::record_vendor_shapes(
            profile,
            &self.adaptation,
            o,
            &mut self.fidelity,
            None,
        )?;
        Ok(())
    }
}
