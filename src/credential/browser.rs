//! One owned OIDC browser transaction; product parameters and identity stay in drivers.
use super::{
    BrowserPrompt, CredentialError as Error, Secret,
    callback::Callback,
    http::AuthHttp,
    oauth::{self, CodeExchange},
};
use oauth2::{AuthUrl, ClientId, CsrfToken, RedirectUrl, Scope, basic::BasicClient};
use std::{collections::BTreeSet, time::Duration};
use tokio::time::Instant;

pub(super) struct BrowserProfile<'a> {
    pub authorize: &'static str,
    pub client: &'a str,
    pub scopes: &'static str,
    pub callback_path: &'static str,
    pub extra: &'a [(&'a str, &'a str)],
    pub timeout: Duration,
}
pub(super) struct BrowserGrant {
    pub prompt: BrowserPrompt,
    pub deadline: Instant,
    callback: Callback,
    verifier: Secret,
    state: Secret,
    nonce: Secret,
    client: String,
}
pub(super) struct BrowserResponse {
    pub status: u16,
    pub body: super::SecretBytes,
    pub nonce: Secret,
    pub client: String,
}
impl BrowserGrant {
    pub async fn exchange_registered(
        mut self,
        http: &AuthHttp,
        token_path: &str,
        resource: &str,
        bootstrap_client: &str,
        registration: super::RegistrationContext<'_>,
    ) -> Result<BrowserResponse, Error> {
        let callback = self
            .callback
            .wait_registration(self.state.expose(), self.deadline)
            .await?;
        drop(self.callback);
        let client = match callback.client_id {
            Some(client) if client == bootstrap_client => return Err(Error::Protocol),
            Some(client) if self.client == bootstrap_client || client == self.client => client,
            None if self.client != bootstrap_client => self.client,
            _ => return Err(Error::IdentityMismatch),
        };
        // Retain the issued registration before code consumption, including invalid_grant.
        (registration.retain_client)(&client)?;
        let (status, body) = http
            .request(
                token_path,
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", &client),
                    ("code", callback.code.expose()),
                    ("redirect_uri", &self.prompt.redirect_uri),
                    ("code_verifier", self.verifier.expose()),
                    ("resource", resource),
                ],
                None,
                self.deadline,
            )
            .await?;
        Ok(BrowserResponse {
            status,
            body,
            nonce: self.nonce,
            client,
        })
    }
    pub async fn begin(profile: BrowserProfile<'_>, port: u16) -> Result<Self, Error> {
        let started = Instant::now();
        let url = url::Url::parse(profile.authorize).map_err(|_| Error::Protocol)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || profile.timeout.is_zero()
            || profile.timeout > Duration::from_secs(900)
        {
            return Err(Error::Protocol);
        }
        let mut names: BTreeSet<&str> = [
            "response_type",
            "client_id",
            "scope",
            "redirect_uri",
            "code_challenge_method",
            "code_challenge",
            "state",
            "nonce",
        ]
        .into_iter()
        .collect();
        for (name, _) in profile.extra {
            if !names.insert(name) || names.len() > 16 {
                return Err(Error::Protocol);
            }
        }
        let callback = Callback::bind(port, profile.callback_path).await?;
        let redirect_uri = callback.redirect_uri();
        let verifier = oauth::random_token()?;
        let state = oauth::random_token()?;
        let nonce = oauth::random_token()?;
        let client = BasicClient::new(ClientId::new(profile.client.into()))
            .set_auth_uri(AuthUrl::from_url(url))
            .set_redirect_uri(RedirectUrl::new(redirect_uri.clone()).map_err(|_| Error::Protocol)?);
        let mut authorization = client
            .authorize_url(|| CsrfToken::new(state.expose().into()))
            .add_scopes(
                profile
                    .scopes
                    .split_ascii_whitespace()
                    .map(|s| Scope::new(s.into())),
            )
            .set_pkce_challenge(oauth::challenge(verifier.expose())?)
            .add_extra_param("nonce", nonce.expose());
        for (name, value) in profile.extra {
            authorization = authorization.add_extra_param(*name, *value);
        }
        let (url, _) = authorization.url();
        Ok(Self {
            prompt: BrowserPrompt {
                authorization_url: url.to_string(),
                redirect_uri,
            },
            deadline: started + profile.timeout,
            callback,
            verifier,
            state,
            nonce,
            client: profile.client.into(),
        })
    }
    pub async fn exchange(
        mut self,
        http: &AuthHttp,
        token_path: &str,
        metadata: &[(&str, &str)],
    ) -> Result<BrowserResponse, Error> {
        let code = self
            .callback
            .wait(self.state.expose(), self.deadline)
            .await?;
        // The listener and accepted connections are gone before authority I/O.
        // Receiving a code does not acknowledge identity or local publication.
        drop(self.callback);
        let (status, body) = oauth::exchange_code(
            http,
            token_path,
            CodeExchange {
                client: &self.client,
                code: code.expose(),
                redirect: &self.prompt.redirect_uri,
                verifier: self.verifier.expose(),
            },
            metadata,
            self.deadline,
        )
        .await?;
        Ok(BrowserResponse {
            status,
            body,
            nonce: self.nonce,
            client: self.client,
        })
    }
}
