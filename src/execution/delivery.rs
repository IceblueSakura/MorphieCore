//! Incremental projection and explicit I/O-owned commit. Encoding is not delivery.
use crate::{
    adapter::{ClientAdapter, ClientEncoder, ClientProtocol},
    execution::{Attempt, AttemptError, Lifecycle},
    protocol::{ResponseMetadata, openai::stream::Obfuscation, stream::SseLimits},
    semantic::{context::StreamOptions, task::generation::StreamEvent},
};
use bytes::Bytes;
use sha2::{Digest, Sha256};

/// Outgoing carrier identity, not an upstream item fact or replay credential.
/// Local decoder counters restart for every response; a history can contain many.
fn delivery_item_id(
    model: &str,
    response: &str,
    owner: crate::semantic::task::generation::ItemId,
) -> String {
    let mut hash = Sha256::new();
    for value in [model, response] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "{}_{digest}",
        crate::protocol::fidelity::generated_item_id(owner)
    )
}

fn response_id() -> Result<String, AttemptError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| AttemptError::Protocol("entropy"))?;
    Ok(format!(
        "resp_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

pub struct ResponseDelivery {
    client: ClientAdapter,
    public_label: String,
    limits: SseLimits,
    options: StreamOptions,
    padding: Option<Obfuscation>,
    encoder: Option<ClientEncoder>,
    source: Option<std::sync::Arc<()>>,
    lifecycle: Lifecycle,
    rendered: bool,
    terminal: bool,
    rejected: bool,
    native_created: Option<serde_json::Number>,
    /// Request-local envelope identity, never stored as upstream metadata or history.
    generated_id: Option<String>,
    storage: Option<(String, crate::protocol::ResponseStorage)>,
}
impl ResponseDelivery {
    pub fn new(
        mut client: ClientAdapter,
        public_label: &str,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Self {
        client.bind_replay_origin(&super::replay::CLIENT_ORIGIN);
        Self {
            client,
            public_label: public_label.into(),
            limits,
            options,
            padding: Some(padding),
            encoder: None,
            source: None,
            lifecycle: Lifecycle::new(),
            rendered: false,
            terminal: false,
            rejected: false,
            native_created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|time| time.as_secs().into()),
            generated_id: None,
            storage: None,
        }
    }
    pub fn is_messages(&self) -> bool {
        self.client.protocol() == ClientProtocol::Messages
    }
    pub(crate) fn with_storage(
        mut self,
        id: &str,
        storage: crate::protocol::ResponseStorage,
    ) -> Self {
        self.storage = Some((id.into(), storage));
        self
    }
    pub fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }
    // Wire IDs can repeat across attempts; only the execution-owned token binds
    // this delivery. A different attempt needs a fresh uncommitted delivery.
    fn bind(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        if self
            .source
            .as_ref()
            .is_some_and(|source| !std::sync::Arc::ptr_eq(source, attempt.identity()))
        {
            self.rejected = true;
            return Err(AttemptError::Delivery("attempt changed"));
        }
        self.source = Some(attempt.identity().clone());
        if self.generated_id.is_none()
            && self.storage.is_none()
            && attempt
                .metadata()
                .is_some_and(|metadata| metadata.id.is_none())
        {
            self.generated_id = Some(response_id()?);
        }
        Ok(())
    }
    fn metadata(&self, attempt: &Attempt) -> Result<ResponseMetadata, AttemptError> {
        let mut metadata = attempt
            .metadata()
            .cloned()
            .ok_or(AttemptError::Protocol("missing response metadata"))?;
        metadata.model = self.public_label.clone();
        if let Some((id, storage)) = &self.storage {
            metadata.id = Some(id.clone());
            metadata.storage = Some(storage.clone());
        }
        if metadata.id.is_none() {
            metadata.id = Some(
                self.generated_id
                    .clone()
                    .ok_or(AttemptError::Delivery("missing delivery identity"))?,
            );
        }
        if attempt.is_native() {
            // This is the Gateway envelope's creation time, not an upstream report.
            metadata.created = Some(
                self.native_created
                    .clone()
                    .ok_or(AttemptError::Protocol("gateway clock"))?,
            );
        }
        Ok(metadata)
    }
    fn ready(&self) -> Result<(), AttemptError> {
        if self.rejected || self.terminal {
            Err(AttemptError::Protocol("delivery closed"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn client_response(
        &self,
        attempt: &Attempt,
    ) -> Result<crate::protocol::DecodedResponse, AttemptError> {
        let mut decoded = attempt.response()?.clone();
        decoded.metadata = self.metadata(attempt)?;
        super::replay::seal_for_client(&mut decoded, &super::replay::CLIENT_ORIGIN)?;
        if attempt.is_native() {
            self.client.project_native(&mut decoded)?;
        }
        if self.client.protocol() == ClientProtocol::Responses || attempt.is_native() {
            for (owner, _) in decoded.semantic.items() {
                if decoded.fidelity.response_item_id(*owner).is_none() {
                    decoded.fidelity.record_response_item_id(
                        *owner,
                        &delivery_item_id(
                            &self.public_label,
                            decoded.metadata.required_id()?,
                            *owner,
                        ),
                    )?;
                }
            }
        }
        Ok(decoded)
    }
    /// Bounded cache-only projection of exactly the delivered values; not stored history.
    pub(crate) fn cache_successor(
        &self,
        attempt: &Attempt,
        request: &crate::adapter::Request,
    ) -> Option<crate::adapter::Request> {
        if self.rejected
            || self.is_messages()
            || self.lifecycle.state() != crate::execution::DeliveryState::Terminal
            || attempt.response().ok()?.semantic.outcome()
                != crate::semantic::task::generation::Outcome::Completed
            || request.task.semantic.items().iter().any(|(_, item)| {
                matches!(item, crate::semantic::task::generation::Item::Reasoning(r) if r.replay.is_some())
            })
        {
            return None;
        }
        let decoded = self.client_response(attempt).ok()?;
        let mut successor = self.client.cache_successor(request, &decoded)?;
        super::replay::open_from_client(&mut successor).ok()?;
        Some(successor)
    }
    pub fn encode_json(&mut self, attempt: &Attempt) -> Result<Vec<u8>, AttemptError> {
        self.encode_json_snapshot(attempt).map(|(_, body)| body)
    }
    /// Finalize JSON delivery and retain the exact projection for stored history.
    pub(crate) fn encode_json_snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        let result = (|| {
            self.ready()?;
            if self.encoder.is_some() {
                return Err(AttemptError::Delivery("stream already started"));
            }
            self.bind(attempt)?;
            let snapshot = self.snapshot(attempt)?;
            self.rendered = true;
            self.terminal = true;
            Ok(snapshot)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    /// The same final representation serves creation, retrieval and stored replay.
    pub(crate) fn snapshot(
        &self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        let (decoded, value) = self.client.snapshot(self.client_response(attempt)?)?;
        let body =
            serde_json::to_vec(&value).map_err(|_| AttemptError::Protocol("body serialization"))?;
        Ok((decoded, body))
    }
    pub fn encode_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            if events.is_empty() {
                return Ok(vec![]);
            }
            if events
                .iter()
                .any(|e| matches!(e, StreamEvent::Terminal { .. }))
            {
                return Err(AttemptError::Delivery(
                    "terminal requires validated upstream EOF",
                ));
            }
            self.encode(attempt, events)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    /// Encode the terminal only after the upstream framing/body has closed cleanly.
    pub fn finish_stream(&mut self, attempt: &Attempt) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let terminal = attempt.terminal_event()?;
            let frames = self.encode(attempt, std::slice::from_ref(terminal))?;
            self.encoder
                .as_ref()
                .ok_or(AttemptError::Delivery("missing stream encoder"))?
                .finish()?;
            self.terminal = true;
            Ok(frames)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    fn encode(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<Vec<Bytes>, AttemptError> {
        self.bind(attempt)?;
        let metadata = self.metadata(attempt)?;
        if self.encoder.is_none() {
            let padding = self
                .padding
                .take()
                .ok_or(AttemptError::Delivery("padding consumed"))?;
            self.encoder = Some(self.client.stream_encoder(
                metadata.clone(),
                self.limits,
                self.options.clone(),
                padding,
            )?);
        }
        let encoder = self.encoder.as_mut().expect("encoder initialized");
        let projected = if attempt.is_native() {
            encoder.project_native(events)?
        } else {
            None
        };
        let events = projected.as_deref().unwrap_or(events);
        let source = attempt
            .fidelity()
            .ok_or(AttemptError::Protocol("missing response fidelity"))?;
        let mut source = std::borrow::Cow::Borrowed(source);
        if self.client.protocol() != ClientProtocol::Messages
            && (self.client.protocol() == ClientProtocol::Responses || attempt.is_native())
        {
            for event in events {
                if let StreamEvent::ItemStarted { item, .. } = event
                    && source.response_item_id(*item).is_none()
                {
                    source.to_mut().record_response_item_id(
                        *item,
                        &delivery_item_id(&self.public_label, metadata.required_id()?, *item),
                    )?;
                }
            }
        }
        let mut frames = vec![];
        encoder.update_metadata(metadata)?;
        for event in events {
            let event = super::replay::seal_event(event.clone(), &super::replay::CLIENT_ORIGIN)?;
            frames.extend(encoder.encode(&event, &source)?);
        }
        self.rendered |= !frames.is_empty();
        Ok(frames)
    }
    /// End a stream with a classified failure (`{"error":{...}}`), never a success
    /// terminal. Responses gets `response.failed`, opening the response when nothing
    /// was sent, since Codex reads `context_length_exceeded` and `insufficient_quota`
    /// only there; Chat gets an error chunk and no `[DONE]`, as OpenAI sends.
    pub fn fail_stream(&mut self, error: &serde_json::Value) -> Result<Vec<Bytes>, AttemptError> {
        let result = (|| {
            self.ready()?;
            let unopened =
                if self.client.protocol() == ClientProtocol::Responses && self.encoder.is_none() {
                    Some(ResponseMetadata {
                        storage: None,
                        id: Some(response_id()?),
                        model: self.public_label.clone(),
                        created: self.native_created.clone(),
                        context: Default::default(),
                        instruction_fidelity: Default::default(),
                    })
                } else {
                    None
                };
            let (frames, terminal) =
                self.client
                    .failure_frames(self.encoder.as_mut(), error, unopened, self.limits)?;
            self.terminal = terminal;
            Ok(frames)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
    }
    /// Call at the external visibility boundary, not when bytes are merely encoded.
    pub fn commit(&mut self) -> Result<(), AttemptError> {
        if self.rejected || !self.rendered {
            return Err(AttemptError::Delivery("no deliverable output"));
        }
        Ok(self.lifecycle.commit()?)
    }
    /// Call after successful final downstream delivery; EOF alone is insufficient.
    pub fn complete(&mut self, attempt: &Attempt) -> Result<(), AttemptError> {
        self.bind(attempt)?;
        attempt.response()?;
        if self.rejected || !self.terminal || self.lifecycle.may_retry_or_fallback() {
            return Err(AttemptError::Delivery("delivery not complete"));
        }
        Ok(self.lifecycle.terminal()?)
    }
    pub fn cancel(&mut self) {
        self.rejected = true;
        self.encoder = None;
        self.padding = None;
        self.source = None;
        let _ = self.lifecycle.terminal();
    }
}
