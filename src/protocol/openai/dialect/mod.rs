//! Trusted OpenAI-compatible wire rules and named shape adaptations.
pub(in crate::protocol::openai) mod accounting_shapes;
pub mod adaptation;
pub(crate) mod adapter_shapes;
pub(in crate::protocol::openai) mod inference_shapes;
