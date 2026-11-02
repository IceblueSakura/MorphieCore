use super::*;
use serde_json::json;

fn configuration() -> serde_json::Value {
    json!({"version":1,"providers":{"opencode-go":{"accounts":[
        {"name":"primary","enabled":true,"credential":{"kind":"api_key","alias":"one"},"base_url":"https://relay.invalid/custom/v1/"},
        {"name":"standby","enabled":false,"credential":{"kind":"api_key","alias":"two"}}
    ]}}})
}
fn parse(value: serde_json::Value) -> Result<Accounts, AccountError> {
    let accounts: Accounts =
        serde_json::from_value(value).map_err(|_| AccountError::Configuration)?;
    accounts.validate()?;
    Ok(accounts)
}
#[test]
fn account_roots_append_declared_suffixes_without_guessing_prefixes() {
    let accounts = parse(configuration()).unwrap();
    let (provider, account) = accounts.account("opencode-go", "primary").unwrap();
    let effective = account.target(provider).unwrap();
    assert_eq!(effective.origin.as_str(), "https://relay.invalid");
    for (operation, path) in [
        (
            crate::provider::ProviderOperation::Models,
            "/custom/v1/models",
        ),
        (
            crate::provider::ProviderOperation::Responses,
            "/custom/v1/responses",
        ),
        (
            crate::provider::ProviderOperation::Messages,
            "/custom/v1/messages",
        ),
    ] {
        assert_eq!(effective.entry(operation).unwrap().path.as_str(), path);
    }
    assert_eq!(
        effective
            .entry(crate::provider::ProviderOperation::Messages)
            .unwrap()
            .auth,
        crate::provider::AuthScheme::ApiKeyHeader("x-api-key")
    );
    let (_, inherited) = accounts.account("opencode-go", "standby").unwrap();
    assert_eq!(inherited.target(provider).unwrap(), *provider);
}
#[test]
fn account_declarations_reject_ambiguous_identity_and_untrusted_targets() {
    let mut value = configuration();
    value["providers"]["opencode-go"]["accounts"][1]["name"] =
        value["providers"]["opencode-go"]["accounts"][0]["name"].clone();
    assert!(parse(value).is_err(), "duplicate account name");
    for root in [
        "http://relay.invalid/v1",
        "https://user@relay.invalid/v1",
        "https://relay.invalid/v1?key=x",
        "https://relay.invalid/v1#fragment",
        "https://relay.invalid/v1/../else",
        "https://relay.invalid/v1/%2e%2e",
        "https://relay.invalid//v1",
    ] {
        let mut value = configuration();
        value["providers"]["opencode-go"]["accounts"][0]["base_url"] = json!(root);
        assert!(parse(value).is_err(), "{root}");
    }
    let mut value = configuration();
    value["providers"]["opencode-go"]["accounts"][0]["credential"] =
        json!({"kind":"oauth","profile":"openai","alias":"one"});
    assert!(parse(value).is_err());
    assert!(parse(json!({"version":1,"providers":{"chatgpt":{"accounts":[{"name":"one","enabled":true,
        "credential":{"kind":"oauth","profile":"openai","alias":"one"},"base_url":"https://api.openai.com/v1"}]}}})).is_err());
    assert!(
        parse(
            json!({"version":1,"providers":{"openai":{"accounts":[{"name":"one","enabled":true,
        "credential":{"kind":"oauth","profile":"openai","alias":"one"}}]}}})
        )
        .is_err()
    );
}
#[test]
fn account_binding_keeps_order_enablement_and_stable_names_outside_secret_storage() {
    let directory = crate::test_files::Directory::new();
    let manager = CredentialManager::new(&directory.path, vec![]).unwrap();
    for (alias, material) in [("one", "synthetic-one"), ("two", "synthetic-two")] {
        manager
            .add_api_key(
                "opencode-go",
                alias,
                crate::credential::Secret::new(material.into()).unwrap(),
            )
            .unwrap();
    }
    let source = parse(configuration()).unwrap();
    let bound = source.bind(&manager).unwrap();
    let accounts = bound.provider("opencode-go");
    assert_eq!(
        accounts
            .iter()
            .map(|a| (a.name.as_str(), a.enabled))
            .collect::<Vec<_>>(),
        vec![("primary", true), ("standby", false)]
    );
    let first = accounts[0].binding_id();
    let mut reversed = configuration();
    reversed["providers"]["opencode-go"]["accounts"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        parse(reversed)
            .unwrap()
            .bind(&manager)
            .unwrap()
            .account("opencode-go", "primary")
            .unwrap()
            .binding_id(),
        first
    );
    let AccountCredential::ApiKey(key) = &accounts[0].credential else {
        panic!("wrong credential kind")
    };
    assert_eq!(key.borrow().unwrap().expose(), "synthetic-one");
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path.join("opencode-go.json")).unwrap())
            .unwrap();
    assert_eq!(document["version"], 1);
    assert!(document.get("pools").is_none());
    assert!(document.get("accounts").is_none());
    let empty = crate::test_files::Directory::new();
    let unprovisioned = CredentialManager::new(&empty.path, vec![]).unwrap();
    assert!(
        parse(configuration())
            .unwrap()
            .bind(&unprovisioned)
            .is_err()
    );
}
