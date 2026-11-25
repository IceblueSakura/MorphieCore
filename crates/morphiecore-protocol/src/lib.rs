//! Wire protocol boundary for the shared semantic model.
//! Protocols own wire grammar and selected projections; shared records preserve
//! owner-local fidelity. No module here selects routes or performs network I/O.

// Generation wire boundaries.
pub mod adapter;
pub mod anthropic;
pub mod google;
pub mod lowering;
pub mod openai;

pub(crate) use morphiecore_semantic as semantic;

// Shared decoded values, fidelity and projection policies.
pub mod attribution;
pub mod cache;
pub use morphiecore_lowering::{
    CodecError, ConversionCause, ConversionFailure, conversion, failure, model_limits, reasoning,
    tool_projection,
};
pub mod request;
pub use request::{ClientProtocol, GenerationRequestEnvelope};
pub mod decoded;
pub mod diagnostics;
pub mod fidelity;
pub mod generation_controls;
pub mod native;
pub mod phase;
pub mod replay_envelope;
pub mod replay_projection;
pub mod reports;
pub mod usage;

// Shared wire validation and framing.
pub mod decision;
pub mod embedding;
pub mod file_constraints;
pub mod image;
pub mod image_constraints;
pub mod json;
pub mod multipart;
pub mod speech;
pub mod sse;
pub mod stream;
pub mod transcription;

// Standalone operation dialects; registration does not activate a binding.
pub mod aliyun_asr;
pub mod aliyun_images;
pub mod aliyun_speech;
pub mod xiaomi_audio;

pub use decoded::{
    DecodedRequest, DecodedResponse, GenerationDelivery, ResponseMetadata, ResponseStorage,
};
