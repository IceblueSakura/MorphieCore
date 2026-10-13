//! Synthetic cross-module gateway contracts.
mod affinity;
mod conversation;
mod credential_fallback;
mod embedding;
mod ingress;
mod models;
mod responses;
mod route_fallback;
mod shutdown;
mod speech;
mod subscription;
pub(in crate::gateway) mod support;
pub(in crate::gateway) mod topology;
mod transcription;
