//! Native Messages response mapping: wire → IR decode and IR → wire encode.
use super::{
    Profile, Role,
    items::{Intake, bind, item_blocks, unsupported},
};
use crate::{
    protocol::{
        CodecError, DecodedRequest, DecodedResponse, fidelity::FidelityRecords,
        native::ReplayTarget,
    },
    semantic::{task::generation::*, value::Presence},
};
use serde_json::Value;

impl Profile {
    pub fn decode_generation_response(
        self,
        bytes: &[u8],
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedResponse, CodecError> {
        let wire = self.decode_response(bytes)?;
        self.decode_generation_message(wire, scope, target)
    }
    pub(crate) fn decode_generation_message(
        self,
        wire: super::Message,
        scope: LocalScope,
        target: &ReplayTarget,
    ) -> Result<DecodedResponse, CodecError> {
        wire.validate(false)?;
        if wire.model != target.model() {
            return Err(CodecError::invalid());
        }
        let mut intake = Intake::new(scope);
        intake.message(Role::Assistant, &wire.content)?;
        let (outcome, progress, details) = super::terminal::decode(&wire)?;
        let mut reports = UsageReports::default();
        reports.replace(super::accounting::report(&wire.usage, UsageStage::Final))?;
        let semantic = GenerationResponse::new(intake.items, outcome)?
            .with_message_envelopes(intake.groups)?
            .with_progress(progress)?
            .with_details(details)?
            .with_usage_reports(reports)?;
        let mut fidelity = FidelityRecords::default();
        bind(&mut fidelity, semantic.items(), target)?;
        let mut evidence = crate::protocol::conversion::generation::EvidenceDelta::default();
        if wire.input_transformations {
            evidence.record_input_transformations(
                crate::protocol::conversion::generation::ConversionBoundary::ResponseDecode,
            );
        }
        Ok(DecodedResponse {
            evidence,
            semantic,
            fidelity,
            metadata: wire.reported_metadata()?,
        })
    }
    pub fn encode_generation_response(
        self,
        response: &DecodedResponse,
        source: &DecodedRequest,
        target: &ReplayTarget,
    ) -> Result<Value, CodecError> {
        response.metadata.validate()?;
        let report = response
            .semantic
            .usage_reports()
            .operation()
            .ok_or_else(unsupported)?;
        if response.semantic.usage_reports().reports().len() != 1
            || report.stage != UsageStage::Final
        {
            return Err(unsupported());
        }
        let mut context = response.metadata.context.clone();
        crate::protocol::reports::single_view(&mut context.reports)?;
        let service_class = std::mem::take(
            &mut context
                .reports
                .latest_mut()
                .expect("single report")
                .service_class,
        );
        if response.metadata.model != target.model()
            || response.metadata.created.is_some()
            || context != Default::default()
        {
            return Err(unsupported());
        }
        let joined = ClientManaged::new(source.semantic.settings().clone())?
            .select_request(&source.semantic)?
            .select_response(&response.semantic)?
            .build()?;
        response
            .fidelity
            .require_native_replay(&joined, &target.origin())?;
        let actual = response.semantic.message_envelopes();
        if actual.len() != 1
            || actual[0].role() != MessageEnvelopeRole::Assistant
            || actual[0].members()
                != response
                    .semantic
                    .items()
                    .iter()
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>()
        {
            return Err(unsupported());
        }
        let content = response
            .semantic
            .items()
            .iter()
            .map(|(id, item)| {
                item_blocks(
                    *id,
                    item,
                    &joined,
                    &response.fidelity,
                    target,
                    &mut vec![],
                    &mut vec![],
                )
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let (stop_reason, stop_sequence) = super::terminal::encode(
            response.semantic.outcome(),
            response.semantic.progress(),
            response
                .semantic
                .items()
                .iter()
                .any(|(_, item)| item.is_call()),
            response.semantic.details(),
        )?;
        let stop_sequence = stop_sequence
            .map(|value| Presence::Value(value.to_owned()))
            .unwrap_or(Presence::Null);
        self.encode_response(&super::Message {
            id: response.metadata.required_id()?.into(),
            model: target.model().into(),
            content,
            stop_reason: Presence::Value(stop_reason),
            stop_sequence,
            stop_details: response.semantic.details().refusal.clone(),
            usage: super::accounting::wire(report, service_class)?,
            input_transformations: false,
        })
    }
}

impl super::Message {
    /// Derived envelope facts only; no timestamp, instruction echo or replay proof is invented.
    pub fn reported_metadata(&self) -> Result<crate::protocol::ResponseMetadata, CodecError> {
        self.validate(false)?;
        let metadata = crate::protocol::ResponseMetadata {
            delivery: None,
            storage: None,
            id: Some(self.id.clone()),
            model: self.model.clone(),
            created: None,
            context: crate::semantic::context::ResponseContext {
                reports: crate::semantic::context::ResponseReports::single(
                    crate::semantic::context::ResponseReport {
                        service_class: self.usage.service_class.clone(),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            instruction_fidelity: Default::default(),
        };
        metadata.validate()?;
        Ok(metadata)
    }
}
