//! Exact bounded access-policy snapshots, independent of rename-sensitive ctime.
use std::{fs::File, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AccessMetadata {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    #[cfg(windows)]
    descriptor: Vec<u8>,
}
impl AccessMetadata {
    pub fn replacement_matches(&self, original: &Self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            fn retained(name: &[u8]) -> bool {
                !matches!(
                    name,
                    b"security.capability" | b"security.ima" | b"security.evm"
                )
            }
            self.entries.iter().all(|(name, _)| retained(name))
                && self
                    .entries
                    .iter()
                    .eq(original.entries.iter().filter(|(name, _)| retained(name)))
        }
        #[cfg(windows)]
        {
            self == original
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
        {
            let _ = original;
            false
        }
    }
}

pub(super) fn read(file: &File, path: &Path) -> Result<AccessMetadata, String> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let _ = path;
        use rustix::fs::{fgetxattr, fgetxattr_query, flistxattr, flistxattr_query};
        const LIMIT: usize = 1024 * 1024;
        let size = flistxattr_query(file)
            .map_err(|e| format!("Cannot snapshot output access metadata: {e}"))?;
        if size > LIMIT {
            return Err("Output access metadata exceeds 1 MiB".into());
        }
        let mut names = vec![0; size];
        let n = flistxattr(file, &mut names[..]).map_err(|e| e.to_string())?;
        if n > names.len() {
            return Err("Output metadata changed while listing".into());
        }
        names.truncate(n);
        let mut names = names
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        names.sort_unstable();
        let mut exact = Vec::new();
        let mut total = 0usize;
        for name in names {
            let size = fgetxattr_query(file, name).map_err(|e| e.to_string())?;
            let next = total
                .checked_add(name.len())
                .and_then(|n| n.checked_add(size))
                .and_then(|n| n.checked_add(16));
            if next.is_none_or(|n| n > LIMIT) {
                return Err("Output access metadata exceeds 1 MiB".into());
            }
            let mut value = vec![0; size];
            let n = fgetxattr(file, name, &mut value[..]).map_err(|e| e.to_string())?;
            if n > value.len() {
                return Err("Output metadata changed while reading".into());
            }
            value.truncate(n);
            total = next.ok_or("Output metadata overflow")?;
            exact.push((name.to_vec(), value));
        }
        Ok(AccessMetadata { entries: exact })
    }
    #[cfg(windows)]
    {
        let _ = file;
        super::windows_metadata::snapshot(path).map(|descriptor| AccessMetadata { descriptor })
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", windows)))]
    {
        let _ = (file, path);
        // No unsafe or text-parsed native ACL API fallback. Viewing stays available;
        // existing-output saves explicitly refuse on these platforms.
        Ok(AccessMetadata {})
    }
}
