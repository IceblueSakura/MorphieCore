//! Trusted model/API sampling policy; interpreted by the shared request disposition owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplingPolicy {
    /// Exact native controls, only where the selected API/model supports them.
    Native,
    Reject,
    /// The selected model ignores these controls.
    Ignored,
}
