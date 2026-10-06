//! Grok Build auth metadata reference; no project branding or environment identity overrides.
//! Wire/UA source (Apache-2.0): https://github.com/xai-org/grok-build/tree/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8
//! Shipping version reference: https://registry.npmjs.org/@xai-official/grok/1.0.46
//! The release version is independent of the source crates' development package versions.
use std::io::IsTerminal;

pub(super) const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
pub(super) const VERSION: &str = "1.0.46";
pub(super) const REFERRER: &str = "grok-build";

pub(super) fn user_agent() -> String {
    format_agent(std::env::consts::OS, std::env::consts::ARCH)
}
fn format_agent(os: &str, arch: &str) -> String {
    // The official generic origin equals the agent, so duplicate products collapse.
    let arch = if arch == "arm64" { "aarch64" } else { arch };
    format!("grok-shell/{VERSION} ({os}; {arch})")
}
pub(super) fn device_surface() -> &'static str {
    surface(std::io::stderr().is_terminal())
}
fn surface(stderr_is_tty: bool) -> &'static str {
    if stderr_is_tty { "cli" } else { "headless" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_agent_matches_the_official_shape_without_project_or_duplicate_origin() {
        assert_eq!(
            format_agent("linux", "x86_64"),
            "grok-shell/1.0.46 (linux; x86_64)"
        );
        assert_eq!(
            format_agent("macos", "arm64"),
            "grok-shell/1.0.46 (macos; aarch64)"
        );
    }

    #[test]
    fn cli_surface_tracks_stderr_tty_and_never_claims_ui() {
        assert_eq!(surface(true), "cli");
        assert_eq!(surface(false), "headless");
    }
}
