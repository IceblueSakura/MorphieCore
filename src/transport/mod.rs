//! Semantically blind transport edges. Framing remains pure; HTTP receives only
//! prepared trusted targets/headers/body and never chooses semantic policy.

pub mod http;
pub mod sse;
