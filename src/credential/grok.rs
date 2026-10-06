//! Fixed xAI personal browser/device/refresh wire, not a selectable authority.
//! Sources: https://auth.x.ai/.well-known/openid-configuration and
//! https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-login/src/device_code.rs
use super::http::{AuthHttp, REQUEST_TIMEOUT};
use super::model::VerifiedIdentity;
use super::{
    CredentialError as Error,
    browser::{BrowserGrant, BrowserProfile},
    grok_metadata::{CLIENT_ID, REFERRER, VERSION, device_surface, user_agent},
    model::DevicePrompt,
    model::{Credential, Secret, now},
    oauth::{self, present},
};
use serde::Deserialize;
use std::{collections::BTreeSet, time::Duration};
use tokio::time::Instant;

pub(super) const SCOPES: &str = "openid profile email offline_access grok-cli:access api:access";
const ORIGIN: &str = "https://auth.x.ai";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Clone)]
pub(super) struct GrokAuthority {
    http: AuthHttp,
    surface: &'static str,
}
#[derive(Debug)]
pub(super) struct DeviceGrant {
    pub prompt: DevicePrompt,
    device_code: Secret,
    interval: Duration,
    deadline: Instant,
}
#[derive(Deserialize)]
struct DeviceWire {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default, deserialize_with = "present")]
    interval: Option<u64>,
}
#[derive(Deserialize)]
struct TokenWire {
    access_token: String,
    #[serde(default, deserialize_with = "present")]
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
    #[serde(default, deserialize_with = "present")]
    id_token: Option<String>,
    #[serde(default, deserialize_with = "present")]
    scope: Option<String>,
}
#[derive(Deserialize)]
struct ErrorWire {
    error: String,
}
#[derive(Deserialize)]
struct IdentityWire {
    sub: String,
}

impl GrokAuthority {
    pub fn new(proxy: Option<&str>) -> Result<Self, Error> {
        Ok(Self {
            http: AuthHttp::with_user_agent(ORIGIN, proxy, &user_agent())?,
            surface: device_surface(),
        })
    }
    #[cfg(test)]
    pub fn synthetic(origin: &str) -> Result<Self, Error> {
        Ok(Self {
            http: AuthHttp::synthetic(origin, &user_agent())?,
            surface: "headless",
        })
    }
    pub async fn browser(&self, client: &str, port: u16) -> Result<BrowserGrant, Error> {
        BrowserGrant::begin(
            BrowserProfile {
                authorize: "https://auth.x.ai/oauth2/authorize",
                client,
                scopes: SCOPES,
                callback_path: "/callback",
                extra: &[("referrer", REFERRER)],
                timeout: LOGIN_TIMEOUT,
            },
            port,
        )
        .await
    }
    pub async fn complete_browser(
        &self,
        grant: BrowserGrant,
    ) -> Result<(Credential, VerifiedIdentity), Error> {
        let deadline = grant.deadline;
        oauth::login_deadline(deadline, async {
            let response = grant
                .exchange(
                    &self.http,
                    "/oauth2/token",
                    &[("x-grok-client-version", VERSION)],
                )
                .await?;
            if response.status != 200 {
                return Err(failure(response.status, &response.body));
            }
            let wire: TokenWire =
                serde_json::from_slice(&response.body).map_err(|_| Error::Protocol)?;
            let id = wire.id_token.as_deref().ok_or(Error::Protocol)?;
            let (status, keys) = self.http.get("/.well-known/jwks.json", deadline).await?;
            if status != 200 {
                return Err(failure(status, &keys));
            }
            let keys = serde_json::from_slice(&keys).map_err(|_| Error::Protocol)?;
            let identity = verify_identity(id, &keys, &response.client, response.nonce.expose())?;
            let credential = parse_token(&response.body, None, SCOPES)?;
            if self.identity(&credential, deadline).await? != identity.subject {
                return Err(Error::IdentityMismatch);
            }
            Ok((credential, identity))
        })
        .await
    }
    pub async fn complete_device(
        &self,
        client: &str,
        grant: DeviceGrant,
    ) -> Result<(Credential, VerifiedIdentity), Error> {
        let deadline = grant.deadline;
        oauth::login_deadline(deadline, async {
            let credential = self.poll(client, grant).await?;
            let subject = self.identity(&credential, deadline).await?;
            Ok((
                credential,
                VerifiedIdentity {
                    subject,
                    scope: None,
                },
            ))
        })
        .await
    }
    pub async fn device(&self, client: &str) -> Result<DeviceGrant, Error> {
        let started = Instant::now();
        let (status, body) = self
            .http
            .request_with_metadata(
                "/oauth2/device/code",
                &[
                    ("client_id", client),
                    ("scope", SCOPES),
                    ("referrer", REFERRER),
                ],
                None,
                &[
                    ("x-grok-client-version", VERSION),
                    ("x-grok-client-surface", self.surface),
                ],
                started + REQUEST_TIMEOUT,
            )
            .await?;
        if status == 404 {
            return Err(Error::DeviceUnavailable);
        }
        if status != 200 {
            return Err(failure(status, &body));
        }
        let wire: DeviceWire = serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
        let interval = wire.interval.unwrap_or(5).max(1);
        if wire.expires_in == 0
            || interval > 300
            || wire.user_code.is_empty()
            || wire.user_code.len() > 64
            || !wire
                .user_code
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(Error::Protocol);
        }
        let deadline = started + Duration::from_secs(wire.expires_in.min(LOGIN_TIMEOUT.as_secs()));
        Ok(DeviceGrant {
            prompt: DevicePrompt {
                verification_uri: verification_uri(&wire.verification_uri)?,
                user_code: wire.user_code,
            },
            device_code: Secret::new(wire.device_code)?,
            interval: Duration::from_secs(interval),
            deadline,
        })
    }
    pub async fn poll(&self, client: &str, mut device: DeviceGrant) -> Result<Credential, Error> {
        loop {
            super::oauth::wait_for_poll(device.interval, device.deadline).await?;
            let (status, body) = self
                .http
                .request_with_metadata(
                    "/oauth2/token",
                    &[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                        ("client_id", client),
                        ("device_code", device.device_code.expose()),
                    ],
                    None,
                    &[
                        ("x-grok-client-version", VERSION),
                        ("x-grok-client-surface", self.surface),
                    ],
                    device.deadline,
                )
                .await
                .map_err(|error| {
                    if error == Error::Timeout && Instant::now() >= device.deadline {
                        Error::Expired
                    } else {
                        error
                    }
                })?;
            if status == 200 {
                return parse_token(&body, None, SCOPES);
            }
            if status == 400 {
                let error: ErrorWire =
                    serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
                match error.error.as_str() {
                    "authorization_pending" => continue,
                    "slow_down" => {
                        device.interval = next_interval(device.interval)?;
                        continue;
                    }
                    "access_denied" | "authorization_denied" => return Err(Error::Denied),
                    "expired_token" => return Err(Error::Expired),
                    _ => {}
                }
            }
            return Err(failure(status, &body));
        }
    }
    pub async fn refresh(
        &self,
        client: &str,
        refresh: &str,
        scopes: &str,
    ) -> Result<Credential, Error> {
        let (status, body) = self
            .http
            .request(
                "/oauth2/token",
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", client),
                    ("refresh_token", refresh),
                ],
                None,
                Instant::now() + REQUEST_TIMEOUT,
            )
            .await?;
        if status != 200 {
            return Err(failure(status, &body));
        }
        parse_token(&body, Some(refresh), scopes)
    }
    pub async fn identity(
        &self,
        credential: &Credential,
        deadline: Instant,
    ) -> Result<String, Error> {
        // Device flow has no browser nonce. Authenticate UserInfo over fixed TLS
        // instead of treating a decoded ID-token payload as identity proof.
        let (status, body) = self
            .http
            .request(
                "/oauth2/userinfo",
                &[],
                Some(credential.access.expose()),
                deadline,
            )
            .await?;
        if status != 200 {
            return Err(failure(status, &body));
        }
        let identity: IdentityWire = serde_json::from_slice(&body).map_err(|_| Error::Protocol)?;
        if identity.sub.is_empty()
            || identity.sub.len() > 256
            || identity.sub.chars().any(char::is_control)
        {
            return Err(Error::Protocol);
        }
        Ok(identity.sub)
    }
    pub async fn revoke(&self, client: &str, refresh: &str) -> Result<(), Error> {
        let (status, _) = self
            .http
            .request(
                "/oauth2/revoke",
                &[
                    ("token", refresh),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", client),
                ],
                None,
                Instant::now() + REQUEST_TIMEOUT,
            )
            .await?;
        if status == 200 {
            Ok(())
        } else {
            Err(Error::Network)
        }
    }
}
pub(super) fn verification_uri(raw: &str) -> Result<String, Error> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return Err(Error::Protocol);
    }
    let url = url::Url::parse(raw).map_err(|_| Error::Protocol)?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("auth.x.ai" | "accounts.x.ai"))
        || url.port().is_some_and(|port| port != 443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Protocol);
    }
    // Deliberately use the base URI, not verification_uri_complete with opaque query data.
    Ok(url.to_string())
}
pub(super) fn next_interval(interval: Duration) -> Result<Duration, Error> {
    interval
        .checked_add(Duration::from_secs(5))
        .filter(|next| next.as_secs() <= 900)
        .ok_or(Error::Expired)
}
fn failure(status: u16, body: &[u8]) -> Error {
    if status == 400
        && serde_json::from_slice::<ErrorWire>(body).is_ok_and(|wire| wire.error == "invalid_grant")
    {
        Error::InvalidGrant
    } else if status == 401 || status == 403 {
        Error::Denied
    } else if status == 400 || status == 404 {
        Error::Protocol
    } else {
        Error::Network
    }
}
fn parse_token(
    body: &[u8],
    previous: Option<&str>,
    previous_scopes: &str,
) -> Result<Credential, Error> {
    let wire: TokenWire = serde_json::from_slice(body).map_err(|_| Error::Protocol)?;
    if !wire.token_type.eq_ignore_ascii_case("bearer") || wire.expires_in == 0 {
        return Err(Error::Protocol);
    }
    let scopes = wire.scope.unwrap_or_else(|| previous_scopes.into());
    validate_scopes(&scopes)?;
    let refresh = wire
        .refresh_token
        .or_else(|| previous.map(str::to_owned))
        .ok_or(Error::Protocol)?;
    Ok(Credential {
        access: Secret::new(wire.access_token)?,
        refresh: Some(Secret::new(refresh)?),
        expires_at: Some(now()?.checked_add(wire.expires_in).ok_or(Error::Protocol)?),
        id_token: None,
        scopes: Some(scopes),
    })
}
pub(super) fn verify_identity(
    token: &str,
    keys: &serde_json::Value,
    client: &str,
    nonce: &str,
) -> Result<VerifiedIdentity, Error> {
    let claims = super::jwt::verify(
        token,
        keys,
        super::jwt::Policy {
            issuer: ORIGIN,
            client,
            algorithm: super::jwt::Algorithm::Es256,
            nonce: Some(nonce),
        },
    )?;
    Ok(VerifiedIdentity {
        subject: claims.subject,
        scope: None,
    })
}
impl super::AuthDriver for GrokAuthority {
    fn profile(&self) -> &'static str {
        "grok"
    }
    fn login_client(&self, options: &super::LoginOptions) -> Result<String, Error> {
        if options.consent
            || (options.method != super::LoginMethod::Browser && options.callback_port.is_some())
        {
            return Err(Error::InvalidInput);
        }
        let client = options.client_id.as_deref().unwrap_or(CLIENT_ID);
        super::model::valid_client(client)?;
        Ok(client.into())
    }
    fn validate(
        &self,
        client: &str,
        identity: Option<&VerifiedIdentity>,
        credential: Option<&Credential>,
    ) -> Result<(), Error> {
        super::model::valid_client(client)?;
        if identity.is_some_and(|i| i.scope.is_some()) {
            return Err(Error::Protocol);
        }
        if let Some(c) = credential {
            if c.refresh.is_none() || c.id_token.is_some() || c.expires_at.is_none() {
                return Err(Error::Protocol);
            }
            validate_scopes(c.scopes.as_deref().ok_or(Error::Permission)?)?;
        }
        Ok(())
    }
    fn login<'a>(
        &'a self,
        client: &'a str,
        options: &'a super::LoginOptions,
        notify: super::LoginObserver<'a>,
    ) -> super::DriverFuture<'a, super::Grant> {
        Box::pin(async move {
            let (credential, identity) = match options.method {
                super::LoginMethod::Device => {
                    let grant = self.device(client).await?;
                    notify(&super::LoginPrompt::Device(grant.prompt.clone()));
                    self.complete_device(client, grant).await?
                }
                super::LoginMethod::Browser => {
                    let grant = self
                        .browser(client, options.callback_port.unwrap_or(0))
                        .await?;
                    notify(&super::LoginPrompt::Browser(grant.prompt.clone()));
                    self.complete_browser(grant).await?
                }
            };
            Ok(super::Grant {
                identity,
                credential,
            })
        })
    }
    fn can_refresh(&self) -> bool {
        true
    }
    fn refresh<'a>(
        &'a self,
        client: &'a str,
        _identity: &'a VerifiedIdentity,
        previous: &'a Credential,
    ) -> super::DriverFuture<'a, super::Grant> {
        Box::pin(async move {
            let credential = self
                .refresh(
                    client,
                    previous
                        .refresh
                        .as_ref()
                        .ok_or(Error::LoginRequired)?
                        .expose(),
                    previous.scopes.as_deref().ok_or(Error::Permission)?,
                )
                .await?;
            let subject = self
                .identity(&credential, Instant::now() + REQUEST_TIMEOUT)
                .await?;
            Ok(super::Grant {
                identity: VerifiedIdentity {
                    subject,
                    scope: None,
                },
                credential,
            })
        })
    }
    fn revoke<'a>(
        &'a self,
        client: &'a str,
        credential: &'a Credential,
    ) -> super::DriverFuture<'a, ()> {
        Box::pin(async move {
            self.revoke(
                client,
                credential
                    .refresh
                    .as_ref()
                    .ok_or(Error::LoginRequired)?
                    .expose(),
            )
            .await
        })
    }
}
pub(super) fn validate_scopes(scopes: &str) -> Result<(), Error> {
    if scopes.len() > 1024 || !scopes.bytes().all(|c| (0x20..=0x7e).contains(&c)) {
        return Err(Error::Protocol);
    }
    let values: BTreeSet<_> = scopes.split(' ').collect();
    if ["openid", "offline_access", "grok-cli:access", "api:access"]
        .iter()
        .any(|s| !values.contains(s))
    {
        return Err(Error::Permission);
    }
    Ok(())
}
