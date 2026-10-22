//! Manual upstream diagnostics, independent of gateway activation and routing.
#[path = "upstream/mod.rs"]
mod upstream;

use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let options = match upstream::Options::try_parse() {
        Ok(options) => options,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            eprintln!("Invalid upstream arguments; use --help. Values are not echoed.");
            return ExitCode::FAILURE;
        }
    };
    tokio::select! {
        result = upstream::execute(options) => match result {
            Ok(report) => { println!("{report}"); ExitCode::SUCCESS }
            Err(error) => { eprintln!("Upstream operation failed: {error}"); ExitCode::FAILURE }
        },
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Upstream operation cancelled; no retry. If OAuth refresh was interrupted, check morphiecore-auth status before continuing.");
            ExitCode::FAILURE
        }
    }
}
