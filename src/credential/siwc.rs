//! Independent public SIWC driver; no Codex client, account header or auth-cache import.
//! https://developers.openai.com/siwc/token-sharing-open-source/sign-in
use super::{
    AuthDriver, Credential, CredentialError as Error, DriverFuture, Grant, LoginMethod,
    LoginObserver, LoginOptions, LoginPrompt, PendingRenewal, RegistrationContext, Renewal, Secret,
    VerifiedIdentity,
    browser::{BrowserGrant, BrowserProfile},
    http::{AuthHttp, REQUEST_TIMEOUT},
    model::now,
    oauth::{self, present},
};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::Instant;

pub const APPLICATION_NAME: &str = "MorphieCore";
pub const APPLICATION_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
pub const USER_AGENT: &str = concat!("MorphieCore/", env!("CARGO_PKG_VERSION"));
pub(super) const BOOTSTRAP_CLIENT: &str = "dynamic_agent_client";
const ORIGIN: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const TOKEN: &str = "/api/accounts/oauth/token";
const REVOKE: &str = "/api/accounts/oauth/revoke";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

pub(super) struct SiwcAuthority {
    http: AuthHttp,
}
#[derive(Deserialize)]
struct TokenWire {
    access_token: String,
    #[serde(default, deserialize_with = "present")]
    refresh_token: Option<String>,
    #[serde(default, deserialize_with = "present")]
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    #[serde(default, deserialize_with = "present")]
    scope: Option<String>,
}
impl SiwcAuthority {
    pub fn new(proxy: Option<&str>) -> Result<Self, Error> {
        Ok(Self {
            http: AuthHttp::with_user_agent(ORIGIN, proxy, USER_AGENT)?,
        })
    }
    #[cfg(test)]
    pub fn synthetic(origin: &str) -> Result<Self, Error> {
        Ok(Self {
            http: AuthHttp::synthetic(origin, USER_AGENT)?,
        })
    }
    async fn identity(
        &self,
        token: &str,
        client: &str,
        nonce: Option<&str>,
        deadline: Instant,
    ) -> Result<VerifiedIdentity, Error> {
        let (status, body) = self.http.get("/.well-known/jwks.json", deadline).await?;
        if status != 200 {
            return Err(failure(status, &body));
        }
        let keys = serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
        verify_identity(token, &keys, client, nonce)
    }
}
pub(super) fn verify_identity(
    token: &str,
    keys: &serde_json::Value,
    client: &str,
    nonce: Option<&str>,
) -> Result<VerifiedIdentity, Error> {
    let verified = super::jwt::verify(
        token,
        keys,
        super::jwt::Policy {
            issuer: ORIGIN,
            client,
            algorithm: super::jwt::Algorithm::Rs256,
            nonce,
        },
    )?;
    let identity = VerifiedIdentity {
        subject: verified.subject,
        scope: None,
    };
    if !identity.valid() {
        return Err(Error::Protocol);
    }
    Ok(identity)
}
fn token(status: u16, body: &[u8], previous: Option<&Credential>) -> Result<PendingRenewal, Error> {
    if status != 200 {
        return Err(failure(status, body));
    }
    let wire: TokenWire = serde_json::from_slice(body).map_err(|_| Error::Protocol)?;
    if !wire.token_type.eq_ignore_ascii_case("bearer") || wire.expires_in == 0 {
        return Err(Error::Protocol);
    }
    let verify_id_token = wire.id_token.is_some();
    let scopes = wire
        .scope
        .or_else(|| previous.and_then(|p| p.scopes.clone()))
        .ok_or(Error::Protocol)?;
    if !scopes.split_ascii_whitespace().any(|s| s == "openid") {
        return Err(Error::Permission);
    }
    let refresh = wire.refresh_token.map(Secret::new).transpose()?;
    if (previous.is_some()
        || scopes
            .split_ascii_whitespace()
            .any(|s| s == "offline_access"))
        && refresh.is_none()
    {
        return Err(Error::Protocol);
    }
    let id_token = wire
        .id_token
        .or_else(|| {
            previous
                .and_then(|p| p.id_token.as_ref())
                .map(|s| s.expose().into())
        })
        .map(Secret::new)
        .transpose()?;
    if id_token.is_none() {
        return Err(Error::Protocol);
    }
    let credential = Credential {
        access: Secret::new(wire.access_token)?,
        refresh,
        id_token,
        expires_at: Some(now()?.checked_add(wire.expires_in).ok_or(Error::Protocol)?),
        scopes: Some(scopes),
    };
    if !credential.valid() {
        return Err(Error::Protocol);
    }
    Ok(PendingRenewal {
        credential,
        verify_id_token,
    })
}
impl AuthDriver for SiwcAuthority {
    fn profile(&self) -> &'static str {
        "siwc"
    }
    fn dynamic_registration(&self) -> bool {
        true
    }
    fn login_client(&self, options: &LoginOptions) -> Result<String, Error> {
        if options.method != LoginMethod::Browser || options.client_id.is_some() {
            return Err(Error::InvalidInput);
        }
        Ok(BOOTSTRAP_CLIENT.into())
    }
    fn validate(
        &self,
        client: &str,
        identity: Option<&VerifiedIdentity>,
        credential: Option<&Credential>,
    ) -> Result<(), Error> {
        super::model::valid_client(client)?;
        if client == BOOTSTRAP_CLIENT && (identity.is_some() || credential.is_some())
            || identity.is_some_and(|i| i.scope.is_some())
            || credential.is_some_and(|c| {
                c.id_token.is_none() || c.expires_at.is_none() || c.scopes.is_none() || !c.valid()
            })
        {
            return Err(Error::Protocol);
        }
        Ok(())
    }
    fn check_access(&self, credential: &Credential) -> Result<(), Error> {
        let scopes = credential.scopes.as_deref().ok_or(Error::Permission)?;
        if ["resource.invoke", "chatgpt.tokens.use.direct"]
            .iter()
            .all(|required| {
                scopes
                    .split_ascii_whitespace()
                    .any(|scope| scope == *required)
            })
        {
            Ok(())
        } else {
            Err(Error::Permission)
        }
    }
    fn login<'a>(
        &'a self,
        _client: &'a str,
        _options: &'a LoginOptions,
        _notify: LoginObserver<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async { Err(Error::Unsupported) })
    }
    fn login_registered<'a>(
        &'a self,
        client: &'a str,
        options: &'a LoginOptions,
        notify: LoginObserver<'a>,
        registration: RegistrationContext<'a>,
    ) -> DriverFuture<'a, Grant> {
        Box::pin(async move {
            self.login_client(options)?;
            if !super::store::valid_host_id(registration.host_id) {
                return Err(Error::InvalidInput);
            }
            let mut extra = vec![
                ("resource", RESOURCE),
                ("ext_agent_host_id", registration.host_id),
            ];
            if client == BOOTSTRAP_CLIENT {
                extra.push(("agent_name_hint", APPLICATION_NAME));
            }
            if options.consent {
                extra.push(("prompt", "consent"));
            }
            let browser = BrowserGrant::begin(
                BrowserProfile {
                    authorize: "https://auth.openai.com/api/accounts/authorize",
                    client,
                    scopes: SCOPES,
                    callback_path: "/auth/callback",
                    extra: &extra,
                    timeout: Duration::from_secs(900),
                },
                options.callback_port.unwrap_or(0),
            )
            .await?;
            notify(&LoginPrompt::Browser(browser.prompt.clone()));
            let deadline = browser.deadline;
            oauth::login_deadline(deadline, async {
                let response = browser
                    .exchange_registered(
                        &self.http,
                        TOKEN,
                        RESOURCE,
                        BOOTSTRAP_CLIENT,
                        registration,
                    )
                    .await?;
                let pending = token(response.status, &response.body, None)?;
                let identity = self
                    .identity(
                        pending
                            .credential
                            .id_token
                            .as_ref()
                            .ok_or(Error::Protocol)?
                            .expose(),
                        &response.client,
                        Some(response.nonce.expose()),
                        deadline,
                    )
                    .await?;
                Ok(Grant {
                    identity,
                    credential: pending.credential,
                })
            })
            .await
        })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn renew<'a>(
        &'a self,
        client: &'a str,
        _identity: &'a VerifiedIdentity,
        previous: &'a Credential,
    ) -> DriverFuture<'a, Renewal> {
        Box::pin(async move {
            if client == BOOTSTRAP_CLIENT {
                return Err(Error::IdentityMismatch);
            }
            let (status, body) = self
                .http
                .request(
                    TOKEN,
                    &[
                        ("grant_type", "refresh_token"),
                        ("client_id", client),
                        (
                            "refresh_token",
                            previous
                                .refresh
                                .as_ref()
                                .ok_or(Error::LoginRequired)?
                                .expose(),
                        ),
                        ("resource", RESOURCE),
                    ],
                    None,
                    Instant::now() + REQUEST_TIMEOUT,
                )
                .await?;
            Ok(Renewal::Pending(token(status, &body, Some(previous))?))
        })
    }
    fn verify_renewal<'a>(
        &'a self,
        client: &'a str,
        identity: &'a VerifiedIdentity,
        pending: &'a PendingRenewal,
    ) -> DriverFuture<'a, VerifiedIdentity> {
        Box::pin(async move {
            if pending.verify_id_token {
                self.identity(
                    pending
                        .credential
                        .id_token
                        .as_ref()
                        .ok_or(Error::Protocol)?
                        .expose(),
                    client,
                    None,
                    Instant::now() + REQUEST_TIMEOUT,
                )
                .await
            } else {
                Ok(identity.clone())
            }
        })
    }
    fn revoke<'a>(&'a self, client: &'a str, credential: &'a Credential) -> DriverFuture<'a, ()> {
        Box::pin(async move {
            if client == BOOTSTRAP_CLIENT {
                return Err(Error::IdentityMismatch);
            }
            let deadline = Instant::now() + REQUEST_TIMEOUT;
            let (status, body) = self
                .http
                .get("/.well-known/openid-configuration", deadline)
                .await?;
            if status != 200 {
                return Err(failure(status, &body));
            }
            #[derive(Deserialize)]
            struct Discovery {
                issuer: String,
                revocation_endpoint: String,
            }
            let discovery: Discovery =
                serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
            if discovery.issuer != ORIGIN
                || discovery.revocation_endpoint != format!("{ORIGIN}{REVOKE}")
            {
                return Err(Error::Protocol);
            }
            let (status, body) = self
                .http
                .request(
                    REVOKE,
                    &[
                        ("client_id", client),
                        ("token_type_hint", "refresh_token"),
                        (
                            "token",
                            credential
                                .refresh
                                .as_ref()
                                .ok_or(Error::LoginRequired)?
                                .expose(),
                        ),
                    ],
                    None,
                    deadline,
                )
                .await?;
            if status == 200 {
                Ok(())
            } else {
                Err(failure(status, &body))
            }
        })
    }
}
fn failure(status: u16, body: &[u8]) -> Error {
    let value: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let code = value.as_ref().and_then(|v| v.get("error")).and_then(|v| {
        v.as_str()
            .or_else(|| v.get("code").and_then(serde_json::Value::as_str))
    });
    match code {
        Some("invalid_grant" | "invalid_refresh_token") => Error::InvalidGrant,
        Some("token_expired" | "refresh_token_expired") => Error::GrantExpired,
        Some("refresh_token_reused") => Error::GrantReused,
        Some("refresh_token_invalidated") => Error::GrantRevoked,
        _ if status == 401 || status == 403 => Error::Denied,
        _ if status == 400 || status == 404 => Error::Protocol,
        _ => Error::Network,
    }
}
