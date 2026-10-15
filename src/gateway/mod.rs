//! Minimal HTTP ingress over immutable, explicitly bound task entries.
mod body;
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
#[cfg(test)]
use crate::{
    protocol::openai::{Adapter, Profile, TargetCaps},
    provider::{ProviderDefinition, SecretMaterial},
};
use error::ApiError;
use runtime::{BoundCandidate, BoundEntry, MESSAGES_FAMILY, Runtime, family};
pub use startup::config::{
    EmbeddingEntry, Entry, ImageEntry, Limits, SpeechEntry, StartupError, TaskEntries,
    TranscriptionEntry,
};
pub use startup::credentials::Credentials;
#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Gateway {
    state: Arc<Runtime>,
}
