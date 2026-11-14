//! Attempt preparation and bounded intake. Wire projection belongs to adapters;
//! transport I/O and downstream commit belong to their respective owners.
use crate::{
    adapter::{GenerationDecoder, UpstreamAdapter},
    protocol::{
        CodecError, ConversionFailure, DecodedResponse, GenerationRequestEnvelope as Request,
        ResponseMetadata,
        conversion::generation::RepresentationError,
        failure::{CarrierReason, SourceFailureEvidence},
        fidelity::FidelityRecords,
        stream::{SseError, SseLimits},
    },
    provider::{ProviderDefinition, RecoveryClass, SecretMaterial, StatusClass, classify_status},
    semantic::task::generation::StreamEvent,
    topology::{Endpoint, PublicModel},
};

#[derive(Clone, Debug, thiserror::Error)]
pub enum AttemptError {
    #[error("resource budget exceeded: {0:?}")]
    Resource(crate::protocol::failure::Resource),
    #[error("deadline elapsed: {0:?}")]
    Deadline(DeadlineReason),
    #[error(transparent)]
    Representation(#[from] RepresentationError),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Conversion(#[from] ConversionFailure),
    #[error(transparent)]
    Sse(#[from] SseError),
    #[error(transparent)]
    Lifecycle(#[from] super::lifecycle::LifecycleError),
    #[error("upstream returned HTTP {status}")]
    Status { status: u16, class: RecoveryClass },
    #[error("transport failure")]
    Transport(crate::transport::http::TransportError),
    #[error("protocol boundary: {0}")]
    Protocol(ProtocolReason),
    #[error("bounded body or stream budget exceeded")]
    Limit,
    #[error("delivery mismatch: {0}")]
    Delivery(DeliveryReason),
}
impl AttemptError {
    pub fn codec(&self) -> Option<&CodecError> {
        match self {
            Self::Codec(e)
            | Self::Sse(SseError::Codec(e))
            | Self::Conversion(ConversionFailure {
                cause: crate::protocol::ConversionCause::Codec(e),
                ..
            }) => Some(e),
            _ => None,
        }
    }
    pub fn representation(&self) -> Option<&RepresentationError> {
        match self {
            Self::Representation(e)
            | Self::Conversion(ConversionFailure {
                cause: crate::protocol::ConversionCause::Representation(e),
                ..
            }) => Some(e),
            _ => match self.codec() {
                Some(CodecError::Representation { cause, .. }) => Some(cause),
                _ => None,
            },
        }
    }
    pub fn locus(&self) -> Option<crate::protocol::conversion::generation::SemanticLocus> {
        if let Self::Conversion(e) = self
            && e.locus.is_some()
        {
            return e.locus;
        }
        match self.codec() {
            Some(CodecError::Unsupported(CarrierReason::Field(field))) => {
                Some(crate::protocol::conversion::generation::SemanticLocus {
                    field: *field,
                    owner: crate::protocol::conversion::generation::Owner::Root,
                })
            }
            _ => None,
        }
    }

    pub fn boundary(&self) -> Option<crate::protocol::conversion::generation::ConversionBoundary> {
        match self {
            Self::Conversion(e) => e.boundary,
            Self::Codec(CodecError::Representation { boundary, .. })
            | Self::Sse(SseError::Codec(CodecError::Representation { boundary, .. })) => {
                Some(*boundary)
            }
            _ => None,
        }
    }
    pub fn source_failure(&self) -> Option<SourceFailureEvidence> {
        match self {
            Self::Codec(CodecError::StreamFailure(e))
            | Self::Sse(SseError::Codec(CodecError::StreamFailure(e))) => Some(*e),
            Self::Conversion(crate::protocol::ConversionFailure {
                cause: crate::protocol::ConversionCause::Codec(CodecError::StreamFailure(e)),
                ..
            }) => Some(*e),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeadlineReason {
    RequestBody,
    Recovery,
    Attempt,
    Exchange,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolReason {
    AttemptAlreadyBegan,
    AttemptIsClosed,
    AttemptNotFinished,
    BodySerialization,
    DeliveryClosed,
    EmbeddingAdmission,
    EmbeddingBindingMismatch,
    EmbeddingSerialization,
    Entropy,
    GatewayClock,
    ImageBindingMismatch,
    ImageSerialization,
    InvalidEndpoint,
    MissingResponseFidelity,
    MissingResponseMetadata,
    ProviderEndpointMismatch,
    PublicModelMismatch,
    RejectedAttempt,
    SpeechAdmission,
    SpeechBindingMismatch,
    SpeechSerialization,
    TranscriptionBinding,
    TranscriptionSerialization,
    UndeclaredOperation,
    UnexpectedResponseMediaType,
    ResponseLength,
}
impl std::fmt::Display for ProtocolReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryReason {
    BodyConsumerClosed,
    PendingContentNeedsDrain,
    AttemptChanged,
    DeliveryNotComplete,
    EndpointCannotStream,
    MissingDeliveryIdentity,
    MissingStreamEncoder,
    NoDeliverableOutput,
    PaddingConsumed,
    StreamAlreadyStarted,
    TerminalProjectionSourceChanged,
    TerminalRequiresValidatedUpstreamEOF,
    UpstreamWasNotAStream,
}
impl std::fmt::Display for DeliveryReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct UpstreamRequest {
    pub origin: String,
    pub method: &'static str,
    pub path: String,
    pub safe_headers: Vec<(String, String)>,
    pub auth_header: (String, String),
    pub body: Vec<u8>,
}
impl std::fmt::Debug for UpstreamRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamRequest")
            .field("method", &self.method)
            .field("auth", &"[redacted]")
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}

pub fn admit(public: &PublicModel, request: &Request) -> Result<(), AttemptError> {
    if public.id.as_str() != request.model {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::PublicModelMismatch,
        ));
    }
    crate::lowering::request::admit_source(request)?;
    Ok(())
}

pub(crate) fn operation_auth(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
) -> Result<crate::provider::AuthScheme, AttemptError> {
    endpoint.validate().map_err(|_| {
        AttemptError::Protocol(crate::execution::attempt::ProtocolReason::InvalidEndpoint)
    })?;
    let entry = provider
        .generation_entry(endpoint.protocol.operation())
        .ok_or(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::UndeclaredOperation,
        ))?;
    if provider.id != endpoint.provider
        || provider.origin != endpoint.target.origin
        || endpoint.declared_path(entry.path).as_ref().ok() != Some(&endpoint.target.path)
        || entry.auth.kind() != endpoint.execution.credential_kind
        || entry.auth.kind() != provider.auth.kind()
    {
        return Err(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::ProviderEndpointMismatch,
        ));
    }
    Ok(entry.auth)
}

pub fn prepare(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<UpstreamRequest, AttemptError> {
    prepare_with_evidence(endpoint, provider, secret, request).map(|projection| projection.value)
}

/// Also names the degradable rules applied to this candidate's body, for diagnostics.
pub fn prepare_with_rules(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<(UpstreamRequest, Vec<&'static str>), AttemptError> {
    prepare_with_evidence(endpoint, provider, secret, request).map(|projection| {
        let rules = projection.evidence.rule_names();
        (projection.value, rules)
    })
}
/// An immutable wire value paired with its bounded serialization measurement.
pub(crate) struct RequestProjection {
    value: serde_json::Value,
    bytes: usize,
    evidence: crate::protocol::conversion::generation::EvidenceDelta,
}
impl RequestProjection {
    pub(crate) fn new(
        projection: crate::protocol::conversion::generation::Conversion<serde_json::Value>,
        limit: usize,
    ) -> Result<Self, crate::semantic::value::ValueError> {
        let bytes = crate::semantic::value::json_size(&projection.value, limit)?;
        Ok(Self {
            value: projection.value,
            bytes,
            evidence: projection.evidence,
        })
    }
    pub(crate) fn encode(endpoint: &Endpoint, request: &Request) -> Result<Self, AttemptError> {
        Self::new(
            endpoint.encode_request_with_evidence(request)?,
            endpoint.execution.request_body_limit,
        )
        .map_err(|_| AttemptError::Limit)
    }
    pub(crate) fn for_client(
        endpoint: &Endpoint,
        request: &Request,
        client: &crate::adapter::ClientAdapter,
    ) -> Result<Self, AttemptError> {
        crate::execution::plan::project_candidate(endpoint, request, Some(client)).map_err(
            |reason| match reason {
                crate::execution::plan::RejectionReason::Conversion(cause) => {
                    AttemptError::Conversion(*cause)
                }
                crate::execution::plan::RejectionReason::BodyLimit => AttemptError::Limit,
                _ => AttemptError::Protocol(ProtocolReason::RejectedAttempt),
            },
        )
    }
    pub(crate) fn value(&self) -> &serde_json::Value {
        &self.value
    }
    pub(crate) fn encoded_len(&self) -> usize {
        self.bytes
    }
    pub(crate) fn evidence(&self) -> &crate::protocol::conversion::generation::EvidenceDelta {
        &self.evidence
    }
}

/// Credential-free final wire data. No generic JSON reconstruction is needed
/// after encoding; reserved schema keys retain their ordinary object semantics.
pub(crate) struct EncodedRequest {
    body: Vec<u8>,
    headers: Vec<(String, String)>,
    // Filled only by gateway lookup materialization, never by direct preparation.
    pub(crate) prefixes: Option<Vec<[u8; 32]>>,
}
impl EncodedRequest {
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            + self.body.capacity()
            + self.headers.capacity() * std::mem::size_of::<(String, String)>()
            + self
                .headers
                .iter()
                .map(|(name, value)| name.capacity() + value.capacity())
                .sum::<usize>()
            + self.prefixes.as_ref().map_or(0, |prefixes| {
                prefixes.capacity() * std::mem::size_of::<[u8; 32]>()
            })
    }
}

pub(crate) fn prepare_with_evidence(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<crate::protocol::conversion::generation::Conversion<UpstreamRequest>, AttemptError> {
    prepare_from(endpoint, provider, secret, request, || {
        encode_projection(
            endpoint,
            request,
            RequestProjection::encode(endpoint, request)?,
        )
    })
}

/// Consume a request-local, credential-free conversion accepted by preflight.
pub(crate) fn prepare_retained(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
    projection: crate::protocol::conversion::generation::Conversion<EncodedRequest>,
) -> Result<crate::protocol::conversion::generation::Conversion<UpstreamRequest>, AttemptError> {
    prepare_from(endpoint, provider, secret, request, || Ok(projection))
}

pub(crate) fn encode_projection(
    endpoint: &Endpoint,
    request: &Request,
    projection: RequestProjection,
) -> Result<crate::protocol::conversion::generation::Conversion<EncodedRequest>, AttemptError> {
    let value = projection.value;
    let mut body = Vec::with_capacity(projection.bytes);
    serde_json::to_writer(&mut body, &value)
        .map_err(|_| AttemptError::Protocol(ProtocolReason::BodySerialization))?;
    Ok(crate::protocol::conversion::generation::Conversion {
        value: EncodedRequest {
            body,
            headers: endpoint.request_headers(request, &value)?,
            prefixes: None,
        },
        evidence: projection.evidence,
    })
}

fn prepare_from(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
    project: impl FnOnce() -> Result<
        crate::protocol::conversion::generation::Conversion<EncodedRequest>,
        AttemptError,
    >,
) -> Result<crate::protocol::conversion::generation::Conversion<UpstreamRequest>, AttemptError> {
    let auth = operation_auth(endpoint, provider)?;
    let streaming = request.delivery.streaming() || endpoint.protocol.requires_streaming_intake();
    if streaming && !endpoint.execution.streaming {
        return Err(AttemptError::Delivery(
            crate::execution::attempt::DeliveryReason::EndpointCannotStream,
        ));
    }
    let projection = project()?;
    let EncodedRequest { body, headers, .. } = projection.value;
    if body.len() > endpoint.execution.request_body_limit {
        return Err(AttemptError::Limit);
    }
    let mut safe_headers = vec![
        ("content-type".into(), "application/json".into()),
        (
            "accept".into(),
            if streaming {
                "text/event-stream"
            } else {
                "application/json"
            }
            .into(),
        ),
    ];
    safe_headers.extend(headers);
    Ok(crate::protocol::conversion::generation::Conversion {
        value: UpstreamRequest {
            origin: endpoint.target.origin.as_str().into(),
            method: "POST",
            path: endpoint.request_path(),
            safe_headers,
            auth_header: auth.auth_header(secret),
            body,
        },
        evidence: projection.evidence,
    })
}
enum Intake {
    Pending,
    Static(Vec<u8>),
    Stream {
        decoder: Box<GenerationDecoder>,
        terminal: Option<StreamEvent>,
    },
    Finished {
        decoded: Box<DecodedResponse>,
        terminal: Option<StreamEvent>,
    },
    Rejected,
}

impl Attempt {
    /// Borrow the existing static intake buffer for private diagnostics before finish.
    pub(crate) fn static_body(&self) -> Option<&[u8]> {
        match &self.intake {
            Intake::Static(bytes) => Some(bytes),
            _ => None,
        }
    }
}

/// Holds bounded decoder state, not a replay log. A terminal is withheld until
/// strict EOF validation so a truncated/trailing-invalid body cannot emit success.
pub struct Attempt {
    decode_failure: Option<DecodeFailure>,
    evidence: crate::protocol::conversion::generation::EvidenceDelta,
    identity: std::sync::Arc<()>,
    adapter: UpstreamAdapter,
    body_limit: usize,
    limits: SseLimits,
    intake: Intake,
}
impl Attempt {
    /// Native observations (structured arguments, native stops) need projection
    /// before OpenAI delivery.
    pub(crate) fn is_native(&self) -> bool {
        self.adapter.is_native()
    }
    pub(crate) fn is_responses(&self) -> bool {
        self.adapter.openai().is_ok_and(|adapter| {
            adapter.protocol == crate::protocol::openai::GenerationApi::Responses
        })
    }
    pub(crate) fn messages_terminal_source(
        &self,
    ) -> Option<crate::protocol::anthropic::messages::client::GoogleTerminalPolicy> {
        use crate::protocol::anthropic::messages::client::GoogleTerminalPolicy as Policy;
        match self.adapter {
            UpstreamAdapter::GoogleGenerateContent { .. } => {
                Some(Policy::GenerateContentWithoutStops)
            }
            UpstreamAdapter::GoogleInteractions { .. } => Some(Policy::InteractionsWithoutStops),
            _ => None,
        }
    }
    pub fn new(adapter: impl Into<UpstreamAdapter>, body_limit: usize, limits: SseLimits) -> Self {
        Self {
            decode_failure: None,
            evidence: Default::default(),
            identity: std::sync::Arc::new(()),
            adapter: adapter.into(),
            body_limit,
            limits,
            intake: Intake::Pending,
        }
    }
    pub fn begin(&mut self, status: u16, content_type: &str) -> Result<(), AttemptError> {
        if !matches!(self.intake, Intake::Pending) {
            return Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::AttemptAlreadyBegan,
            ));
        }
        let result = (|| {
            if let StatusClass::Failure(class) = classify_status(status) {
                return Err(AttemptError::Status { status, class });
            }
            let media = content_type
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            self.intake = if media == "text/event-stream" {
                Intake::Stream {
                    decoder: Box::new(self.adapter.stream_decoder(
                        status,
                        content_type,
                        self.limits,
                        self.body_limit,
                    )?),
                    terminal: None,
                }
            } else if self.adapter.accepts_json()
                && (media == "application/json"
                    || media.starts_with("application/") && media.ends_with("+json"))
            {
                Intake::Static(vec![])
            } else {
                return Err(AttemptError::Protocol(
                    crate::execution::attempt::ProtocolReason::UnexpectedResponseMediaType,
                ));
            };
            Ok(())
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    /// At most one upstream frame is consumed; the caller retains the suffix and
    /// awaits downstream readiness before invoking this again.
    pub fn push(&mut self, chunk: &[u8]) -> Result<(usize, Vec<StreamEvent>), AttemptError> {
        if matches!(self.intake, Intake::Rejected) {
            return Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::RejectedAttempt,
            ));
        }
        let result = (|| {
            let (n, events) = match &mut self.intake {
                Intake::Static(body) => {
                    if body.len().saturating_add(chunk.len()) > self.body_limit {
                        return Err(AttemptError::Limit);
                    }
                    body.extend_from_slice(chunk);
                    (chunk.len(), vec![])
                }
                Intake::Stream { decoder, terminal } => {
                    let (n, mut events) = decoder.consume(chunk)?;
                    self.evidence.extend(decoder.take_evidence());
                    if matches!(events.last(), Some(StreamEvent::Terminal { .. })) {
                        *terminal = events.pop();
                    }
                    (n, events)
                }
                _ => {
                    return Err(AttemptError::Protocol(
                        crate::execution::attempt::ProtocolReason::AttemptIsClosed,
                    ));
                }
            };
            Ok((n, events))
        })();
        if result.is_err() {
            self.observe_decode_failure("push");
            self.reject();
        }
        result
    }
    /// Drain effects from successfully returned source units, including EOF.
    /// Rejection preserves any earlier undrained evidence, never a failed unit's delta.
    pub fn take_evidence(&mut self) -> crate::protocol::conversion::generation::EvidenceDelta {
        std::mem::take(&mut self.evidence)
    }
    pub(crate) fn identity(&self) -> &std::sync::Arc<()> {
        &self.identity
    }
    /// Unregistered upstream event types skipped so far.
    pub fn ignored_events(&self) -> usize {
        match &self.intake {
            Intake::Stream { decoder, .. } => decoder.ignored_events(),
            _ => 0,
        }
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        match &self.intake {
            Intake::Finished { decoded, .. } => Some(&decoded.metadata),
            Intake::Stream { decoder, .. } => decoder.metadata(),
            _ => None,
        }
    }

    pub fn fidelity(&self) -> Option<&FidelityRecords> {
        match &self.intake {
            Intake::Finished { decoded, .. } => Some(&decoded.fidelity),
            Intake::Stream { decoder, .. } => Some(decoder.fidelity()),
            _ => None,
        }
    }
    pub fn finish(&mut self) -> Result<&DecodedResponse, AttemptError> {
        if matches!(self.intake, Intake::Rejected) {
            return Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::RejectedAttempt,
            ));
        }
        if !matches!(self.intake, Intake::Finished { .. }) {
            let result = (|| -> Result<Intake, AttemptError> {
                let (decoded, terminal) = match &mut self.intake {
                    Intake::Static(body) => (self.adapter.decode_response(body)?, None),
                    Intake::Stream { decoder, terminal } => (decoder.finish()?, terminal.take()),
                    _ => {
                        return Err(AttemptError::Protocol(
                            crate::execution::attempt::ProtocolReason::AttemptIsClosed,
                        ));
                    }
                };
                Ok(Intake::Finished {
                    decoded: Box::new(decoded),
                    terminal,
                })
            })();
            match result {
                Ok(mut finished) => {
                    if let Intake::Finished { decoded, .. } = &mut finished {
                        self.evidence.extend(std::mem::take(&mut decoded.evidence));
                    }
                    self.intake = finished;
                }
                Err(error) => {
                    self.observe_decode_failure("finish");
                    self.reject();
                    return Err(error);
                }
            }
        }
        self.response()
    }
    pub fn response(&self) -> Result<&DecodedResponse, AttemptError> {
        match &self.intake {
            Intake::Finished { decoded, .. } => Ok(decoded),
            Intake::Rejected => Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::RejectedAttempt,
            )),
            _ => Err(AttemptError::Protocol(
                crate::execution::attempt::ProtocolReason::AttemptNotFinished,
            )),
        }
    }
    pub(crate) fn terminal_event(&self) -> Result<&StreamEvent, AttemptError> {
        self.response()?;
        match &self.intake {
            Intake::Finished {
                terminal: Some(terminal),
                ..
            } => Ok(terminal),
            _ => Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::UpstreamWasNotAStream,
            )),
        }
    }
    /// A validated successful terminal removes only the recovery-to-output clock.
    /// Transport EOF and decoder finish are still required for publication/completion.
    pub(crate) fn recovery_ready_terminal(&self) -> bool {
        use crate::semantic::task::generation::{IncompleteReason, StreamTerminal};
        matches!(&self.intake, Intake::Stream { terminal: Some(StreamEvent::Terminal { terminal, details }), .. }
            if *terminal == StreamTerminal::Completed
                || (*terminal == StreamTerminal::Incomplete && details.incomplete == Some(IncompleteReason::MaxOutputTokens)))
    }
    /// A validated failed Responses terminal is evidence even before transport EOF.
    /// It does not establish successful completion or permit terminal publication.
    pub(crate) fn reported_failure(&self) -> Option<SourceFailureEvidence> {
        if !self.is_responses() {
            return None;
        }
        if let Intake::Stream { decoder, .. } = &self.intake
            && let Some(source) = decoder.event_failure()
        {
            return Some(source);
        }
        if let Some(source) = self.metadata().and_then(|m| m.source_failure) {
            return Some(source);
        }
        match &self.intake {
            Intake::Stream {
                terminal:
                    Some(StreamEvent::Terminal {
                        terminal:
                            crate::semantic::task::generation::StreamTerminal::Failed
                            | crate::semantic::task::generation::StreamTerminal::Error,
                        details,
                    }),
                ..
            } => Some(crate::protocol::openai::terminal::failed_terminal(details)),
            _ => {
                crate::protocol::openai::terminal::failed_response(&self.response().ok()?.semantic)
            }
        }
    }
    pub fn cancel(&mut self) {
        self.reject();
    }
    fn observe_decode_failure(&mut self, operation: &'static str) {
        let last_event = match &self.intake {
            Intake::Stream { decoder, .. } => decoder.last_event(),
            _ => None,
        };
        self.decode_failure.get_or_insert(DecodeFailure {
            operation,
            last_event,
        });
    }
    pub(crate) fn decode_failure(&self) -> Option<DecodeFailure> {
        self.decode_failure
    }
    pub(crate) fn has_terminal(&self) -> bool {
        matches!(
            &self.intake,
            Intake::Stream {
                terminal: Some(_),
                ..
            } | Intake::Finished {
                terminal: Some(_),
                ..
            }
        )
    }
    fn reject(&mut self) {
        self.intake = Intake::Rejected;
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::{
        execution::ResponseDelivery,
        protocol::{
            native::ReplayTarget,
            openai::{
                Dialect, GenerationApi, GenerationCodec, TargetContract, obfuscation::Obfuscation,
            },
        },
        semantic::context::StreamOptions,
    };
    use serde_json::{Value, json};

    #[test]
    fn absent_upstream_id_is_filled_only_on_the_stable_delivery_copy() {
        let mut attempt = Attempt::new(
            UpstreamAdapter::GoogleGenerateContent {
                target: ReplayTarget::new("synthetic", "model").unwrap(),
            },
            4096,
            SseLimits::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        let wire = format!(
            "data: {}\n\n",
            json!({
                "modelVersion":"model",
                "candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"MAX_TOKENS"}]
            })
        );
        let (_, events) = attempt.push(wire.as_bytes()).unwrap();
        attempt.finish().unwrap();
        let observed = attempt.response().unwrap().clone();
        let delivery = || {
            ResponseDelivery::new(
                crate::adapter::ClientAdapter::openai(
                    GenerationCodec::new(GenerationApi::Responses, Dialect::MorphieCore, None),
                    TargetContract::full(),
                ),
                "public",
                SseLimits::default(),
                StreamOptions::default(),
                Obfuscation::Disabled,
            )
        };
        let mut stream = delivery();
        let mut frames = stream
            .encode_events(&attempt, &events)
            .map(|batch| batch.into_bytes())
            .unwrap();
        frames.extend(
            stream
                .finish_stream(&attempt)
                .map(|batch| batch.into_bytes())
                .unwrap(),
        );
        let wire = String::from_utf8(frames.into_iter().flatten().collect()).unwrap();
        let responses: Vec<Value> = wire
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data: ")
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v.get("response").cloned())
            })
            .collect();
        assert!(responses.len() >= 2);
        let id = responses[0]["id"].as_str().unwrap();
        assert!(id.starts_with("resp_"));
        assert!(responses.iter().all(|r| r["id"] == id));
        let mut json_delivery = delivery();
        let body: Value =
            serde_json::from_slice(&json_delivery.encode_json(&attempt).unwrap()).unwrap();
        assert!(body["id"].as_str().unwrap().starts_with("resp_"));
        assert_ne!(body["id"], id);
        assert_eq!(attempt.response().unwrap(), &observed);
        assert!(observed.metadata.id.is_none());
        assert!(observed.metadata.created.is_none());
        observed.metadata.validate().unwrap();
        let mut without_id = observed.metadata.clone();
        without_id.created = Some(1.into());
        assert!(
            crate::protocol::openai::events::EventEncoder::new(
                GenerationApi::Responses,
                without_id,
                crate::protocol::openai::TargetContract::full(),
            )
            .is_err()
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodeFailure {
    pub operation: &'static str,
    pub last_event: Option<crate::protocol::stream::SourceEvent>,
}
