//! Minimal loopback bootstrap; startup never prints credential values.
use clap::Parser;
use morphiecore::credential::{CredentialError, default_config_directory};
use morphiecore::gateway::startup::bootstrap::Bootstrap;
#[derive(Parser)]
struct Options {
    /// Validate through production construction without listening or starting maintenance.
    #[arg(long)]
    validate: bool,
    #[arg(long, requires = "validate")]
    json: bool,
    /// Override the default ~/.local/share/morphiecore/credentials directory.
    #[arg(long)]
    credentials_dir: Option<std::path::PathBuf>,
    /// Override the default ~/.local/share/morphiecore/gateway.json file.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
    /// Override gateway.json and environment proxies.
    #[arg(long)]
    proxy: Option<String>,
}
impl Options {
    fn paths(self) -> Result<(Option<std::path::PathBuf>, std::path::PathBuf), CredentialError> {
        let directory = self.credentials_dir;
        let config = match self.config {
            Some(path) => path,
            None => default_config_directory()?.join("gateway.json"),
        };
        Ok((directory, config))
    }
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let options = match Options::try_parse() {
        Ok(options) => options,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return std::process::ExitCode::SUCCESS;
            }
            eprintln!("Invalid startup arguments; use --help. Values are not echoed.");
            return std::process::ExitCode::FAILURE;
        }
    };
    let proxy = options.proxy.clone();
    let validate = options.validate;
    let json = options.json;
    let (directory, config) = match options.paths() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("MorphieCore startup failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if validate {
        return match Bootstrap::validate(&config, directory.as_deref(), proxy.as_deref()) {
            Ok(()) => {
                if json {
                    println!("{}", serde_json::json!({"valid":true}));
                } else {
                    println!("Gateway configuration and local bindings are valid.");
                }
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"error":{"message":error.to_string()}})
                    );
                } else {
                    eprintln!("MorphieCore validation failed: {error}");
                }
                std::process::ExitCode::FAILURE
            }
        };
    }
    let bootstrap = match Bootstrap::from_options(&config, directory.as_deref(), proxy.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("MorphieCore startup failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let listener = match tokio::net::TcpListener::bind(bootstrap.listen).await {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Loopback listener failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if let Ok(address) = listener.local_addr() {
        println!("MorphieCore listening on http://{address}");
    }
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let owner = bootstrap.gateway.clone();
    let result = bootstrap.gateway.serve(listener, shutdown).await;
    owner.flush_diagnostics().await;
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("MorphieCore server stopped with an I/O error");
            std::process::ExitCode::FAILURE
        }
    }
}
