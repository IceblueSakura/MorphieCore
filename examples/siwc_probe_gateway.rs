//! Fixed uncapped probe listener. No persistent pools or implicit credential renewal.
use clap::Parser;
use morphiecore::{
    credential::{CredentialManager, Secret, builtin_drivers, read_private_file},
    gateway::{Credentials, Entry, Gateway, Limits},
    protocol::openai::Profile,
    provider::SecretMaterial,
    topology::catalog,
};
use std::{net::SocketAddr, path::PathBuf, process::ExitCode};

#[derive(Parser)]
struct Options {
    #[arg(long)]
    credentials_dir: PathBuf,
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    live: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    client_key: Secret,
    bind: SocketAddr,
    proxy: Option<String>,
    diagnostics: PathBuf,
    models: Vec<String>,
    max_attempts: usize,
}

impl Configuration {
    fn valid(&self) -> bool {
        self.bind == SocketAddr::from(([127, 0, 0, 1], 0))
            && self.models == ["gpt-6.1-sol"]
            && self.max_attempts == 1
    }
}

async fn run(options: Options) -> Result<(), ()> {
    if !options.live {
        return Err(());
    }
    let raw = read_private_file(&options.config, 65536).map_err(|_| ())?;
    let configuration: Configuration = serde_json::from_slice(&raw).map_err(|_| ())?;
    if !configuration.valid() {
        return Err(());
    }
    let manager = CredentialManager::new(
        &options.credentials_dir,
        builtin_drivers(None).map_err(|_| ())?,
    )
    .map_err(|_| ())?;
    // Selection is deliberate and fail-closed: never pick the first usable account.
    let accounts = manager.list(Some("openai")).map_err(|_| ())?;
    if accounts.len() != 1 {
        return Err(());
    }
    let access = manager
        .bind_access("openai", &accounts[0].account)
        .map_err(|_| ())?;
    access.borrow().map_err(|_| ())?;
    let binding = catalog::SUBSCRIPTION_BINDINGS
        .iter()
        .find(|binding| binding.profile == "openai" && binding.model == "gpt-6.1-sol")
        .ok_or(())?;
    let mut credentials = Credentials::new();
    credentials.insert_account(binding.credential(), access);
    let gateway = Gateway::new(
        catalog::default_topology().map_err(|_| ())?,
        vec![Entry {
            model: binding.model.into(),
            protocol: Profile::Responses,
            endpoint: binding.endpoint_id(),
        }],
        credentials,
        SecretMaterial::new(configuration.client_key.expose()).map_err(|_| ())?,
        Limits::default(),
        configuration.proxy.as_deref(),
    )
    .map_err(|_| ())?
    .with_probe_diagnostics(&configuration.diagnostics)
    .map_err(|_| ())?;
    let listener = tokio::net::TcpListener::bind(configuration.bind)
        .await
        .map_err(|_| ())?;
    println!(
        "MorphieCore listening on http://{}",
        listener.local_addr().map_err(|_| ())?
    );
    let owner = gateway.clone();
    let result = gateway
        .serve(listener, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    owner.flush_probe_diagnostics().await;
    result.map_err(|_| ())
}

#[tokio::main]
async fn main() -> ExitCode {
    let Ok(options) = Options::try_parse() else {
        eprintln!("Invalid probe arguments; values suppressed.");
        return ExitCode::FAILURE;
    };
    match run(options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => {
            eprintln!("SIWC probe unavailable; private diagnostics suppressed.");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_is_closed_and_excludes_alias_and_credential_material() {
        let base = serde_json::json!({
            "client_key":"synthetic-client-token-0001","bind":"127.0.0.1:0",
            "proxy":null,"diagnostics":"synthetic.jsonl",
            "models":["gpt-6.1-sol"],"max_attempts":1});
        let configuration: Configuration = serde_json::from_value(base.clone()).unwrap();
        assert!(configuration.valid());
        for (field, value) in [
            ("models", serde_json::json!(["grok-4.7"])),
            ("bind", serde_json::json!("0.0.0.0:0")),
            ("max_attempts", serde_json::json!(2)),
        ] {
            let mut invalid = base.clone();
            invalid[field] = value;
            let configuration: Configuration = serde_json::from_value(invalid).unwrap();
            assert!(!configuration.valid());
        }
        let mut invalid = base;
        invalid["alias"] = serde_json::json!("synthetic");
        assert!(serde_json::from_value::<Configuration>(invalid).is_err());
    }
}
