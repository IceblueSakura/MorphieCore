//! Synthetic cross-module gateway contracts.
mod affinity;
mod conversation;
mod decision;
mod embedding;
mod ingress;
mod listener;
mod models;
mod responses;
pub(crate) use morphiecore_test_support::responses_profile as responses_wire;
mod route_fallback;
mod shutdown;
mod speech;
mod subscription;
pub(crate) mod support;
pub(crate) mod topology;
mod transcription;
