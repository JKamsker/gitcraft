//! Raw object materialization: no checkout/filter/encoding machinery can run, even
//! if shared configuration or info/attributes changes after preflight. The budget
//! counts indexed bytes, block rounding and namespace overhead without expansion.
use super::{Entry, fail};
use crate::git;
use std::{fs::File, io::Write, path::Path};

pub(super) fn write(repo: &Path, root: &Path, entries: &[Entry]) -> Result<(), String> {
    #[cfg(windows)]
    let resolved_links = super::links::resolve(&super::links::targets(repo, entries)?)?;
    // Install links last: regular writes cannot traverse any staged symlink.
    for links in [false, true] {
        for entry in entries.iter().filter(|e| (e.mode == "120000") == links) {
            crate::processes::deadline::check()?;
            let bytes = git::command::run_inert_objects(
                repo,
                &["cat-file", "blob", &entry.oid],
                None,
                entry.size as usize + 1,
                &[0],
            )?;
            if bytes.len() as u64 != entry.size {
                return Err(fail("staged-paths"));
            }
            let path = root.join(&entry.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if links {
                let target = std::str::from_utf8(&bytes).map_err(|_| fail("staged-links"))?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, &path).map_err(|_| fail("staged-paths"))?;
                #[cfg(windows)]
                {
                    // Windows distinguishes file and directory links. Derive the type
                    // from the complete manifest, never from an unstaged source target.
                    let destination = resolved_links
                        .get(&super::folded(&entry.path))
                        .ok_or_else(|| fail("staged-links"))?;
                    let prefix = format!("{destination}/");
                    let directory = entries
                        .iter()
                        .any(|e| super::folded(&e.path).starts_with(&prefix));
                    if directory {
                        std::os::windows::fs::symlink_dir(target, &path)
                    } else {
                        std::os::windows::fs::symlink_file(target, &path)
                    }
                    .map_err(|_| fail("staged-paths"))?;
                }
                #[cfg(not(any(unix, windows)))]
                return Err(fail("staged-paths"));
            } else {
                let mut file = File::options()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                for chunk in bytes.chunks(8192) {
                    crate::processes::deadline::check()?;
                    file.write_all(chunk).map_err(|e| e.to_string())?;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = if entry.mode == "100755" { 0o755 } else { 0o644 };
                    file.set_permissions(std::fs::Permissions::from_mode(mode))
                        .map_err(|_| fail("staged-paths"))?;
                }
            }
        }
    }
    Ok(())
}
