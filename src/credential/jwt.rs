//! Fixed-profile OIDC policy over standard JOSE/JWK verification; no key discovery.
use super::{CredentialError as Error, model::now};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use jsonwebtoken::{
    DecodingKey, Validation,
    jwk::{AlgorithmParameters, EllipticCurve, JwkSet, KeyOperations, PublicKeyUse},
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

#[derive(Clone, Copy)]
pub(super) enum Algorithm {
    Rs256,
    Es256,
}
impl Algorithm {
    fn name(self) -> &'static str {
        match self {
            Self::Rs256 => "RS256",
            Self::Es256 => "ES256",
        }
    }
    fn library(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::Rs256 => jsonwebtoken::Algorithm::RS256,
            Self::Es256 => jsonwebtoken::Algorithm::ES256,
        }
    }
}
pub(super) struct Policy<'a> {
    pub issuer: &'a str,
    pub client: &'a str,
    pub algorithm: Algorithm,
    pub nonce: Option<&'a str>,
}
pub(super) struct VerifiedClaims {
    pub subject: String,
}
#[derive(Deserialize)]
struct PayloadEncoding {
    b64: Option<bool>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    sub: String,
    exp: u64,
    azp: Option<String>,
    nbf: Option<u64>,
    iat: Option<u64>,
    nonce: Option<String>,
}
fn parts(jwt: &str) -> Result<(&str, &str, &str), Error> {
    if jwt.len() > 16384 {
        return Err(Error::Protocol);
    }
    let mut parts = jwt.split('.');
    let result = (
        parts.next().ok_or(Error::Protocol)?,
        parts.next().ok_or(Error::Protocol)?,
        parts.next().ok_or(Error::Protocol)?,
    );
    if parts.next().is_some() || result.0.is_empty() || result.1.is_empty() || result.2.is_empty() {
        return Err(Error::Protocol);
    }
    Ok(result)
}
pub(super) fn verify(
    jwt: &str,
    keys: &serde_json::Value,
    policy: Policy<'_>,
) -> Result<VerifiedClaims, Error> {
    let (raw_header, _, signature) = parts(jwt)?;
    // Keep b64 typed (including duplicate rejection); it is an untyped extra in
    // the library Header. Neither inline JWK nor header URLs select authority.
    let encoding: PayloadEncoding =
        serde_json::from_slice(&decode(raw_header)?).map_err(|_| Error::Protocol)?;
    let header = jsonwebtoken::decode_header(jwt).map_err(|_| Error::Protocol)?;
    let kid = header
        .kid
        .as_deref()
        .filter(|kid| !kid.is_empty() && kid.len() <= 256)
        .ok_or(Error::Protocol)?;
    if header.alg != policy.algorithm.library()
        || header.crit.is_some_and(|v| !v.is_empty())
        || encoding.b64 == Some(false)
    {
        return Err(Error::Protocol);
    }
    if keys
        .get("keys")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|keys| keys.len() > 16)
    {
        return Err(Error::Protocol);
    }
    let keys: JwkSet = serde_json::from_value(keys.clone()).map_err(|_| Error::Protocol)?;
    let mut matching = keys
        .keys
        .iter()
        .filter(|key| key.common.key_id.as_deref() == Some(kid));
    let key = matching.next().ok_or(Error::Protocol)?;
    if matching.next().is_some()
        || key
            .common
            .key_algorithm
            .is_some_and(|alg| alg.to_string() != policy.algorithm.name())
        || key
            .common
            .public_key_use
            .as_ref()
            .is_some_and(|usage| *usage != PublicKeyUse::Signature)
        || key
            .common
            .key_operations
            .as_ref()
            .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
    {
        return Err(Error::Protocol);
    }
    // Profile resource/curve constraints precede the library's key construction.
    match (policy.algorithm, &key.algorithm) {
        (Algorithm::Rs256, AlgorithmParameters::RSA(params)) => {
            let n = decode(&params.n)?;
            let e = decode(&params.e)?;
            if !(256..=1024).contains(&n.len()) || e.is_empty() || e.len() > 8 {
                return Err(Error::Protocol);
            }
        }
        (Algorithm::Es256, AlgorithmParameters::EllipticCurve(params)) => {
            if params.curve != EllipticCurve::P256
                || decode(&params.x)?.len() != 32
                || decode(&params.y)?.len() != 32
                || decode(signature)?.len() != 64
            {
                return Err(Error::Protocol);
            }
        }
        _ => return Err(Error::Protocol),
    }
    let key = DecodingKey::from_jwk(key).map_err(|_| Error::Protocol)?;
    let mut validation = Validation::new(policy.algorithm.library());
    validation.set_required_spec_claims(&["iss", "aud", "sub", "exp"]);
    validation.set_issuer(&[policy.issuer]);
    validation.set_audience(&[policy.client]);
    validation.leeway = 0;
    // Our nbf/iat allowance is 30s, while exp has no grace. The library's single
    // leeway cannot express that distinction. Additional OIDC checks stay below.
    // https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html
    validation.validate_nbf = false;
    let claims = jsonwebtoken::decode::<Claims>(jwt, &key, &validation)
        .map_err(|_| Error::Protocol)?
        .claims;
    validate_claims(&claims, &policy, now()?)?;
    Ok(VerifiedClaims {
        subject: claims.sub,
    })
}
fn validate_claims(claims: &Claims, policy: &Policy<'_>, timestamp: u64) -> Result<(), Error> {
    let valid_audience = match &claims.aud {
        Audience::One(value) => value == policy.client,
        Audience::Many(values) => {
            values.len() <= 16
                && values.iter().any(|v| v == policy.client)
                && (values.len() == 1 || claims.azp.as_deref() == Some(policy.client))
        }
    };
    if claims.iss != policy.issuer
        || !valid_audience
        || claims.exp <= timestamp
        || claims.azp.as_deref().is_some_and(|v| v != policy.client)
        || claims.nbf.is_some_and(|v| v > timestamp.saturating_add(30))
        || claims.iat.is_some_and(|v| v > timestamp.saturating_add(30))
        || claims.sub.is_empty()
        || claims.sub.len() > 256
        || claims.sub.chars().any(char::is_control)
    {
        return Err(Error::Protocol);
    }
    if let Some(nonce) = policy.nonce {
        let actual = claims.nonce.as_deref().ok_or(Error::Protocol)?;
        if nonce.is_empty() || !bool::from(actual.as_bytes().ct_eq(nonce.as_bytes())) {
            return Err(Error::Protocol);
        }
    }
    Ok(())
}
fn decode(value: &str) -> Result<super::SecretBytes, Error> {
    B64.decode(value)
        .map(super::SecretBytes::new)
        .map_err(|_| Error::Protocol)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expiry_has_no_grace_while_future_nbf_and_iat_have_exactly_thirty_seconds() {
        let policy = Policy {
            issuer: "issuer",
            client: "client",
            algorithm: Algorithm::Rs256,
            nonce: None,
        };
        let mut claims: Claims = serde_json::from_value(serde_json::json!({"iss":"issuer","aud":"client","sub":"subject","exp":101,"nbf":130,"iat":130})).unwrap();
        assert!(validate_claims(&claims, &policy, 100).is_ok());
        claims.exp = 100;
        assert!(validate_claims(&claims, &policy, 100).is_err());
        claims.exp = 101;
        claims.nbf = Some(131);
        assert!(validate_claims(&claims, &policy, 100).is_err());
        claims.nbf = Some(130);
        claims.iat = Some(131);
        assert!(validate_claims(&claims, &policy, 100).is_err());
    }
}
