//! Owned synthetic filesystem fixtures shared by library, CLI and example tests.
#![allow(dead_code)]
use std::path::PathBuf;

fn temporary_directory(private: bool) -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("morphiecore-test-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(if private {
            0o700
        } else {
            0o755
        }));
    }
    #[cfg(not(unix))]
    let _ = private;
    let directory = builder.tempdir().unwrap();
    #[cfg(unix)]
    if !private {
        // umask must not turn this permission-test parent into a privacy barrier.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    directory
}
pub fn private_directory() -> tempfile::TempDir {
    temporary_directory(true)
}

pub struct Directory {
    pub path: PathBuf,
    _owner: tempfile::TempDir,
}
impl Directory {
    pub fn new() -> Self {
        // A traversable, non-writable parent keeps root-permission rejection
        // meaningful. A private ancestor would mask an intentionally public root.
        let owner = temporary_directory(false);
        Self {
            path: owner.path().join("store"),
            _owner: owner,
        }
    }
    pub fn existing() -> Self {
        let owner = private_directory();
        Self {
            path: owner.path().into(),
            _owner: owner,
        }
    }
}
