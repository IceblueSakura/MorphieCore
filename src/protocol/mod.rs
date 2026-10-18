//! Wire protocol boundary for the shared semantic model.
pub mod aliyun_asr;
pub mod aliyun_speech;
pub mod anthropic;
pub(crate) mod attachments;
pub mod cache;
pub(crate) mod cache_affinity;
mod decoded;
pub(crate) mod json;
pub mod stream;
pub use decoded::{CodecError, DecodedRequest, DecodedResponse, ResponseMetadata, ResponseStorage};
pub mod extensions;
pub mod fidelity;
pub mod file_constraints;
mod generation_controls;
pub mod google;
pub mod image_constraints;
pub mod interactions;
pub mod native;
pub mod openai;
pub mod openrouter_images;
pub mod openrouter_speech;
pub mod reasoning;
pub mod replay_envelope;
pub(crate) mod usage;
pub mod xiaomi_audio;

/// One pure upstream encoding and its body-free adaptation receipts.
pub(crate) struct RequestProjection {
    pub value: serde_json::Value,
    pub rules: Vec<&'static str>,
    pub controls: Vec<crate::lowering::generation::RequestControlLoss>,
}
