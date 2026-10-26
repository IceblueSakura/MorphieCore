//! Manual diagnostics over the same embedded Provider metadata as the gateway.
#[cfg(test)]
mod tests;
mod wire;

use clap::{Args, Parser, Subcommand, ValueEnum};
use morphiecore::{
    credential::{
        CredentialError, CredentialManager, CredentialRef, Secret, builtin_drivers,
        resolve_store_directory,
    },
    execution::UpstreamRequest,
    provider::{
        AuthScheme, CredentialKind, EndpointPath, GenerationOperation, ProviderDefinition,
        ProviderOperation, SecretMaterial, catalog,
    },
    transport::http::HttpTransport,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const RESPONSE_BYTES: usize = 4 << 20;
const PROMPT: &str = "Reply with the single word OK.";

#[derive(Parser)]
#[command(
    about = "Query a Provider's model list or send one text probe using shared embedded metadata"
)]
pub struct Options {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Print the full model-list JSON response once; no pagination or inference.
    Models(Target),
    /// Send a fixed short prompt to an exact upstream model, without retries.
    Probe(Probe),
}
#[derive(Args)]
struct Target {
    /// Provider ID from the compiled providers.json catalog.
    provider: String,
    /// Override the default pool with one local API-key alias, never a key value.
    #[arg(long, conflicts_with = "account")]
    api_key: Option<String>,
    /// Override the default pool with one local OAuth account alias.
    #[arg(long, conflicts_with = "api_key")]
    account: Option<String>,
    #[arg(long)]
    store: Option<PathBuf>,
    /// Override system/environment proxies for credential refresh and upstream HTTP.
    #[arg(long)]
    proxy: Option<String>,
    /// Deadline for credential refresh and upstream response EOF.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=300))]
    timeout_seconds: u64,
}
#[derive(Args)]
struct Probe {
    #[command(flatten)]
    target: Target,
    /// Exact upstream name; need not exist in the local model catalog.
    #[arg(long)]
    model: String,
    #[arg(long, value_enum)]
    protocol: Protocol,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Protocol {
    OpenaiChat,
    OpenaiResponses,
    Anthropic,
    Google,
}
impl Protocol {
    fn operation(self) -> GenerationOperation {
        match self {
            Self::OpenaiChat => GenerationOperation::ChatCompletions,
            Self::OpenaiResponses => GenerationOperation::Responses,
            Self::Anthropic => GenerationOperation::AnthropicMessages,
            Self::Google => GenerationOperation::GoogleGenerateContent,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::OpenaiChat => "openai-chat",
            Self::OpenaiResponses => "openai-responses",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("invalid operation options")]
    Invalid,
    #[error("unknown Provider; update providers.json and rebuild")]
    Provider,
    #[error("operation endpoint is not declared in providers.json")]
    Endpoint,
    #[error("default credential pool is missing; configure it or select an explicit credential")]
    Pool,
    #[error("credential kind/profile does not match the Provider endpoint")]
    CredentialMismatch,
    #[error("credential operation: {0}")]
    Credential(#[from] CredentialError),
    #[error("transport failed; no retry")]
    Network,
    #[error("operation timed out; no retry; check credential status if refresh was in progress")]
    Timeout,
    #[error("upstream HTTP {0}; response body suppressed; no retry")]
    Http(u16),
    #[error("upstream response exceeds the diagnostic bound")]
    Limit,
    #[error("invalid or unsupported upstream response; body suppressed")]
    Response,
    #[error("probe did not complete with nonempty text; no retry")]
    Incomplete,
}
struct Plan {
    provider: &'static ProviderDefinition,
    path: EndpointPath,
    auth: AuthScheme,
    oauth: Option<&'static str>,
    siwc: bool,
    body: Option<Value>,
}
impl Command {
    fn target(&self) -> &Target {
        match self {
            Self::Models(c) => c,
            Self::Probe(c) => &c.target,
        }
    }
    fn plan(&self) -> Result<Plan, Error> {
        let target = self.target();
        if target.api_key.is_some() && target.account.is_some()
            || !(1..=300).contains(&target.timeout_seconds)
        {
            return Err(Error::Invalid);
        }
        let provider = catalog::get(&target.provider).ok_or(Error::Provider)?;
        let operation = match self {
            Self::Models(_) => ProviderOperation::Models,
            Self::Probe(options) => options.protocol.operation().into(),
        };
        let entry = provider.entry(operation).ok_or(Error::Endpoint)?;
        let oauth = match entry.auth.kind() {
            CredentialKind::OAuth(profile) if target.api_key.is_none() => Some(profile),
            CredentialKind::ApiKey if target.account.is_none() => None,
            _ => return Err(Error::CredentialMismatch),
        };
        let siwc = oauth == Some("openai");
        let (path, body) = match self {
            Self::Models(_) => (entry.path.clone(), None),
            Self::Probe(options) => {
                if options.model.is_empty()
                    || options.model.len() > 256
                    || options
                        .model
                        .chars()
                        .any(|c| c.is_control() || c.is_whitespace())
                {
                    return Err(Error::Invalid);
                }
                let model = if options.protocol == Protocol::Google {
                    options
                        .model
                        .strip_prefix("models/")
                        .unwrap_or(&options.model)
                } else {
                    &options.model
                };
                let path = options
                    .protocol
                    .operation()
                    .resolve_path(entry.path, model, siwc)
                    .map_err(|_| Error::Invalid)?;
                (
                    path,
                    Some(wire::request(options.protocol, &options.model, siwc)),
                )
            }
        };
        Ok(Plan {
            provider,
            path,
            auth: entry.auth,
            oauth,
            siwc,
            body,
        })
    }
}
fn selected_credential(
    manager: &CredentialManager,
    target: &Target,
    plan: &Plan,
) -> Result<CredentialRef, Error> {
    let member = if let Some(alias) = &target.api_key {
        CredentialRef::ApiKey {
            alias: alias.clone(),
        }
    } else if let Some(alias) = &target.account {
        CredentialRef::OAuth {
            profile: plan.oauth.ok_or(Error::CredentialMismatch)?.into(),
            alias: alias.clone(),
        }
    } else {
        let binding = plan.provider.credential_pool.as_ref().ok_or(Error::Pool)?;
        let pool = manager
            .pool(plan.provider.id.as_str(), binding.as_str())?
            .ok_or(Error::Pool)?;
        pool.config.validate()?;
        pool.config.members.into_iter().next().ok_or(Error::Pool)?
    };
    match (&member, plan.auth.kind()) {
        (CredentialRef::ApiKey { .. }, CredentialKind::ApiKey) => {}
        (CredentialRef::OAuth { profile, .. }, CredentialKind::OAuth(expected))
            if profile == expected => {}
        _ => return Err(Error::CredentialMismatch),
    }
    Ok(member)
}
/// Only the selected member is borrowed. Expiry may refresh it; failure never selects another.
async fn credential(
    manager: &CredentialManager,
    target: &Target,
    plan: &Plan,
) -> Result<(Secret, bool), Error> {
    match selected_credential(manager, target, plan)? {
        CredentialRef::ApiKey { alias } => Ok((
            manager
                .bind_api_key(plan.provider.id.as_str(), &alias)?
                .borrow()?,
            false,
        )),
        CredentialRef::OAuth { profile, alias } => {
            let binding = manager.bind_access(&profile, &alias)?;
            let pending = manager
                .list(Some(&profile))?
                .iter()
                .any(|s| s.account == alias && s.renewal_pending);
            if pending {
                manager.refresh(&profile, &alias).await?;
                return Ok((binding.borrow()?.access, true));
            }
            match binding.borrow() {
                Ok(grant) => Ok((grant.access, false)),
                Err(CredentialError::Expired) => {
                    manager.refresh(&profile, &alias).await?;
                    Ok((binding.borrow()?.access, true))
                }
                Err(error) => Err(error.into()),
            }
        }
    }
}
pub async fn execute(options: Options) -> Result<Value, Error> {
    let plan = options.command.plan()?;
    let target = options.command.target();
    let transport = HttpTransport::new(target.proxy.as_deref()).map_err(|_| Error::Invalid)?;
    let root = resolve_store_directory(target.store.clone())?;
    let drivers = if plan.oauth.is_some() {
        builtin_drivers(target.proxy.as_deref())?
    } else {
        vec![]
    };
    let manager = CredentialManager::new(root, drivers)?;
    tokio::time::timeout(
        Duration::from_secs(target.timeout_seconds),
        run(&options.command, plan, &manager, &transport),
    )
    .await
    .map_err(|_| Error::Timeout)?
}
async fn run(
    command: &Command,
    plan: Plan,
    manager: &CredentialManager,
    transport: &HttpTransport,
) -> Result<Value, Error> {
    let start = Instant::now();
    let target = command.target();
    let (secret, refreshed) = credential(manager, target, &plan).await?;
    let material = SecretMaterial::new(secret.expose()).map_err(|_| Error::Invalid)?;
    let mut headers = vec![(
        "accept".into(),
        if plan.siwc && plan.body.is_some() {
            "text/event-stream"
        } else {
            "application/json"
        }
        .into(),
    )];
    if plan.body.is_some() {
        headers.push(("content-type".into(), "application/json".into()));
    }
    if matches!(command, Command::Probe(options) if options.protocol == Protocol::Anthropic) {
        headers.push((
            "anthropic-version".into(),
            morphiecore::protocol::anthropic::messages::API_VERSION.into(),
        ));
    }
    let prepared = UpstreamRequest {
        origin: plan.provider.origin.as_str().into(),
        path: plan.path.as_str().into(),
        method: if plan.body.is_some() { "POST" } else { "GET" },
        safe_headers: headers,
        auth_header: plan.auth.auth_header(&material),
        body: plan
            .body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| Error::Invalid)?
            .unwrap_or_default(),
    };
    let mut response = transport
        .send(prepared, Duration::from_secs(target.timeout_seconds))
        .await
        .map_err(|e| {
            if e == morphiecore::transport::http::TransportError::Timeout {
                Error::Timeout
            } else {
                Error::Network
            }
        })?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Error::Http(status));
    }
    let expected = if plan.siwc && plan.body.is_some() {
        "text/event-stream"
    } else {
        "application/json"
    };
    match response.headers().get("content-type") {
        None if plan.siwc && plan.body.is_some() => {}
        Some(value) => {
            let content_type = value
                .to_str()
                .ok()
                .and_then(|s| s.parse::<mime::Mime>().ok())
                .ok_or(Error::Response)?;
            if content_type.essence_str() != expected {
                return Err(Error::Response);
            }
        }
        None => return Err(Error::Response),
    }
    if response
        .content_length()
        .is_some_and(|n| n > RESPONSE_BYTES as u64)
    {
        return Err(Error::Limit);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        if e.is_timeout() {
            Error::Timeout
        } else {
            Error::Network
        }
    })? {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_BYTES {
            return Err(Error::Limit);
        }
        bytes.extend_from_slice(&chunk);
    }
    let Command::Probe(options) = command else {
        let value = wire::decode(&bytes)?;
        if contains_secret(&value, secret.expose()) {
            return Err(Error::Response);
        }
        return Ok(value);
    };
    let result = if plan.siwc {
        wire::responses_stream(&bytes)?
    } else {
        wire::probe(&bytes, options.protocol)?
    };
    let report = json!({"operation":"probe", "provider":target.provider, "model":options.model, "protocol":options.protocol.name(), "http_status":status, "requests":1, "oauth_refreshed":refreshed, "elapsed_ms":start.elapsed().as_millis(), "response_bytes":bytes.len(), "result":result});
    if contains_secret(&report, secret.expose()) {
        return Err(Error::Response);
    }
    Ok(report)
}
fn contains_secret(value: &Value, secret: &str) -> bool {
    match value {
        Value::String(text) => text.contains(secret),
        Value::Array(values) => values.iter().any(|v| contains_secret(v, secret)),
        Value::Object(values) => values
            .iter()
            .any(|(k, v)| k.contains(secret) || contains_secret(v, secret)),
        _ => false,
    }
}
