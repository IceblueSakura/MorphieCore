//! One client delivery projection. Renderers and retained history consume its events.
use super::*;
use crate::conversion::generation::{Change, ConversionBoundary, Field, Owner};
use crate::lowering::disposition::{DeliveryDispositions, DeliveryFact, Disposition, Rule};
use crate::semantic::task::generation::*;
use crate::semantic::value::Presence;
use std::collections::{BTreeMap, BTreeSet};

mod snapshot;
pub use snapshot::emit;

impl ClientAdapter {
    pub fn delivery_dispositions(&self, streaming: bool) -> DeliveryDispositions {
        use DeliveryFact::*;
        use Disposition::*;
        let protocol = self.protocol();
        DeliveryDispositions::from_facts(|fact| match (protocol, fact) {
            // A model-authored refusal is not a classifier stop.
            (ClientProtocol::Messages, Refusal) => Approximate(Rule::MESSAGES_REFUSAL_AS_TEXT),
            (ClientProtocol::Chat, OpeningService) => Omit(Rule::OMIT_CHAT_OPENING_SERVICE),
            (ClientProtocol::Chat, Reasoning) if self.omits_readable_reasoning() => {
                Omit(Rule::OMIT_CHAT_READABLE_REASONING)
            }
            (ClientProtocol::Chat | ClientProtocol::Messages, Phase) => {
                Omit(Rule::OMIT_DELIVERY_MESSAGE_PHASE)
            }
            (ClientProtocol::Messages, ExecutionHints) => Omit(Rule::OMIT_MESSAGES_EXECUTION_HINTS),
            (ClientProtocol::Chat, ExecutionHints) if streaming => {
                Omit(Rule::OMIT_CHAT_STREAM_EXECUTION_HINTS)
            }
            (ClientProtocol::Chat, ExecutionHints) => {
                Omit(Rule::OMIT_CHAT_UNSUPPORTED_EXECUTION_HINTS)
            }
            (ClientProtocol::Messages, Settings) => Omit(Rule::OMIT_MESSAGES_INCIDENTAL_SETTINGS),
            (ClientProtocol::Chat, Settings) => Omit(Rule::OMIT_CHAT_DELIVERED_SETTINGS),
            (ClientProtocol::Messages, Fingerprint) => Omit(Rule::OMIT_MESSAGES_SYSTEM_FINGERPRINT),
            (ClientProtocol::Responses, Fingerprint) => {
                Omit(Rule::OMIT_RESPONSES_SYSTEM_FINGERPRINT)
            }
            (ClientProtocol::Chat, Annotations) if streaming => {
                Omit(Rule::OMIT_CHAT_STREAM_ANNOTATIONS)
            }
            (ClientProtocol::Messages, Annotations) => Omit(Rule::OMIT_MESSAGES_ANNOTATIONS),
            (ClientProtocol::Messages, Probabilities) => Omit(Rule::OMIT_MESSAGES_PROBABILITIES),
            (ClientProtocol::Messages, TerminalExplanation) => {
                Omit(Rule::OMIT_MESSAGES_TERMINAL_EXPLANATION)
            }
            _ => Native,
        })
    }
}

pub struct ClientProjection {
    client: ClientAdapter,
    dispositions: DeliveryDispositions,
    streaming: bool,
    native: bool,
    state: Option<StreamState>,
    grouping: Option<crate::openai::lowering::native_response::EventProjection>,
    omitted: BTreeSet<ItemId>,
    calls: BTreeMap<ItemId, bool>,
    losses: Vec<Effect>,
    messages: Option<crate::anthropic::messages::client::Schedule>,
    progress: Option<InteractionProgress>,
    limits: crate::stream::SseLimits,
    without_stops: bool,
    ambiguous_stops: bool,
    terminal_policy: Option<crate::anthropic::messages::client::GoogleTerminalPolicy>,
    metadata: Option<ResponseMetadata>,
    fidelity: FidelityRecords,
    source_lifecycles: BTreeMap<ItemId, ItemLifecycle>,
    text_sources: BTreeSet<ItemId>,
    plain_text_source: bool,
}
impl ClientProjection {
    pub fn new(client: ClientAdapter, native: bool, streaming: bool) -> Self {
        let grouping = match client.protocol() {
            ClientProtocol::Chat => Some(if native {
                crate::openai::lowering::native_response::EventProjection::new(GenerationApi::Chat)
            } else {
                crate::openai::lowering::native_response::EventProjection::for_phase()
            }),
            ClientProtocol::Responses if native => Some(
                crate::openai::lowering::native_response::EventProjection::new(
                    GenerationApi::Responses,
                ),
            ),
            _ => None,
        };
        let messages = (client.protocol() == ClientProtocol::Messages).then(Default::default);
        Self {
            dispositions: client.delivery_dispositions(streaming),
            client,
            streaming,
            native,
            state: Some(StreamState::new()),
            grouping,
            messages,
            progress: None,
            limits: Default::default(),
            without_stops: false,
            ambiguous_stops: false,
            terminal_policy: None,
            metadata: None,
            fidelity: Default::default(),
            source_lifecycles: BTreeMap::new(),
            text_sources: BTreeSet::new(),
            plain_text_source: true,
            omitted: BTreeSet::new(),
            calls: BTreeMap::new(),
            losses: vec![],
        }
    }
    pub fn with_limits(mut self, limits: crate::stream::SseLimits) -> Self {
        self.limits = limits;
        self
    }
    pub fn with_grouping(mut self, group: bool) -> Self {
        if !group {
            self.grouping = None;
        }
        self
    }
    pub fn update_fidelity(&mut self, source: &FidelityRecords) -> Result<(), CodecError> {
        self.fidelity.copy_response_records(source);
        if let Some(state) = &self.state {
            self.fidelity.merge_item_reports(source, state)?;
            for item in state.items() {
                if let Some(id) = source.response_item_id(item.id)
                    && (self.client.protocol() != ClientProtocol::Responses
                        || self.fidelity.response_item_id(item.id).is_none())
                {
                    self.fidelity.record_response_item_id(item.id, id)?;
                }
            }
        }
        Ok(())
    }
    pub fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    pub fn with_messages_terminal(
        mut self,
        without_stops: bool,
        ambiguous_stops: bool,
        policy: Option<crate::anthropic::messages::client::GoogleTerminalPolicy>,
    ) -> Self {
        self.without_stops = without_stops;
        self.ambiguous_stops = ambiguous_stops;
        self.terminal_policy = policy;
        self
    }
    fn loss(&mut self, rule: Rule, owner: Owner) {
        self.reported_loss(rule.effect(ConversionBoundary::EventProject).at(owner));
    }
    pub fn take_evidence(&mut self) -> crate::conversion::generation::EvidenceDelta {
        std::mem::take(&mut self.losses).into_iter().collect()
    }
    pub fn started(&self) -> bool {
        self.state.as_ref().is_some_and(StreamState::started)
    }
    pub fn finished(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.terminal().is_some())
    }
    pub fn snapshot(&self) -> Result<GenerationResponse, CodecError> {
        Ok(materialize(
            self.state.as_ref().ok_or(CodecError::invalid())?,
        )?)
    }
    fn reported_loss(&mut self, effect: Effect) {
        if !self.losses.contains(&effect) {
            self.losses.push(effect);
        }
    }
    pub fn metadata(&mut self, source: &ResponseMetadata) -> Result<ResponseMetadata, CodecError> {
        source.validate()?;
        crate::reports::relation(&source.context.reports, true)?;
        if source.context.reports.iter().any(|report| {
            report
                .settings
                .as_ref()
                .is_some_and(|settings| !settings.safety.is_empty())
        }) {
            return Err(CodecError::Unsupported(CarrierReason::Safety));
        }
        let mut metadata = source.clone();
        if let Disposition::Omit(rule) = self.dispositions.get(DeliveryFact::Settings) {
            if metadata
                .context
                .reports
                .iter()
                .any(|report| report.settings.is_some())
                || metadata.context.instruction_messages.is_some()
            {
                self.loss(rule, Owner::Root);
            }
            for report in metadata.context.reports.iter_mut() {
                report.settings = None;
            }
            metadata.context.instruction_messages = None;
            metadata.instruction_fidelity = Default::default();
        }
        if let Disposition::Omit(rule) = self.dispositions.get(DeliveryFact::Fingerprint) {
            if metadata.context.system_fingerprint.as_ref().is_some() {
                self.loss(rule, Owner::Root);
            }
            metadata.context.system_fingerprint = None;
        }
        if let Disposition::Omit(rule) = self.dispositions.get(DeliveryFact::ExecutionHints) {
            let mut meaningful = metadata.context.execution.clone();
            let keep_metadata = self.client.protocol() == ClientProtocol::Chat && !self.streaming;
            if keep_metadata || meaningful.metadata == Presence::Null {
                meaningful.metadata = Presence::Absent;
            }
            if meaningful != Default::default() {
                self.loss(rule, Owner::Root);
            }
            let association = metadata.context.execution.metadata.clone();
            metadata.context.execution = Default::default();
            if keep_metadata {
                metadata.context.execution.metadata = association;
            }
        }
        if self.client.protocol() == ClientProtocol::Chat
            && let crate::semantic::context::ResponseReports::Lifecycle { opening, terminal } =
                &metadata.context.reports
        {
            let service_class = terminal.as_ref().and_then(|report| report.service_class);
            if opening
                .as_ref()
                .is_some_and(|report| report.service_class.is_some())
                && let Disposition::Omit(rule) = self.dispositions.get(DeliveryFact::OpeningService)
            {
                self.loss(rule, Owner::Root);
            }
            metadata.context.reports = crate::semantic::context::ResponseReports::Single(Box::new(
                crate::semantic::context::ResponseReport {
                    settings: None,
                    service_class,
                },
            ));
        }
        match &self.client {
            ClientAdapter::Messages => {
                for loss in crate::anthropic::messages::client::metadata_losses(&metadata) {
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                crate::anthropic::messages::client::project_metadata(&mut metadata)?;
            }
            ClientAdapter::OpenAi { adapter, .. } => {
                let profile = adapter.protocol;
                if profile == GenerationApi::Responses
                    && let Some(rule) =
                        crate::openai::lowering::projection::request_metadata_echo(&metadata)
                {
                    self.losses
                        .push(rule.effect(ConversionBoundary::EventProject));
                }
                if let Some(rule) =
                    crate::openai::lowering::projection::reported_settings(&mut metadata, profile)
                        .map_err(|cause| CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause,
                    })?
                {
                    self.reported_loss(rule.effect(ConversionBoundary::EventProject));
                }
                if let Some(rule) = crate::openai::lowering::projection::reported_service_class(
                    &mut metadata,
                    profile,
                ) {
                    self.reported_loss(rule.effect(ConversionBoundary::EventProject));
                }
            }
        }
        self.metadata = Some(metadata.clone());
        Ok(metadata)
    }
    pub fn snapshot_metadata(
        &mut self,
        source: &ResponseMetadata,
    ) -> Result<ResponseMetadata, CodecError> {
        let mut metadata = self.metadata(source)?;
        if self.client.protocol() == ClientProtocol::Responses
            && crate::reports::terminal_view(&mut metadata.context.reports)?
        {
            self.reported_loss(
                crate::lowering::disposition::Rule::OMIT_OPENING_REPORT
                    .effect(ConversionBoundary::ResponseProject),
            );
        }
        Ok(metadata)
    }
    pub fn project(&mut self, events: &[StreamEvent]) -> Result<Vec<StreamEvent>, CodecError> {
        let result = (|| {
            let mut selected = vec![];
            for event in events {
                if self.client.protocol() == ClientProtocol::Chat
                    && matches!(
                        event,
                        StreamEvent::ItemStarted {
                            kind: ItemKind::Program { .. }
                                | ItemKind::ProgramOutput { .. }
                                | ItemKind::ToolResult(_)
                                | ItemKind::CustomResult(_),
                            ..
                        }
                    )
                {
                    return Err(CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause: crate::openai::lowering::RepresentationError::Tools,
                    });
                }
                if let StreamEvent::ItemStarted {
                    kind: ItemKind::Compaction,
                    replay: Some(replay),
                    ..
                } = event
                    && replay.value.replay_token().is_none()
                {
                    return Err(CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause: crate::openai::lowering::RepresentationError::ReplayPhase,
                    });
                }
                if let StreamEvent::ItemStarted {
                    item,
                    kind: ItemKind::Reasoning,
                    ..
                } = event
                    && let Disposition::Omit(rule) = self.dispositions.get(DeliveryFact::Reasoning)
                {
                    if self.omitted.len() >= MAX_ITEMS {
                        return Err(CodecError::Limit);
                    }
                    self.omitted.insert(*item);
                    self.loss(rule, Owner::item(*item));
                    continue;
                }
                if crate::openai::lowering::readable_reasoning::owner(event)
                    .is_some_and(|id| self.omitted.contains(&id))
                {
                    continue;
                }
                let mut event = event.clone();
                match &mut event {
                    StreamEvent::ItemStarted {
                        item,
                        kind: ItemKind::Message { phase },
                        ..
                    } => {
                        if phase.is_some()
                            && let Disposition::Omit(rule) =
                                self.dispositions.get(DeliveryFact::Phase)
                        {
                            *phase = None;
                            self.losses.push(
                                rule.effect(ConversionBoundary::EventProject)
                                    .at(Owner::item(*item)),
                            );
                        }
                    }
                    StreamEvent::ItemStarted {
                        item,
                        kind: ItemKind::ToolCall { format, .. },
                        ..
                    } => {
                        self.calls.insert(*item, false);
                        if self.client.protocol() == ClientProtocol::Messages {
                            *format = ArgumentFormat::Json;
                        }
                    }
                    StreamEvent::PartStarted { kind, .. }
                        if self.client.protocol() == ClientProtocol::Messages
                            && *kind == PartKind::Arguments =>
                    {
                        *kind = PartKind::StructuredArguments;
                    }
                    StreamEvent::ValueFinished { item, part }
                        if self.client.protocol() == ClientProtocol::Messages
                            && self.calls.get(item) == Some(&false) =>
                    {
                        selected.push(StreamEvent::Delta {
                            item: *item,
                            part: *part,
                            fragment: "{}".into(),
                            logprobs: vec![],
                        });
                    }
                    StreamEvent::Delta {
                        item,
                        fragment,
                        logprobs,
                        ..
                    } => {
                        if let Some(nonempty) = self.calls.get_mut(item) {
                            *nonempty |= !fragment.is_empty();
                        }
                        if !logprobs.is_empty()
                            && let Disposition::Omit(rule) =
                                self.dispositions.get(DeliveryFact::Probabilities)
                        {
                            logprobs.clear();
                            self.loss(rule, Owner::item(*item));
                        }
                    }
                    StreamEvent::PartStarted { kind, .. } if *kind == PartKind::Refusal => {
                        if let Disposition::Approximate(rule) =
                            self.dispositions.get(DeliveryFact::Refusal)
                        {
                            *kind = PartKind::Text;
                            self.loss(rule, Owner::Root);
                        }
                    }
                    StreamEvent::AnnotationAdded { item, .. } => {
                        if let Disposition::Omit(rule) =
                            self.dispositions.get(DeliveryFact::Annotations)
                        {
                            self.loss(rule, Owner::item(*item));
                            continue;
                        }
                    }
                    StreamEvent::TextMetadata {
                        item,
                        annotations,
                        logprobs,
                        ..
                    } => {
                        if !annotations.is_empty()
                            && let Disposition::Omit(rule) =
                                self.dispositions.get(DeliveryFact::Annotations)
                        {
                            annotations.clear();
                            self.loss(rule, Owner::item(*item));
                        }
                        if let Disposition::Omit(rule) =
                            self.dispositions.get(DeliveryFact::Probabilities)
                        {
                            if logprobs.as_ref().is_some_and(|v| !v.is_empty()) {
                                self.loss(rule, Owner::item(*item));
                            }
                            *logprobs = None;
                        }
                        if self.client.protocol() == ClientProtocol::Messages {
                            continue;
                        }
                    }
                    StreamEvent::LogprobsSnapshot { item, logprobs, .. } => {
                        if let Disposition::Omit(rule) =
                            self.dispositions.get(DeliveryFact::Probabilities)
                        {
                            if !logprobs.is_empty() {
                                self.loss(rule, Owner::item(*item));
                            }
                            continue;
                        }
                    }
                    StreamEvent::SafetyReport(report) if report.is_assessment_only() => {
                        self.losses.push(super::safety_assessment_loss());
                        continue;
                    }
                    StreamEvent::SafetyReport(_) => {
                        return Err(CodecError::Unsupported(CarrierReason::Safety));
                    }
                    StreamEvent::MessageEnvelope(_)
                        if self.client.protocol() == ClientProtocol::Messages =>
                    {
                        continue;
                    }
                    StreamEvent::Progress(progress) => {
                        if self.client.protocol() == ClientProtocol::Messages {
                            self.progress = Some(*progress);
                        } else if !matches!(
                            progress,
                            InteractionProgress::Unreported
                                | InteractionProgress::TurnFinished
                                | InteractionProgress::AwaitingToolResults
                        ) {
                            return Err(CodecError::Unsupported(CarrierReason::Terminal));
                        }
                        continue;
                    }
                    StreamEvent::UsageReport(report)
                        if self.client.protocol() != ClientProtocol::Messages =>
                    {
                        report.validate()?;
                        if report.has_unrepresented_details() {
                            self.reported_loss(crate::usage::report_loss(
                                crate::conversion::generation::ConversionBoundary::EventProject,
                            ));
                        }
                        if report.scope != UsageScope::Operation
                            || report.stage != UsageStage::Final
                        {
                            continue;
                        }
                        let Some(usage) = report.scalar_summary() else {
                            continue;
                        };
                        event = StreamEvent::Usage(usage);
                    }
                    StreamEvent::Terminal { details, .. } => {
                        if self.client.protocol() == ClientProtocol::Responses
                            && matches!(&details.incomplete, Some(IncompleteReason::Other(reason)) if !matches!(reason.as_str(), "max_messages" | "steered"))
                        {
                            details.incomplete = Some(IncompleteReason::Unspecified);
                            self.loss(
                                Rule::OMIT_RESPONSES_UNREPRESENTED_INCOMPLETE_REASON,
                                Owner::Root,
                            );
                        }
                        if let ClientAdapter::OpenAi { adapter, .. } = &self.client {
                            self.losses.extend(
                                crate::openai::terminal::terminal_losses(
                                    details,
                                    adapter.protocol,
                                )?
                                .into_iter()
                                .map(|rule| rule.effect(ConversionBoundary::EventProject)),
                            );
                            details.refusal = None;
                            details.explanation = None;
                        }
                        if details.explanation.as_ref().is_some()
                            && let Disposition::Omit(rule) =
                                self.dispositions.get(DeliveryFact::TerminalExplanation)
                        {
                            details.explanation = None;
                            self.loss(rule, Owner::Root);
                        }
                    }
                    _ => {}
                }
                if self.client.protocol() == ClientProtocol::Messages {
                    let report = match &event {
                        StreamEvent::Usage(usage) => Some(UsageReport::operation(*usage)),
                        StreamEvent::UsageReport(report) => Some(report.clone()),
                        _ => None,
                    };
                    if let Some(report) = report {
                        let scalar_source = match &event {
                            StreamEvent::Usage(usage) => Some(*usage),
                            _ => None,
                        };
                        let selected =
                            match crate::anthropic::messages::accounting::project(&report) {
                                Ok(selected) => selected,
                                // The request-derived completed view is available only at EOF.
                                Err(CodecError::Unsupported(_)) => None,
                                Err(error) => return Err(error),
                            };
                        self.losses
                            .extend(crate::anthropic::messages::client::report_losses(
                                &report,
                                selected.as_ref(),
                                crate::conversion::generation::ConversionBoundary::EventProject,
                            ));
                        if scalar_source.is_some()
                            && scalar_source
                                != selected.as_ref().and_then(UsageReport::scalar_summary)
                        {
                            let effect = crate::anthropic::messages::client::usage_loss()
                                .with_boundary(ConversionBoundary::EventProject);
                            if !self.losses.contains(&effect) {
                                self.losses.push(effect);
                            }
                        }
                        let Some(selected) = selected else { continue };
                        event = StreamEvent::UsageReport(selected);
                    }
                }
                if let StreamEvent::Usage(usage) = &mut event
                    && let ClientAdapter::OpenAi { adapter, .. } = &self.client
                {
                    let profile = adapter.protocol;
                    let rules = adapter.adaptation.rules.clone();
                    let selected = crate::openai::lowering::native_response::client_usage(*usage)?;
                    self.losses
                        .extend(crate::openai::lowering::native_response::usage_loss(
                            Some(*usage),
                            selected,
                            crate::conversion::generation::ConversionBoundary::EventProject,
                        ));
                    let Some(selected) = selected else { continue };
                    *usage = selected;
                    if profile == GenerationApi::Responses {
                        let (value, losses) =
                            crate::openai::lowering::projection::responses_usage(*usage, &rules)
                                .map_err(|cause| CodecError::Representation {
                                    boundary: ConversionBoundary::EventProject,
                                    cause,
                                })?;
                        *usage = value;
                        self.losses.extend(
                            losses
                                .into_iter()
                                .map(|r| r.effect(ConversionBoundary::EventProject)),
                        );
                    }
                }
                if self.client.protocol() == ClientProtocol::Chat
                    && !self.native
                    && self.grouping.is_some()
                {
                    match &event {
                        StreamEvent::ItemStarted { item, kind, .. } => {
                            if matches!(kind, ItemKind::Message { .. } | ItemKind::ToolCall { .. })
                            {
                                self.source_lifecycles
                                    .insert(*item, ItemLifecycle::InProgress);
                            }
                            self.plain_text_source &= matches!(kind, ItemKind::Message { .. });
                        }
                        StreamEvent::PartStarted { item, kind, .. } => {
                            self.plain_text_source &= *kind == PartKind::Text;
                            if *kind == PartKind::Text {
                                self.text_sources.insert(*item);
                            }
                        }
                        StreamEvent::Delta { logprobs, .. } => {
                            self.plain_text_source &= logprobs.is_empty();
                        }
                        StreamEvent::LogprobsSnapshot { logprobs, .. } => {
                            self.plain_text_source &= logprobs.is_empty();
                        }
                        StreamEvent::AnnotationAdded { .. } => {
                            self.plain_text_source = false;
                        }
                        StreamEvent::TextMetadata {
                            annotations,
                            logprobs,
                            ..
                        } => {
                            self.plain_text_source &= annotations.is_empty() && logprobs.is_none();
                        }
                        StreamEvent::ItemFinished { item, status, .. } => {
                            if let Some(observed) = self.source_lifecycles.get_mut(item) {
                                *observed = *status;
                            }
                        }
                        StreamEvent::Terminal { terminal, details } => {
                            let status = if *terminal == StreamTerminal::Incomplete {
                                ItemLifecycle::Incomplete
                            } else {
                                ItemLifecycle::Completed
                            };
                            let changed: Vec<_> = self
                                .source_lifecycles
                                .iter()
                                .filter(|(_, observed)| **observed != status)
                                .map(|(item, observed)| (*item, *observed))
                                .collect();
                            if !changed.is_empty() {
                                if *terminal != StreamTerminal::Incomplete
                                    || details.incomplete != Some(IncompleteReason::MaxOutputTokens)
                                    || !self.plain_text_source
                                    || self.text_sources.len() != self.source_lifecycles.len()
                                    || changed
                                        .iter()
                                        .any(|(_, observed)| *observed == ItemLifecycle::InProgress)
                                {
                                    return Err(CodecError::Representation {
                                        boundary: ConversionBoundary::EventProject,
                                        cause:
                                            crate::openai::lowering::RepresentationError::Terminal,
                                    });
                                }
                                for (item, _) in changed {
                                    self.reported_loss(crate::lowering::disposition::Rule::OMIT_CHAT_TEXT_ITEM_STATUS.effect(ConversionBoundary::EventProject).at(Owner::item(item)));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                selected.push(event);
            }
            if let Some(grouping) = &mut self.grouping {
                selected = grouping.project(&selected)?;
                self.losses.extend(grouping.take_evidence());
            }
            if let Some(schedule) = &mut self.messages {
                let mut ordered = vec![];
                for event in selected {
                    let (events, losses) = schedule.project(&event, self.limits)?;
                    self.losses.extend(losses);
                    for event in events {
                        ordered.push(event);
                    }
                }
                selected = ordered;
            }
            let mut output = vec![];
            for mut event in selected {
                if self.client.protocol() == ClientProtocol::Responses
                    && let StreamEvent::MessageEnvelope(group) = &event
                {
                    let items = snapshot_items(self.state.as_ref().ok_or(CodecError::invalid())?)?;
                    crate::openai::lowering::message_groups::project(
                        &items,
                        std::slice::from_ref(group),
                        GenerationApi::Responses,
                    )
                    .map_err(|cause| CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause,
                    })?;
                    self.losses.push(
                        crate::lowering::disposition::Rule::OMIT_RESPONSES_MESSAGE_ENVELOPES
                            .effect(ConversionBoundary::EventProject),
                    );
                    continue;
                }
                if self.client.protocol() == ClientProtocol::Messages
                    && let StreamEvent::Terminal { terminal, details } = &mut event
                {
                    let state = self.state.as_ref().ok_or(CodecError::invalid())?;
                    let mut reports = crate::anthropic::messages::accounting::project_reports(
                        state.usage_reports(),
                    )?;
                    if reports.operation().is_none()
                        && let Some(usage) = self
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.delivery.as_ref())
                            .and_then(|delivery| delivery.usage)
                    {
                        let report = UsageReport::operation(usage);
                        if let Some(projected) =
                            crate::anthropic::messages::accounting::project(&report)?
                        {
                            reports.replace(projected)?;
                        }
                    }
                    if reports.operation().is_none() {
                        return Err(CodecError::Invalid(crate::failure::ShapeReason::Usage));
                    }
                    if &reports != state.usage_reports() {
                        self.losses.push(crate::usage::report_loss(
                            crate::conversion::generation::ConversionBoundary::EventProject,
                        ));
                        for report in reports.reports() {
                            let usage = StreamEvent::UsageReport(report.clone());
                            self.state = Some(reduce(
                                self.state.take().ok_or(CodecError::invalid())?,
                                usage.clone(),
                            )?);
                            output.push(usage);
                        }
                    }
                    let state = self.state.as_ref().ok_or(CodecError::invalid())?;
                    let calls = state
                        .items()
                        .iter()
                        .any(|item| matches!(item.kind, ItemKind::ToolCall { .. }));
                    let mut progress = self
                        .progress
                        .take()
                        .unwrap_or(InteractionProgress::Unreported);
                    let source = GenerationResponse::from_resources(
                        snapshot_items(state)?,
                        match terminal {
                            StreamTerminal::Completed => Outcome::Completed,
                            StreamTerminal::Incomplete => Outcome::Incomplete,
                            StreamTerminal::Cancelled => Outcome::Cancelled,
                            _ => Outcome::Failed,
                        },
                        state.resources().clone(),
                    )?
                    .with_progress(progress)?
                    .with_details(details.clone())?;
                    if let Some(policy) = self.terminal_policy
                        && let Some((view, effect)) = policy.project(
                            &source,
                            crate::conversion::generation::ConversionBoundary::EventProject,
                        )?
                    {
                        *details = view;
                        self.losses.push(effect);
                    }
                    if !self.native
                        && *terminal == StreamTerminal::Completed
                        && calls
                        && details.completion == Some(CompletionCause::NaturalOrStopSequence)
                    {
                        details.completion = None;
                        progress = InteractionProgress::AwaitingToolResults;
                    } else if self.without_stops
                        && let Some((view, effect)) =
                            crate::anthropic::messages::client::completed_without_stops(
                                &source,
                                crate::conversion::generation::ConversionBoundary::EventProject,
                            )?
                    {
                        *details = view;
                        progress = InteractionProgress::TurnFinished;
                        self.losses.push(effect);
                    } else if self.ambiguous_stops
                        && *terminal == StreamTerminal::Completed
                        && !calls
                        && matches!(
                            progress,
                            InteractionProgress::Unreported | InteractionProgress::TurnFinished
                        )
                        && matches!(
                            details.completion,
                            None | Some(CompletionCause::NaturalOrStopSequence)
                        )
                    {
                        details.completion = Some(CompletionCause::Natural);
                        progress = InteractionProgress::TurnFinished;
                        self.losses.push(Effect::new(
                            "messages.ambiguous-stop-as-end-turn.v1",
                            Field::Completion,
                            ConversionBoundary::EventProject,
                            Change::Approximate,
                        ));
                    }
                    if progress != InteractionProgress::Unreported {
                        let progress = StreamEvent::Progress(progress);
                        self.state = Some(reduce(
                            self.state.take().ok_or(CodecError::invalid())?,
                            progress.clone(),
                        )?);
                        output.push(progress);
                    }
                }
                self.state = Some(reduce(
                    self.state.take().ok_or(CodecError::invalid())?,
                    event.clone(),
                )?);
                let state = self.state.as_ref().expect("reduced event");
                match &event {
                    StreamEvent::ItemStarted { item, .. }
                    | StreamEvent::ItemFinished { item, .. } => {
                        crate::openai::events::sync_replays(
                            state,
                            &mut self.fidelity,
                            Some(*item),
                        )?;
                    }
                    StreamEvent::Terminal { .. } => {
                        crate::openai::events::sync_replays(state, &mut self.fidelity, None)?;
                    }
                    _ => {}
                }
                if matches!(event, StreamEvent::Terminal { .. })
                    && let (ClientAdapter::OpenAi { adapter, contract }, Some(metadata)) =
                        (&self.client, &self.metadata)
                {
                    crate::openai::lowering::validate_response(
                        &self.snapshot()?,
                        &self.fidelity,
                        metadata,
                        adapter.protocol,
                        &adapter.contract(contract),
                    )
                    .map_err(|cause| CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause,
                    })?;
                }
                output.push(event);
            }
            Ok(output)
        })();
        if result.is_err() {
            self.state = None;
        }
        result
    }
}
