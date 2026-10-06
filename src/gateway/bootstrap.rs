//! Explicit private files only. Neither upstream secrets nor account selectors come from env.
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
    Credentials, Entry, Gateway, ImageEntry, Limits, SpeechEntry, StartupError, TranscriptionEntry,
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
        let manager = CredentialManager::new(
            directory,
            crate::credential::builtin_drivers(None).map_err(|_| StartupError::Credentials)?,
        )
        .map_err(|_| StartupError::Credentials)?;
        let mut entries = Vec::new();
        let mut image_entries = Vec::new();
        let mut speech_entries = Vec::new();
        let mut transcription_entries = Vec::new();
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
                if selected
                    .as_ref()
                    .is_some_and(|set| !set.contains(binding.model))
                {
                    continue;
                }
                used = true;
                activated.insert(binding.model.to_owned());
                for protocol in binding.protocols {
                    let family = match protocol {
                        crate::topology::ProtocolProfile::OpenAiChat => Profile::Chat,
                        crate::topology::ProtocolProfile::OpenAiResponses => Profile::Responses,
                    };
                    entries.push(Entry {
                        model: binding.model.into(),
                        protocol: family,
                        endpoint: binding.endpoint_id(*protocol),
                    });
                }
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
            if !known {
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
        let mut gateway = Gateway::new_with_media(
            catalog::default_topology().map_err(|_| StartupError::Binding)?,
            entries,
            image_entries,
            speech_entries,
            transcription_entries,
            credentials,
            SecretMaterial::new(configuration.client_key.expose())
                .map_err(|_| StartupError::Credentials)?,
            Limits::default(),
            configuration.proxy.as_deref(),
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
