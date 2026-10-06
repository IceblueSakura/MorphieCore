//! Pool configuration is explicit, ordered and independent of remote credential validity.

#[test]
fn oauth_pool_borrows_distinct_principals_without_refresh_or_cross_profile_substitution() {
    use super::model::{Account, AccountState};
    let dir = test_support::Directory::new();
    let manager = CredentialManager::new(&dir.path, builtin_drivers(None).unwrap()).unwrap();
    for alias in ["first", "second"] {
        let mut account = Account::new("grok", alias, "synthetic-client");
        account.identity = Some(VerifiedIdentity {
            subject: format!("subject-{alias}"),
            scope: None,
        });
        account
            .replace_credential(Some(Credential {
                access: Secret::new(format!("synthetic-{alias}")).unwrap(),
                refresh: Some(Secret::new("synthetic-refresh".into()).unwrap()),
                id_token: None,
                scopes: Some(
                    "openid profile email offline_access grok-cli:access api:access".into(),
                ),
                expires_at: if alias == "first" {
                    Some(1)
                } else {
                    Some(4000000000)
                },
            }))
            .unwrap();
        account.state = AccountState::Active;
        account.advance().unwrap();
        manager
            .store
            .transaction()
            .unwrap()
            .publish(&account)
            .unwrap();
    }
    let from_wire: CredentialRef =
        serde_json::from_str(r#"{"kind":"oauth","profile":"grok","alias":"first"}"#).unwrap();
    assert!(matches!(from_wire, CredentialRef::OAuth { .. }));
    let config = CredentialPool {
        members: ["first", "second"]
            .into_iter()
            .map(|alias| CredentialRef::OAuth {
                profile: "grok".into(),
                alias: alias.into(),
            })
            .collect(),
        fallback: true,
        max_attempts: 2,
    };
    manager
        .set_pool("grok", "grok-oauth", 0, config.clone())
        .unwrap();
    let access = manager.bind_pool("grok", &config).unwrap();
    let PoolMember::OAuth(first) = &access.members[0] else {
        panic!("OAuth")
    };
    let PoolMember::OAuth(second) = &access.members[1] else {
        panic!("OAuth")
    };
    assert_eq!(first.borrow().unwrap_err(), CredentialError::Expired);
    let grant = second.borrow().unwrap();
    assert_eq!(grant.access.expose(), "synthetic-second");
    let headers = crate::provider::subscription::headers(
        crate::provider::AuthScheme::OAuthBearer("grok"),
        &grant,
    )
    .unwrap();
    assert!(headers.is_empty());
    assert!(
        crate::provider::subscription::headers(
            crate::provider::AuthScheme::OAuthBearer("codex"),
            &grant
        )
        .is_err()
    );
}
use super::*;
#[test]
fn provider_pool_keeps_order_and_rejects_duplicates_stale_updates_or_missing_keys() {
    let dir = test_support::Directory::new();
    let manager = CredentialManager::new(&dir.path, vec![]).unwrap();
    for name in ["first", "second"] {
        manager
            .add_api_key(
                "alpha",
                name,
                Secret::new(format!("synthetic-{name}")).unwrap(),
            )
            .unwrap();
    }
    let config = CredentialPool {
        members: vec![
            CredentialRef::ApiKey {
                alias: "second".into(),
            },
            CredentialRef::ApiKey {
                alias: "first".into(),
            },
        ],
        fallback: true,
        max_attempts: 2,
    };
    let status = manager
        .set_pool("alpha", "generation", 0, config.clone())
        .unwrap();
    assert_eq!(status.revision, 1);
    assert!(
        manager
            .set_pool("alpha", "generation", 0, config.clone())
            .is_err()
    );
    let pool = manager
        .bind_pool("alpha", &manager.pools().unwrap()[0].2.config)
        .unwrap();
    let PoolMember::ApiKey(first) = &pool.members[0] else {
        panic!("key")
    };
    assert_eq!(first.borrow().unwrap().expose(), "synthetic-second");
    let mut duplicate = config.clone();
    duplicate.members.push(duplicate.members[0].clone());
    assert!(manager.set_pool("alpha", "other", 0, duplicate).is_err());
    assert!(manager.set_pool("beta", "other", 0, config).is_err());
    manager
        .set_api_key_enabled("alpha", "second", 1, false)
        .unwrap();
    assert!(first.borrow().is_err());
    let pool = manager.bind_pool("alpha", &status.config).unwrap();
    let PoolMember::ApiKey(first) = &pool.members[0] else {
        panic!("key")
    };
    assert!(first.borrow().is_err());
}
