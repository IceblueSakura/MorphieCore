//! Trusted OpenAI-compatible wire rules and named shape adaptations.
//! One `Dialect` names what a Provider changes about the standard API; the
//! trusted binding selects it and no request payload can.
pub(in crate::openai) mod accounting_shapes;
pub mod adaptation;
pub mod adapter_shapes;
pub mod bailian;
pub(in crate::openai) mod inference_shapes;
pub mod modelbest;
mod trusted;

pub use trusted::Dialect;
