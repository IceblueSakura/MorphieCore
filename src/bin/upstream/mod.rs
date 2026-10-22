//! Explicit operator targets and credential lifecycle; no gateway or model registration.
#[cfg(test)]
mod tests;
mod wire;

use clap::{Args, Parser, Subcommand, ValueEnum};
use morphiecore::{
    credential::{
        CredentialError, CredentialManager, Secret, builtin_drivers, resolve_store_directory,
    },
    execution::UpstreamRequest,
    provider::{
        AuthScheme, CredentialKind, EndpointPath, GenerationOperation, ProviderId, SecretMaterial,
        TrustedOrigin, catalog,
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
    about = "Manually list upstream models or send one bounded text probe; never activates models"
)]
pub struct Options {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Fetch and print the full model-list JSON response once; no pagination or inference.
    Models(Target),
    /// Send one fixed short prompt to an exact upstream model, without retries.
    Probe(Probe),
}
#[derive(Args)]
struct Target {
    /// Registered Provider ID, or the credential domain for an explicit custom target.
    #[arg(long)]
    provider: String,
    #[arg(long, value_enum)]
    protocol: Protocol,
    /// Local API-key alias, not the key itself. Mutually exclusive with --account.
    #[arg(long, conflicts_with = "account", required_unless_present = "account")]
    api_key: Option<String>,
    /// Local OAuth account alias in the selected Provider's registered profile.
    #[arg(long, conflicts_with = "api_key", required_unless_present = "api_key")]
    account: Option<String>,
    #[arg(long)]
    store: Option<PathBuf>,
    /// Explicit trusted origin, without path/query; API keys only. Requires --path.
    #[arg(long, requires = "path")]
    origin: Option<String>,
    /// Explicit operation path; API keys only. Google probe paths are model collections.
    #[arg(long)]
    path: Option<String>,
    /// Override API-key header scheme for a provider dialect. Never accepts raw headers.
    #[arg(long, value_enum)]
    auth: Option<KeyAuth>,
    /// Explicit proxy for both credential refresh and upstream HTTP. No environment proxy.
    #[arg(long)]
    proxy: Option<String>,
    /// Whole operation deadline, including credential refresh and response EOF.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=300))]
    timeout_seconds: u64,
}
#[derive(Args)]
struct Probe {
    #[command(flatten)]
    target: Target,
    /// Exact upstream name; need not exist in the local catalog or a model-list result.
    #[arg(long)]
    model: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Protocol {
    OpenaiChat,
    OpenaiResponses,
    Anthropic,
    Google,
}
#[derive(Clone, Copy, ValueEnum)]
enum KeyAuth {
    Bearer,
    XApiKey,
    XGoogApiKey,
}
impl KeyAuth {
    fn scheme(self) -> AuthScheme {
        match self {
            Self::Bearer => AuthScheme::Bearer,
            Self::XApiKey => AuthScheme::ApiKeyHeader("x-api-key"),
            Self::XGoogApiKey => AuthScheme::ApiKeyHeader("x-goog-api-key"),
        }
    }
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
    fn auth(self) -> AuthScheme {
        match self {
            Self::OpenaiChat | Self::OpenaiResponses => AuthScheme::Bearer,
            Self::Anthropic => AuthScheme::ApiKeyHeader("x-api-key"),
            Self::Google => AuthScheme::ApiKeyHeader("x-goog-api-key"),
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
    #[error("invalid target or operation options")]
    Invalid,
    #[error("no documented default endpoint; specify --path with an API-key credential")]
    Endpoint,
    #[error("OAuth requires its registered Provider, protocol and target; overrides are forbidden")]
    OAuthTarget,
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
    origin: TrustedOrigin,
    path: String,
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
        ProviderId::new(&target.provider).map_err(|_| Error::Invalid)?;
        if target.api_key.is_some() == target.account.is_some()
            || !(1..=300).contains(&target.timeout_seconds)
        {
            return Err(Error::Invalid);
        }
        let provider = catalog::all()
            .into_iter()
            .find(|p| p.id.as_str() == target.provider);
        let oauth = if target.account.is_some() {
            if target.origin.is_some()
                || target.path.is_some()
                || target.auth.is_some()
                || target.protocol != Protocol::OpenaiResponses
            {
                return Err(Error::OAuthTarget);
            }
            match provider.as_ref().map(|p| p.auth.kind()) {
                Some(CredentialKind::OAuth(profile)) => Some(profile),
                _ => return Err(Error::OAuthTarget),
            }
        } else {
            None
        };
        let siwc = oauth == Some("openai");
        let origin = match &target.origin {
            Some(origin) if target.path.is_some() => {
                TrustedOrigin::parse(origin).map_err(|_| Error::Invalid)?
            }
            Some(_) => return Err(Error::Invalid),
            None => provider.as_ref().ok_or(Error::Endpoint)?.origin.clone(),
        };
        let operation = provider
            .as_ref()
            .and_then(|p| p.generation_entry(target.protocol.operation()));
        let auth = if let Some(profile) = oauth {
            AuthScheme::OAuthBearer(profile)
        } else {
            target.auth.map(KeyAuth::scheme).unwrap_or_else(|| {
                operation.map_or(target.protocol.auth(), |entry| match entry.auth {
                    AuthScheme::OAuthBearer(_) => AuthScheme::Bearer,
                    auth => auth,
                })
            })
        };
        let path = match self {
            Self::Probe(_) => target
                .path
                .as_deref()
                .or(operation.map(|e| e.path.as_str())),
            Self::Models(_) => {
                target
                    .path
                    .as_deref()
                    .or(match (target.provider.as_str(), target.protocol) {
                        ("openai" | "grok", Protocol::OpenaiChat | Protocol::OpenaiResponses) => {
                            Some("/v1/models")
                        }
                        ("deepseek", Protocol::OpenaiChat | Protocol::OpenaiResponses) => {
                            Some("/models")
                        }
                        ("openrouter", Protocol::OpenaiChat | Protocol::OpenaiResponses) => {
                            Some("/api/v1/models")
                        }
                        ("google", Protocol::Google) => Some("/v1beta/models"),
                        _ => None,
                    })
            }
        }
        .ok_or(Error::Endpoint)?;
        let mut path = EndpointPath::new(path)
            .map_err(|_| Error::Invalid)?
            .as_str()
            .to_owned();
        let body = match self {
            Self::Models(_) => None,
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
                if target.protocol == Protocol::Google {
                    let model = options
                        .model
                        .strip_prefix("models/")
                        .unwrap_or(&options.model);
                    if model.is_empty()
                        || !model
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    {
                        return Err(Error::Invalid);
                    }
                    path = EndpointPath::new(&format!("{path}/{model}:generateContent"))
                        .map_err(|_| Error::Invalid)?
                        .as_str()
                        .into();
                }
                Some(wire::request(target.protocol, &options.model, siwc))
            }
        };
        Ok(Plan {
            origin,
            path,
            auth,
            oauth,
            siwc,
            body,
        })
    }
}

/// Refresh only locally expired access or a pending verified renewal. Never on HTTP failure.
async fn credential(
    manager: &CredentialManager,
    target: &Target,
    profile: Option<&str>,
) -> Result<(Secret, bool), Error> {
    if let Some(profile) = profile {
        let alias = target.account.as_deref().ok_or(Error::Invalid)?;
        let binding = manager.bind_access(profile, alias)?;
        let pending = manager
            .list(Some(profile))?
            .iter()
            .any(|s| s.account == alias && s.renewal_pending);
        if pending {
            manager.refresh(profile, alias).await?;
            return Ok((binding.borrow()?.access, true));
        }
        match binding.borrow() {
            Ok(grant) => Ok((grant.access, false)),
            Err(CredentialError::Expired) => {
                manager.refresh(profile, alias).await?;
                Ok((binding.borrow()?.access, true))
            }
            Err(error) => Err(error.into()),
        }
    } else {
        Ok((
            manager
                .bind_api_key(
                    &target.provider,
                    target.api_key.as_deref().ok_or(Error::Invalid)?,
                )?
                .borrow()?,
            false,
        ))
    }
}

pub async fn execute(options: Options) -> Result<Value, Error> {
    let plan = options.command.plan()?; // Reject unsafe targets before loading or refreshing credentials.
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
    let (secret, refreshed) = credential(manager, target, plan.oauth).await?;
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
    if target.protocol == Protocol::Anthropic {
        headers.push(("anthropic-version".into(), "2023-06-01".into()));
    }
    let prepared = UpstreamRequest {
        origin: plan.origin.as_str().into(),
        path: plan.path,
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
            if e == morphiecore::provider::ErrorClass::Timeout {
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
        wire::probe(&bytes, target.protocol)?
    };
    let report = json!({"operation":"probe", "provider":target.provider, "model":options.model, "protocol":target.protocol.name(), "http_status":status, "requests":1, "oauth_refreshed":refreshed, "elapsed_ms":start.elapsed().as_millis(), "response_bytes":bytes.len(), "result":result});
    // Only allowlisted metadata is reported; even an adversarial list must not echo auth.
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
