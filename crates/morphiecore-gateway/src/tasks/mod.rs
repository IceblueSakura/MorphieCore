//! Frozen standalone task endpoints. Each public model binds one catalog route,
//! its Provider and a single API key, and makes one attempt.
pub(crate) mod embeddings;
mod exchange;
pub(crate) mod images;
pub(crate) mod speech;
pub(crate) mod transcription;

use crate::{
    Credentials, StartupError,
    provider::{CredentialBindingId, CredentialKind, ProviderDefinition, ProviderId},
    startup::credentials,
    topology::{
        CompiledTopology, embeddings::EmbeddingRoute, images::ImageRoute, speech::SpeechRoute,
        transcription::TranscriptionRoute,
    },
};
use std::{collections::BTreeMap, sync::Arc};

/// A standalone task model bound at startup.
pub(crate) struct BoundTask<R> {
    pub(crate) route: R,
    pub(crate) provider: ProviderDefinition,
    pub(crate) secret: credentials::Source,
    pub(crate) account: Option<String>,
}
/// The trusted target a standalone route names.
pub(crate) trait TaskRoute: Clone {
    fn target(&self) -> (&ProviderId, &CredentialBindingId);
    fn retarget(&mut self, provider: &ProviderDefinition) -> Result<(), StartupError>;
}
macro_rules! task_route {
    ($($route:ty => $operation:ident),*) => {$(
        impl TaskRoute for $route {
            fn target(&self) -> (&ProviderId, &CredentialBindingId) {
                (&self.endpoint.provider, &self.endpoint.credential)
            }
            fn retarget(&mut self, provider: &ProviderDefinition) -> Result<(), StartupError> {
                if provider.id != self.endpoint.provider {
                    return Err(StartupError::Credentials);
                }
                let operation = provider.entry(crate::provider::ProviderOperation::$operation)
                    .ok_or(StartupError::Binding)?;
                self.endpoint.target.origin = provider.origin.clone();
                self.endpoint.target.path = operation.path.clone();
                Ok(())
            }
        }
    )*};
}
task_route!(
    ImageRoute => Images, SpeechRoute => Speech,
    TranscriptionRoute => Transcription, EmbeddingRoute => Embeddings
);

/// Bind at most 64 unique models, each to its route's Provider and exactly one
/// API key without credential fallback.
pub(crate) fn bind<'t, E, R: TaskRoute + 't>(
    topology: &'t CompiledTopology,
    entries: Vec<E>,
    credentials: &Credentials,
    route: impl Fn(&E) -> Option<&'t R>,
    model: impl Fn(&E) -> &str,
) -> Result<BTreeMap<String, Arc<BoundTask<R>>>, StartupError> {
    if entries.len() > 64 {
        return Err(StartupError::Binding);
    }
    let mut result = BTreeMap::new();
    for entry in entries {
        let mut route = route(&entry).ok_or(StartupError::Binding)?.clone();
        let (provider, credential) = route.target();
        let binding = credentials
            .task(model(&entry), credential)
            .ok_or(StartupError::Credentials)?;
        let provider = match &binding.provider {
            Some(configured) => configured.clone(),
            None => topology
                .provider(provider.as_str())
                .ok_or(StartupError::Binding)?
                .clone(),
        };
        if provider.id != *route.target().0 {
            return Err(StartupError::Credentials);
        }
        if binding.provider.is_some() {
            route.retarget(&provider)?;
        }
        let secret = binding.source.clone();
        secret.check(CredentialKind::ApiKey, provider.id.as_str())?;
        let bound = BoundTask {
            route,
            provider,
            secret,
            account: binding.name.clone(),
        };
        if result
            .insert(model(&entry).to_owned(), Arc::new(bound))
            .is_some()
        {
            return Err(StartupError::Binding);
        }
    }
    Ok(result)
}
