//! Explicit private files only. Neither upstream secrets nor account selectors come from env.
#[cfg(test)]
#[path = "embedding_bootstrap_tests.rs"]
mod embedding_tests;
#[cfg(test)]
#[path = "openrouter_image_bootstrap_tests.rs"]
mod image_tests;
#[cfg(test)]
#[path = "managed_key_bootstrap_tests.rs"]
mod managed_key_tests;
#[cfg(test)]
#[path = "openrouter_speech_bootstrap_tests.rs"]
mod speech_tests;
#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "tokenplan_audio_bootstrap_tests.rs"]
mod tokenplan_audio_tests;
use super::{
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
    /// Trusted global ceiling; probes set one to forbid hidden extra dispatches.
    #[serde(default)]
    max_attempts: Option<usize>,
}
fn listen() -> SocketAddr {
    ([127, 0, 0, 1], 8080).into()
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
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                for protocol in binding.protocols {
                    if bridges.contains(binding.model)
                        && *protocol == crate::topology::ProtocolProfile::OpenAiResponses
                    {
                        continue;
                    }
                    let family = match protocol {
                        crate::topology::ProtocolProfile::OpenAiChat => Profile::Chat,
                        crate::topology::ProtocolProfile::OpenAiResponses => Profile::Responses,
                        crate::topology::ProtocolProfile::AnthropicMessages => {
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
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
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
        if selected.as_ref().is_some_and(|set| *set != activated) {
            return Err(StartupError::Binding);
        }
        let mut gateway = Gateway::new_with_media_and_environment_proxy(
            catalog::default_topology().map_err(|_| StartupError::Binding)?,
            entries,
            image_entries,
            speech_entries,
            transcription_entries,
            embedding_entries,
            credentials,
            SecretMaterial::new(configuration.client_key.expose())
                .map_err(|_| StartupError::Credentials)?,
            Limits::default(),
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
            Some(path) => gateway.with_probe_diagnostics(&path)?,
            None => gateway,
        };
        Ok(Self {
            gateway,
            listen: configuration.bind,
        })
    }
}
