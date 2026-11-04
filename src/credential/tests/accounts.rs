//! Shared account declarations bind managed identities without inference or refresh.
use crate::{
    account::{
        Account as Declaration, AccountCredential, Accounts, CredentialRef, ProviderAccounts,
    },
    credential::{
        model::{Account, AccountState},
        tests::support as test_support,
        *,
    },
};

#[test]
fn named_oauth_accounts_keep_distinct_principals_and_network_free_borrowing() {
    let dir = test_support::Directory::new();
    let manager = CredentialManager::new(&dir.path, builtin_drivers(None).unwrap()).unwrap();
    for alias in ["first", "second"] {
        let mut account = Account::new("grok", alias, "synthetic-client").unwrap();
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
                expires_at: Some(if alias == "first" { 1 } else { 4000000000 }),
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
    let source = Accounts {
        version: 1,
        providers: [(
            "grok".into(),
            ProviderAccounts {
                recovery: Default::default(),
                accounts: ["first", "second"]
                    .into_iter()
                    .map(|alias| Declaration {
                        name: alias.into(),
                        enabled: true,
                        base_url: None,
                        credential: CredentialRef {
                            alias: alias.into(),
                        },
                    })
                    .collect(),
            },
        )]
        .into(),
    };
    let bound = source.bind(&manager).unwrap();
    let accounts = bound.provider("grok");
    let AccountCredential::OAuth(first) = &accounts[0].credential else {
        panic!("OAuth")
    };
    let AccountCredential::OAuth(second) = &accounts[1].credential else {
        panic!("OAuth")
    };
    assert_eq!(first.borrow().unwrap_err(), CredentialError::Expired);
    let grant = second.borrow().unwrap();
    assert_eq!(grant.access.expose(), "synthetic-second");
    assert!(
        crate::provider::subscription::headers(
            crate::provider::AuthScheme::OAuthBearer("grok"),
            &grant
        )
        .unwrap()
        .is_empty()
    );
    let duplicate = Accounts {
        version: 1,
        providers: [(
            "grok".into(),
            ProviderAccounts {
                recovery: Default::default(),
                accounts: vec![
                    Declaration {
                        name: "one".into(),
                        enabled: false,
                        base_url: None,
                        credential: CredentialRef {
                            alias: "first".into(),
                        },
                    },
                    Declaration {
                        name: "two".into(),
                        enabled: true,
                        base_url: None,
                        credential: CredentialRef {
                            alias: "second".into(),
                        },
                    },
                ],
            },
        )]
        .into(),
    };
    {
        let tx = manager.store.transaction().unwrap();
        let mut other = tx.load("grok", "second").unwrap().account.unwrap();
        other.identity.as_mut().unwrap().subject = "subject-first".into();
        tx.publish(&other).unwrap();
    }
    assert!(
        duplicate.bind(&manager).is_err(),
        "disabled accounts still reserve subjects"
    );
}
