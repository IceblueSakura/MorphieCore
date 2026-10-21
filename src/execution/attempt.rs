//! Attempt preparation and bounded intake. Wire projection belongs to adapters;
//! transport I/O and downstream commit belong to their respective owners.
use crate::{
    adapter::{AdapterError, GenerationDecoder, Request, UpstreamAdapter},
    lowering::generation::RepresentationError,
    protocol::{
        CodecError, DecodedResponse, ResponseMetadata,
        fidelity::FidelityRecords,
        stream::{SseError, SseLimits},
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
impl AttemptError {
    /// The HTTP status of an upstream error reported inside the stream.
    pub fn stream_failure(&self) -> Option<u16> {
        match self {
            Self::Codec(CodecError::StreamFailure(status))
            | Self::Sse(SseError::Codec(CodecError::StreamFailure(status))) => Some(*status),
            _ => None,
        }
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

pub fn admit(
    public: &PublicModel,
    request: &Request,
) -> Result<GenerationRequirements, AttemptError> {
    if public.id.as_str() != request.model {
        return Err(AttemptError::Protocol("public model mismatch"));
    }
    request.check_context(public.standard_context)?;
    let mut contract = public.contract.clone();
    // Messages tool errors have an explicit candidate-local projection on
    // non-native targets; admission must not erase them for native candidates.
    if request.source == Some(crate::adapter::ClientProtocol::Messages) {
        contract.tool_result_errors = true;
    }
    Ok(request.check_semantic(&contract)?)
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
        || endpoint.declared_path(entry.path) != endpoint.target.path.as_str()
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
    prepare_with_rules(endpoint, provider, secret, request).map(|(prepared, _)| prepared)
}

/// Also names the degradable rules applied to this candidate's body, for diagnostics.
pub fn prepare_with_rules(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<(UpstreamRequest, Vec<&'static str>), AttemptError> {
    prepare_with_evidence(endpoint, provider, secret, request)
        .map(|(prepared, rules, _)| (prepared, rules))
}
pub(crate) fn prepare_with_evidence(
    endpoint: &Endpoint,
    provider: &ProviderDefinition,
    secret: &SecretMaterial,
    request: &Request,
) -> Result<
    (
        UpstreamRequest,
        Vec<&'static str>,
        Vec<crate::lowering::generation::RequestControlLoss>,
    ),
    AttemptError,
> {
    let auth = operation_auth(endpoint, provider)?;
    let streaming = request.delivery.streaming() || endpoint.representation.forced_stream();
    if streaming && !endpoint.execution.streaming {
        return Err(AttemptError::Delivery("endpoint cannot stream"));
    }
    let projection = endpoint.encode_request_with_evidence(request)?;
    let value = projection.value;
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
    safe_headers.extend(endpoint.request_headers(request, &value)?);
    Ok((
        UpstreamRequest {
            origin: endpoint.target.origin.as_str().into(),
            method: "POST",
            path: endpoint.request_path(),
            safe_headers,
            auth_header: auth.auth_header(secret),
            body,
        },
        projection.rules,
        projection.controls,
    ))
}

enum Intake {
    Pending,
    Static(Vec<u8>),
    Stream(Box<GenerationDecoder>),
    Finished(Box<DecodedResponse>),
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
    identity: std::sync::Arc<()>,
    adapter: UpstreamAdapter,
    body_limit: usize,
    limits: SseLimits,
    intake: Intake,
    terminal: Option<StreamEvent>,
}
impl Attempt {
    /// Native observations (structured arguments, native stops) need projection
    /// before OpenAI delivery.
    pub(crate) fn is_native(&self) -> bool {
        self.adapter.is_native()
    }
    pub(crate) fn is_responses(&self) -> bool {
        self.adapter
            .openai()
            .is_ok_and(|adapter| adapter.protocol == crate::protocol::openai::Profile::Responses)
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
            identity: std::sync::Arc::new(()),
            adapter: adapter.into(),
            body_limit,
            limits,
            intake: Intake::Pending,
            terminal: None,
        }
    }
    pub fn begin(&mut self, status: u16, content_type: &str) -> Result<(), AttemptError> {
        if !matches!(self.intake, Intake::Pending) {
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
            self.intake = if media == "text/event-stream" {
                Intake::Stream(Box::new(self.adapter.stream_decoder(
                    status,
                    content_type,
                    self.limits,
                    self.body_limit,
                )?))
            } else if self.adapter.accepts_json()
                && (media == "application/json"
                    || media.starts_with("application/") && media.ends_with("+json"))
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
        if matches!(self.intake, Intake::Rejected) {
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
                Intake::Stream(decoder) => decoder.consume(chunk)?,
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
    /// Unregistered upstream event types skipped so far.
    pub fn ignored_events(&self) -> usize {
        match &self.intake {
            Intake::Stream(d) => d.ignored_events(),
            _ => 0,
        }
    }
    pub fn metadata(&self) -> Option<&ResponseMetadata> {
        match &self.intake {
            Intake::Finished(decoded) => Some(&decoded.metadata),
            Intake::Stream(d) => d.metadata(),
            _ => None,
        }
    }
    pub fn fidelity(&self) -> Option<&FidelityRecords> {
        match &self.intake {
            Intake::Finished(decoded) => Some(&decoded.fidelity),
            Intake::Stream(d) => Some(d.fidelity()),
            _ => None,
        }
    }
    pub fn finish(&mut self) -> Result<&DecodedResponse, AttemptError> {
        if matches!(self.intake, Intake::Rejected) {
            return Err(AttemptError::Protocol("rejected attempt"));
        }
        if !matches!(self.intake, Intake::Finished(_)) {
            let result = (|| -> Result<DecodedResponse, AttemptError> {
                Ok(match &mut self.intake {
                    Intake::Static(body) => self.adapter.decode_response(body)?,
                    Intake::Stream(d) => d.finish()?,
                    _ => return Err(AttemptError::Protocol("attempt is closed")),
                })
            })();
            match result {
                Ok(decoded) => {
                    self.intake = Intake::Finished(Box::new(decoded));
                }
                Err(error) => {
                    self.reject();
                    return Err(error);
                }
            }
        }
        self.response()
    }
    pub fn response(&self) -> Result<&DecodedResponse, AttemptError> {
        match &self.intake {
            Intake::Finished(decoded) => Ok(decoded),
            Intake::Rejected => Err(AttemptError::Protocol("rejected attempt")),
            _ => Err(AttemptError::Protocol("attempt not finished")),
        }
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
        self.intake = Intake::Rejected;
        self.terminal = None;
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::{
        execution::ResponseDelivery,
        protocol::{
            native::ReplayTarget,
            openai::{Adapter, Dialect, Profile, TargetCaps, obfuscation::Obfuscation},
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
                "responseId":"reported","modelVersion":"model",
                "candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"MAX_TOKENS"}]
            })
        );
        let (_, events) = attempt.push(wire.as_bytes()).unwrap();
        attempt.finish().unwrap();
        // Simulate a protocol that does not report response identity.
        let Intake::Finished(decoded) = &mut attempt.intake else {
            panic!("finished attempt")
        };
        decoded.metadata.id = None;
        let observed = attempt.response().unwrap().clone();
        let delivery = || {
            ResponseDelivery::new(
                crate::adapter::ClientAdapter::openai(
                    Adapter::new(Profile::Responses, Dialect::MorphieCore, None),
                    TargetCaps::full(),
                ),
                "public",
                SseLimits::default(),
                StreamOptions::default(),
                Obfuscation::Disabled,
            )
        };
        let mut stream = delivery();
        let mut frames = stream.encode_events(&attempt, &events).unwrap();
        frames.extend(stream.finish_stream(&attempt).unwrap());
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
            crate::protocol::openai::events::EventEncoder::new(Profile::Responses, without_id,)
                .is_err()
        );
    }

    #[test]
    fn required_native_wire_id_is_not_repaired_by_delivery() {
        let mut attempt = Attempt::new(
            UpstreamAdapter::GoogleGenerateContent {
                target: ReplayTarget::new("synthetic", "model").unwrap(),
            },
            4096,
            SseLimits::default(),
        );
        attempt.begin(200, "text/event-stream").unwrap();
        assert!(
            attempt
                .push(b"data: {\"modelVersion\":\"model\"}\n\n")
                .is_err()
        );
        assert!(attempt.finish().is_err());
    }
}
