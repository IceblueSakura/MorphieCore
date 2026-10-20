//! OpenAI bidirectional adapter: one profile plus the adaptation a binding selected.
mod request;
pub use crate::protocol::openai::dialect::Dialect;
use crate::{
    adapter::{AdapterError, Request},
    protocol::{
        CodecError, DecodedResponse,
        openai::{
            self, Profile, TargetCaps, dialect::adaptation::Adaptation, events::EventDecoder,
            lowering::lower_response,
        },
    },
    semantic::value::ReplayOrigin,
};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Adapter {
    pub protocol: Profile,
    pub adaptation: Adaptation,
}
impl Adapter {
    pub fn new(protocol: Profile, dialect: Dialect, scope: Option<ReplayOrigin>) -> Self {
        Self {
            protocol,
            adaptation: dialect.adaptation(protocol, scope),
        }
    }
    pub fn contract(&self, contract: &TargetCaps) -> TargetCaps {
        let mut result = contract.clone();
        result.adaptation = self.adaptation.clone();
        result.images.intersect(&self.adaptation.images);
        result.files.intersect(&self.adaptation.files);
        result.cache.intersect(self.adaptation.cache);
        result
    }
    pub fn decode_response(&self, bytes: &[u8]) -> Result<DecodedResponse, CodecError> {
        let value = crate::protocol::json::decode(bytes)?;
        let mut decoded = match self.protocol {
            Profile::Chat => {
                openai::chat::envelope::validate_response(&value)?;
                openai::chat::response::decode_with(&value, &self.adaptation)?
            }
            Profile::Responses => {
                openai::responses::envelope::validate_response_snapshot_with(
                    &value,
                    &self.adaptation,
                    openai::DecodeDirection::Upstream,
                )?;
                openai::responses::response::decode_with(&value, &self.adaptation)?
            }
        };
        if let Some(scope) = &self.adaptation.scope {
            decoded.fidelity.bind_replay_origin(scope)?;
        }
        for (_, item) in decoded.semantic.items() {
            if let crate::semantic::task::generation::Item::Message(m) = item {
                for p in &m.parts {
                    if matches!(
                        p.content,
                        crate::semantic::task::generation::ContentPart::Audio(_)
                            | crate::semantic::task::generation::ContentPart::AudioReference(_)
                    ) {
                        decoded.fidelity.record_audio(
                            p.id,
                            &p.content,
                            self.adaptation.scope.clone(),
                        )?;
                    }
                }
            }
        }
        Ok(decoded)
    }
    pub fn encode_response(
        &self,
        response: &DecodedResponse,
        contract: &TargetCaps,
    ) -> Result<Value, AdapterError> {
        let target = lower_response(
            &response.semantic,
            &response.fidelity,
            &response.metadata,
            self.protocol,
            self.contract(contract),
        )?;
        Ok(match self.protocol {
            Profile::Chat => openai::chat::envelope::encode_response(&target)?,
            Profile::Responses => openai::responses::envelope::encode_response(&target)?,
        })
    }
    pub fn event_decoder(&self) -> EventDecoder {
        let decoder = EventDecoder::new(self.protocol).with_adaptation(self.adaptation.clone());
        match &self.adaptation.scope {
            Some(scope) => decoder.with_replay_origin(scope.clone()),
            None => decoder,
        }
    }
}
