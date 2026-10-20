//! Pure credential redaction for opt-in capture, before any unit is queued.
use serde_json::{Value, json};

pub(super) fn authentication(name: &str) -> bool {
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    name.contains("authorization")
        || name.contains("apikey")
        || name.contains("password")
        || name.contains("secret")
        || name.contains("credential")
        || name.contains("privatekey")
        || name.contains("signingkey")
        || name.ends_with("token")
        || matches!(
            name.as_str(),
            "cookie"
                | "setcookie"
                | "key"
                | "jwt"
                | "auth"
                | "authentication"
                | "bearer"
                | "clientkey"
                | "accesskey"
                | "accountkey"
                | "wwwauthenticate"
                | "proxyauthenticate"
                | "authenticationinfo"
                | "proxyauthenticationinfo"
        )
}
pub(super) fn authentication_query(name: &str) -> bool {
    authentication(name)
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "sig" | "signature" | "x-amz-signature" | "x-goog-signature" | "code"
        )
}
fn capture_url(text: &str) -> Option<(url::Url, bool)> {
    if text.starts_with('/') || text.starts_with('?') {
        let base = url::Url::parse("https://capture.invalid/").ok()?;
        return url::Url::options()
            .base_url(Some(&base))
            .parse(text)
            .ok()
            .map(|url| (url, true));
    }
    url::Url::parse(text)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| (url, false))
}
pub(super) fn sanitize(value: &mut Value, redacted: &mut bool, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Object(values) => {
            if values
                .get("name")
                .or_else(|| values.get("key"))
                .and_then(Value::as_str)
                .is_some_and(authentication)
                && let Some(value) = values.get_mut("value")
            {
                *value = json!("[REDACTED]");
                *redacted = true;
            }
            for (name, value) in values {
                if authentication(name) {
                    *value = json!("[REDACTED]");
                    *redacted = true;
                } else if !sanitize(value, redacted, depth + 1) {
                    return false;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                if !sanitize(value, redacted, depth + 1) {
                    return false;
                }
            }
        }
        Value::String(text) => {
            if text == "[REDACTED]" {
                return true;
            }
            // Function arguments/results can contain serialized authentication objects.
            if matches!(
                text.trim_start().as_bytes().first(),
                Some(b'{' | b'[' | b'"')
            ) {
                let Ok(mut nested) = crate::protocol::json::decode(text.as_bytes()) else {
                    return false;
                };
                let mut nested_redacted = false;
                if !sanitize(&mut nested, &mut nested_redacted, depth + 1) {
                    return false;
                }
                if nested_redacted {
                    let Ok(encoded) = serde_json::to_string(&nested) else {
                        return false;
                    };
                    *text = encoded;
                    *redacted = true;
                }
            } else if let Some((mut url, relative)) = capture_url(text) {
                let mut changed = !url.username().is_empty() || url.password().is_some();
                let _ = url.set_username("");
                let _ = url.set_password(None);
                let mut query = vec![];
                for (key, value) in url.query_pairs() {
                    let mut value = Value::String(value.into_owned());
                    if authentication_query(&key) {
                        value = json!("[REDACTED]");
                        changed = true;
                    } else if !sanitize(&mut value, &mut changed, depth + 1) {
                        return false;
                    }
                    let Value::String(value) = value else {
                        return false;
                    };
                    query.push((key.into_owned(), value));
                }
                if url.fragment().is_some_and(|fragment| {
                    url::form_urlencoded::parse(fragment.as_bytes())
                        .any(|(key, _)| authentication_query(&key))
                }) {
                    url.set_fragment(None);
                    changed = true;
                }
                if changed {
                    if url.query().is_some() {
                        url.query_pairs_mut().clear().extend_pairs(query);
                    }
                    *text = if relative {
                        url[url::Position::BeforePath..].into()
                    } else {
                        url.into()
                    };
                    *redacted = true;
                }
            }
        }
        _ => {}
    }
    true
}
