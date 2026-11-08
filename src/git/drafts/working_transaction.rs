//! Native per-file exchanges with owned backups. A failed batch rolls back only entries
//! still equal to our published inode and bytes. External edits are retained, never clobbered.
use super::*;
pub(super) type Contents = BTreeMap<String, Option<(String, Vec<u8>)>>;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod native {
    use super::*;
    use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags, openat, renameat_with, unlinkat};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    struct Item {
        dir: File,
        name: String,
        spare: String,
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
        after_id: Option<(u64, u64)>,
        before_id: Option<(u64, u64)>,
        applied: bool,
    }
    fn read(dir: &File, name: &str) -> Result<Option<(File, Vec<u8>)>, String> {
        let fd = match openat(
            dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let mut f = File::from(fd);
        if !f.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Only regular files can be exchanged".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut f)
            .take(FILE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > FILE_LIMIT {
            return Err("File exceeds transaction limit".into());
        }
        Ok(Some((f, bytes)))
    }
    fn id(f: &File) -> Result<(u64, u64), String> {
        let m = f.metadata().map_err(|e| e.to_string())?;
        Ok((m.dev(), m.ino()))
    }
    impl Item {
        fn owned(&self) -> Result<bool, String> {
            let now = read(&self.dir, &self.name)?;
            Ok(match (now, self.after_id, self.after.as_ref()) {
                (None, None, None) => true,
                (Some((f, b)), Some(want), Some(bytes)) => id(&f)? == want && b == *bytes,
                _ => false,
            })
        }
        fn spare_matches(
            &self,
            token: Option<(u64, u64)>,
            bytes: Option<&Vec<u8>>,
        ) -> Result<bool, String> {
            Ok(match (read(&self.dir, &self.spare)?, token, bytes) {
                (Some((f, b)), Some(want), Some(bytes)) => id(&f)? == want && b == *bytes,
                (None, None, None) => true,
                _ => false,
            })
        }
        fn rollback(&mut self) -> Result<(), String> {
            if !self.applied {
                return Ok(());
            }
            if !self.owned()? {
                return Err(format!(
                    "{} was edited concurrently; its new contents and the owned backup {} are retained",
                    self.name, self.spare
                ));
            }
            // The displaced file itself is the rollback payload, including a racing
            // writer's bytes. Never remove or replace it during rollback.
            match (&self.before, &self.after) {
                (Some(_), Some(_)) => renameat_with(
                    &self.dir,
                    self.spare.as_str(),
                    &self.dir,
                    self.name.as_str(),
                    RenameFlags::EXCHANGE,
                ),
                (Some(_), None) => renameat_with(
                    &self.dir,
                    self.spare.as_str(),
                    &self.dir,
                    self.name.as_str(),
                    RenameFlags::NOREPLACE,
                ),
                (None, Some(_)) => renameat_with(
                    &self.dir,
                    self.name.as_str(),
                    &self.dir,
                    self.spare.as_str(),
                    RenameFlags::NOREPLACE,
                ),
                (None, None) => return Ok(()),
            }
            .map_err(|e| format!("Rollback requires attention; backup {}: {e}", self.spare))?;
            self.applied = false;
            Ok(())
        }
    }
    impl Drop for Item {
        fn drop(&mut self) {
            if !self.applied
                && self
                    .spare_matches(self.after_id, self.after.as_ref())
                    .unwrap_or(false)
            {
                let _ = unlinkat(&self.dir, self.spare.as_str(), AtFlags::empty());
            }
        }
    }
    pub(super) fn apply(
        repo: &Path,
        before: &Contents,
        after: &Contents,
        check: &dyn Fn() -> Result<(), String>,
        commit: &dyn Fn() -> Result<(), String>,
        permissions: Option<&BTreeMap<String, u32>>,
        active: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let identity = crate::git::recovery::pinned_worktree(repo, active)?;
        if identity.host.is_some() {
            return Err("Atomic working-folder exchange requires a native macOS/Linux checkout; remote files were not changed".into());
        }
        let pin = identity.root_pin.as_ref().ok_or("Missing root pin")?;
        let mut items = Vec::new();
        for (path, target) in after {
            valid_path(path)?;
            let expected = before.get(path).ok_or("Missing before-file snapshot")?;
            if expected == target {
                continue;
            }
            let (dir, name) = crate::git::recovery::pinned_parent(
                Path::new(&pin.physical),
                Path::new(&identity.common),
                path,
                pin,
            )?;
            let old = read(&dir, &name)?;
            if old.as_ref().map(|(_, b)| b) != expected.as_ref().map(|(_, b)| b) {
                return Err(format!("{path} changed before exchange"));
            }
            static ID: AtomicU64 = AtomicU64::new(0);
            let spare = format!(
                ".gitcraft-exchange-{}-{}-{}",
                git::now(),
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            );
            let mut after_id = None;
            if let Some((mode, bytes)) = target {
                let fd = openat(
                    &dir,
                    spare.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_bits_truncate(0o600),
                )
                .map_err(|e| e.to_string())?;
                let mut file = File::from(fd);
                let permissions = if let Some(bits) = permissions.and_then(|p| p.get(path)) {
                    std::fs::Permissions::from_mode(bits & 0o777)
                } else if let Some((f, _)) = &old {
                    let mut permissions = f.metadata().map_err(|e| e.to_string())?.permissions();
                    if expected.as_ref().is_some_and(|(old, _)| old != mode) {
                        let bits = permissions.mode() & 0o777;
                        permissions = std::fs::Permissions::from_mode(if mode == "100755" {
                            bits | ((bits & 0o444) >> 2)
                        } else {
                            bits & !0o111
                        });
                    }
                    permissions
                } else {
                    std::fs::Permissions::from_mode(if mode == "100755" { 0o700 } else { 0o600 })
                };
                file.write_all(bytes)
                    .and_then(|_| file.set_permissions(permissions))
                    .and_then(|_| file.sync_all())
                    .map_err(|e| e.to_string())?;
                after_id = Some(id(&file)?);
            }
            items.push(Item {
                dir,
                name,
                spare,
                before: expected.as_ref().map(|(_, b)| b.clone()),
                after: target.as_ref().map(|(_, b)| b.clone()),
                after_id,
                before_id: old.as_ref().map(|(f, _)| id(f)).transpose()?,
                applied: false,
            });
        }
        let result: Result<(), String> = (|| {
            check()?;
            for item in &mut items {
                if !active() {
                    return Err("Working-folder exchange cancelled".into());
                }
                check()?;
                let flags = if item.before.is_some() && item.after.is_some() {
                    RenameFlags::EXCHANGE
                } else {
                    RenameFlags::NOREPLACE
                };
                if item.after.is_some() {
                    renameat_with(
                        &item.dir,
                        item.spare.as_str(),
                        &item.dir,
                        item.name.as_str(),
                        flags,
                    )
                } else {
                    renameat_with(
                        &item.dir,
                        item.name.as_str(),
                        &item.dir,
                        item.spare.as_str(),
                        flags,
                    )
                }
                .map_err(|e| e.to_string())?;
                item.applied = true;
                if item.before.is_some()
                    && !item.spare_matches(item.before_id, item.before.as_ref())?
                {
                    return Err("File changed at the exchange boundary".into());
                }
                if !item.owned()? {
                    return Err("File changed immediately after exchange".into());
                }
            }
            check()?;
            commit()?;
            Ok(())
        })();
        if let Err(error) = result {
            let failures = items
                .iter_mut()
                .rev()
                .filter_map(|i| i.rollback().err())
                .collect::<Vec<_>>();

            return Err(format!(
                "{error}. {}",
                if failures.is_empty() {
                    "Owned changes rolled back".into()
                } else {
                    failures.join("; ")
                }
            ));
        }
        for i in &items {
            if i.before.is_some() {
                if !i.spare_matches(i.before_id, i.before.as_ref())? {
                    return Err(format!(
                        "Applied; backup {} changed concurrently and was retained",
                        i.spare
                    ));
                }
                unlinkat(&i.dir, i.spare.as_str(), AtFlags::empty())
                    .map_err(|e| format!("Applied; owned backup {} retained: {e}", i.spare))?;
            }
        }
        Ok(())
    }
}
pub(super) fn apply(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    check: &dyn Fn() -> Result<(), String>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        native::apply(repo, before, after, check, &|| Ok(()), None, active)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (repo, before, after, check, active);
        Err(
            "Atomic working-folder exchange is unavailable on this platform; no files changed"
                .into(),
        )
    }
}

pub(super) fn apply_with_commit(
    repo: &Path,
    before: &Contents,
    after: &Contents,
    check: &dyn Fn() -> Result<(), String>,
    commit: &dyn Fn() -> Result<(), String>,
    permissions: Option<&BTreeMap<String, u32>>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        native::apply(repo, before, after, check, commit, permissions, active)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (repo, before, after, check, commit, permissions, active);
        Err("Native atomic working-folder exchange is unavailable; no files changed".into())
    }
}
