//! Wire protocol boundary for the shared semantic model.
pub mod aliyun_asr;
pub mod aliyun_speech;
pub mod anthropic;
pub mod cache;
pub(crate) mod cache_affinity;
mod decoded;
pub use decoded::{CodecError, DecodedRequest, DecodedResponse, ResponseMetadata};
pub mod extensions;
pub mod fidelity;
pub mod file_constraints;
pub mod google;
pub mod image_constraints;
pub mod native;
pub mod openai;
pub mod openrouter_images;
pub mod openrouter_speech;
pub mod replay_envelope;
