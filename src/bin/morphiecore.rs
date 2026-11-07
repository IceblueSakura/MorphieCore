//! Minimal loopback bootstrap; startup never prints credential values.
use clap::Parser;
use morphiecore::credential::{CredentialError, default_config_directory};
use morphiecore::gateway::startup::bootstrap::Bootstrap;
#[derive(Parser)]
struct Options {
    /// Validate through production construction without listening or starting maintenance.
    #[arg(long)]
    validate: bool,
    /// Show the compiled model/binding catalog without opening configuration or credentials.
    #[arg(long, conflicts_with_all = ["validate", "explain", "config", "credentials_dir", "proxy"])]
    catalog: bool,
    /// Validate and explain effective settings and account-bound candidates, without serving.
    #[arg(long, conflicts_with = "validate")]
    explain: bool,
    #[arg(long)]
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
    if options.json && !options.validate && !options.explain && !options.catalog {
        eprintln!("--json requires --validate, --explain or --catalog.");
        return std::process::ExitCode::FAILURE;
    }
    if options.catalog {
        return report(
            morphiecore::gateway::startup::inspection::catalog(),
            options.json,
        );
    }
    let proxy = options.proxy.clone();
    let validate = options.validate;
    let explain = options.explain;
    let json = options.json;
    let (directory, config) = match options.paths() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("MorphieCore startup failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if explain {
        return report(
            Bootstrap::explain(&config, directory.as_deref(), proxy.as_deref()),
            json,
        );
    }
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
                    println!("{}", serde_json::json!({"error":error.diagnostic()}));
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

fn report(
    result: Result<serde_json::Value, morphiecore::gateway::StartupError>,
    json: bool,
) -> std::process::ExitCode {
    match result {
        Ok(value) => {
            if json {
                println!("{value}");
            } else {
                if let Some(semantics) = value["semantics"].as_str() {
                    println!("{semantics}");
                }
                for section in [
                    "routing",
                    "oauth",
                    "delivery",
                    "resources",
                    "providers",
                    "proxy_source",
                    "diagnostics_enabled",
                    "capture",
                ] {
                    if let Some(settings) = value.get(section) {
                        println!(
                            "{section}: {}",
                            serde_json::to_string_pretty(settings).expect("JSON value")
                        );
                    }
                }
                if let Some(rows) = value
                    .get("models")
                    .or_else(|| value.get("selections"))
                    .and_then(|v| v.as_array())
                {
                    for row in rows {
                        println!("{}", serde_json::to_string_pretty(row).expect("JSON value"));
                    }
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                println!("{}", serde_json::json!({"error":error.diagnostic()}));
            } else {
                eprintln!("Configuration inspection failed: {error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
