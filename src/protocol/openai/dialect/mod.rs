//! Trusted OpenAI-compatible wire rules and named shape adaptations.
//! One `Dialect` names what a Provider changes about the standard API; the
//! trusted binding selects it and no request payload can.
pub(in crate::protocol::openai) mod accounting_shapes;
pub mod adaptation;
pub(crate) mod adapter_shapes;
pub(crate) mod bailian;
pub(in crate::protocol::openai) mod inference_shapes;
mod trusted;

pub use trusted::Dialect;
