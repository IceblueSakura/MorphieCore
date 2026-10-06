//! Private local files: fs-mistrust verifies paths; atomicwrites owns durable replacement.
use super::{CredentialError as Error, SecretBytes};
use fs_mistrust::{CheckedDir, Mistrust};
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::Path,
};

pub(super) struct FileLock(File);
impl std::fmt::Debug for FileLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileLock([private])")
    }
}
impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
pub(super) struct Directory {
    checked: CheckedDir,
    mistrust: Mistrust,
}
impl Directory {
    pub fn open(root: &Path) -> Result<Self, Error> {
        // Do not recursively create missing operator-selected parents.
        if !root
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .is_dir()
        {
            return Err(Error::Storage);
        }
        let mut builder = Mistrust::builder();
        builder.ignore_environment();
        #[cfg(unix)]
        builder.trust_no_group_id();
        let mistrust = builder.build().map_err(|_| Error::Storage)?;
        if root.symlink_metadata().is_ok_and(|m| m.is_symlink()) {
            return Err(Error::Storage);
        }
        let checked = mistrust
            .verifier()
            .make_secure_dir(root)
            .map_err(|_| Error::Storage)?;
        let directory = Self { checked, mistrust };
        directory.check()?;
        Ok(directory)
    }
    pub fn check(&self) -> Result<(), Error> {
        self.mistrust
            .verifier()
            .require_directory()
            .check(self.checked.as_path())
            .map_err(|_| Error::Storage)?;
        #[cfg(windows)]
        super::storage_windows::check(self.checked.as_path())?;
        Ok(())
    }
    pub fn read(&self, name: &str, limit: u64) -> Result<Option<SecretBytes>, Error> {
        self.check()?;
        let Some(_) = self.metadata(name)? else {
            return Ok(None);
        };
        let file = self
            .checked
            .open(name, OpenOptions::new().read(true))
            .map_err(|_| Error::Storage)?;
        let mut bytes = SecretBytes::new(Vec::new());
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Storage)?;
        if bytes.len() as u64 > limit {
            return Err(Error::Storage);
        }
        Ok(Some(bytes))
    }
    fn metadata(&self, name: &str) -> Result<Option<std::fs::Metadata>, Error> {
        let path = self.checked.join(name).map_err(|_| Error::Storage)?;
        let metadata = match path.symlink_metadata() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Storage),
        };
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(Error::Storage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
                return Err(Error::Storage);
            }
        }
        self.mistrust
            .verifier()
            .require_file()
            .check(&path)
            .map_err(|_| Error::Storage)?;
        #[cfg(windows)]
        super::storage_windows::check(&path)?;
        Ok(Some(metadata))
    }
    pub fn lock(&self, name: &str) -> Result<FileLock, Error> {
        self.check()?;
        self.metadata(name)?;
        let file = self
            .checked
            .open(name, OpenOptions::new().read(true).write(true).create(true))
            .map_err(|_| Error::Storage)?;
        self.metadata(name)?;
        file.try_lock().map_err(|e| match e {
            TryLockError::WouldBlock => Error::Busy,
            TryLockError::Error(_) => Error::Storage,
        })?;
        Ok(FileLock(file))
    }
    /// Only a callback error is known to precede replacement. Internal errors
    /// may follow rename/fsync and must keep the caller's recovery marker armed.
    pub fn write(
        &self,
        name: &str,
        bytes: &[u8],
        before_commit: impl FnOnce() -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.check()?;
        self.metadata(name)?;
        let path = self.checked.join(name).map_err(|_| Error::Storage)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        atomicwrites::AtomicFile::new(path, atomicwrites::AllowOverwrite)
            .write_with_options(
                |file| {
                    file.write_all(bytes).map_err(|_| Error::Storage)?;
                    before_commit()
                },
                options,
            )
            .map_err(|error| match error {
                atomicwrites::Error::User(error) => error,
                atomicwrites::Error::Internal(_) => Error::PersistenceUncertain,
            })
    }
    pub fn names(&self) -> Result<Vec<String>, Error> {
        self.check()?;
        let entries = self
            .checked
            .read_directory(".")
            .map_err(|_| Error::Storage)?;
        let mut names = Vec::new();
        for (index, entry) in entries.enumerate() {
            if index >= 16384 {
                return Err(Error::PoolFull);
            }
            let entry = entry.map_err(|_| Error::Storage)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Storage)?;
            // Library-owned remnants after an interrupted atomic write are never loaded.
            if name.starts_with(".atomicwrite") {
                continue;
            }
            if entry.file_type().map_err(|_| Error::Storage)?.is_dir() {
                return Err(Error::LegacyStore);
            }
            names.push(name);
        }
        names.sort();
        Ok(names)
    }
}
