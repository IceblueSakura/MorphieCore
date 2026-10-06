//! Typed CLI presentation over CredentialManager; no credential values in arguments.
use clap::{Args, Parser, Subcommand, ValueEnum};
use morphiecore::credential::{
    CredentialError as Error, CredentialManager, CredentialPool, CredentialStatus, LoginMethod,
    LoginOptions, LoginPrompt, LogoutOutcome, Secret, builtin_drivers, read_private_file,
};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(subcommand_precedence_over_arg = true)]
struct Options {
    /// Authorization profile for OAuth operations; not an inference Provider selector.
    profile: Option<String>,
    #[arg(long, global = true)]
    store: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    List,
    Login(Login),
    Refresh(Account),
    Logout {
        #[command(flatten)]
        account: Account,
        #[arg(long)]
        revoke: bool,
    },
    ApiKey {
        #[command(subcommand)]
        operation: KeyCommand,
    },
    Pool {
        #[command(subcommand)]
        operation: PoolCommand,
    },
}
#[derive(Args)]
struct Account {
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
    account: Account,
    #[arg(long)]
    client_id: Option<String>,
    #[arg(long, value_enum)]
    method: Option<Method>,
    #[arg(long)]
    callback_port: Option<u16>,
    /// Explicitly request ChatGPT plan consent; ordinary SIWC re-login does not.
    #[arg(long)]
    consent: bool,
}
#[derive(Args)]
struct Key {
    #[arg(long)]
    domain: String,
    #[arg(long)]
    alias: String,
}
#[derive(Subcommand)]
enum KeyCommand {
    Add {
        #[command(flatten)]
        key: Key,
        #[arg(long)]
        secret_file: PathBuf,
    },
    Replace {
        #[command(flatten)]
        key: Key,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        secret_file: PathBuf,
    },
    Enable {
        #[command(flatten)]
        key: Key,
        #[arg(long)]
        revision: u64,
    },
    Disable {
        #[command(flatten)]
        key: Key,
        #[arg(long)]
        revision: u64,
    },
    Remove {
        #[command(flatten)]
        key: Key,
        #[arg(long)]
        revision: u64,
    },
    List {
        #[arg(long)]
        domain: Option<String>,
    },
}
#[derive(Subcommand)]
enum PoolCommand {
    List,
    Set {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        binding: String,
        #[arg(long)]
        revision: u64,
        #[arg(long)]
        file: PathBuf,
    },
}
fn print<T: serde::Serialize>(value: &T) -> Result<(), Error> {
    println!(
        "{}",
        serde_json::to_string(value).map_err(|_| Error::Storage)?
    );
    Ok(())
}
fn secret(path: &std::path::Path) -> Result<Secret, Error> {
    let mut bytes = read_private_file(path, 16386)?;
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    Secret::new(
        std::str::from_utf8(&bytes)
            .map_err(|_| Error::InvalidInput)?
            .to_owned(),
    )
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
async fn execute(options: Options) -> Result<(), Error> {
    let root = options.store.ok_or(Error::InvalidInput)?;
    let profile = options.profile.as_deref();
    if matches!(
        options.command,
        Command::ApiKey { .. } | Command::Pool { .. }
    ) && profile.is_some()
    {
        return Err(Error::InvalidInput);
    }
    match options.command {
        Command::ApiKey { operation } => {
            // Validate input before opening a new destination store.
            let input = match &operation {
                KeyCommand::Add { secret_file, .. } | KeyCommand::Replace { secret_file, .. } => {
                    Some(secret(secret_file)?)
                }
                _ => None,
            };
            let manager = CredentialManager::new(root, vec![])?;
            match operation {
                KeyCommand::Add { key, .. } => {
                    print(&manager.add_api_key(&key.domain, &key.alias, input.unwrap())?)
                }
                KeyCommand::Replace { key, revision, .. } => print(&manager.replace_api_key(
                    &key.domain,
                    &key.alias,
                    revision,
                    input.unwrap(),
                )?),
                KeyCommand::Enable { key, revision } => {
                    print(&manager.set_api_key_enabled(&key.domain, &key.alias, revision, true)?)
                }
                KeyCommand::Disable { key, revision } => {
                    print(&manager.set_api_key_enabled(&key.domain, &key.alias, revision, false)?)
                }
                KeyCommand::Remove { key, revision } => {
                    print(&manager.remove_api_key(&key.domain, &key.alias, revision)?)
                }
                KeyCommand::List { domain } => print(&manager.list_api_keys(domain.as_deref())?),
            }
        }
        Command::Pool { operation } => {
            let manager = CredentialManager::new(root, builtin_drivers(None)?)?;
            match operation {
                PoolCommand::List => print(&manager.pools()?),
                PoolCommand::Set {
                    provider,
                    binding,
                    revision,
                    file,
                } => {
                    let config: CredentialPool =
                        serde_json::from_slice(&read_private_file(&file, 65536)?)
                            .map_err(|_| Error::InvalidInput)?;
                    print(&manager.set_pool(&provider, &binding, revision, config)?)
                }
            }
        }
        Command::List => {
            let manager = CredentialManager::new(root, builtin_drivers(None)?)?;
            match profile {
                Some(p) => print(
                    &manager
                        .list(Some(p))?
                        .into_iter()
                        .map(CredentialStatus::OAuth)
                        .collect::<Vec<_>>(),
                ),
                None => print(&manager.inventory()?),
            }
        }
        Command::Login(login) => {
            let profile = profile.ok_or(Error::InvalidInput)?;
            let method = login.method.unwrap_or(if profile == "siwc" {
                Method::Browser
            } else {
                Method::Device
            });
            if login.callback_port.is_some() && matches!(method, Method::Device) {
                return Err(Error::InvalidInput);
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
            let drivers = builtin_drivers(login.account.proxy.as_deref())?;
            drivers
                .iter()
                .find(|driver| driver.profile() == profile)
                .ok_or(Error::UnknownProfile)?
                .login_client(&settings)?;
            if profile == "siwc" {
                eprintln!("Continue with ChatGPT — MorphieCore");
                eprintln!(
                    "Identity, granted permissions and tokens are stored only in the selected owner-only local store. No ChatGPT history is imported. Plan usage is only for your own authorized tasks in this application."
                );
                eprintln!("Manage usage: https://chatgpt.com/settings/usage");
            }
            let manager = CredentialManager::new(root, drivers)?;
            let status = manager
                .login(profile, &login.account.account, settings, prompt)
                .await?;
            if let Some(enabled) = status.plan_usage_enabled {
                eprintln!(
                    "{}",
                    if enabled {
                        "Using ChatGPT plan."
                    } else {
                        "Identity signed in; ChatGPT plan usage disabled. Use login --consent only when you choose to enable it."
                    }
                );
            }
            print(&status)
        }
        Command::Refresh(account) => {
            let manager = CredentialManager::new(root, builtin_drivers(account.proxy.as_deref())?)?;
            print(
                &manager
                    .refresh(profile.ok_or(Error::InvalidInput)?, &account.account)
                    .await?,
            )
        }
        Command::Logout { account, revoke } => {
            let manager = CredentialManager::new(root, builtin_drivers(account.proxy.as_deref())?)?;
            let outcome = manager
                .logout(
                    profile.ok_or(Error::InvalidInput)?,
                    &account.account,
                    revoke,
                )
                .await?;
            println!(
                "{}",
                match outcome {
                    LogoutOutcome::LocalOnly =>
                        "Local session cleared; remote revocation was not requested.",
                    LogoutOutcome::Revoked =>
                        "Local session cleared; remote revocation acknowledged.",
                    LogoutOutcome::RevocationUnconfirmed =>
                        "Local session cleared; remote revocation NOT confirmed.",
                    LogoutOutcome::AlreadySignedOut => "No local account session.",
                }
            );
            if outcome == LogoutOutcome::RevocationUnconfirmed {
                return Err(Error::Network);
            }
            Ok(())
        }
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
    tokio::select! {
        result=execute(options) => match result { Ok(())=>ExitCode::SUCCESS, Err(error)=>{eprintln!("Credential operation failed: {error}");ExitCode::FAILURE} },
        _=tokio::signal::ctrl_c()=>{eprintln!("Credential operation cancelled. Check local status: pending renewal can be verified with refresh; uncertain consumption requires login.");ExitCode::FAILURE}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clap_preserves_profile_generic_operations_and_rejects_secrets_in_argv() {
        for command in [
            "third login --store private --account one",
            "grok list --store private",
            "list --store private",
            "api-key add --store private --domain alpha --alias one --secret-file private.key",
            "pool list --store private",
        ] {
            assert!(
                Options::try_parse_from(std::iter::once("auth").chain(command.split_whitespace()))
                    .is_ok(),
                "{command}"
            );
        }
        assert!(
            Options::try_parse_from(["auth", "api-key", "add", "--key", "never-print"]).is_err()
        );
    }
}
