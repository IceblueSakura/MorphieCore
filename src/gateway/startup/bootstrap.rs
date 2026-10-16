//! Startup-only private service and model configuration. No credential values from env.
#[cfg(test)]
mod managed_key_tests;
#[cfg(test)]
mod media_tests;
#[cfg(test)]
mod tests;
use crate::{
    credential::{CredentialManager, Secret},
    gateway::{Gateway, Limits, StartupError},
    provider::SecretMaterial,
    semantic::value::{JsonLimits, parse_json},
};
#[cfg(test)]
use crate::{protocol::openai::Profile, topology::catalog};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    client_key: Secret,
    #[serde(default = "listen")]
    bind: SocketAddr,
    credentials_dir: Option<PathBuf>,
    #[serde(default = "models_file")]
    models_file: PathBuf,
    proxy: Option<String>,
    diagnostics: Option<PathBuf>,
    capture: Option<crate::gateway::CaptureConfig>,
    concurrency: Option<usize>,
    exchange_timeout_ms: Option<u64>,
    delta_coalescing: Option<crate::execution::delivery::DeltaCoalescing>,
}
fn listen() -> SocketAddr {
    ([127, 0, 0, 1], 8080).into()
}
fn models_file() -> PathBuf {
    "models.json".into()
}
fn relative(parent: &Path, path: &Path) -> Result<PathBuf, StartupError> {
    if path.as_os_str().is_empty() {
        return Err(StartupError::Configuration("gateway path"));
    }
    Ok(parent.join(path))
}
fn read_configuration<T: serde::de::DeserializeOwned>(
    path: &Path,
    domain: &'static str,
) -> Result<T, StartupError> {
    let bytes = crate::credential::read_private_file(path, 65536)
        .map_err(|_| StartupError::Configuration(domain))?;
    let value = parse_json(
        &bytes,
        JsonLimits {
            bytes: 65536,
            depth: 16,
            nodes: 16384,
        },
    )
    .map_err(|_| StartupError::Configuration(domain))?;
    serde_json::from_value(value).map_err(|_| StartupError::Configuration(domain))
}

pub struct Bootstrap {
    pub gateway: Gateway,
    pub listen: SocketAddr,
}
impl Bootstrap {
    pub fn from_directory(directory: &Path) -> Result<Self, StartupError> {
        Self::from_files(
            &directory.join("gateway.json"),
            &directory.join("credentials"),
        )
    }
    /// An explicit library store does not opt into environment proxies.
    pub fn from_files(configuration: &Path, directory: &Path) -> Result<Self, StartupError> {
        Self::load(configuration, Some(directory), None, false)
    }
    /// CLI overrides beat file values. Omitted paths retain independent defaults.
    pub fn from_options(
        configuration: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::load(configuration, directory, proxy, true)
    }
    fn load(
        path: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
        environment: bool,
    ) -> Result<Self, StartupError> {
        let configuration: Configuration = read_configuration(path, "gateway")?;
        if !configuration.bind.ip().is_loopback() {
            return Err(StartupError::Listener);
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let directory = match directory {
            Some(path) if !path.as_os_str().is_empty() => path.to_owned(),
            Some(_) => return Err(StartupError::Configuration("credential directory")),
            None => match configuration.credentials_dir {
                Some(path) => relative(parent, &path)?,
                None => crate::credential::resolve_store_directory(None)
                    .map_err(|_| StartupError::Credentials)?,
            },
        };
        let models: super::models::Models =
            read_configuration(&relative(parent, &configuration.models_file)?, "models")?;
        let defaults = Limits::default();
        let limits = Limits {
            delta_coalescing: configuration.delta_coalescing,
            implicit_cache_affinity: models.implicit_cache_affinity,
            concurrency: configuration.concurrency.unwrap_or(defaults.concurrency),
            exchange_timeout: configuration
                .exchange_timeout_ms
                .map(Duration::from_millis)
                .unwrap_or(defaults.exchange_timeout),
            ..defaults
        };
        let manager = CredentialManager::new(
            directory,
            crate::credential::builtin_drivers(None).map_err(|_| StartupError::Credentials)?,
        )
        .map_err(|_| StartupError::Credentials)?;
        let plan = models.bind(&manager)?;
        let mut gateway = Gateway::bind(
            plan.topology,
            plan.entries,
            plan.credentials,
            SecretMaterial::new(configuration.client_key.expose())
                .map_err(|_| StartupError::Credentials)?,
            limits,
            proxy.or(configuration.proxy.as_deref()),
            environment,
        )?;
        if let Some(path) = configuration.diagnostics {
            gateway = gateway.with_diagnostics(&relative(parent, &path)?)?;
        }
        if let Some(mut capture) = configuration.capture {
            capture.path = relative(parent, &capture.path)?;
            gateway = gateway.with_capture(capture)?;
        }
        Ok(Self {
            gateway,
            listen: configuration.bind,
        })
    }
}
