//! Synthetic cross-module gateway contracts.
mod affinity;
mod conversation;
mod embedding;
mod ingress;
mod models;
mod responses;
pub(in crate::gateway) use morphiecore_test_support::responses_profile as responses_wire;
mod route_fallback;
mod shutdown;
mod speech;
mod subscription;
pub(in crate::gateway) mod support;
pub(in crate::gateway) mod topology;
mod transcription;
