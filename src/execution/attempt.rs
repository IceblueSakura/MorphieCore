//! Attempt preparation and bounded intake. Wire projection belongs to adapters;
//! transport I/O and downstream commit belong to their respective owners.
use crate::{
    adapter::{AdapterError, Request, UpstreamAdapter},
    lowering::generation::RepresentationError,
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata,
        fidelity::FidelityRecords,
        openai::{
            Profile,
            chat_sse::ChatSseDecoder,
            sse::{ResponsesSseDecoder, SseError, SseLimits},
        },
    },
    provider::{ErrorClass, ProviderDefinition, SecretMaterial, StatusClass, classify_status},
    semantic::task::generation::{GenerationRequirements, StreamEvent},
    topology::{Endpoint, PublicModel},
};

#[derive(Debug, thiserror::Error)]
pub enum AttemptError {
    #[error(transparent)]
    Representation(#[from] RepresentationError),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error(transparent)]
    Sse(#[from] SseError),
    #[error(transparent)]
    Lifecycle(#[from] super::lifecycle::LifecycleError),
    #[error("upstream returned HTTP {status}")]
    Status { status: u16, class: ErrorClass },
    #[error("transport failure")]
    Transport(ErrorClass),
    #[error("protocol boundary: {0}")]
    Protocol(&'static str),
    #[error("bounded body or stream budget exceeded")]
    Limit,
    #[error("delivery mismatch: {0}")]
    Delivery(&'static str),
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

pub fn admit(
    public: &PublicModel,
    request: &Request,
) -> Result<GenerationRequirements, AttemptError> {
    if public.id.as_str() != request.model {
        return Err(AttemptError::Protocol("public model mismatch"));
    }
    request.check_context(public.standard_context)?;
    Ok(request.check_semantic(&public.contract)?)
}

pub(crate) fn operation_auth(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
) -> Result<crate::provider::AuthScheme, AttemptError> {
    endpoint
        .validate()
        .map_err(|_| AttemptError::Protocol("invalid endpoint"))?;
    let entry = provider
        .generation_entry(endpoint.protocol.operation())
        .ok_or(AttemptError::Protocol("undeclared operation"))?;
    if provider.id != endpoint.provider
        || provider.origin != endpoint.target.origin
        || entry.path != &endpoint.target.path
        || entry.auth.kind() != endpoint.execution.credential_kind
        || entry.auth.kind() != provider.auth.kind()
    {
        return Err(AttemptError::Protocol("provider/endpoint mismatch"));
    }
    Ok(entry.auth)
}

pub fn prepare(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<UpstreamRequest, AttemptError> {
    let auth = operation_auth(endpoint, provider)?;
    if endpoint.protocol == crate::topology::ProtocolProfile::AnthropicMessages
        && endpoint
            .representation
            .replay_origin
            .as_ref()
            .map(|o| o.as_str())
            != Some(provider.id.as_str())
    {
        return Err(AttemptError::Protocol("native provider replay scope"));
    }
    let streaming = request.delivery.streaming()
        || endpoint
            .representation
            .adaptation
            .rules
            .responses_forced_stream;
    if streaming && !endpoint.execution.streaming {
        return Err(AttemptError::Delivery("endpoint cannot stream"));
    }
    let value = endpoint.adapter().encode_request(
        request,
        &endpoint.upstream_model,
        &endpoint.representation,
    )?;
    let body =
        serde_json::to_vec(&value).map_err(|_| AttemptError::Protocol("body serialization"))?;
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
    if matches!(endpoint.adapter(), UpstreamAdapter::AnthropicMessages(_)) {
        let session = if endpoint.representation.adaptation.rules.opencode_go_headers {
            Some(endpoint.representation.cache.opencode_session(
                request.conversation.as_ref(),
                request.cache_session.as_ref(),
                request.inferred_affinity,
            )?)
        } else {
            None
        };
        safe_headers.extend(super::messages::native_headers(
            endpoint,
            session.as_deref(),
        )?);
    } else {
        safe_headers.extend(
            endpoint
                .adapter()
                .request_headers(request, endpoint.representation.cache)?,
        );
    }
    Ok(UpstreamRequest {
        origin: endpoint.target.origin.as_str().into(),
        method: "POST",
        path: endpoint.target.path.as_str().into(),
        safe_headers,
        auth_header: auth.auth_header(secret),
        body,
    })
}

enum Intake {
    Pending,
    Static(Vec<u8>),
    Responses(ResponsesSseDecoder),
    Chat(ChatSseDecoder),
    Closed,
}

/// Holds bounded decoder state, not a replay log. A terminal is withheld until
/// strict EOF validation so a truncated/trailing-invalid body cannot emit success.
pub struct Attempt {
    identity: std::sync::Arc<()>,
    adapter: UpstreamAdapter,
    body_limit: usize,
    limits: SseLimits,
    intake: Intake,
    decoded: Option<DecodedResponse>,
    terminal: Option<StreamEvent>,
    rejected: bool,
    native_source: Option<(
        crate::protocol::DecodedRequest,
        crate::protocol::anthropic::ReplayTarget,
        crate::semantic::task::generation::LocalScope,
    )>,
}
impl Attempt {
    pub fn new(adapter: impl Into<UpstreamAdapter>, body_limit: usize, limits: SseLimits) -> Self {
        Self {
            identity: std::sync::Arc::new(()),
            adapter: adapter.into(),
            body_limit,
            limits,
            intake: Intake::Pending,
            decoded: None,
            terminal: None,
            rejected: false,
            native_source: None,
        }
    }
    /// Explicit immutable request context for native static intake, captured before I/O.
    pub fn native_messages(
        source: crate::protocol::DecodedRequest,
        target: crate::protocol::anthropic::ReplayTarget,
        scope: crate::semantic::task::generation::LocalScope,
        body_limit: usize,
    ) -> Result<Self, AttemptError> {
        let profile = crate::protocol::anthropic::Profile::AdaptiveTextTools;
        profile.encode_generation_request(&source.semantic, &source.fidelity, &target)?;
        let mut attempt = Self::new(
            UpstreamAdapter::AnthropicMessages(profile),
            body_limit,
            SseLimits::default(),
        );
        attempt.native_source = Some((source, target, scope));
        Ok(attempt)
    }
    pub fn begin(&mut self, status: u16, content_type: &str) -> Result<(), AttemptError> {
        if self.rejected || !matches!(self.intake, Intake::Pending) {
            return Err(AttemptError::Protocol("attempt already began"));
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
            if matches!(self.adapter, UpstreamAdapter::AnthropicMessages(_)) {
                if self.native_source.is_none() {
                    return Err(AttemptError::Protocol("missing native request context"));
                }
                if media != "application/json"
                    && !(media.starts_with("application/") && media.ends_with("+json"))
                {
                    return Err(AttemptError::Protocol("native intake requires JSON"));
                }
                self.intake = Intake::Static(vec![]);
                return Ok(());
            }
            let adapter = self.adapter.openai()?;
            self.intake = if media == "text/event-stream" {
                match adapter.protocol {
                    Profile::Chat => Intake::Chat(ChatSseDecoder::with_decoder(
                        status,
                        content_type,
                        self.limits,
                        adapter.event_decoder(),
                    )?),
                    Profile::Responses => Intake::Responses(ResponsesSseDecoder::with_decoder(
                        status,
                        content_type,
                        self.limits,
                        adapter.event_decoder(),
                    )?),
                }
            } else if media == "application/json"
                || media.starts_with("application/") && media.ends_with("+json")
            {
                Intake::Static(vec![])
            } else {
                return Err(AttemptError::Protocol("unexpected response media type"));
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
        if self.rejected {
            return Err(AttemptError::Protocol("rejected attempt"));
        }
        let result = (|| {
            let (n, mut events) = match &mut self.intake {
                Intake::Static(body) => {
                    if body.len().saturating_add(chunk.len()) > self.body_limit {
                        return Err(AttemptError::Limit);
                    }
                    body.extend_from_slice(chunk);
                    (chunk.len(), vec![])
                }
                Intake::Responses(decoder) => decoder.consume(chunk)?,
                Intake::Chat(decoder) => decoder.consume(chunk)?,
                _ => return Err(AttemptError::Protocol("attempt is closed")),
            };
            if matches!(events.last(), Some(StreamEvent::Terminal { .. })) {
                self.terminal = events.pop();
            }
            Ok((n, events))
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    pub(crate) fn identity(&self) -> &std::sync::Arc<()> {
        &self.identity
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        if let Some(decoded) = &self.decoded {
            return Some(&decoded.metadata);
        }
        match &self.intake {
            Intake::Responses(d) => d.metadata(),
            Intake::Chat(d) => d.metadata(),
            _ => None,
        }
    }
    pub fn fidelity(&self) -> Option<&FidelityRecords> {
        if let Some(decoded) = &self.decoded {
            return Some(&decoded.fidelity);
        }
        match &self.intake {
            Intake::Responses(d) => Some(d.fidelity()),
            Intake::Chat(d) => Some(d.fidelity()),
            _ => None,
        }
    }
    pub fn finish(&mut self) -> Result<&DecodedResponse, AttemptError> {
        if self.rejected {
            return Err(AttemptError::Protocol("rejected attempt"));
        }
        if self.decoded.is_none() {
            let result = (|| -> Result<DecodedResponse, AttemptError> {
                Ok(match &mut self.intake {
                    Intake::Static(body) => match (&self.adapter, &self.native_source) {
                        (
                            UpstreamAdapter::AnthropicMessages(profile),
                            Some((source, target, scope)),
                        ) => profile.decode_generation_response(body, *scope, source, target)?,
                        _ => self.adapter.decode_response(body)?,
                    },
                    Intake::Responses(d) => {
                        d.finish()?;
                        d.materialize()?
                    }
                    Intake::Chat(d) => {
                        d.finish()?;
                        d.materialize()?
                    }
                    _ => return Err(AttemptError::Protocol("attempt is closed")),
                })
            })();
            match result {
                Ok(decoded) => {
                    self.decoded = Some(decoded);
                    self.intake = Intake::Closed;
                    self.native_source = None;
                }
                Err(error) => {
                    self.reject();
                    return Err(error);
                }
            }
        }
        self.decoded
            .as_ref()
            .ok_or(AttemptError::Protocol("attempt not finished"))
    }
    pub fn response(&self) -> Result<&DecodedResponse, AttemptError> {
        if self.rejected {
            return Err(AttemptError::Protocol("rejected attempt"));
        }
        self.decoded
            .as_ref()
            .ok_or(AttemptError::Protocol("attempt not finished"))
    }
    pub(crate) fn terminal_event(&self) -> Result<&StreamEvent, AttemptError> {
        self.response()?;
        self.terminal
            .as_ref()
            .ok_or(AttemptError::Delivery("upstream was not a stream"))
    }
    pub fn cancel(&mut self) {
        self.reject();
    }
    fn reject(&mut self) {
        self.rejected = true;
        self.intake = Intake::Closed;
        self.decoded = None;
        self.terminal = None;
        self.native_source = None;
    }
}
