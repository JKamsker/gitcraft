//! Delete only a terminal receipt opened relative to a retained private directory.
//! Empty directories stay: a final pathname removal would reopen a substitution race.
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

trait Retirement {
    fn receipt(&mut self) -> io::Result<Vec<u8>>;
    fn remove(&mut self) -> io::Result<()>;
}
fn retire(owner: &mut impl Retirement, verify: impl FnOnce(&[u8]) -> bool) -> io::Result<()> {
    if !verify(&owner.receipt()?) {
        return Err(io::Error::other("unconfirmed terminal receipt"));
    }
    owner.remove()
}
pub fn retire_private_receipt(path: &Path, verify: impl FnOnce(&[u8]) -> bool) -> io::Result<()> {
    retire(&mut Pinned::open(path)?, verify)
}
struct Pinned {
    directory: crate::PrivateDirectory,
    terminal: File,
}
impl Pinned {
    fn open(path: &Path) -> io::Result<Self> {
        let directory = crate::PrivateDirectory::open(path)?;
        let terminal = directory.open_file("terminal", crate::PrivateFileMode::ReadDelete)?;
        if terminal.metadata()?.len() > 4096 {
            return Err(io::Error::other("oversized terminal receipt"));
        }
        Ok(Self {
            directory,
            terminal,
        })
    }
}
impl Retirement for Pinned {
    fn receipt(&mut self) -> io::Result<Vec<u8>> {
        bounded(&mut self.terminal)
    }
    fn remove(&mut self) -> io::Result<()> {
        self.directory.retire_terminal(&self.terminal)
    }
}
fn bounded(file: &mut File) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other("oversized terminal receipt"));
    }
    Ok(bytes)
}
#[cfg(windows)]
pub(super) fn private_acl(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            *,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let fail = |e: windows::core::Error| io::Error::other(e.to_string());
    // SAFETY: all pointers refer to owned Win32 outputs/buffers and stay live through
    // validation. Security descriptor and token are released on every result path.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(fail)?;
        let result = (|| {
            let mut needed = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
            let mut words = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
            GetTokenInformation(
                token,
                TokenUser,
                Some(words.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
            .map_err(fail)?;
            let user = &*words.as_ptr().cast::<TOKEN_USER>();
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            let mut owner = PSID::default();
            let mut acl = std::ptr::null_mut();
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                Some(&mut acl),
                None,
                Some(&mut descriptor),
            )
            .ok()
            .map_err(fail)?;
            let checked = (|| {
                EqualSid(owner, user.User.Sid).map_err(fail)?;
                let mut control = 0;
                let mut revision = 0;
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision)
                    .map_err(fail)?;
                if control & SE_DACL_PROTECTED.0 == 0 || acl.is_null() || (*acl).AceCount != 1 {
                    return Err(io::Error::other("endpoint DACL is not private"));
                }
                let mut ace = std::ptr::null_mut();
                GetAce(acl, 0, &mut ace).map_err(fail)?;
                let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                if ace.Header.AceType != 0 || ace.Header.AceSize < 16 {
                    return Err(io::Error::other("endpoint ACE is not a user allow entry"));
                }
                let sid = (&ace.SidStart as *const u32).cast::<u8>();
                let sid_len = 8usize + usize::from(*sid.add(1)) * 4;
                if usize::from(ace.Header.AceSize) < 8 + sid_len {
                    return Err(io::Error::other("endpoint ACE has an incomplete SID"));
                }
                EqualSid(PSID(sid.cast_mut().cast()), user.User.Sid).map_err(fail)
            })();
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            checked
        })();
        let _ = CloseHandle(token);
        result
    }
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn replacement_never_changes_retirement_identity_and_missing_proof_erases_nothing() {
        struct Fake {
            held: &'static str,
            path: &'static str,
            removed: Vec<&'static str>,
        }
        impl Retirement for Fake {
            fn receipt(&mut self) -> io::Result<Vec<u8>> {
                self.path = "foreign";
                Ok(b"receipt".to_vec())
            }
            fn remove(&mut self) -> io::Result<()> {
                self.path = "foreign-again";
                self.removed.push(self.held);
                Ok(())
            }
        }
        let mut fake = Fake {
            held: "original",
            path: "original",
            removed: vec![],
        };
        assert!(retire(&mut fake, |_| false).is_err());
        assert!(fake.removed.is_empty());
        retire(&mut fake, |b| b == b"receipt").unwrap();
        assert_eq!(fake.removed, ["original"]);
    }
}
