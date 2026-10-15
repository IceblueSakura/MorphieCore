//! Explicit private files only. Neither upstream secrets nor account selectors come from env.
#[cfg(test)]
mod managed_key_tests;
#[cfg(test)]
mod media_tests;
#[cfg(test)]
mod tests;
use crate::gateway::{
    Credentials, EmbeddingEntry, Entry, Gateway, ImageEntry, Limits, SpeechEntry, StartupError,
    TranscriptionEntry,
};
use crate::{
    credential::{CredentialManager, Secret},
    protocol::openai::Profile,
    provider::{CredentialBindingId, SecretMaterial},
    topology::catalog,
};
use std::{collections::BTreeSet, net::SocketAddr, path::Path};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    client_key: Secret,
    #[serde(default = "listen")]
    bind: SocketAddr,
    #[serde(default)]
    proxy: Option<String>,
    #[serde(default)]
    diagnostics: Option<std::path::PathBuf>,
    #[serde(default)]
    models: Option<Vec<String>>,
    /// Explicit Responses entry on the fixed Chat endpoint; never a fallback route.
    #[serde(default)]
    responses_via_chat: Vec<String>,
    #[serde(default = "enabled")]
    implicit_cache_affinity: bool,
    /// Failover order of a listed model's Providers; unlisted Providers stay unused.
    #[serde(default)]
    failover: std::collections::BTreeMap<String, Vec<String>>,
    /// Trusted global ceiling across all route and credential candidates.
    #[serde(default)]
    max_attempts: Option<usize>,
}
fn listen() -> SocketAddr {
    ([127, 0, 0, 1], 8080).into()
}
fn enabled() -> bool {
    true
}
pub struct Bootstrap {
    pub gateway: Gateway,
    pub listen: SocketAddr,
}
impl Bootstrap {
    pub fn from_directory(directory: &Path) -> Result<Self, StartupError> {
        Self::from_files(&directory.join("gateway.json"), directory)
    }
    pub fn from_files(configuration: &Path, directory: &Path) -> Result<Self, StartupError> {
        Self::load(configuration, directory, None, false)
    }
    /// Binary startup opts into environment proxies; an explicit argument beats the file.
    pub fn from_files_with_proxy(
        configuration: &Path,
        directory: &Path,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::load(configuration, directory, proxy, true)
    }
    fn load(
        configuration: &Path,
        directory: &Path,
        proxy: Option<&str>,
        environment: bool,
    ) -> Result<Self, StartupError> {
        let bytes = crate::credential::read_private_file(configuration, 65536)
            .map_err(|_| StartupError::Credentials)?;
        let configuration: Configuration =
            serde_json::from_slice(&bytes).map_err(|_| StartupError::Credentials)?;
        if configuration
            .max_attempts
            .is_some_and(|n| !(1..=64).contains(&n))
        {
            return Err(StartupError::Credentials);
        }
        if !configuration.bind.ip().is_loopback() {
            return Err(StartupError::Listener);
        }
        let selected = configuration
            .models
            .as_ref()
            .map(|models| models.iter().cloned().collect::<BTreeSet<_>>());
        if configuration.models.as_ref().is_some_and(|models| {
            models.is_empty()
                || models.len() > 64
                || selected.as_ref().unwrap().len() != models.len()
        }) {
            return Err(StartupError::Binding);
        }
        let bridges: BTreeSet<_> = configuration.responses_via_chat.iter().cloned().collect();
        if bridges.len() != configuration.responses_via_chat.len()
            || bridges.len() > 64
            || bridges.iter().any(|model| {
                selected.as_ref().is_none_or(|set| !set.contains(model))
                    || !catalog::API_KEY_BINDINGS.iter().any(|binding| {
                        binding.model == model
                            && binding
                                .protocols
                                .contains(&crate::topology::ProtocolProfile::OpenAiChat)
                    })
            })
        {
            return Err(StartupError::Binding);
        }
        if configuration.failover.iter().any(|(model, providers)| {
            selected.as_ref().is_none_or(|set| !set.contains(model))
                || providers.is_empty()
                || providers.len() > 8
                || providers.iter().collect::<BTreeSet<_>>().len() != providers.len()
                || providers.iter().any(|provider| {
                    !catalog::API_KEY_BINDINGS.iter().any(|binding| {
                        binding.model == model && (binding.provider)().id.as_str() == provider
                    }) && !catalog::SUBSCRIPTION_BINDINGS.iter().any(|binding| {
                        binding.model == model && (binding.provider)().id.as_str() == provider
                    }) && !catalog::MESSAGES_BINDINGS.iter().any(|binding| {
                        binding.model == model && binding.provider().id.as_str() == provider
                    })
                })
        }) {
            return Err(StartupError::Binding);
        }
        let mut serving = BTreeSet::new();
        let manager = CredentialManager::new(
            directory,
            crate::credential::builtin_drivers(None).map_err(|_| StartupError::Credentials)?,
        )
        .map_err(|_| StartupError::Credentials)?;
        let mut entries = Vec::new();
        let mut image_entries = Vec::new();
        let mut speech_entries = Vec::new();
        let mut transcription_entries = Vec::new();
        let mut embedding_entries = Vec::new();
        let mut credentials = Credentials::new();
        let mut activated = BTreeSet::new();
        for (provider, id, status) in manager.pools().map_err(|_| StartupError::Credentials)? {
            let mut known = false;
            let mut used = false;
            for binding in catalog::API_KEY_BINDINGS {
                if binding.credential != id || (binding.provider)().id.as_str() != provider {
                    continue;
                }
                known = true;
                // Voice input is deliberately selected, not inferred from a text pool.
                if binding.public_model().contract.audio_input
                    && !selected
                        .as_ref()
                        .is_some_and(|set| set.contains(binding.model))
                {
                    continue;
                }
                if selected
                    .as_ref()
                    .is_some_and(|set| !set.contains(binding.model))
                    || configuration
                        .failover
                        .get(binding.model)
                        .is_some_and(|order| !order.contains(&provider))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                serving.insert((binding.model.to_owned(), provider.clone()));
                for protocol in binding.protocols {
                    if bridges.contains(binding.model)
                        && *protocol == crate::topology::ProtocolProfile::OpenAiResponses
                    {
                        continue;
                    }
                    let family = match protocol {
                        crate::topology::ProtocolProfile::OpenAiChat => Profile::Chat,
                        crate::topology::ProtocolProfile::OpenAiResponses => Profile::Responses,
                        crate::topology::ProtocolProfile::AnthropicMessages
                        | crate::topology::ProtocolProfile::GoogleGenerateContent
                        | crate::topology::ProtocolProfile::GoogleInteractions => {
                            return Err(StartupError::Binding);
                        }
                    };
                    entries.push(Entry {
                        model: binding.model.into(),
                        protocol: family,
                        endpoint: binding.endpoint_id(*protocol),
                    });
                }
                if bridges.contains(binding.model) {
                    entries.push(Entry {
                        model: binding.model.into(),
                        protocol: Profile::Responses,
                        endpoint: binding.endpoint_id(crate::topology::ProtocolProfile::OpenAiChat),
                    });
                }
            }
            for binding in catalog::MESSAGES_BINDINGS {
                if binding.credential != id || binding.provider().id.as_str() != provider {
                    continue;
                }
                known = true;
                // A shared text pool never implicitly switches clients to a native protocol.
                if !selected
                    .as_ref()
                    .is_some_and(|set| set.contains(binding.model))
                    || configuration
                        .failover
                        .get(binding.model)
                        .is_some_and(|order| !order.contains(&provider))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                let endpoint = binding.endpoint().id;
                serving.insert((binding.model.to_owned(), provider.clone()));
                entries.extend([Profile::Responses, Profile::Chat].map(|protocol| Entry {
                    model: binding.model.into(),
                    protocol,
                    endpoint: endpoint.clone(),
                }));
            }
            for binding in catalog::GOOGLE_BINDINGS {
                if binding.credential != id || binding.provider().id.as_str() != provider {
                    continue;
                }
                known = true;
                if selected
                    .as_ref()
                    .is_some_and(|set| !set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                for endpoint in [binding.endpoint().id, binding.interactions_endpoint().id] {
                    entries.extend([Profile::Responses, Profile::Chat].map(|protocol| Entry {
                        model: binding.model.into(),
                        protocol,
                        endpoint: endpoint.clone(),
                    }));
                }
            }
            for binding in catalog::EMBEDDING_BINDINGS {
                if binding.credential != id || (binding.provider)().id.as_str() != provider {
                    continue;
                }
                known = true;
                if !selected
                    .as_ref()
                    .is_some_and(|set| set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                embedding_entries.push(EmbeddingEntry {
                    model: binding.model.into(),
                });
            }
            for binding in catalog::IMAGE_BINDINGS {
                if binding.credential != id || binding.provider().id.as_str() != provider {
                    continue;
                }
                known = true;
                // Image operations require deliberate selection, even with an existing pool.
                if !selected
                    .as_ref()
                    .is_some_and(|set| set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                image_entries.push(ImageEntry {
                    model: binding.model.into(),
                });
            }
            for binding in catalog::SPEECH_BINDINGS {
                if binding.credential != id || binding.provider().id.as_str() != provider {
                    continue;
                }
                known = true;
                // A shared pool never implicitly enables a new media operation.
                if !selected
                    .as_ref()
                    .is_some_and(|set| set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                speech_entries.push(SpeechEntry {
                    model: binding.model.into(),
                });
            }
            for binding in catalog::TRANSCRIPTION_BINDINGS {
                if binding.credential != id || binding.provider().id.as_str() != provider {
                    continue;
                }
                known = true;
                if !selected
                    .as_ref()
                    .is_some_and(|set| set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                transcription_entries.push(TranscriptionEntry {
                    model: binding.model.into(),
                });
            }
            for binding in catalog::SUBSCRIPTION_BINDINGS {
                if binding.credential().as_str() != id
                    || (binding.provider)().id.as_str() != provider
                {
                    continue;
                }
                known = true;
                if selected
                    .as_ref()
                    .is_some_and(|set| !set.contains(binding.model))
                    || configuration
                        .failover
                        .get(binding.model)
                        .is_some_and(|order| !order.contains(&provider))
                    || bridges.contains(binding.model)
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                serving.insert((binding.model.to_owned(), provider.clone()));
                entries.push(Entry {
                    model: binding.model.into(),
                    protocol: Profile::Responses,
                    endpoint: binding.endpoint_id(),
                });
            }
            // Explicit selection cannot activate an unrelated stale pool.
            // Unfiltered startup still rejects unknown configuration.
            if !known && selected.is_none() {
                return Err(StartupError::Binding);
            }
            if used {
                let mut access = manager
                    .bind_pool(&provider, &status.config)
                    .map_err(|_| StartupError::Credentials)?;
                if let Some(cap) = configuration.max_attempts {
                    access.max_attempts = access.max_attempts.min(cap);
                }
                credentials.insert_pool(
                    CredentialBindingId::new(&id).map_err(|_| StartupError::Credentials)?,
                    access,
                )?;
            }
        }
        if selected.as_ref().is_some_and(|set| *set != activated)
            || configuration.failover.iter().any(|(model, providers)| {
                providers
                    .iter()
                    .any(|p| !serving.contains(&(model.clone(), p.clone())))
            })
        {
            return Err(StartupError::Binding);
        }
        let mut gateway = Gateway::bind(
            catalog::topology(&configuration.failover).map_err(|_| StartupError::Binding)?,
            crate::gateway::TaskEntries {
                generation: entries,
                images: image_entries,
                speech: speech_entries,
                transcriptions: transcription_entries,
                embeddings: embedding_entries,
            },
            credentials,
            SecretMaterial::new(configuration.client_key.expose())
                .map_err(|_| StartupError::Credentials)?,
            Limits {
                implicit_cache_affinity: configuration.implicit_cache_affinity,
                ..Limits::default()
            },
            proxy.or(configuration.proxy.as_deref()),
            environment,
        )?;
        if let Some(cap) = configuration.max_attempts {
            // This freshly built state is not published or shared yet. The same
            // ceiling governs the entire Route, not one budget per credential pool.
            let state = std::sync::Arc::get_mut(&mut gateway.state).ok_or(StartupError::Binding)?;
            for entry in state.entries.values_mut() {
                let entry = std::sync::Arc::get_mut(entry).ok_or(StartupError::Binding)?;
                entry.policy.max_attempts = entry.policy.max_attempts.min(cap);
            }
        }
        let gateway = match configuration.diagnostics {
            Some(path) => gateway.with_diagnostics(&path)?,
            None => gateway,
        };
        Ok(Self {
            gateway,
            listen: configuration.bind,
        })
    }
}
