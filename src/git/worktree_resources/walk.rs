//! Linux descriptor-relative measurement. Iteration, stack and retained evidence are bounded.
use super::classify::{Membership, artifact};
use super::{Category, Completeness, Inventory};
use crate::git::worktree_inspection::unix::{child, open_root};
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, Dir, FileType};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Duration, Instant};

const ENTRIES: usize = 20_000;
const DEPTH: usize = 64;

pub(super) fn measure(found: &mut Inventory, membership: &Membership) -> Result<(), String> {
    measure_with(found, membership, ENTRIES, &|| {
        crate::tasks::cancellation().is_cancelled()
    })
}

pub(super) fn measure_with(
    found: &mut Inventory,
    membership: &Membership,
    budget: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let root = open_root(&found.root)?;
    let cargo = membership.tracked.contains(Path::new("Cargo.toml"))
        && membership
            .ignored
            .contains(Path::new("target/.rustc_info.json"));
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let identity = fs::fstat(&root).map_err(|e| e.to_string())?;
    (identity.st_dev, identity.st_ino).hash(&mut hash);
    found.root.hash(&mut hash);
    found.fingerprint.hash(&mut hash);
    found.head.hash(&mut hash);
    found.branch.hash(&mut hash);
    found.presence.hash(&mut hash);
    membership.tracked.hash(&mut hash);
    membership.untracked.hash(&mut hash);
    membership.ignored.hash(&mut hash);
    (
        found.staged,
        found.unstaged,
        found.conflicted,
        found.untracked,
    )
        .hash(&mut hash);
    let started = Instant::now();
    let mut state = Walk {
        found,
        membership,
        cargo,
        budget,
        seen: 0,
        hash,
        deadline: started + Duration::from_secs(5),
        cancelled,
    };
    if let Err(reason) = state.directory(&root, Path::new(""), 0) {
        for m in &mut state.found.measurements {
            m.status = Completeness::Partial(reason.clone());
        }
    }
    if cargo
        && !state
            .found
            .blockers
            .iter()
            .any(|b| b == "resources-ambiguous-build")
    {
        state.found.candidates.push(state.found.root.join("target"));
    }
    if membership
        .tracked
        .iter()
        .chain(&membership.untracked)
        .any(|p| p.starts_with("target"))
    {
        state
            .found
            .blockers
            .push("resources-ambiguous-build".into());
        state.found.candidates.clear();
    }
    let identity = |fd: &OwnedFd| {
        fs::fstat(fd)
            .map(|s| (s.st_dev, s.st_ino))
            .map_err(|e| e.to_string())
    };
    if identity(&root)? != identity(&open_root(&state.found.root)?)? {
        for m in &mut state.found.measurements {
            m.status = Completeness::Partial("resources-stale".into());
        }
        state.found.candidates.clear();
    }
    state.found.fingerprint = format!("{:016x}", state.hash.finish());
    Ok(())
}
struct Walk<'a> {
    found: &'a mut Inventory,
    membership: &'a Membership,
    cargo: bool,
    budget: usize,
    seen: usize,
    hash: std::collections::hash_map::DefaultHasher,
    deadline: Instant,
    cancelled: &'a dyn Fn() -> bool,
}
impl Walk<'_> {
    fn directory(&mut self, fd: &OwnedFd, rel: &Path, depth: usize) -> Result<(), String> {
        if depth > DEPTH {
            return Err("resources-depth".into());
        }
        let entries = Dir::read_from(fd).map_err(|e| e.to_string())?;
        for entry in entries {
            // Check while enumerating, before retaining entries or opening children.
            if (self.cancelled)() {
                return Err("resources-cancelled".into());
            }
            if self.seen >= self.budget || Instant::now() >= self.deadline {
                return Err("resources-budget".into());
            }
            self.seen += 1;
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if matches!(name.to_bytes(), b"." | b".." | b".git") {
                continue;
            }
            let path = rel.join(std::ffi::OsStr::from_bytes(name.to_bytes()));
            let stat =
                fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|e| e.to_string())?;
            path.hash(&mut self.hash);
            (
                stat.st_dev,
                stat.st_ino,
                stat.st_size,
                stat.st_mtime,
                stat.st_mtime_nsec,
            )
                .hash(&mut self.hash);
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::Directory => {
                    let dir = child(fd, name)?;
                    if fs::statat(&dir, ".git", AtFlags::SYMLINK_NOFOLLOW).is_ok() {
                        return Err("resources-nested".into());
                    }
                    self.directory(&dir, &path, depth + 1)?;
                }
                FileType::RegularFile => {
                    let held = crate::git::worktree_inspection::unix::inspect_entry(fd, name)?;
                    let opened = fs::fstat(&held).map_err(|e| e.to_string())?;
                    if (opened.st_dev, opened.st_ino, opened.st_mode)
                        != (stat.st_dev, stat.st_ino, stat.st_mode)
                    {
                        return Err("resources-stale".into());
                    }
                    let category = self.membership.category(&path, self.cargo);
                    if category == Category::Build
                        && !artifact(&path)
                        && !self
                            .found
                            .blockers
                            .iter()
                            .any(|b| b == "resources-ambiguous-build")
                    {
                        self.found.blockers.push("resources-ambiguous-build".into());
                    }
                    let m = &mut self.found.measurements[category.index()];
                    m.bytes = m
                        .bytes
                        .saturating_add(u64::try_from(stat.st_size).unwrap_or(0));
                    m.files += 1;
                }
                _ => return Err("resources-link".into()),
            }
        }
        Ok(())
    }
}
use std::os::unix::ffi::OsStrExt;
