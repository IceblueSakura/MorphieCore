//! Outgoing event driver: contract locking, projection and semantic reduction.
use crate::protocol::failure::{CarrierReason, ShapeReason};
mod chat;
mod responses;
use crate::protocol::openai::{events::*, lowering::projection};
use serde_json::json;

pub struct EventEncoder {
    pub(super) profile: GenerationApi,
    pub(super) metadata: ResponseMetadata,
    /// None is rejection, including failures before or after semantic reduction.
    pub(super) state: Option<StreamState>,
    pub(super) fidelity: FidelityRecords,
    pub(super) contract: TargetContract,
    target_locked: bool,
    sequence: u64,
    pub(super) chat_audio_pending: Vec<u8>,
    pub(super) projection: Vec<crate::protocol::conversion::generation::Effect>,
    /// Atomic output items have no in-progress wire status. Hold their opener
    /// and following frames until observed item closure supplies a lawful value.
    pub(super) pending_program_outputs: std::collections::BTreeMap<ItemId, usize>,
    pub(super) held_frames: Vec<Value>,
    held_bytes: usize,
}
impl EventEncoder {
    pub fn new(profile: GenerationApi, metadata: ResponseMetadata) -> Result<Self, CodecError> {
        metadata.validate()?;
        crate::protocol::reports::relation(&metadata.context.reports, true)?;
        validate_metadata(profile, &metadata)?;
        Ok(Self {
            profile,
            metadata,
            state: Some(StreamState::new()),
            fidelity: FidelityRecords::default(),
            contract: TargetContract::full(),
            target_locked: false,
            sequence: 0,
            chat_audio_pending: Vec::new(),
            projection: Vec::new(),
            pending_program_outputs: Default::default(),
            held_frames: vec![],
            held_bytes: 0,
        })
    }
    /// Refresh reported settings/completion metadata without changing response identity.
    pub fn update_metadata(&mut self, metadata: ResponseMetadata) -> Result<(), CodecError> {
        if self.state.is_none() {
            return Err(CodecError::invalid());
        }
        let result = (|| {
            if self.state()?.terminal().is_some()
                || self.metadata.id != metadata.id
                || self.metadata.model != metadata.model
                || self.metadata.created != metadata.created
            {
                return Err(CodecError::Invalid(ShapeReason::Metadata));
            }
            metadata.validate()?;
            validate_metadata(self.profile, &metadata)?;
            if self.profile == GenerationApi::Responses
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
                return Err(CodecError::invalid());
            }
            if self.profile == GenerationApi::Chat && self.state()?.started() {
                let old = &self.metadata.context;
                if !old.system_fingerprint.is_absent()
                    && old.system_fingerprint != metadata.context.system_fingerprint
                {
                    // Intake normalization is not permission to rewrite already emitted facts.
                    return Err(CodecError::Invalid(ShapeReason::Metadata));
                }
            }
            crate::protocol::reports::update(
                &self.metadata.context.reports,
                &metadata.context.reports,
            )?;
            Ok(())
        })();
        if result.is_err() {
            self.state = None;
        } else {
            self.metadata = metadata;
        }
        result
    }
    pub fn with_contract(mut self, contract: TargetContract) -> Self {
        if self.target_locked && self.contract != contract {
            self.state = None;
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
        if self.state.is_none() {
            return Err(CodecError::invalid());
        }
        self.target_locked = true;
        let checked_projection = self.projection.len();
        let result = (|| {
            let metadata_echo = if self.profile == GenerationApi::Responses {
                projection::request_metadata_echo(&self.metadata, self.contract.reported_facts)
                    .map_err(|cause| CodecError::Representation {
                        boundary: crate::protocol::conversion::generation::ConversionBoundary::EventProject,
                        cause,
                    })?
            } else {
                None
            };
            if self.profile == GenerationApi::Chat
                && (self.metadata.context.settings().is_some()
                    || self.metadata.context.instruction_messages.is_some())
            {
                if self.contract.reported_facts
                    == crate::protocol::conversion::generation::ReportedFactPolicy::StrictComplete
                {
                    return Err(CodecError::Unsupported(CarrierReason::Unavailable));
                }
                self.record_loss(projection::ProjectionRule::OmitChatReportedSettings);
            }
            crate::protocol::openai::lowering::events::check_event(
                self.state()?,
                event,
                self.profile,
                &self.contract,
            )
            .map_err(|cause| CodecError::Representation {
                boundary: crate::protocol::conversion::generation::ConversionBoundary::EventProject,
                cause,
            })?;
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
                    return Err(CodecError::invalid());
                }
                self.fidelity.record_response_item_id(*item, &id)?;
            }
            // Chat summary chunks have not emitted a wire item identity yet.
            // Bind the issuer's late identity before rendering encrypted replay.
            if self.profile == GenerationApi::Chat
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
            let probabilities = if self.profile == GenerationApi::Chat {
                self.chat_probabilities(event)?
            } else {
                None
            };
            self.state = Some(reduce(
                self.state.take().ok_or(CodecError::invalid())?,
                event.clone(),
            )?);
            self.fidelity
                .merge_item_reports(source, self.state.as_ref().expect("state"))?;
            // Replay is event authority, never restored from the source sidecar.
            match event {
                StreamEvent::ItemStarted { item, .. } | StreamEvent::ItemFinished { item, .. } => {
                    sync_replays(
                        self.state.as_ref().ok_or(CodecError::invalid())?,
                        &mut self.fidelity,
                        Some(*item),
                    )?
                }
                StreamEvent::Terminal { .. } => sync_replays(
                    self.state.as_ref().ok_or(CodecError::invalid())?,
                    &mut self.fidelity,
                    None,
                )?,
                _ => {}
            }
            if matches!(event, StreamEvent::Terminal { .. }) {
                self.fidelity.copy_response_records(source);
            }
            let mut values = if self.profile == GenerationApi::Responses {
                self.responses(event)?
            } else {
                self.chat(event, probabilities)?
            };
            for v in &mut values {
                if self.profile == GenerationApi::Responses {
                    v["sequence_number"] = json!(self.sequence);
                    self.sequence += 1;
                }
                bounded(v)?;
            }
            if let Some(rule) = metadata_echo
                && values.iter().any(|value| value.get("response").is_some())
            {
                record_stages(
                    &mut self.projection,
                    [rule.effect(
                        crate::protocol::conversion::generation::ConversionBoundary::EventProject,
                    )],
                );
            }
            if self.profile == GenerationApi::Responses
                && !values.is_empty()
                && !self.metadata.context.system_fingerprint.is_absent()
            {
                self.record_loss(projection::ProjectionRule::OmitResponsesSystemFingerprint);
            }
            if !self.pending_program_outputs.is_empty() || !self.held_frames.is_empty() {
                if matches!(event, StreamEvent::Terminal { .. })
                    && !self.pending_program_outputs.is_empty()
                {
                    return Err(CodecError::invalid());
                }
                for value in values {
                    // Include entry overhead and space for the eventual status.
                    self.held_bytes = self
                        .held_bytes
                        .checked_add(
                            serde_json::to_vec(&value)
                                .map_err(|_| CodecError::invalid())?
                                .len()
                                + std::mem::size_of::<Value>()
                                + 32,
                        )
                        .ok_or(CodecError::Limit)?;
                    if self.held_bytes > MAX_TOTAL_BYTES || self.held_frames.len() >= MAX_ITEMS {
                        return Err(CodecError::Limit);
                    }
                    self.held_frames.push(value);
                }
                values = if self.pending_program_outputs.is_empty() {
                    self.held_bytes = 0;
                    std::mem::take(&mut self.held_frames)
                } else {
                    vec![]
                };
            }
            Ok(values)
        })();
        if result.is_err() {
            self.state = None;
            self.held_frames.clear();
            self.pending_program_outputs.clear();
            self.held_bytes = 0;
            self.projection.truncate(checked_projection);
        }
        result
    }
    pub fn finish(&self) -> Result<(), CodecError> {
        if self.state.is_none()
            || !self.held_frames.is_empty()
            || !self.pending_program_outputs.is_empty()
        {
            return Err(CodecError::invalid());
        }
        end_of_stream(self.state()?)?;
        Ok(())
    }
    /// Successful prefixes remain evidence even when a later event is rejected.
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        std::mem::take(&mut self.projection).into_iter().collect()
    }
    pub fn projection(&self) -> &[crate::protocol::conversion::generation::Effect] {
        &self.projection
    }
    fn record_loss(&mut self, loss: projection::ProjectionRule) {
        record_stages(
            &mut self.projection,
            [(loss).effect(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseProject,
            )],
        );
    }
    pub(super) fn state(&self) -> Result<&StreamState, CodecError> {
        self.state.as_ref().ok_or(CodecError::invalid())
    }
}
/// Event delivery re-labels every copied stage and keeps the first occurrence.
fn record_stages(
    projection: &mut Vec<crate::protocol::conversion::generation::Effect>,
    stages: impl IntoIterator<Item = crate::protocol::conversion::generation::Effect>,
) {
    for mut stage in stages {
        stage = stage.with_boundary(
            crate::protocol::conversion::generation::ConversionBoundary::EventProject,
        );
        if !projection.contains(&stage) {
            projection.push(stage);
        }
    }
}
fn validate_metadata(
    profile: GenerationApi,
    metadata: &ResponseMetadata,
) -> Result<(), CodecError> {
    for report in metadata.context.reports.iter() {
        if let Some(class) = report.service_class.value() {
            crate::protocol::openai::service::class_wire(*class, profile)?;
        }
        if let Some(settings) = &report.settings {
            settings.validate()?;
            if !settings.safety.is_empty() {
                return Err(CodecError::Unsupported(CarrierReason::Safety));
            }
        }
    }
    metadata.required_id()?;
    let created = metadata
        .created
        .as_ref()
        .ok_or(CodecError::Invalid(ShapeReason::Metadata))?;
    if profile == GenerationApi::Chat {
        crate::protocol::reports::validate_single(&metadata.context.reports)?;
        crate::protocol::openai::chat::envelope::validate_context(&metadata.context.execution)?;
        if created.as_u64().is_none() || metadata.context.execution.metadata.value().is_some() {
            // Chat chunks have no metadata slot; never silently discard a static fact.
            return Err(CodecError::Unsupported(CarrierReason::Metadata));
        }
    }
    Ok(())
}
