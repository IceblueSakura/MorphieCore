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
        } else if catalog.transcription_model(label).is_some() {
            let mut bindings = crate::topology::catalog::TRANSCRIPTION_BINDINGS
                .iter()
                .filter(|binding| {
                    binding.model == *label && providers.contains(&binding.provider().id.as_str())
                });
            let binding = bindings.next().expect("selected transcription provider");
            assert!(bindings.next().is_none(), "select exactly one ASR binding");
            serde_json::json!({"binding":binding.endpoint_id})
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
    pub(super) fn new(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
    ) -> Result<Self, StartupError> {
        let directory = crate::credential::tests::support::private_directory();
        let manager =
            crate::credential::CredentialManager::new(directory.path().join("credentials"), vec![])
                .unwrap();
        for &(provider, pool, key) in credentials {
            manager
                .add_api_key(
                    provider,
                    "one",
                    crate::credential::Secret::new(key.into()).unwrap(),
                )
                .unwrap();
            manager
                .set_pool(
                    provider,
                    pool,
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
        write_configuration(directory.path(), service, models);
        let boot = Bootstrap::from_directory(directory.path())?;
        Ok(Self {
            boot,
            _directory: directory,
        })
    }
}

pub(super) fn service() -> serde_json::Value {
    serde_json::json!({
        "client_key": "synthetic-gateway-client-token-0001",
        "bind": "127.0.0.1:0"
    })
}
