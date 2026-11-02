//! One client delivery projection. Renderers and retained history consume its events.
use super::*;
use crate::protocol::conversion::generation::{Change, ConversionBoundary, Field, Owner};
use crate::semantic::task::generation::*;
use crate::semantic::value::Presence;
use std::collections::{BTreeMap, BTreeSet};

mod snapshot;
pub(crate) use snapshot::emit;

#[derive(Clone, Copy)]
pub(crate) enum Fact {
    Reasoning,
    Phase,
    ExecutionHints,
    Settings,
    Fingerprint,
    Annotations,
    Probabilities,
    TerminalExplanation,
    Refusal,
    OpeningService,
}
impl Fact {
    pub(crate) const ALL: [Self; 10] = [
        Self::Reasoning,
        Self::Phase,
        Self::ExecutionHints,
        Self::Settings,
        Self::Fingerprint,
        Self::Annotations,
        Self::Probabilities,
        Self::TerminalExplanation,
        Self::Refusal,
        Self::OpeningService,
    ];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Reasoning => "reasoning",
            Self::Phase => "message_phase",
            Self::ExecutionHints => "execution_hints",
            Self::Settings => "reported_settings",
            Self::Fingerprint => "system_fingerprint",
            Self::Annotations => "annotations",
            Self::Probabilities => "probabilities",
            Self::TerminalExplanation => "terminal_explanation",
            Self::Refusal => "refusal",
            Self::OpeningService => "opening_service_report",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Disposition {
    Native,
    Omit(&'static str),
    Approximate(&'static str),
}
impl ClientAdapter {
    pub(crate) fn disposition(&self, fact: Fact, streaming: bool) -> Disposition {
        use Disposition::*;
        use Fact::*;
        match (self.protocol(), fact) {
            // A model-authored refusal is not a classifier stop.
            (ClientProtocol::Messages, Refusal) => Approximate("messages.refusal-as-text.v1"),
            (ClientProtocol::Chat, OpeningService) => Omit("chat.omit-opening-service-report.v1"),
            (ClientProtocol::Chat, Reasoning) if self.omits_readable_reasoning() => {
                Omit("chat.omit-readable-reasoning.v1")
            }
            (ClientProtocol::Chat | ClientProtocol::Messages, Phase) => {
                Omit("generation.omit-delivery-message-phase.v1")
            }
            (ClientProtocol::Messages, ExecutionHints) => {
                Omit("messages.omit-reported-execution-hints.v1")
            }
            (ClientProtocol::Chat, ExecutionHints) if streaming => {
                Omit("chat.omit-stream-execution-hints.v1")
            }
            (ClientProtocol::Chat, ExecutionHints) => {
                Omit("chat.omit-unsupported-execution-hints.v1")
            }
            (ClientProtocol::Messages, Settings) => Omit("messages.omit-incidental-settings.v1"),
            (ClientProtocol::Chat, Settings) => Omit("chat.omit-reported-settings.v1"),
            (ClientProtocol::Messages, Fingerprint) => Omit("messages.omit-system-fingerprint.v1"),
            (ClientProtocol::Responses, Fingerprint) => {
                Omit("responses.omit-system-fingerprint.v1")
            }
            (ClientProtocol::Chat, Annotations) if streaming => {
                Omit("chat.omit-stream-annotations.v1")
            }
            (ClientProtocol::Messages, Annotations) => Omit("messages.omit-annotations.v1"),
            (ClientProtocol::Messages, Probabilities) => {
                Omit("messages.omit-probability-reports.v1")
            }
            (ClientProtocol::Messages, TerminalExplanation) => {
                Omit("messages.omit-terminal-explanation.v1")
            }
            _ => Native,
        }
    }
}

pub(crate) struct ClientProjection {
    client: ClientAdapter,
    streaming: bool,
    native: bool,
    state: Option<StreamState>,
    grouping: Option<crate::protocol::openai::lowering::native_response::EventProjection>,
    omitted: BTreeSet<ItemId>,
    calls: BTreeMap<ItemId, bool>,
    losses: Vec<Effect>,
    messages: Option<crate::protocol::anthropic::messages::client::Schedule>,
    progress: Option<InteractionProgress>,
    limits: crate::protocol::stream::SseLimits,
    without_stops: bool,
    ambiguous_stops: bool,
    terminal_policy: Option<crate::protocol::anthropic::messages::client::GoogleTerminalPolicy>,
    metadata: Option<ResponseMetadata>,
    fidelity: FidelityRecords,
    source_lifecycles: BTreeMap<ItemId, ItemLifecycle>,
    text_sources: BTreeSet<ItemId>,
    plain_text_source: bool,
}
impl ClientProjection {
    pub(crate) fn new(client: ClientAdapter, native: bool, streaming: bool) -> Self {
        let grouping = match client.protocol() {
            ClientProtocol::Chat => Some(if native {
                crate::protocol::openai::lowering::native_response::EventProjection::new(
                    GenerationApi::Chat,
                )
            } else {
                crate::protocol::openai::lowering::native_response::EventProjection::for_phase()
            }),
            ClientProtocol::Responses if native => Some(
                crate::protocol::openai::lowering::native_response::EventProjection::new(
                    GenerationApi::Responses,
                ),
            ),
            _ => None,
        };
        let messages = (client.protocol() == ClientProtocol::Messages).then(Default::default);
        Self {
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
    pub(crate) fn with_limits(mut self, limits: crate::protocol::stream::SseLimits) -> Self {
        self.limits = limits;
        self
    }
    pub(crate) fn with_grouping(mut self, group: bool) -> Self {
        if !group {
            self.grouping = None;
        }
        self
    }
    pub(crate) fn update_fidelity(&mut self, source: &FidelityRecords) -> Result<(), CodecError> {
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
    pub(crate) fn fidelity(&self) -> &FidelityRecords {
        &self.fidelity
    }
    pub(crate) fn with_messages_terminal(
        mut self,
        without_stops: bool,
        ambiguous_stops: bool,
        policy: Option<crate::protocol::anthropic::messages::client::GoogleTerminalPolicy>,
    ) -> Self {
        self.without_stops = without_stops;
        self.ambiguous_stops = ambiguous_stops;
        self.terminal_policy = policy;
        self
    }
    fn loss(&mut self, rule: &'static str, field: Field, owner: Owner) {
        let effect =
            Effect::new(rule, field, ConversionBoundary::EventProject, Change::Omit).at(owner);
        if !self.losses.contains(&effect) {
            self.losses.push(effect);
        }
    }
    pub(crate) fn take_evidence(
        &mut self,
    ) -> crate::protocol::conversion::generation::EvidenceDelta {
        std::mem::take(&mut self.losses).into_iter().collect()
    }
    pub(crate) fn started(&self) -> bool {
        self.state.as_ref().is_some_and(StreamState::started)
    }
    pub(crate) fn finished(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.terminal().is_some())
    }
    pub(crate) fn snapshot(&self) -> Result<GenerationResponse, CodecError> {
        Ok(materialize(
            self.state.as_ref().ok_or(CodecError::invalid())?,
        )?)
    }
    fn reported_loss(&mut self, effect: Effect) {
        if !self.losses.contains(&effect) {
            self.losses.push(effect);
        }
    }
    pub(crate) fn metadata(
        &mut self,
        source: &ResponseMetadata,
    ) -> Result<ResponseMetadata, CodecError> {
        source.validate()?;
        crate::protocol::reports::relation(&source.context.reports, true)?;
        if source.context.reports.iter().any(|report| {
            report
                .settings
                .as_ref()
                .is_some_and(|settings| !settings.safety.is_empty())
        }) {
            return Err(CodecError::Unsupported(CarrierReason::Safety));
        }
        let mut metadata = source.clone();
        if let Disposition::Omit(rule) = self.client.disposition(Fact::Settings, self.streaming) {
            if metadata
                .context
                .reports
                .iter()
                .any(|report| report.settings.is_some())
                || metadata.context.instruction_messages.is_some()
            {
                self.reported_loss(Effect::new(
                    rule,
                    ("response.reported.settings").into(),
                    ConversionBoundary::EventProject,
                    Change::Omit,
                ));
            }
            for report in metadata.context.reports.iter_mut() {
                report.settings = None;
            }
            metadata.context.instruction_messages = None;
            metadata.instruction_fidelity = Default::default();
        }
        if let Disposition::Omit(rule) = self.client.disposition(Fact::Fingerprint, self.streaming)
        {
            if metadata.context.system_fingerprint.as_ref().is_some() {
                self.loss(
                    rule,
                    ("response.context.system_fingerprint").into(),
                    Owner::Root,
                );
            }
            metadata.context.system_fingerprint = None;
        }
        if let Disposition::Omit(rule) = self
            .client
            .disposition(Fact::ExecutionHints, self.streaming)
        {
            let mut meaningful = metadata.context.execution.clone();
            let keep_metadata = self.client.protocol() == ClientProtocol::Chat && !self.streaming;
            if keep_metadata || meaningful.metadata == Presence::Null {
                meaningful.metadata = Presence::Absent;
            }
            if meaningful != Default::default() {
                self.loss(rule, Field::Representation, Owner::Root);
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
                && let Disposition::Omit(rule) = self
                    .client
                    .disposition(Fact::OpeningService, self.streaming)
            {
                self.reported_loss(Effect::new(
                    rule,
                    Field::OpeningReport,
                    ConversionBoundary::EventProject,
                    Change::Omit,
                ));
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
                for loss in crate::protocol::anthropic::messages::client::metadata_losses(&metadata)
                {
                    if !self.losses.contains(&loss) {
                        self.losses.push(loss);
                    }
                }
                crate::protocol::anthropic::messages::client::project_metadata(&mut metadata)?;
            }
            ClientAdapter::OpenAi { adapter, .. } => {
                let profile = adapter.protocol;
                if profile == GenerationApi::Responses
                    && let Some(rule) =
                        crate::protocol::openai::lowering::projection::request_metadata_echo(
                            &metadata,
                        )
                {
                    self.losses
                        .push(rule.effect(ConversionBoundary::EventProject));
                }
                if let Some(rule) =
                    crate::protocol::openai::lowering::projection::reported_settings(
                        &mut metadata,
                        profile,
                    )
                    .map_err(|cause| CodecError::Representation {
                        boundary: ConversionBoundary::EventProject,
                        cause,
                    })?
                {
                    self.reported_loss(rule.effect(ConversionBoundary::EventProject));
                }
                if let Some(rule) =
                    crate::protocol::openai::lowering::projection::reported_service_class(
                        &mut metadata,
                        profile,
                    )
                {
                    self.reported_loss(rule.effect(ConversionBoundary::EventProject));
                }
            }
        }
        self.metadata = Some(metadata.clone());
        Ok(metadata)
    }
    pub(crate) fn snapshot_metadata(
        &mut self,
        source: &ResponseMetadata,
    ) -> Result<ResponseMetadata, CodecError> {
        let mut metadata = self.metadata(source)?;
        if self.client.protocol() == ClientProtocol::Responses
            && crate::protocol::reports::terminal_view(&mut metadata.context.reports)?
        {
            self.reported_loss(
                crate::lowering::disposition::Rule::OMIT_OPENING_REPORT
                    .effect(ConversionBoundary::ResponseProject),
            );
        }
        Ok(metadata)
    }
    pub(crate) fn project(
        &mut self,
        events: &[StreamEvent],
    ) -> Result<Vec<StreamEvent>, CodecError> {
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
                        cause: crate::protocol::openai::lowering::RepresentationError::Tools,
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
                        cause: crate::protocol::openai::lowering::RepresentationError::ReplayPhase,
                    });
                }
                if let StreamEvent::ItemStarted {
                    item,
                    kind: ItemKind::Reasoning,
                    ..
                } = event
                    && let Disposition::Omit(rule) =
                        self.client.disposition(Fact::Reasoning, self.streaming)
                {
                    if self.omitted.len() >= MAX_ITEMS {
                        return Err(CodecError::Limit);
                    }
                    self.omitted.insert(*item);
                    self.loss(rule, Field::DeliveredProjection, Owner::item(*item));
                    continue;
                }
                if crate::protocol::openai::lowering::readable_reasoning::owner(event)
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
                            && let Disposition::Omit(_) =
                                self.client.disposition(Fact::Phase, self.streaming)
                        {
                            *phase = None;
                            self.losses.push(
                                crate::protocol::phase::delivery_omission("event.project")
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
                                self.client.disposition(Fact::Probabilities, self.streaming)
                        {
                            logprobs.clear();
                            self.loss(rule, Field::Representation, Owner::item(*item));
                        }
                    }
                    StreamEvent::PartStarted { kind, .. } if *kind == PartKind::Refusal => {
                        if let Disposition::Approximate(rule) =
                            self.client.disposition(Fact::Refusal, self.streaming)
                        {
                            *kind = PartKind::Text;
                            let effect = Effect::new(
                                rule,
                                ("generation.content.refusal").into(),
                                ConversionBoundary::EventProject,
                                Change::Approximate,
                            );
                            if !self.losses.contains(&effect) {
                                self.losses.push(effect);
                            }
                        }
                    }
                    StreamEvent::AnnotationAdded { item, .. } => {
                        if let Disposition::Omit(rule) =
                            self.client.disposition(Fact::Annotations, self.streaming)
                        {
                            self.loss(rule, Field::Representation, Owner::item(*item));
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
                                self.client.disposition(Fact::Annotations, self.streaming)
                        {
                            annotations.clear();
                            self.loss(rule, Field::Representation, Owner::item(*item));
                        }
                        if let Disposition::Omit(rule) =
                            self.client.disposition(Fact::Probabilities, self.streaming)
                        {
                            if logprobs.as_ref().is_some_and(|v| !v.is_empty()) {
                                self.loss(rule, Field::Representation, Owner::item(*item));
                            }
                            *logprobs = None;
                        }
                        if self.client.protocol() == ClientProtocol::Messages {
                            continue;
                        }
                    }
                    StreamEvent::LogprobsSnapshot { item, logprobs, .. } => {
                        if let Disposition::Omit(rule) =
                            self.client.disposition(Fact::Probabilities, self.streaming)
                        {
                            if !logprobs.is_empty() {
                                self.loss(rule, Field::Representation, Owner::item(*item));
                            }
                            continue;
                        }
                    }
                    StreamEvent::SafetyReport(report) if report.is_assessment_only() => {
                        self.losses
                            .push(super::safety_assessment_loss("event.project"));
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
                            self.reported_loss(crate::protocol::usage::report_loss(
                                "event.project",
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
                                "responses.omit-unrepresented-incomplete-reason.v1",
                                Field::Incomplete,
                                Owner::Root,
                            );
                        }
                        if let ClientAdapter::OpenAi { adapter, .. } = &self.client {
                            self.losses.extend(
                                crate::protocol::openai::terminal::terminal_losses(
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
                            && let Disposition::Omit(rule) = self
                                .client
                                .disposition(Fact::TerminalExplanation, self.streaming)
                        {
                            details.explanation = None;
                            self.loss(rule, Field::Representation, Owner::Root);
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
                            match crate::protocol::anthropic::messages::accounting::project(&report)
                            {
                                Ok(selected) => selected,
                                // The request-derived completed view is available only at EOF.
                                Err(CodecError::Unsupported(_)) => None,
                                Err(error) => return Err(error),
                            };
                        self.losses.extend(
                            crate::protocol::anthropic::messages::client::report_losses(
                                &report,
                                selected.as_ref(),
                                "event.project",
                            ),
                        );
                        if scalar_source.is_some()
                            && scalar_source
                                != selected.as_ref().and_then(UsageReport::scalar_summary)
                        {
                            let effect = crate::protocol::anthropic::messages::client::usage_loss()
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
                    let selected =
                        crate::protocol::openai::lowering::native_response::client_usage(*usage)?;
                    self.losses.extend(
                        crate::protocol::openai::lowering::native_response::usage_loss(
                            Some(*usage),
                            selected,
                            "event.project",
                        ),
                    );
                    let Some(selected) = selected else { continue };
                    *usage = selected;
                    if profile == GenerationApi::Responses {
                        let (value, losses) =
                            crate::protocol::openai::lowering::projection::responses_usage(
                                *usage, &rules,
                            )
                            .map_err(|cause| {
                                CodecError::Representation {
                                    boundary: ConversionBoundary::EventProject,
                                    cause,
                                }
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
                                    return Err(CodecError::Representation { boundary: ConversionBoundary::EventProject, cause: crate::protocol::openai::lowering::RepresentationError::Terminal });
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
                    crate::protocol::openai::lowering::message_groups::project(
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
                    let mut reports =
                        crate::protocol::anthropic::messages::accounting::project_reports(
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
                            crate::protocol::anthropic::messages::accounting::project(&report)?
                        {
                            reports.replace(projected)?;
                        }
                    }
                    if reports.operation().is_none() {
                        return Err(CodecError::Invalid(
                            crate::protocol::failure::ShapeReason::Usage,
                        ));
                    }
                    if &reports != state.usage_reports() {
                        self.losses
                            .push(crate::protocol::usage::report_loss("event.project"));
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
                        && let Some((view, effect)) = policy.project(&source, "event.project")?
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
                            crate::protocol::anthropic::messages::client::completed_without_stops(
                                &source,
                                "event.project",
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
                        crate::protocol::openai::events::sync_replays(
                            state,
                            &mut self.fidelity,
                            Some(*item),
                        )?;
                    }
                    StreamEvent::Terminal { .. } => {
                        crate::protocol::openai::events::sync_replays(
                            state,
                            &mut self.fidelity,
                            None,
                        )?;
                    }
                    _ => {}
                }
                if matches!(event, StreamEvent::Terminal { .. })
                    && let (ClientAdapter::OpenAi { adapter, contract }, Some(metadata)) =
                        (&self.client, &self.metadata)
                {
                    crate::protocol::openai::lowering::validate_response(
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
