//! Validated, compile-time Provider metadata shared by topology and manual diagnostics.
//! Edit providers.json and rebuild; no runtime files, overrides or hot reload.
use crate::{
    provider::{
        AuthScheme, EndpointPath, ProviderDefinition, ProviderEndpoint, ProviderError, ProviderId,
        ProviderOperation, TrustedOrigin,
    },
    semantic::value::{JsonLimits, parse_json},
};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Authentication {
    Bearer,
    XApiKey,
    XGoogApiKey,
    #[serde(rename = "oauth")]
    OAuth {
        profile: Profile,
    },
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Profile {
    Openai,
    Grok,
}
impl Authentication {
    fn scheme(self) -> AuthScheme {
        match self {
            Self::Bearer => AuthScheme::Bearer,
            Self::XApiKey => AuthScheme::ApiKeyHeader("x-api-key"),
            Self::XGoogApiKey => AuthScheme::ApiKeyHeader("x-goog-api-key"),
            Self::OAuth {
                profile: Profile::Openai,
            } => AuthScheme::OAuthBearer("openai"),
            Self::OAuth {
                profile: Profile::Grok,
            } => AuthScheme::OAuthBearer("grok"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    base_url: String,
    auth: Authentication,
    endpoints: BTreeMap<ProviderOperation, Entry>,
    sources: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    suffix: String,
    auth: Option<Authentication>,
}
fn parse(bytes: &[u8]) -> Result<BTreeMap<String, ProviderDefinition>, ProviderError> {
    let invalid = || ProviderError::InvalidCatalog;
    let value = parse_json(bytes, JsonLimits::ENVELOPE).map_err(|_| invalid())?;
    let raw: BTreeMap<String, Definition> = serde_json::from_value(value).map_err(|_| invalid())?;
    if raw.is_empty() || raw.len() > 64 {
        return Err(invalid());
    }
    raw.into_iter()
        .map(|(name, definition)| {
            let auth = definition.auth.scheme();
            if definition.endpoints.is_empty()
                || definition.sources.is_empty()
                || definition
                    .sources
                    .iter()
                    .any(|s| !s.starts_with("https://") || s.chars().any(char::is_control))
            {
                return Err(invalid());
            }
            let endpoints = definition
                .endpoints
                .into_iter()
                .map(|(operation, entry)| {
                    let override_auth = entry.auth.map(Authentication::scheme);
                    if override_auth.is_some_and(|a| a.kind() != auth.kind()) {
                        return Err(invalid());
                    }
                    Ok((
                        operation,
                        ProviderEndpoint {
                            path: EndpointPath::new(&entry.path)?,
                            suffix: EndpointPath::new(&entry.suffix)?,
                            auth: override_auth,
                        },
                    ))
                })
                .collect::<Result<_, ProviderError>>()?;
            let provider = ProviderDefinition {
                id: ProviderId::new(&name)?,
                origin: TrustedOrigin::parse(&definition.base_url)?,
                endpoints,
                auth,
            };
            Ok((name, provider))
        })
        .collect()
}
fn registry() -> &'static BTreeMap<String, ProviderDefinition> {
    static CATALOG: OnceLock<BTreeMap<String, ProviderDefinition>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        parse(include_bytes!("providers.json")).expect("valid embedded provider catalog")
    })
}
pub fn get(id: &str) -> Option<&'static ProviderDefinition> {
    registry().get(id)
}
pub fn all() -> Vec<ProviderDefinition> {
    registry().values().cloned().collect()
}

// Named constructors keep model declarations concise; all facts live in the JSON source.
macro_rules! providers {
    ($($function:ident => $id:literal),* $(,)?) => { $(
        pub fn $function() -> ProviderDefinition { get($id).expect("catalog provider").clone() }
    )* };
}
providers! {
    openai => "openai", chatgpt => "chatgpt", grok => "grok", deepseek => "deepseek", xiaomi => "xiaomi",
    modelbest => "modelbest", openrouter => "openrouter", opencode_go => "opencode-go",
    nvidia => "nvidia", aliyun_dashscope_cn => "aliyun-dashscope-cn",
    aliyun_tokenplan_cn => "aliyun-tokenplan-cn", zhipu => "zhipu", google => "google",
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_schema_rejects_duplicate_fields_and_unsafe_or_unknown_declarations() {
        assert!(parse(br#"{"deepseek":{},"deepseek":{}}"#).is_err());
        let original: serde_json::Value =
            serde_json::from_slice(include_bytes!("providers.json")).unwrap();
        for (pointer, replacement) in [
            (
                "/deepseek/base_url",
                serde_json::json!("https://key@host.invalid"),
            ),
            (
                "/deepseek/endpoints/models/path",
                serde_json::json!("/models?key=secret"),
            ),
            (
                "/deepseek/auth",
                serde_json::json!({"kind":"oauth","profile":"other"}),
            ),
            (
                "/deepseek/endpoints/models/auth",
                serde_json::json!({"kind":"oauth","profile":"openai"}),
            ),
        ] {
            let mut value = original.clone();
            if let Some(slot) = value.pointer_mut(pointer) {
                *slot = replacement;
            } else {
                value["deepseek"]["endpoints"]["models"]["auth"] = replacement;
            }
            assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
}
