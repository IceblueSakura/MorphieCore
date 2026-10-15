//! Minimal loopback bootstrap; startup never prints credential values.
use clap::Parser;
use morphiecore::credential::{CredentialError, default_config_directory, resolve_store_directory};
use morphiecore::gateway::startup::bootstrap::Bootstrap;
#[derive(Parser)]
struct Options {
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
    fn paths(self) -> Result<(std::path::PathBuf, std::path::PathBuf), CredentialError> {
        let directory = resolve_store_directory(self.credentials_dir)?;
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
    let (directory, config) = match options.paths() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("MorphieCore startup failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let bootstrap = match Bootstrap::from_files_with_proxy(&config, &directory, proxy.as_deref()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn credentials_directory_is_optional_and_overrides_keep_config_independent() {
        let default_store = resolve_store_directory(None).unwrap();
        let default_config = default_store.parent().unwrap().join("gateway.json");
        assert_eq!(
            Options::try_parse_from(["morphiecore"])
                .unwrap()
                .paths()
                .unwrap(),
            (default_store.clone(), default_config.clone())
        );
        assert!(
            Options::try_parse_from(["morphiecore"])
                .unwrap()
                .credentials_dir
                .is_none()
        );
        let selected =
            Options::try_parse_from(["morphiecore", "--credentials-dir", "selected-store"])
                .unwrap()
                .paths()
                .unwrap();
        assert_eq!(selected, (PathBuf::from("selected-store"), default_config));
        let selected = Options::try_parse_from([
            "morphiecore",
            "--credentials-dir",
            "selected-store",
            "--config",
            "separate-gateway.json",
        ])
        .unwrap()
        .paths()
        .unwrap();
        assert_eq!(
            selected,
            (
                PathBuf::from("selected-store"),
                PathBuf::from("separate-gateway.json")
            )
        );
        assert_eq!(
            Options::try_parse_from(["morphiecore", "--config", "separate-gateway.json"])
                .unwrap()
                .paths()
                .unwrap(),
            (default_store, PathBuf::from("separate-gateway.json"))
        );
    }
}
