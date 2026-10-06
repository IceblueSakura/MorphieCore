//! CLI-only path selection; library constructors still require an explicit store.
use super::CredentialError;
use std::{ffi::OsString, path::PathBuf};

#[cfg(windows)]
const HOME_VARIABLE: &str = "USERPROFILE";
#[cfg(not(windows))]
const HOME_VARIABLE: &str = "HOME";

/// Select an operator override or this application's home-relative store.
/// This resolves a path only: no directory creation, discovery or credential I/O.
pub fn resolve_store_directory(explicit: Option<PathBuf>) -> Result<PathBuf, CredentialError> {
    match explicit {
        Some(path) if path.as_os_str().is_empty() => Err(CredentialError::InvalidInput),
        Some(path) => Ok(path),
        None => home_store(std::env::var_os(HOME_VARIABLE)),
    }
}

fn home_store(home: Option<OsString>) -> Result<PathBuf, CredentialError> {
    let home = home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(CredentialError::DefaultStoreUnavailable)?;
    Ok(home
        .join(".local")
        .join("share")
        .join("morphiecore")
        .join("credentials"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_is_a_fixed_child_of_an_absolute_home() {
        let home = PathBuf::from(if cfg!(windows) {
            r"C:\synthetic-home"
        } else {
            "/synthetic-home"
        });
        assert_eq!(
            home_store(Some(home.clone().into_os_string())).unwrap(),
            home.join(".local/share/morphiecore/credentials")
        );
    }

    #[test]
    fn missing_empty_or_relative_home_never_selects_the_working_directory() {
        for home in [None, Some("".into()), Some("relative-home".into())] {
            assert_eq!(
                home_store(home).unwrap_err(),
                CredentialError::DefaultStoreUnavailable
            );
        }
    }

    #[test]
    fn an_explicit_directory_does_not_require_home_but_cannot_be_empty() {
        let explicit = PathBuf::from("selected-store");
        assert_eq!(
            resolve_store_directory(Some(explicit.clone())).unwrap(),
            explicit
        );
        assert_eq!(
            resolve_store_directory(Some(PathBuf::new())).unwrap_err(),
            CredentialError::InvalidInput
        );
    }
}
