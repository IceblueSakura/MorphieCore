//! Synthetic private-file configuration helpers; the fixture owns its credential directory.
use super::*;

pub(super) fn write_private(path: &std::path::Path, bytes: &[u8]) {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).unwrap().write_all(bytes).unwrap();
}

pub(super) fn write_configuration(
    directory: &std::path::Path,
    mut service: serde_json::Value,
    models: serde_json::Value,
) {
    service["models_file"] = serde_json::json!("models.json");
    write_private(
        &directory.join("gateway.json"),
        &serde_json::to_vec(&service).unwrap(),
    );
    write_private(
        &directory.join("models.json"),
        &serde_json::to_vec(&models).unwrap(),
    );
}

/// Fixtures explicitly select catalog targets; the loader never discovers models.
pub(super) fn selected_models(labels: &[&str], providers: &[&str]) -> serde_json::Value {
    let catalog = catalog::default_topology().unwrap();
    let mut models = serde_json::Map::new();
    for label in labels {
        let model = if let Some(public) = catalog.model(label) {
            let candidates: Vec<_> = catalog
                .route_endpoints(&public.route)
                .into_iter()
                .filter(|endpoint| providers.contains(&endpoint.provider.as_str()))
                .map(|endpoint| serde_json::json!({"binding":endpoint.id.as_str()}))
                .collect();
            serde_json::json!({"candidates":candidates})
        } else {
            serde_json::json!({})
        };
        models.insert((*label).into(), model);
    }
    serde_json::json!({"models":models})
}

pub(super) struct Fixture {
    boot: Bootstrap,
    _directory: tempfile::TempDir,
}
impl std::ops::Deref for Fixture {
    type Target = Bootstrap;
    fn deref(&self) -> &Bootstrap {
        &self.boot
    }
}
impl Fixture {
    pub(super) fn from_lookup(
        mut get: impl FnMut(&str) -> Result<Option<String>, StartupError>,
    ) -> Result<Self, StartupError> {
        let directory = crate::credential::tests::support::private_directory();
        let manager =
            crate::credential::CredentialManager::new(directory.path().join("credentials"), vec![])
                .unwrap();
        let mut added = std::collections::BTreeSet::new();
        for binding in catalog::API_KEY_BINDINGS {
            if let Some(key) = get(binding.credential)? {
                if !added.insert(binding.credential) {
                    continue;
                }
                let provider = (binding.provider)().id;
                manager
                    .add_api_key(
                        provider.as_str(),
                        "one",
                        crate::credential::Secret::new(key)
                            .map_err(|_| StartupError::Credentials)?,
                    )
                    .unwrap();
                manager
                    .set_pool(
                        provider.as_str(),
                        binding.credential,
                        0,
                        crate::credential::CredentialPool {
                            members: vec![crate::credential::CredentialRef::ApiKey {
                                alias: "one".into(),
                            }],
                            fallback: false,
                            max_attempts: 1,
                        },
                    )
                    .unwrap();
            }
        }
        let service = serde_json::json!({"client_key":get("client_key")?.ok_or(StartupError::Credentials)?,"bind":get("bind")?.unwrap_or("127.0.0.1:0".into())});
        let pools = manager.pools().unwrap();
        let providers: Vec<_> = pools
            .iter()
            .map(|(provider, _, _)| provider.as_str())
            .collect();
        let labels: Vec<String> = match get("models")? {
            Some(value) => serde_json::from_str(&value).map_err(|_| StartupError::Binding)?,
            None => catalog::API_KEY_BINDINGS
                .iter()
                .filter(|b| {
                    providers.contains(&(b.provider)().id.as_str())
                        && !b.public_model().contract.audio_input
                })
                .map(|b| b.model.to_owned())
                .collect(),
        };
        let mut models = selected_models(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            &providers,
        );
        if let Some(value) = get("model_config")? {
            models = serde_json::from_str(&value).map_err(|_| StartupError::Binding)?;
        }
        if get("reasoning_as_text")?.as_deref() == Some("true") {
            for model in models["models"].as_object_mut().unwrap().values_mut() {
                if let Some(candidates) = model
                    .get_mut("candidates")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for candidate in candidates {
                        candidate["reasoning_as_text"] = serde_json::json!(true);
                    }
                }
            }
        }
        write_configuration(directory.path(), service, models);
        let boot = Bootstrap::from_directory(directory.path())?;
        Ok(Self {
            boot,
            _directory: directory,
        })
    }
}
