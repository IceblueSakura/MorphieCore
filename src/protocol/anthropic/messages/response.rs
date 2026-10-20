//! Native Messages response mapping: wire → IR decode and IR → wire encode.
use super::{
    NativeProjection, Profile, RequestRule, Role,
    items::{Intake, bind, item_blocks, unsupported},
    request::project,
    settings::effort_level,
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
            return Err(CodecError::Invalid("native model binding"));
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
        if wire.input_transformations {
            fidelity.record_omission(crate::protocol::fidelity::INPUT_TRANSFORMATIONS_OMISSION);
        }
        Ok(DecodedResponse {
            semantic,
            fidelity,
            metadata: wire.reported_metadata()?,
        })
    }
    pub fn encode_generation_request(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
    ) -> Result<NativeProjection, CodecError> {
        self.encode_request_envelope(
            request,
            fidelity,
            target,
            &Presence::Absent,
            &Default::default(),
        )
    }
    pub(super) fn encode_request_envelope(
        self,
        request: &GenerationRequest,
        fidelity: &FidelityRecords,
        target: &ReplayTarget,
        policy: &Presence<crate::semantic::context::ServicePolicy>,
        limits: &crate::protocol::model_limits::ModelLimits,
    ) -> Result<NativeProjection, CodecError> {
        let (request, fidelity, transferred) = crate::protocol::native::native_reasoning_history(
            request,
            fidelity,
            target,
            &[
                ReplayFormat::AnthropicMessagesThinking,
                ReplayFormat::AnthropicMessagesRedactedThinking,
            ],
        )?;
        let mut normalized = vec![];
        let mut omitted = vec![];
        let mut rules = std::collections::BTreeSet::new();
        if transferred {
            rules.insert(RequestRule::TransferVisibleHistory);
        }
        let phase = crate::protocol::phase::omit_request(&request)?;
        if phase.is_some() {
            rules.insert(RequestRule::OmitMessagePhase);
        }
        let mut wire = project(
            phase.as_ref().unwrap_or(&request),
            &fidelity,
            target,
            &mut normalized,
            &mut omitted,
            &mut rules,
        )?;
        wire.service_policy = policy.clone();
        limits.check_values(
            Some(wire.max_tokens),
            wire.output_config
                .value()
                .and_then(|config| config.effort.value())
                .copied()
                .into_iter()
                .chain(wire.messages.iter().filter_map(|message| message.effort))
                .map(effort_level),
        )?;
        let value = self.encode_request(&wire)?;
        Ok(NativeProjection {
            value,
            normalized_arguments: normalized,
            omitted_execution: omitted,
            rules: rules.into_iter().collect(),
            control_losses: phase
                .is_some()
                .then(|| {
                    crate::lowering::generation::RequestControlLoss::Projection(
                        crate::protocol::phase::omission(RequestRule::OmitMessagePhase.name()),
                    )
                })
                .into_iter()
                .collect(),
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
        crate::protocol::openai::reports::single_view(&mut context.reports)?;
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
                    &mut Default::default(),
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
        let stop_reason = super::StopReason::read(&Value::String(stop_reason.into()))?;
        let stop_sequence = stop_sequence
            .map(|value| Presence::Value(value.to_owned()))
            .unwrap_or(Presence::Null);
        self.encode_response(&super::Message {
            id: response.metadata.required_id()?.into(),
            model: target.model().into(),
            content,
            stop_reason: Presence::Value(stop_reason),
            stop_sequence,
            usage: super::accounting::wire(report, service_class)?,
            input_transformations: false,
        })
    }
}
