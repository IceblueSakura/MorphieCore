//! Incremental projection and explicit I/O-owned commit. Encoding is not delivery.
use crate::{
    adapter::Adapter,
    execution::{Attempt, AttemptError, Lifecycle},
    lowering::generation::GenerationRepresentationContract,
    protocol::{
        ResponseMetadata,
        openai::{
            Profile,
            chat_sse::ChatSseEncoder,
            sse::{Obfuscation, ResponsesSseEncoder, SseLimits},
        },
    },
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

enum Encoder {
    Responses(ResponsesSseEncoder),
    Chat(ChatSseEncoder),
}

pub struct ResponseDelivery {
    adapter: Adapter,
    contract: GenerationRepresentationContract,
    public_label: String,
    limits: SseLimits,
    options: StreamOptions,
    padding: Option<Obfuscation>,
    encoder: Option<Encoder>,
    source: Option<std::sync::Arc<()>>,
    lifecycle: Lifecycle,
    rendered: bool,
    terminal: bool,
    rejected: bool,
}
impl ResponseDelivery {
    pub fn new(
        adapter: Adapter,
        contract: GenerationRepresentationContract,
        public_label: &str,
        limits: SseLimits,
        options: StreamOptions,
        padding: Obfuscation,
    ) -> Self {
        Self {
            adapter,
            contract,
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
        }
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
        Ok(())
    }
    fn metadata(&self, attempt: &Attempt) -> Result<ResponseMetadata, AttemptError> {
        let mut metadata = attempt
            .metadata()
            .cloned()
            .ok_or(AttemptError::Protocol("missing response metadata"))?;
        metadata.model = self.public_label.clone();
        Ok(metadata)
    }
    fn ready(&self) -> Result<(), AttemptError> {
        if self.rejected || self.terminal {
            Err(AttemptError::Protocol("delivery closed"))
        } else {
            Ok(())
        }
    }
    fn client_response(
        &self,
        attempt: &Attempt,
    ) -> Result<crate::protocol::DecodedResponse, AttemptError> {
        let mut decoded = attempt.response()?.clone();
        decoded.metadata = self.metadata(attempt)?;
        if self.adapter.protocol == Profile::Responses {
            for (owner, _) in decoded.semantic.items() {
                if decoded.fidelity.response_item_id(*owner).is_none() {
                    decoded.fidelity.record_response_item_id(
                        *owner,
                        &delivery_item_id(&self.public_label, &decoded.metadata.id, *owner),
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
        use crate::protocol::cache_affinity::{MAX_PROMPT_BYTES, prefixes};
        if self.rejected
            || self.lifecycle.state() != crate::execution::DeliveryState::Terminal
            || attempt.response().ok()?.semantic.outcome()
                != crate::semantic::task::generation::Outcome::Completed
        {
            return None;
        }
        let mut input = self
            .adapter
            .encode_request(request, &self.public_label, &self.contract)
            .ok()?;
        prefixes(&input, self.adapter.protocol)?;
        let decoded = self.client_response(attempt).ok()?;
        let output = self
            .adapter
            .encode_response(&decoded, &self.contract)
            .ok()?;
        crate::semantic::value::json_size(&output, MAX_PROMPT_BYTES).ok()?;
        let (field, items) = match self.adapter.protocol {
            Profile::Chat => (
                "messages",
                vec![output.get("choices")?.get(0)?.get("message")?.clone()],
            ),
            Profile::Responses => ("input", output.get("output")?.as_array()?.clone()),
        };
        input.get_mut(field)?.as_array_mut()?.extend(items);
        prefixes(&input, self.adapter.protocol)?;
        crate::semantic::value::json_size(&input, MAX_PROMPT_BYTES).ok()?;
        self.adapter
            .decode_request(&serde_json::to_vec(&input).ok()?)
            .ok()
    }
    pub fn encode_json(&mut self, attempt: &Attempt) -> Result<Vec<u8>, AttemptError> {
        let result = (|| {
            self.ready()?;
            if self.encoder.is_some() {
                return Err(AttemptError::Delivery("stream already started"));
            }
            self.bind(attempt)?;
            let decoded = self.client_response(attempt)?;
            let value = self.adapter.encode_response(&decoded, &self.contract)?;
            let body = serde_json::to_vec(&value)
                .map_err(|_| AttemptError::Protocol("body serialization"))?;
            self.rendered = true;
            self.terminal = true;
            Ok(body)
        })();
        if result.is_err() {
            self.rejected = true;
        }
        result
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
            match self.encoder.as_ref() {
                Some(Encoder::Responses(e)) => e.finish()?,
                Some(Encoder::Chat(e)) => e.finish()?,
                None => return Err(AttemptError::Delivery("missing stream encoder")),
            }
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
            let contract = self.adapter.contract(&self.contract);
            let padding = self
                .padding
                .take()
                .ok_or(AttemptError::Delivery("padding consumed"))?;
            self.encoder = Some(match self.adapter.protocol {
                Profile::Responses => Encoder::Responses(ResponsesSseEncoder::new(
                    metadata.clone(),
                    contract,
                    self.limits,
                    padding,
                )?),
                Profile::Chat => Encoder::Chat(ChatSseEncoder::new(
                    metadata.clone(),
                    contract,
                    self.limits,
                    self.options.clone(),
                    padding,
                )?),
            });
        }
        let source = attempt
            .fidelity()
            .ok_or(AttemptError::Protocol("missing response fidelity"))?;
        let mut source = std::borrow::Cow::Borrowed(source);
        if self.adapter.protocol == Profile::Responses {
            for event in events {
                if let StreamEvent::ItemStarted { item, .. } = event
                    && source.response_item_id(*item).is_none()
                {
                    source.to_mut().record_response_item_id(
                        *item,
                        &delivery_item_id(&self.public_label, &metadata.id, *item),
                    )?;
                }
            }
        }
        let mut frames = vec![];
        match self.encoder.as_mut().expect("encoder initialized") {
            Encoder::Responses(encoder) => {
                encoder.update_metadata(metadata)?;
                for event in events {
                    frames.extend(encoder.encode(event, &source)?);
                }
            }
            Encoder::Chat(encoder) => {
                encoder.update_metadata(metadata)?;
                for event in events {
                    frames.extend(encoder.encode(event, &source)?);
                }
            }
        }
        self.rendered |= !frames.is_empty();
        Ok(frames)
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
