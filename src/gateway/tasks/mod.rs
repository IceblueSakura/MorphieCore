//! Frozen standalone task endpoints. Each public model binds one catalog route,
//! its Provider and a single API key, and makes one attempt.
pub(in crate::gateway) mod embeddings;
mod exchange;
pub(in crate::gateway) mod images;
pub(in crate::gateway) mod speech;
pub(in crate::gateway) mod transcription;

use crate::{
    gateway::{Credentials, StartupError, startup::credentials},
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, ProviderId},
    topology::{
        CompiledTopology, embeddings::EmbeddingRoute, images::ImageRoute, speech::SpeechRoute,
        transcription::TranscriptionRoute,
    },
};
use std::{collections::BTreeMap, sync::Arc};

/// A standalone task model bound at startup.
pub(in crate::gateway) struct BoundTask<R> {
    pub(in crate::gateway) route: R,
    pub(in crate::gateway) provider: ProviderDefinition,
    pub(in crate::gateway) secret: credentials::Source,
}
/// The trusted target a standalone route names.
pub(in crate::gateway) trait TaskRoute: Clone {
    fn target(&self) -> (&ProviderId, &CredentialBindingId);
}
macro_rules! task_route {
    ($($route:ty),*) => {$(
        impl TaskRoute for $route {
            fn target(&self) -> (&ProviderId, &CredentialBindingId) {
                (&self.endpoint.provider, &self.endpoint.credential)
            }
        }
    )*};
}
task_route!(ImageRoute, SpeechRoute, TranscriptionRoute, EmbeddingRoute);

/// Bind at most 64 unique models, each to its route's Provider and exactly one
/// API key without credential fallback.
pub(in crate::gateway) fn bind<'t, E, R: TaskRoute + 't>(
    topology: &'t CompiledTopology,
    entries: Vec<E>,
    credentials: &Credentials,
    route: impl Fn(&E) -> Option<&'t R>,
    model: impl Fn(E) -> String,
) -> Result<BTreeMap<String, Arc<BoundTask<R>>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let route = route(&entry).ok_or(StartupError::Binding)?.clone();
        let (provider, credential) = route.target();
        let provider = topology
            .provider(provider.as_str())
            .ok_or(StartupError::Binding)?
            .clone();
        let binding = credentials
            .get(credential)
            .ok_or(StartupError::Credentials)?;
        if binding.fallback || binding.sources.len() != 1 {
            return Err(StartupError::Credentials);
        }
        let secret = binding.sources[0].clone();
        secret.check(CredentialKind::ApiKey, provider.id.as_str())?;
        let bound = BoundTask {
            route,
            provider,
            secret,
        };
        if result.insert(model(entry), Arc::new(bound)).is_some() {
            return Err(StartupError::Binding);
        }
    }
    Ok(result)
}
