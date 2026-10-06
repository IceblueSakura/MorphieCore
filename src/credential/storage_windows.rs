//! Supplement fs-mistrust's documented lack of Windows ACL checks; no ACL mutation.
use super::CredentialError as Error;
use std::{os::windows::fs::MetadataExt, path::Path};
use windows_permissions::{
    constants::{AceFlags, AceType, SeObjectType, SecurityInformation},
    utilities, wrappers,
};

pub(super) fn check(path: &Path) -> Result<(), Error> {
    let current = utilities::current_process_sid().map_err(|_| Error::Storage)?;
    let system = wrappers::ConvertStringSidToSid("S-1-5-18").map_err(|_| Error::Storage)?;
    let admins = wrappers::ConvertStringSidToSid("S-1-5-32-544").map_err(|_| Error::Storage)?;
    let absolute = std::path::absolute(path).map_err(|_| Error::Storage)?;
    for (index, path) in absolute.ancestors().enumerate() {
        let metadata = path.symlink_metadata().map_err(|_| Error::Storage)?;
        // Includes junctions and other reparse points, not only symbolic links.
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Storage);
        }
        let descriptor = wrappers::GetNamedSecurityInfo(
            path.as_os_str(),
            SeObjectType::SE_FILE_OBJECT,
            SecurityInformation::Owner | SecurityInformation::Dacl,
        )
        .map_err(|_| Error::Storage)?;
        let trusted =
            |sid: &windows_permissions::Sid| sid == &*current || sid == &*system || sid == &*admins;
        if !descriptor.owner().is_some_and(trusted) {
            return Err(Error::Storage);
        }
        let acl = descriptor.dacl().ok_or(Error::Storage)?;
        let mut private_inheritance = false;
        for n in 0..acl.len() {
            let ace = acl.get_ace(n).ok_or(Error::Storage)?;
            match ace.ace_type() {
                AceType::ACCESS_DENIED_ACE_TYPE => (),
                AceType::ACCESS_ALLOWED_ACE_TYPE => {
                    private_inheritance |= ace.sid().is_some_and(trusted)
                        && ace
                            .flags()
                            .contains(AceFlags::ObjectInherit | AceFlags::ContainerInherit)
                        && !ace.flags().contains(AceFlags::NoPropagateInherit);
                    // Parents may be traversable/readable, but untrusted subjects
                    // cannot create, replace, delete or change security on our path.
                    let dangerous = index == 0 || ace.mask().bits() & 0x500D_0156 != 0;
                    if dangerous && !ace.sid().is_some_and(trusted) {
                        return Err(Error::Storage);
                    }
                }
                _ => return Err(Error::Storage),
            }
        }
        // Atomic writers create descendants with inherited security. Without a
        // private inheritable DACL, the process default could expose temporary data.
        // https://learn.microsoft.com/windows/win32/secauthz/automatic-propagation-of-inheritable-aces
        if index == 0 && metadata.is_dir() && !private_inheritance {
            return Err(Error::Storage);
        }
    }
    Ok(())
}
