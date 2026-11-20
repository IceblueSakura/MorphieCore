//! Synthetic private-file configuration helpers; the fixture owns its credential directory.
use super::*;

pub fn write_private(path: &std::path::Path, bytes: &[u8]) {
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

pub fn write_configuration(
    directory: &std::path::Path,
    mut service: serde_json::Value,
    mut models: serde_json::Value,
) {
    service["version"] = serde_json::json!(1);
    models["version"] = serde_json::json!(1);
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

pub fn write_accounts(directory: &Path, accounts: &serde_json::Value) {
    write_private(
        &directory.join("accounts.json"),
        &serde_json::to_vec(accounts).unwrap(),
    );
}

/// Fixtures explicitly select catalog targets; the loader never discovers models.
pub fn selected_models(labels: &[&str], providers: &[&str]) -> serde_json::Value {
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
            let mut bindings = catalog
                .transcription_bindings(label)
                .filter(|route| providers.contains(&route.endpoint.provider.as_str()));
            let binding = bindings.next().expect("selected transcription provider");
            assert!(bindings.next().is_none(), "select exactly one ASR binding");
            serde_json::json!({"binding":binding.endpoint.id.as_str(),"account":format!("{}-api-key",binding.endpoint.provider.as_str())})
        } else {
            let provider = catalog
                .image_route(label)
                .map(|r| &r.endpoint.provider)
                .or_else(|| catalog.speech_route(label).map(|r| &r.endpoint.provider))
                .or_else(|| catalog.embedding_route(label).map(|r| &r.endpoint.provider))
                .expect("selected task model");
            serde_json::json!({"account":format!("{}-api-key",provider.as_str())})
        };
        models.insert((*label).into(), model);
    }
    serde_json::json!({"models":models})
}

pub struct Fixture {
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
    #[cfg(test)]
    pub(super) fn record_spans(&mut self) -> opentelemetry_sdk::trace::InMemorySpanExporter {
        let (sink, _, spans) = crate::diagnostics::Sink::memory_signals(crate::LogLevel::Warn);
        std::sync::Arc::get_mut(&mut self.boot.gateway.state)
            .unwrap()
            .diagnostics = Some(sink);
        spans
    }

    pub fn record_failures(&mut self) -> opentelemetry_sdk::logs::InMemoryLogExporter {
        let (sink, exporter) = crate::diagnostics::Sink::memory();
        std::sync::Arc::get_mut(&mut self.boot.gateway.state)
            .unwrap()
            .diagnostics = Some(sink);
        exporter
    }
}
impl Fixture {
    pub fn loopback_authority(&mut self, origin: &str) {
        let runtime = std::sync::Arc::get_mut(&mut self.boot.gateway.state)
            .expect("unshared fixture runtime");
        runtime.transport = runtime.transport.clone().with_loopback_authority(origin);
    }
    pub fn force_upstream_stream(&mut self) {
        std::sync::Arc::get_mut(&mut self.boot.gateway.state)
            .expect("unshared fixture runtime")
            .limits
            .force_upstream_stream = true;
    }

    pub fn directory(&self) -> &Path {
        self._directory.path()
    }

    pub fn text_model_json(&self, model: &str) -> &[u8] {
        &self.boot.gateway.state.text_models.models[model]
    }

    pub fn capabilities_json(&self) -> &[u8] {
        &self.boot.gateway.state.capabilities.0
    }

    pub fn new(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
    ) -> Result<Self, StartupError> {
        Self::with_credentials(service, models, credentials, &[])
    }

    pub fn with_oauth(
        service: serde_json::Value,
        models: serde_json::Value,
        provider: &str,
        pool: &str,
    ) -> Result<Self, StartupError> {
        Self::with_credentials(service, models, &[], &[(provider, pool)])
    }

    pub fn with_accounts(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
        accounts: serde_json::Value,
    ) -> Result<Self, StartupError> {
        Self::build(service, models, credentials, &[], Some(accounts))
    }
    fn with_credentials(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
        oauth: &[(&str, &str)],
    ) -> Result<Self, StartupError> {
        Self::build(service, models, credentials, oauth, None)
    }
    fn build(
        service: serde_json::Value,
        models: serde_json::Value,
        credentials: &[(&str, &str, &str)],
        oauth: &[(&str, &str)],
        configured: Option<serde_json::Value>,
    ) -> Result<Self, StartupError> {
        let directory = morphiecore_test_support::filesystem::private_directory();
        let root = directory.path().join("credentials");
        let drivers =
            crate::credential::drivers::builtin_drivers_with_environment(None, false).unwrap();
        let manager = crate::credential::CredentialManager::new(&root, drivers).unwrap();
        let mut providers = serde_json::Map::new();
        for &(provider, name, key) in credentials {
            manager
                .add_api_key(
                    provider,
                    name,
                    crate::credential::Secret::new(key.into()).unwrap(),
                )
                .unwrap();
            let entry = providers
                .entry(provider)
                .or_insert_with(|| serde_json::json!({"accounts":[]}));
            entry["accounts"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({
                    "name":name,"enabled":true,"credential":{"alias":name}
                }));
        }
        for &(provider, name) in oauth {
            let definition = crate::provider::catalog::get(provider).expect("registered Provider");
            let crate::provider::CredentialKind::OAuth(profile) = definition.auth.kind() else {
                panic!("OAuth Provider");
            };
            let siwc = profile == "openai";
            let account = serde_json::json!({
                "record_id":"0123456789abcdef0123456789abcdef","profile":profile,"alias":name,
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
                    "version":1,"provider":profile,"revision":1,"oauth":{name:account},"api_keys":{}
                }))
                .unwrap(),
            );
            providers.insert(
                provider.into(),
                serde_json::json!({"accounts":[{
                    "name":name,"enabled":true,"credential":{"alias":name}
                }]}),
            );
        }
        write_accounts(
            directory.path(),
            &configured.unwrap_or_else(|| serde_json::json!({"version":1,"providers":providers})),
        );
        write_configuration(directory.path(), service, models);
        let boot = Bootstrap::from_directory(directory.path())?;
        Ok(Self {
            boot,
            _directory: directory,
        })
    }
}

pub fn service() -> serde_json::Value {
    serde_json::json!({
        "auth_file": "auth.json",
        "bind": "127.0.0.1:0"
    })
}
