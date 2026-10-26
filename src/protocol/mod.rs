//! Wire protocol boundary for the shared semantic model.
//! Protocols own wire grammar and selected projections; shared records preserve
//! owner-local fidelity. No module here selects routes or performs network I/O.

// Generation wire boundaries.
pub mod anthropic;
pub mod google;
pub mod grok;
pub mod openai;

// Shared decoded values, fidelity and projection policies.
pub(crate) mod attribution;
pub mod cache;
pub mod conversion;
mod request;
pub use conversion::{ConversionCause, ConversionFailure};
pub use request::{ClientProtocol, GenerationRequestEnvelope};
mod decoded;
pub(crate) mod diagnostics;
pub mod failure;
pub mod fidelity;
mod generation_controls;
pub mod model_limits;
pub mod native;
pub(crate) mod phase;
pub mod reasoning;
pub mod replay_envelope;
pub(crate) mod replay_projection;
pub(crate) mod reports;
pub(crate) mod usage;

// Shared wire validation and framing.
pub mod embedding;
pub mod file_constraints;
pub mod image;
pub mod image_constraints;
pub(crate) mod json;
pub mod speech;
pub mod stream;
pub mod transcription;

// Standalone operation dialects; registration does not activate a binding.
pub mod aliyun_asr;
pub mod aliyun_speech;
pub mod openrouter_images;
pub mod openrouter_speech;
pub mod xiaomi_audio;

pub use decoded::{
    CodecError, DecodedRequest, DecodedResponse, GenerationDelivery, ResponseMetadata,
    ResponseStorage,
};
