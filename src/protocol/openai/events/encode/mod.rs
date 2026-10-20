//! Outgoing event driver: contract locking, projection and semantic reduction.
mod responses;
use crate::protocol::openai::events::*;
use crate::protocol::openai::lowering::projection;
use serde_json::json;

pub struct EventEncoder {
    pub(super) profile: Profile,
    pub(super) metadata: ResponseMetadata,
    pub(super) state: Option<StreamState>,
    pub(super) fidelity: FidelityRecords,
    pub(super) contract: TargetCaps,
    poisoned: bool,
    target_locked: bool,
    sequence: u64,
    pub(super) attachment: Option<crate::protocol::attachments::Carrier>,
    pub(super) chat_audio_pending: Vec<u8>,
    pub(super) projection: Vec<projection::ProjectionStage>,
}
impl EventEncoder {
    pub fn new(profile: Profile, metadata: ResponseMetadata) -> Result<Self, CodecError> {
        metadata.validate()?;
        crate::protocol::openai::reports::relation(&metadata.context.reports, true)?;
        validate_metadata(profile, &metadata)?;
        Ok(Self {
            profile,
            metadata,
            state: Some(StreamState::new()),
            fidelity: FidelityRecords::default(),
            contract: TargetCaps::full(),
            poisoned: false,
            target_locked: false,
            sequence: 0,
            attachment: None,
            chat_audio_pending: Vec::new(),
            projection: Vec::new(),
        })
    }
    pub(crate) fn set_attachment(
        &mut self,
        value: crate::protocol::attachments::Carrier,
    ) -> Result<(), CodecError> {
        crate::protocol::attachments::check_chat_support(
            self.profile,
            self.contract.adaptation.rules.structured_chat_reasoning,
        )?;
        if self.poisoned || self.state()?.terminal().is_some() || self.attachment.is_some() {
            return Err(CodecError::Invalid("attachment carrier"));
        }
        self.attachment = Some(value);
        Ok(())
    }
    /// Refresh reported settings/completion metadata without changing response identity.
    pub fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        let result = (|| {
            if self.state()?.terminal().is_some()
                || self.metadata.id != metadata.id
                || self.metadata.model != metadata.model
                || self.metadata.created != metadata.created
            {
                return Err(CodecError::Invalid("metadata changed"));
            }
            metadata.validate()?;
            validate_metadata(self.profile, &metadata)?;
            if self.profile == Profile::Responses
                && self.sequence != 0
                && matches!(
                    self.metadata.context.reports,
                    crate::semantic::context::ResponseReports::Lifecycle { .. }
                )
                && self
                    .metadata
                    .context
                    .reports
                    .at(crate::semantic::context::ReportPhase::Opening)
                    != metadata
                        .context
                        .reports
                        .at(crate::semantic::context::ReportPhase::Opening)
            {
                return Err(CodecError::Invalid("emitted opening report changed"));
            }
            if self.profile == Profile::Chat && self.state()?.started() {
                let old = &self.metadata.context;
                if !old.system_fingerprint.is_absent()
                    && old.system_fingerprint != metadata.context.system_fingerprint
                {
                    // Intake normalization is not permission to rewrite already emitted facts.
                    return Err(CodecError::Invalid("bound Chat metadata changed"));
                }
            }
            crate::protocol::openai::reports::update(
                &self.metadata.context.reports,
                &metadata.context.reports,
            )?;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        } else {
            self.metadata = metadata;
        }
        result
    }
    pub fn with_contract(mut self, contract: TargetCaps) -> Self {
        if self.target_locked && self.contract != contract {
            self.poisoned = true;
        } else {
            self.contract = contract;
        }
        self
    }
    pub fn encode(
        &mut self,
        event: &StreamEvent,
        source: &FidelityRecords,
    ) -> Result<Vec<Value>, CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        self.target_locked = true;
        let checked_projection = self.projection.len();
        let result = (|| {
            if self.profile == Profile::Chat
                && (self.metadata.context.settings().is_some()
                    || self.metadata.context.instruction_messages.is_some())
            {
                if self.contract.reported_facts
                    == crate::lowering::generation::ReportedFactPolicy::StrictComplete
                {
                    return Err(CodecError::Unsupported("required reported settings".into()));
                }
                let mut stage = projection::ProjectionStage::response(
                    self.profile,
                    self.contract.adaptation.profile_id,
                    Some(projection::LossRule::OmitChatReportedSettings),
                );
                stage.direction = projection::ProjectionDirection::Event;
                if !self.projection.contains(&stage) {
                    self.projection.push(stage);
                }
            }
            crate::protocol::openai::lowering::events::check_event(
                self.state()?,
                event,
                self.profile,
                &self.contract,
            )
            .map_err(|_| CodecError::Unsupported("event target representation".into()))?;
            if let StreamEvent::ItemStarted { item, .. } = event {
                let id = source
                    .response_item_id(*item)
                    .map(str::to_owned)
                    .unwrap_or_else(|| crate::protocol::fidelity::generated_item_id(*item));
                if self
                    .state()?
                    .items()
                    .iter()
                    .any(|old| self.fidelity.response_item_id(old.id) == Some(id.as_str()))
                {
                    return Err(CodecError::Invalid("duplicate target wire identity"));
                }
                self.fidelity.record_response_item_id(*item, &id)?;
            }
            // Chat summary chunks have not emitted a wire item identity yet.
            // Bind the issuer's late identity before rendering encrypted replay.
            if self.profile == Profile::Chat
                && self.contract.adaptation.rules.structured_chat_reasoning
                && let StreamEvent::ItemFinished {
                    item,
                    replay: Some(_),
                    ..
                } = event
                && let Some(id) = source.response_item_id(*item)
            {
                self.fidelity.record_response_item_id(*item, id)?;
            }
            let probabilities = if self.profile == Profile::Chat {
                self.chat_probabilities(event)?
            } else {
                None
            };
            self.state = Some(reduce(
                self.state.take().ok_or(CodecError::Invalid("state"))?,
                event.clone(),
            )?);
            self.fidelity
                .merge_item_reports(source, self.state.as_ref().expect("state"))?;
            // Replay is event authority, never restored from the source sidecar.
            match event {
                StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. } => {
                    sync_replays(
                        self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                        &mut self.fidelity,
                        Some(*item),
                    )?
                }
                StreamEvent::Terminal { .. } => sync_replays(
                    self.state.as_ref().ok_or(CodecError::Invalid("state"))?,
                    &mut self.fidelity,
                    None,
                )?,
                _ => {}
            }
            if matches!(event, StreamEvent::Terminal { .. }) {
                self.fidelity.copy_response_records(source);
            }
            let mut values = if self.profile == Profile::Responses {
                self.responses(event)?
            } else {
                self.chat(event, probabilities)?
            };
            for v in &mut values {
                if self.profile == Profile::Responses {
                    v["sequence_number"] = json!(self.sequence);
                    self.sequence += 1;
                }
                bounded(v)?;
            }
            if self.profile == Profile::Responses
                && !values.is_empty()
                && !self.metadata.context.system_fingerprint.is_absent()
            {
                self.record_loss(projection::LossRule::OmitResponsesSystemFingerprint);
            }
            Ok(values)
        })();
        if result.is_err() {
            self.poisoned = true;
            self.projection.truncate(checked_projection);
        }
        result
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned {
            return Err(CodecError::Invalid("rejected stream"));
        }
        end_of_stream(self.state()?)?;
        Ok(())
    }
    /// Successful prefixes remain evidence even when a later event is rejected.
    pub fn projection(&self) -> &[projection::ProjectionStage] {
        &self.projection
    }
    fn record_loss(&mut self, loss: projection::LossRule) {
        use projection::{ProjectionDirection, ProjectionStage};
        let mut stage = ProjectionStage::response(
            self.profile,
            self.contract.adaptation.profile_id,
            Some(loss),
        );
        stage.direction = ProjectionDirection::Event;
        if !self.projection.contains(&stage) {
            self.projection.push(stage);
        }
    }
    pub(super) fn state(&self) -> Result<&StreamState, CodecError> {
        self.state.as_ref().ok_or(CodecError::Invalid("state"))
    }
}
fn validate_metadata(profile: Profile, metadata: &ResponseMetadata) -> Result<(), CodecError> {
    for report in metadata.context.reports.iter() {
        if let Some(class) = report.service_class.value() {
            crate::protocol::openai::service::class_wire(*class, profile)?;
        }
        if let Some(settings) = &report.settings {
            settings.validate()?;
            if !settings.safety.is_empty() {
                return Err(CodecError::Unsupported("reported safety settings".into()));
            }
        }
    }
    metadata.required_id()?;
    let created = metadata
        .created
        .as_ref()
        .ok_or(CodecError::Invalid("created time"))?;
    if profile == Profile::Chat {
        crate::protocol::openai::reports::validate_single(&metadata.context.reports)?;
        crate::protocol::openai::chat::envelope::validate_context(&metadata.context.execution)?;
        if created.as_u64().is_none() || !metadata.context.execution.metadata.is_absent() {
            // Chat chunks have no metadata slot; never silently discard a static fact.
            return Err(CodecError::Unsupported("Chat stream metadata".into()));
        }
    }
    Ok(())
}
