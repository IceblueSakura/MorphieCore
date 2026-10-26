//! Minimal HTTP ingress over immutable, explicitly bound task entries.
mod body;
mod capture;
mod diagnostics;
mod error;
mod generation;
mod ingress;
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
pub use capture::{CaptureConfig, CaptureStatus, Selection as CaptureSelection};
use error::ApiError;
use runtime::{BoundCandidate, BoundEntry, Runtime};
pub use startup::credentials::Credentials;
pub use startup::{
    entries::{
        ChatProfile, EmbeddingEntry, Entry, ImageEntry, SpeechEntry, TaskEntries,
        TranscriptionEntry,
    },
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
    pub fn with_capture(
        mut self,
        config: crate::gateway::CaptureConfig,
    ) -> Result<Self, StartupError> {
        let state = Arc::get_mut(&mut self.state).ok_or(StartupError::Environment)?;
        if config.enabled() {
            state.capture = Some(
                crate::gateway::capture::Sink::open(config)
                    .map_err(|_| StartupError::Environment)?,
            );
        } else {
            state.capture = None;
        }
        Ok(self)
    }
    pub fn capture_status(&self) -> Option<crate::gateway::CaptureStatus> {
        self.state
            .capture
            .as_ref()
            .map(crate::gateway::capture::Sink::status)
    }
    /// Enable bounded metadata in a new private operator file, before sharing the gateway.
    pub fn with_diagnostics(mut self, path: &std::path::Path) -> Result<Self, StartupError> {
        let state = Arc::get_mut(&mut self.state).ok_or(StartupError::Environment)?;
        state.diagnostics = Some(
            crate::gateway::diagnostics::Sink::open(path).map_err(|_| StartupError::Environment)?,
        );
        Ok(self)
    }
    /// Best-effort bounded drain, for an owner shutting down its test listener.
    pub async fn flush_diagnostics(&self) {
        if let Some(sink) = &self.state.diagnostics {
            sink.flush().await;
        }
        if let Some(sink) = &self.state.capture {
            sink.flush().await;
        }
    }
}
