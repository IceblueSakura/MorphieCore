//! Named-account operations over the shared trusted account source.
use clap::{Args, Parser, Subcommand, ValueEnum};
use morphiecore::{
    account::{Account, AccountError, Accounts, CredentialRef},
    credential::{
        AccountStatus, ApiKeyStatus, CredentialError, CredentialManager, LoginMethod, LoginOptions,
        LoginPrompt, LogoutOutcome, Secret, builtin_drivers, default_config_directory,
        read_private_file, resolve_store_directory,
    },
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
struct Options {
    /// Account declarations, outside the secret store.
    #[arg(long, global = true)]
    accounts: Option<PathBuf>,
    #[arg(long, global = true)]
    store: Option<PathBuf>,
    /// Emit one redacted JSON document, including on operation failure.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Inspect configured accounts without login, refresh or inference.
    Status {
        #[arg(long)]
        provider: Option<String>,
    },
    /// Validate declarations, trusted targets and local credential references.
    Validate,
    /// Explain configured participation without opening the credential store.
    Explain {
        #[arg(long)]
        provider: Option<String>,
    },
    Key {
        #[command(subcommand)]
        operation: KeyCommand,
    },
    Login(Login),
    Refresh(Selected),
    Logout {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revoke: bool,
    },
    /// Remove one local OAuth registration and its subject reservation; no revocation.
    Remove {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revision: u64,
    },
}
#[derive(Args)]
struct Selected {
    #[arg(long)]
    provider: String,
    #[arg(long)]
    account: String,
    #[arg(long)]
    proxy: Option<String>,
}
#[derive(Clone, Copy, ValueEnum)]
enum Method {
    Device,
    Browser,
}
#[derive(Args)]
struct Login {
    #[command(flatten)]
    selected: Selected,
    #[arg(long)]
    client_id: Option<String>,
    #[arg(long, value_enum)]
    method: Option<Method>,
    #[arg(long)]
    callback_port: Option<u16>,
    #[arg(long)]
    consent: bool,
}
#[derive(Subcommand)]
enum KeyCommand {
    Add {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        secret_file: PathBuf,
    },
    Replace {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        secret_file: PathBuf,
    },
    Enable {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revision: u64,
    },
    Disable {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revision: u64,
    },
    Remove {
        #[command(flatten)]
        selected: Selected,
        #[arg(long)]
        revision: u64,
    },
}
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Accounts(#[from] AccountError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error("operation does not match the configured credential kind")]
    Kind,
    #[error("credential store is missing; provision the configured accounts first")]
    Missing,
    #[error("local session cleared; remote revocation was NOT confirmed")]
    Revocation,
}
fn secret(path: &Path) -> Result<Secret, Error> {
    let mut bytes = read_private_file(path, 16386)?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    Ok(Secret::new(
        std::str::from_utf8(&bytes)
            .map_err(|_| CredentialError::InvalidInput)?
            .to_owned(),
    )?)
}
fn prompt(prompt: &LoginPrompt) {
    match prompt {
        LoginPrompt::Device(p) => eprintln!(
            "Open {} and enter {}. Approve only the login you initiated.",
            p.verification_uri, p.user_code
        ),
        LoginPrompt::Browser(p) => eprintln!(
            "Open this first-party authorization URL in your browser:\n{}\nCallback: {}\nCheck CLI output for final login confirmation.",
            p.authorization_url, p.redirect_uri
        ),
    }
}
fn oauth(account: &Account) -> Result<(&str, &str), Error> {
    match &account.credential {
        CredentialRef::OAuth { profile, alias } => Ok((profile, alias)),
        _ => Err(Error::Kind),
    }
}
fn api_key(account: &Account) -> Result<&str, Error> {
    match &account.credential {
        CredentialRef::ApiKey { alias } => Ok(alias),
        _ => Err(Error::Kind),
    }
}
fn key_status(provider: &str, account: &Account, status: ApiKeyStatus) -> Value {
    json!({"provider":provider,"account":account.name,"enabled":account.enabled,"kind":"api_key",
        "state":status.state,"revision":status.revision,"generation":status.generation,"recovery_required":status.recovery_required})
}
fn oauth_status(provider: &str, account: &Account, status: AccountStatus) -> Value {
    json!({"provider":provider,"account":account.name,"enabled":account.enabled,"kind":"oauth",
        "state":status.state,"access":status.access,"revision":status.revision,"generation":status.generation,
        "expires_at":status.expires_at,"renewal_pending":status.renewal_pending,"login_pending":status.login_pending,
        "plan_usage_enabled":status.plan_usage_enabled,"recovery_required":status.recovery_required})
}
async fn execute(options: Options) -> Result<Value, Error> {
    let path = match options.accounts {
        Some(path) => path,
        None => default_config_directory()?.join("accounts.json"),
    };
    let accounts = Accounts::load(&path)?;
    if let Command::Explain { provider } = &options.command {
        if let Some(provider) = provider
            && !accounts.providers.contains_key(provider)
        {
            return Err(AccountError::Provider(provider.clone()).into());
        }
        let declarations: Vec<_> = accounts.providers.iter().filter(|(id,_)| provider.as_ref().is_none_or(|p| p == *id))
            .flat_map(|(id, configured)| configured.accounts.iter().map(move |account| json!({
                "provider":id,"account":account.name,"enabled":account.enabled,
                "kind":match account.credential {CredentialRef::ApiKey{..}=>"api_key",CredentialRef::OAuth{..}=>"oauth"},
                "endpoint_override":account.base_url.is_some(),"selection":"route order at request boundary"
            }))).collect();
        return Ok(json!({"accounts":declarations}));
    }
    let root = resolve_store_directory(options.store)?;
    if matches!(options.command, Command::Status { .. } | Command::Validate) && !root.is_dir() {
        return Err(Error::Missing);
    }
    match options.command {
        Command::Explain { .. } => unreachable!("handled without credential storage"),
        Command::Validate => {
            let manager = CredentialManager::new(root, builtin_drivers(None)?)?;
            let bound = accounts.bind(&manager)?;
            Ok(json!({"valid":true,"accounts_checked":bound.all().count()}))
        }
        Command::Status { provider } => {
            if let Some(provider) = &provider
                && !accounts.providers.contains_key(provider)
            {
                return Err(AccountError::Provider(provider.clone()).into());
            }
            let manager = CredentialManager::new(root, builtin_drivers(None)?)?;
            let mut statuses = Vec::new();
            for (id, configured) in &accounts.providers {
                if provider.as_ref().is_some_and(|p| p != id) {
                    continue;
                }
                for account in &configured.accounts {
                    let status = match &account.credential {
                        CredentialRef::ApiKey { alias } => manager
                            .list_api_keys(Some(id))?
                            .into_iter()
                            .find(|s| &s.alias == alias)
                            .map(|s| key_status(id, account, s)),
                        CredentialRef::OAuth { profile, alias } => manager
                            .list(Some(profile))?
                            .into_iter()
                            .find(|s| &s.account == alias)
                            .map(|s| oauth_status(id, account, s)),
                    };
                    statuses.push(status.unwrap_or_else(||json!({
                        "provider":id,"account":account.name,"enabled":account.enabled,
                        "kind":match &account.credential {CredentialRef::ApiKey{..}=>"api_key",CredentialRef::OAuth{..}=>"oauth"},
                        "state":"unprovisioned","access":"unavailable"
                    })));
                }
            }
            Ok(json!({"accounts":statuses}))
        }
        Command::Key { operation } => {
            let selected = match &operation {
                KeyCommand::Add { selected, .. }
                | KeyCommand::Replace { selected, .. }
                | KeyCommand::Enable { selected, .. }
                | KeyCommand::Disable { selected, .. }
                | KeyCommand::Remove { selected, .. } => selected,
            };
            let (_, account) = accounts.account(&selected.provider, &selected.account)?;
            let alias = api_key(account)?;
            // Parse secret input before creating a destination store.
            let material = match &operation {
                KeyCommand::Add { secret_file, .. } | KeyCommand::Replace { secret_file, .. } => {
                    Some(secret(secret_file)?)
                }
                _ => None,
            };
            let manager = CredentialManager::new(root, vec![])?;
            let provider = selected.provider.clone();
            let status = match operation {
                KeyCommand::Add { .. } => {
                    manager.add_api_key(&provider, alias, material.ok_or(Error::Kind)?)?
                }
                KeyCommand::Replace { revision, .. } => manager.replace_api_key(
                    &provider,
                    alias,
                    revision,
                    material.ok_or(Error::Kind)?,
                )?,
                KeyCommand::Enable { revision, .. } => {
                    manager.set_api_key_enabled(&provider, alias, revision, true)?
                }
                KeyCommand::Disable { revision, .. } => {
                    manager.set_api_key_enabled(&provider, alias, revision, false)?
                }
                KeyCommand::Remove { revision, .. } => {
                    manager.remove_api_key(&provider, alias, revision)?
                }
            };
            Ok(key_status(&provider, account, status))
        }
        Command::Login(login) => {
            let (_, account) =
                accounts.account(&login.selected.provider, &login.selected.account)?;
            let (profile, alias) = oauth(account)?;
            let method = login.method.unwrap_or(if profile == "openai" {
                Method::Browser
            } else {
                Method::Device
            });
            if login.callback_port.is_some() && matches!(method, Method::Device) {
                return Err(CredentialError::InvalidInput.into());
            }
            let settings = LoginOptions {
                method: match method {
                    Method::Device => LoginMethod::Device,
                    Method::Browser => LoginMethod::Browser,
                },
                client_id: login.client_id,
                callback_port: login.callback_port,
                consent: login.consent,
            };
            let drivers = builtin_drivers(login.selected.proxy.as_deref())?;
            drivers
                .iter()
                .find(|driver| driver.profile() == profile)
                .ok_or(CredentialError::UnknownProfile)?
                .login_client(&settings)?;
            if profile == "openai" {
                eprintln!(
                    "Continue with ChatGPT — MorphieCore\nCredentials remain in protected local storage. Plan usage is only for your own authorized tasks. Manage usage: https://chatgpt.com/settings/usage"
                );
            }
            let manager = CredentialManager::new(root, drivers)?;
            let status = manager.login(profile, alias, settings, prompt).await?;
            Ok(oauth_status(&login.selected.provider, account, status))
        }
        Command::Refresh(selected) => {
            let (_, account) = accounts.account(&selected.provider, &selected.account)?;
            let (profile, alias) = oauth(account)?;
            let manager =
                CredentialManager::new(root, builtin_drivers(selected.proxy.as_deref())?)?;
            let status = manager.refresh(profile, alias).await?;
            Ok(oauth_status(&selected.provider, account, status))
        }
        Command::Logout { selected, revoke } => {
            let (_, account) = accounts.account(&selected.provider, &selected.account)?;
            let (profile, alias) = oauth(account)?;
            let manager =
                CredentialManager::new(root, builtin_drivers(selected.proxy.as_deref())?)?;
            let outcome = manager.logout(profile, alias, revoke).await?;
            if outcome == LogoutOutcome::RevocationUnconfirmed {
                return Err(Error::Revocation);
            }
            Ok(
                json!({"provider":selected.provider,"account":account.name,"outcome":match outcome {
                    LogoutOutcome::LocalOnly=>"local_only",LogoutOutcome::Revoked=>"revoked",
                    LogoutOutcome::AlreadySignedOut=>"already_signed_out",LogoutOutcome::RevocationUnconfirmed=>unreachable!(),
                }}),
            )
        }
        Command::Remove { selected, revision } => {
            let (_, account) = accounts.account(&selected.provider, &selected.account)?;
            let (profile, alias) = oauth(account)?;
            let manager =
                CredentialManager::new(root, builtin_drivers(selected.proxy.as_deref())?)?;
            Ok(oauth_status(
                &selected.provider,
                account,
                manager.remove_account(profile, alias, revision)?,
            ))
        }
    }
}
fn print_human(value: &Value) {
    if value.get("valid") == Some(&Value::Bool(true)) {
        println!(
            "Account configuration and local references are valid ({} accounts).",
            value["accounts_checked"]
        );
        return;
    }
    let rows = value
        .get("accounts")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_else(|| std::slice::from_ref(value));
    for row in rows {
        print!(
            "{}/{}:",
            row["provider"].as_str().unwrap_or(""),
            row["account"].as_str().unwrap_or("")
        );
        for field in [
            "kind",
            "enabled",
            "state",
            "access",
            "revision",
            "outcome",
            "endpoint_override",
            "selection",
            "renewal_pending",
            "login_pending",
            "recovery_required",
            "plan_usage_enabled",
        ] {
            if let Some(value) = row.get(field)
                && !value.is_null()
            {
                print!(
                    " {field}={}",
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string())
                );
            }
        }
        println!();
    }
}
#[tokio::main]
async fn main() -> ExitCode {
    let options = match Options::try_parse() {
        Ok(options) => options,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            eprintln!("Invalid credential arguments; use --help. Values are not echoed.");
            return ExitCode::FAILURE;
        }
    };
    let json = options.json;
    let result = tokio::select! {result=execute(options)=>result,_=tokio::signal::ctrl_c()=>Err(Error::Credential(CredentialError::Superseded))};
    match result {
        Ok(value) => {
            if json {
                println!("{value}");
            } else {
                print_human(&value);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                println!("{}", json!({"error":{"message":error.to_string()}}));
            } else {
                eprintln!("Credential operation failed: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
#[path = "../../tests/support/filesystem.rs"]
mod test_files;
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn status_preserves_provisioned_revisions_when_another_account_is_missing() {
        use std::io::Write;
        let directory = test_files::private_directory();
        let store = directory.path().join("credentials");
        let manager = CredentialManager::new(&store, vec![]).unwrap();
        manager
            .add_api_key(
                "openai",
                "private-locator",
                Secret::new("synthetic-material".into()).unwrap(),
            )
            .unwrap();
        let mut file = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        write!(file,"{}",json!({"version":1,"providers":{"openai":{"accounts":[
            {"name":"primary","enabled":true,"credential":{"kind":"api_key","alias":"private-locator"}},
            {"name":"backup","enabled":true,"credential":{"kind":"api_key","alias":"private-unprovisioned-locator"}}
        ]}}})).unwrap();
        let options = Options::try_parse_from([
            "auth",
            "--store",
            store.to_str().unwrap(),
            "--accounts",
            file.path().to_str().unwrap(),
            "status",
            "--provider",
            "openai",
        ])
        .unwrap();
        let status = execute(options).await.unwrap();
        assert_eq!(status["accounts"][0]["revision"], 1);
        assert_eq!(status["accounts"][1]["account"], "backup");
        assert_eq!(status["accounts"][1]["state"], "unprovisioned");
        for private in [
            "private-locator",
            "private-unprovisioned-locator",
            "synthetic-material",
        ] {
            assert!(!status.to_string().contains(private));
        }
    }
    #[test]
    fn cli_names_configured_accounts_and_rejects_credential_bypasses() {
        for args in [
            vec!["auth", "status"],
            vec!["auth", "validate", "--json"],
            vec![
                "auth",
                "login",
                "--provider",
                "chatgpt",
                "--account",
                "primary",
            ],
            vec![
                "auth",
                "key",
                "add",
                "--provider",
                "openai",
                "--account",
                "primary",
                "--secret-file",
                "private",
            ],
        ] {
            assert!(Options::try_parse_from(args).is_ok());
        }
        for args in [
            vec!["auth", "pool", "list"],
            vec!["auth", "api-key", "list"],
            vec!["auth", "grok", "login", "--account", "one"],
            vec![
                "auth",
                "key",
                "add",
                "--provider",
                "openai",
                "--account",
                "one",
                "--key",
                "never-print",
            ],
        ] {
            assert!(Options::try_parse_from(args).is_err());
        }
    }
}
