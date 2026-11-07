//! Startup-only private service and model configuration. No credential values from env.
#[cfg(test)]
mod account_tests;
#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod conformance_tests;
#[cfg(test)]
mod managed_key_tests;
#[cfg(test)]
mod media_tests;
#[cfg(test)]
mod messages_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
use crate::{
    credential::{CredentialManager, Secret},
    gateway::{Gateway, StartupError},
    provider::SecretMaterial,
};
#[cfg(test)]
use crate::{gateway::Limits, protocol::openai::GenerationApi, topology::catalog};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use super::settings::Configuration;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthConfiguration {
    version: u32,
    agents: Vec<AgentKeyConfiguration>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentKeyConfiguration {
    agent_id: String,
    key: Secret,
    #[serde(default)]
    name: String,
    #[serde(default)]
    desc: String,
}

impl AuthConfiguration {
    fn into_agents(
        self,
    ) -> Result<std::collections::BTreeMap<String, SecretMaterial>, StartupError> {
        if self.version != 1 {
            return Err(crate::configuration::ConfigurationError::new(
                "auth",
                "version",
                "expected schema version 1",
            )
            .into());
        }
        let mut agents = std::collections::BTreeMap::new();
        for (index, agent) in self.agents.into_iter().enumerate() {
            let invalid = |field, reason| -> StartupError {
                crate::configuration::ConfigurationError::new(
                    "auth",
                    format!("agents[{index}].{field}"),
                    reason,
                )
                .into()
            };
            if agent.name.len() > 256
                || agent.name.chars().any(char::is_control)
                || agent.desc.len() > 2048
            {
                return Err(invalid(
                    "name/desc",
                    "metadata exceeds the allowed length or contains control characters",
                ));
            }
            let key = SecretMaterial::new(agent.key.expose())
                .map_err(|_| invalid("key", "expected printable non-space ASCII key"))?;
            if agents.insert(agent.agent_id, key).is_some() {
                return Err(invalid(
                    "agent_id",
                    "duplicate Agent identity; assign distinct IDs",
                ));
            }
        }
        Ok(agents)
    }
}
fn relative(parent: &Path, path: &Path) -> Result<PathBuf, StartupError> {
    if path.as_os_str().is_empty() {
        return Err(StartupError::Configuration("gateway path"));
    }
    Ok(parent.join(path))
}
use crate::configuration::read as read_configuration;

pub struct Bootstrap {
    pub gateway: Gateway,
    pub listen: SocketAddr,
    explanation: serde_json::Value,
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
        Self::load(configuration, Some(directory), None, false, true)
    }
    /// CLI overrides beat file values. Omitted paths retain independent defaults.
    pub fn from_options(
        configuration: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
    ) -> Result<Self, StartupError> {
        Self::load(configuration, directory, proxy, true, true)
    }
    /// Production construction without listener, capture or diagnostic activation.
    pub fn validate(
        configuration: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
    ) -> Result<(), StartupError> {
        Self::load(configuration, directory, proxy, true, false).map(|_| ())
    }
    /// Validate local bindings and describe effective budgets without starting any service.
    pub fn explain(
        configuration: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
    ) -> Result<serde_json::Value, StartupError> {
        Self::load(configuration, directory, proxy, true, false).map(|boot| boot.explanation)
    }
    fn load(
        path: &Path,
        directory: Option<&Path>,
        proxy: Option<&str>,
        environment: bool,
        activate: bool,
    ) -> Result<Self, StartupError> {
        let configuration: Configuration = read_configuration(path, "gateway")?;
        if configuration.version != 1 {
            return Err(crate::configuration::ConfigurationError::new(
                "gateway",
                "version",
                "expected schema version 1",
            )
            .into());
        }
        if !configuration.bind.ip().is_loopback() {
            return Err(StartupError::Listener);
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let auth: AuthConfiguration =
            read_configuration(&relative(parent, &configuration.auth_file)?, "auth")?;
        let agents = auth.into_agents()?;
        let directory = match directory {
            Some(path) if !path.as_os_str().is_empty() => path.to_owned(),
            Some(_) => return Err(StartupError::Configuration("credential directory")),
            None => match &configuration.credentials_dir {
                Some(path) => relative(parent, path)?,
                None => crate::credential::resolve_store_directory(None)
                    .map_err(|_| StartupError::Credentials)?,
            },
        };
        let models: super::models::Models =
            read_configuration(&relative(parent, &configuration.models_file)?, "models")?;
        if !activate && !directory.is_dir() {
            return Err(StartupError::Credentials);
        }
        let accounts =
            crate::account::Accounts::load(&relative(parent, &configuration.accounts_file)?)?;
        let mut limits = configuration.limits();
        limits.provider_recovery = accounts
            .providers
            .iter()
            .map(|(id, provider)| (id.clone(), provider.recovery.clone()))
            .collect();
        limits.validate()?;
        let manager = CredentialManager::new(
            directory,
            crate::credential::drivers::builtin_drivers_with_environment(
                proxy.or(configuration.proxy.as_deref()),
                environment,
            )
            .map_err(|_| StartupError::Credentials)?,
        )
        .map_err(|_| StartupError::Credentials)?;
        let accounts = accounts.bind(&manager)?;
        let plan = models.bind(&accounts)?;
        let explanation = serde_json::json!({
            "valid":true,"version":1,
            "routing":configuration.routing,"oauth":configuration.oauth,
            "delivery":configuration.delivery,"resources":configuration.resources,
            "proxy_source":if proxy.is_some() {"cli"} else if configuration.proxy.is_some() {"config"} else {"environment"},
            "diagnostics_enabled":configuration.diagnostics.is_some(),
            "capture":configuration.capture.as_ref().map(|capture| serde_json::json!({
                "enabled":capture.enabled(),
                "client_request":capture.client_request,"upstream_request":capture.upstream_request,
                "upstream_response":capture.upstream_response,"client_response":capture.client_response,
                "max_unit_bytes":capture.max_unit_bytes,"max_exchange_bytes":capture.max_exchange_bytes,
                "max_total_bytes":capture.max_total_bytes
            })),
            "providers":limits.provider_recovery,
            "selections":plan.explanation,
            "semantics":"Body collection has a separate deadline. Execution starts after admission; model and endpoint deadlines narrow it. Omitted service fields use application defaults. Context/input capacities are planning metadata; output maxima do not fill absent request caps."
        });
        let mut gateway = Gateway::bind(
            plan.topology,
            plan.entries,
            plan.credentials,
            agents,
            limits,
            proxy.or(configuration.proxy.as_deref()),
            environment,
        )?;
        if let Some(path) = configuration.diagnostics {
            let path = relative(parent, &path)?;
            crate::gateway::diagnostics::Sink::validate(&path)
                .map_err(|_| StartupError::Environment)?;
            if activate {
                gateway = gateway.with_diagnostics(&path)?;
            }
        }
        if let Some(mut capture) = configuration.capture {
            capture.path = relative(parent, &capture.path)?;
            if capture.enabled() {
                capture
                    .validate()
                    .map_err(|_| StartupError::Configuration("capture"))?;
            }
            if activate {
                gateway = gateway.with_capture(capture)?;
            }
        }
        Ok(Self {
            gateway,
            listen: configuration.bind,
            explanation,
        })
    }
}
