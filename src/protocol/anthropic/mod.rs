//! Selected native wire types, not a second semantic model or an execution API.
mod request;
mod response;
mod shape;
use crate::protocol::CodecError;
pub use request::*;
pub use response::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    AdaptiveTextTools,
}
impl Profile {
    pub fn decode_request(self, bytes: &[u8]) -> Result<Request, CodecError> {
        Request::read(&shape::parse(bytes)?)
    }
    pub fn encode_request(self, request: &Request) -> Result<Value, CodecError> {
        request.validate()?;
        let wire = request.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
    pub fn decode_response(self, bytes: &[u8]) -> Result<Message, CodecError> {
        Message::read(&shape::parse(bytes)?, false)
    }
    pub fn encode_response(self, message: &Message) -> Result<Value, CodecError> {
        message.validate(false)?;
        let wire = message.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
    /// Validates one event's shape only; ordering/finality belongs to the stream decoder.
    pub fn decode_event(self, name: &str, bytes: &[u8]) -> Result<Event, CodecError> {
        let value = shape::parse(bytes)?;
        if value.get("type").and_then(Value::as_str) != Some(name) {
            return Err(CodecError::Invalid("Anthropic event type"));
        }
        Event::read(&value)
    }
    pub fn encode_event(self, event: &Event) -> Result<Value, CodecError> {
        event.validate()?;
        let wire = event.wire();
        shape::bounded(&wire)?;
        Ok(wire)
    }
}
