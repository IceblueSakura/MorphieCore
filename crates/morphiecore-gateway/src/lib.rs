//! Minimal HTTP ingress over immutable, explicitly bound task entries.
mod body;
mod capture;
mod diagnostics;
mod error;
mod generation;
mod ingress;
mod renewal;
mod routing;
mod runtime;
pub mod startup;
mod tasks;
#[cfg(test)]
mod tests;

pub use crate::execution::delivery::DeltaCoalescing;
#[cfg(test)]
use crate::protocol::openai::GenerationApi;
#[cfg(test)]
use crate::protocol::openai::GenerationCodec;
#[cfg(test)]
use crate::protocol::openai::TargetContract;
#[cfg(test)]
use crate::provider::ProviderDefinition;
#[cfg(test)]
use crate::provider::SecretMaterial;
pub use diagnostics::{DiagnosticsConfig, LogLevel};
use error::ApiError;
pub(crate) use morphiecore_catalog::{provider, topology};
pub(crate) use morphiecore_protocol::{self as protocol, adapter, lowering};
pub(crate) use morphiecore_runtime::{account, configuration, credential, execution, transport};
pub(crate) use morphiecore_semantic as semantic;
use runtime::{BoundCandidate, BoundEntry, Runtime};
pub use startup::credentials::Credentials;
pub use startup::{
    entries::{EmbeddingEntry, Entry, ImageEntry, SpeechEntry, TaskEntries, TranscriptionEntry},
    error::StartupError,
    limits::Limits,
};
#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Gateway {
    state: Arc<Runtime>,
}
/// Optional observation sinks, installed before the gateway is shared.
impl Gateway {
    /// Enable correlated Logs/Traces at a trusted OTLP/HTTP base URL.
    pub fn with_diagnostics(mut self, config: DiagnosticsConfig) -> Result<Self, StartupError> {
        let state = Arc::get_mut(&mut self.state).ok_or(StartupError::Environment)?;
        state.diagnostics =
            Some(crate::diagnostics::Sink::open(&config).map_err(|_| StartupError::Environment)?);
        state.capture = config
            .private_bodies
            .then(|| capture::Sink::new(config.body_bytes));
        Ok(self)
    }
    /// Best-effort bounded drain, for an owner shutting down its test listener.
    pub async fn flush_diagnostics(&self) {
        if let Some(sink) = &self.state.diagnostics {
            sink.flush().await;
        }
    }
    /// Stop observation producers first; a composition root then closes both providers.
    pub async fn shutdown_diagnostics(&self) {
        if let Some(sink) = &self.state.diagnostics {
            sink.shutdown().await;
        }
    }
}
