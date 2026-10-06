//! Closed execution metadata from trusted authorization, never inbound headers.
use crate::{
    credential::{AccessGrant, siwc},
    provider::AuthScheme,
};
pub(crate) fn headers(auth: AuthScheme, grant: &AccessGrant) -> Result<Vec<(String, String)>, ()> {
    match auth {
        AuthScheme::OAuthBearer("siwc") if grant.identity.scope.is_none() => {
            Ok(vec![("user-agent".into(), siwc::USER_AGENT.into())])
        }
        AuthScheme::OAuthBearer("grok") if grant.identity.scope.is_none() => Ok(vec![]),
        _ => Err(()),
    }
}
