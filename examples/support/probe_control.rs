//! Share the Python/SQLite control plane without exposing request bodies or keys.
#[path = "child_process.rs"]
mod child_process;
use serde_json::Value;
use std::time::Duration;
use tokio::process::Command;

pub async fn call(action: &str, data: Value) -> String {
    let result = async {
        let run = std::env::var("MORPHIECORE_PROBE_RUN").map_err(|_| ())?;
        let input = serde_json::to_vec(&data).map_err(|_| ())?;
        if input.len() > 16384 {
            return Err(());
        }
        let mut command = Command::new(
            std::env::var_os("MORPHIECORE_PROBE_PYTHON").unwrap_or_else(|| "python3".into()),
        );
        command
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/probe.py"))
            .args(["control", &run, action]);
        let output = child_process::run(&mut command, &input, Duration::from_secs(10), 256)
            .await
            .map_err(|_| ())?;
        if !output.status.success() {
            return Err(());
        }
        String::from_utf8(output.stdout)
            .map(|s| s.trim().to_owned())
            .map_err(|_| ())
    }
    .await;
    result.unwrap_or_else(|_| {
        eprintln!("shared probe ledger rejected operation; no retry");
        std::process::exit(2)
    })
}
