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
        &directory.join("auth.json"),
        br#"{"version":1,"agents":[{"agent_id":"fixture-agent","key":"synthetic-gateway-client-token-0001"}]}"#,
    );
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
    pub(super) fn force_upstream_stream(&mut self) {
        std::sync::Arc::get_mut(&mut self.boot.gateway.state)
            .expect("unshared fixture runtime")
            .limits
            .force_upstream_stream = true;
    }

    pub(super) fn directory(&self) -> &Path {
        self._directory.path()
    }

    pub(super) fn new(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
    ) -> Result<Self, StartupError> {
        Self::with_credentials(service, models, credentials, &[])
    }

    pub(super) fn with_oauth(
        service: serde_json::Value,
        models: serde_json::Value,
        provider: &str,
        pool: &str,
    ) -> Result<Self, StartupError> {
        Self::with_credentials(service, models, &[], &[(provider, pool)])
    }

    fn with_credentials(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
        oauth: &[(&str, &str)],
    ) -> Result<Self, StartupError> {
        let directory = crate::test_files::private_directory();
        let root = directory.path().join("credentials");
        let drivers =
            crate::credential::drivers::builtin_drivers_with_environment(None, false).unwrap();
        let manager = crate::credential::CredentialManager::new(&root, drivers).unwrap();
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
        for &(profile, pool) in oauth {
            let siwc = profile == "openai";
            let account = serde_json::json!({
                "profile":profile,"alias":"one",
                "client_id":if siwc {"oaiapp_synthetic"} else {"synthetic-client"},
                "identity":{"subject":"synthetic-subject","scope":null},
                "revision":2,"generation":1,"login_attempt":null,"state":"active",
                "credential":{
                    "access":"synthetic-oauth-access","refresh":"synthetic-refresh",
                    "id_token":if siwc {Some("synthetic-id-token")} else {None},
                    "expires_at":4000000000u64,
                    "scopes":if siwc {
                        "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"
                    } else {
                        "openid profile email offline_access grok-cli:access api:access"
                    }
                }
            });
            write_private(
                &root.join(format!("{profile}.json")),
                &serde_json::to_vec(&serde_json::json!({
                    "provider":profile,"revision":1,"oauth":{"one":account},
                    "api_keys":{},"pools":{}
                }))
                .unwrap(),
            );
            manager
                .set_pool(
                    profile,
                    pool,
                    0,
                    crate::credential::CredentialPool {
                        members: vec![crate::credential::CredentialRef::OAuth {
                            profile: profile.into(),
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
        "auth_file": "auth.json",
        "bind": "127.0.0.1:0"
    })
}
