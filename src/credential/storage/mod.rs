//! Private file storage and store-directory selection.
pub(in crate::credential) mod directory;
pub(in crate::credential) mod fs;
pub(in crate::credential) mod store;
#[cfg(windows)]
pub(in crate::credential) mod windows;
