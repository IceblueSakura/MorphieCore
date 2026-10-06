//! Wire protocol boundary for the shared semantic model.
pub mod adaptation;
pub mod aliyun_asr;
pub mod aliyun_speech;
pub mod cache;
mod decoded;
pub use decoded::{CodecError, DecodedRequest, DecodedResponse, ResponseMetadata};
pub mod extensions;
pub mod fidelity;
pub mod file_constraints;
pub mod image_constraints;
pub mod openai;
pub mod openrouter_images;
pub mod openrouter_speech;
