//! Closed execution metadata from trusted authorization, never inbound headers.
use crate::{
    credential::{AccessGrant, CredentialError, drivers::siwc},
    provider::AuthScheme,
};
pub fn headers(
    auth: AuthScheme,
    grant: &AccessGrant,
) -> Result<Vec<(String, String)>, CredentialError> {
    match auth {
        AuthScheme::OAuthBearer("openai") if grant.identity.scope.is_none() => {
            Ok(vec![("user-agent".into(), siwc::USER_AGENT.into())])
        }
        AuthScheme::OAuthBearer("grok") if grant.identity.scope.is_none() => Ok(vec![]),
        _ => Err(CredentialError::Unsupported),
    }
}
