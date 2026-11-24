//! Standalone tasks with one dispatched target per exchange.
pub(crate) mod embeddings;
mod exchange;
pub(crate) mod images;
mod lifecycle;
pub(crate) mod speech;
pub(crate) mod transcription;

use crate::{
    Credentials, StartupError,
    provider::{CredentialBindingId, ProviderDefinition, ProviderId},
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
    pub(crate) health: Option<crate::routing::executors::Scopes>,
}
/// The trusted target a standalone route names.
pub(crate) trait TaskRoute: Clone {
    fn target(&self) -> (&ProviderId, &CredentialBindingId);
    fn retarget(&mut self, provider: &ProviderDefinition) -> Result<(), StartupError>;
    fn delivery(&self) -> (&str, &str, &crate::topology::ExecutionContract);
}
macro_rules! task_route {
    ($($route:ty => $operation:ident),*) => {$(
        impl TaskRoute for $route {
            fn target(&self) -> (&ProviderId, &CredentialBindingId) {
                (&self.endpoint.provider, &self.endpoint.credential)
            }
            fn delivery(&self) -> (&str, &str, &crate::topology::ExecutionContract) {
                (self.model.as_str(), &self.endpoint.upstream_model, &self.endpoint.execution)
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
impl TaskRoute for ImageRoute {
    fn target(&self) -> (&ProviderId, &CredentialBindingId) {
        (&self.endpoint.provider, &self.endpoint.credential)
    }
    fn delivery(&self) -> (&str, &str, &crate::topology::ExecutionContract) {
        (
            self.model.as_str(),
            &self.endpoint.upstream_model,
            &self.endpoint.execution,
        )
    }
    fn retarget(&mut self, provider: &ProviderDefinition) -> Result<(), StartupError> {
        if provider.id != self.endpoint.provider {
            return Err(StartupError::Credentials);
        }
        let images = provider
            .entry(crate::provider::ProviderOperation::Images)
            .ok_or(StartupError::Binding)?;
        let edit = provider
            .entry(crate::provider::ProviderOperation::ImagesEdit)
            .ok_or(StartupError::Binding)?;
        self.endpoint.target.origin = provider.origin.clone();
        self.endpoint.target.path = images.path.clone();
        self.endpoint.edit_path = edit.path.clone();
        Ok(())
    }
}
task_route!(
    SpeechRoute => Speech,
    TranscriptionRoute => Transcription, EmbeddingRoute => Embeddings
);

/// Bind at most 64 unique models, each to its route's Provider and exactly one
/// credential without fallback.
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
        let route = route(&entry).ok_or(StartupError::Binding)?.clone();
        let binding = credentials
            .task(model(&entry), route.target().1)
            .ok_or(StartupError::Credentials)?;
        let bound = bind_target(topology, route, binding)?;
        if result
            .insert(model(&entry).to_owned(), Arc::new(bound))
            .is_some()
        {
            return Err(StartupError::Binding);
        }
    }
    Ok(result)
}
fn bind_target<R: TaskRoute>(
    topology: &CompiledTopology,
    mut route: R,
    binding: &credentials::Binding,
) -> Result<BoundTask<R>, StartupError> {
    let provider = match &binding.provider {
        Some(configured) => configured.clone(),
        None => topology
            .provider(route.target().0.as_str())
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
    secret.check(route.delivery().2.credential_kind, provider.id.as_str())?;
    Ok(BoundTask {
        route,
        provider,
        secret,
        account: binding.name.clone(),
        health: None,
    })
}
