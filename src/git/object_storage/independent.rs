//! Deliberate independence copies a self-contained pack; donors are read-only throughout.
//! An alternate hard-link backup survives success/failure for recoverability.
use super::{files, pack_rows, topology::Topology, valid_oid};
use crate::{connections, remote};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_PACK_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub repo: PathBuf,
    profile: connections::Context,
    primary: PathBuf,
    alternate: Vec<u8>,
    topology: Topology,
    refs: Vec<u8>,
}
fn references(repo: &Path, active: &dyn Fn() -> bool) -> Result<Vec<u8>, String> {
    let mut snapshot = Vec::new();
    for args in [
        vec![
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
        ],
        vec!["rev-parse", "--verify", "HEAD"],
        vec!["ls-files", "--stage", "-z"],
        vec!["reflog", "show", "--all", "--format=%H"],
        vec!["worktree", "list", "--porcelain"],
    ] {
        snapshot.extend(super::bytes(repo, &args, active)?);
        snapshot.push(0xff);
    }
    Ok(snapshot)
}
pub(super) fn prepare(
    repo: &Path,
    primary: &Path,
    topology: &Topology,
    active: &dyn Fn() -> bool,
) -> Result<Snapshot, String> {
    let Some(alternate) = topology.alternates.as_ref().filter(|s| !s.is_empty()) else {
        return Err("This repository has no primary alternate link to remove.".into());
    };
    if topology.promisor
        || topology
            .stores
            .iter()
            .any(|s| !s.available || s.error.is_some())
        || !topology.warnings.is_empty()
    {
        return Err("Make independent is unavailable: resolve missing donors, malformed/cyclic alternates or promisor/HTTP-alternate state first. No download or donor repair is attempted.".into());
    }
    if topology
        .worktrees
        .lines()
        .filter(|s| s.starts_with("worktree "))
        .count()
        != 1
    {
        return Err("Make independent requires a single-worktree clone. Other worktrees share these objects; close/remove them with Git before preparing independence.".into());
    }
    let own = super::line(repo, &["rev-parse", "--absolute-git-dir"], active)?;
    let common = super::line(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        active,
    )?;
    if own != common {
        return Err("Make independent is unavailable in a linked worktree; its object store belongs to the common repository.".into());
    }
    for store in &topology.stores {
        for pack in files::packs(&store.path, 64, active)? {
            if files::size(&pack.with_extension("promisor"), active)?.is_some() {
                return Err("A promisor pack is present. Make independent requires complete local objects and never fetches them.".into());
            }
        }
    }
    for path in [
        primary.to_owned(),
        remote::join(primary, "info"),
        remote::join(primary, "info/alternates"),
    ] {
        if remote::try_is_link(&path).map_err(|e| e.to_string())? {
            return Err("Make independent refuses symlinked object directories/info/alternate files. Inspect and resolve ownership with Git first.".into());
        }
    }
    Ok(Snapshot {
        repo: repo.into(),
        profile: connections::current(),
        primary: primary.into(),
        alternate: alternate.clone(),
        topology: topology.clone(),
        refs: references(repo, active)?,
    })
}
fn current(snapshot: &Snapshot, active: &dyn Fn() -> bool) -> Result<(), String> {
    if snapshot.profile != connections::current() {
        return Err("The profile changed; prepare independence again.".into());
    }
    let topology = super::topology::read(&snapshot.repo, &snapshot.primary, active)?;
    if topology != snapshot.topology || references(&snapshot.repo, active)? != snapshot.refs {
        return Err("Refs, index, reflogs, worktrees or alternate donors changed; inspect again before making this clone independent.".into());
    }
    Ok(())
}
fn required(repo: &Path, active: &dyn Fn() -> bool) -> Result<BTreeSet<String>, String> {
    let out = super::text(
        repo,
        &[
            "rev-list",
            "--objects",
            "--all",
            "--reflog",
            "--indexed-objects",
            "--missing=print",
            "--no-object-names",
        ],
        active,
    )?;
    let mut objects = BTreeSet::new();
    for line in out.lines() {
        if !valid_oid(line) {
            return Err("Required objects are missing, promised or unreadable. Borrowing is preserved; restore the donor or complete this clone with Git first.".into());
        }
        objects.insert(line.into());
    }
    if objects.is_empty() {
        return Err("No reachable objects were found; independence is not prepared for an empty/unborn repository.".into());
    }
    Ok(objects)
}
fn copy_pack(repo: &Path) -> Result<String, String> {
    // No --thin and no object/delta reuse: every base is inside the produced pack.
    let pack = super::super::command::bounded::run(
        repo,
        &[
            "--no-lazy-fetch",
            "--no-replace-objects",
            "-c",
            "pack.threads=2",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "pack-objects",
            "--stdout",
            "--all",
            "--reflog",
            "--indexed-objects",
            "--revs",
            "--no-reuse-delta",
            "--no-reuse-object",
        ],
        Vec::new(),
        MAX_PACK_BYTES,
    )?;
    if !pack.starts_with(b"PACK") {
        return Err("Git did not produce a complete pack; borrowing is preserved.".into());
    }
    let indexed = super::super::command::bounded::run(
        repo,
        &[
            "--no-lazy-fetch",
            "--no-replace-objects",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "index-pack",
            "--stdin",
            "--strict",
        ],
        pack,
        4096,
    )?;
    let out = String::from_utf8(indexed).map_err(|_| "Invalid index-pack response")?;
    let oid = out
        .trim()
        .strip_prefix("pack\t")
        .ok_or("Git did not confirm an imported pack")?;
    if !valid_oid(oid) {
        return Err("Invalid imported pack ID".into());
    }
    Ok(oid.into())
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(primary: &Path) -> Result<Self, String> {
        let path = remote::join(primary, "info/gitcraft-independent.lock");
        if let Some(t) = remote::parse(&path) {
            let out = remote::exec(
                &t.host,
                &remote::Request {
                    script: format!("umask 077; mkdir -- {}", remote::sh_path(&t.path)),
                    timeout: Some(Duration::from_secs(20)),
                    ..Default::default()
                },
            )?;
            if out.code != 0 {
                return Err("Another independence action or retained lock exists. Inspect objects/info/gitcraft-independent.lock before retrying.".into());
            }
        } else {
            std::fs::create_dir(&path).map_err(|e| {
                format!(
                    "Could not acquire independence lock ({}): {e}",
                    path.display()
                )
            })?;
        }
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(t) = remote::parse(&self.0) {
            let _ = remote::exec(
                &t.host,
                &remote::Request {
                    script: format!("rmdir -- {}", remote::sh_path(&t.path)),
                    timeout: Some(Duration::from_secs(20)),
                    ..Default::default()
                },
            );
        } else {
            let _ = std::fs::remove_dir(&self.0);
        }
    }
}
fn detach(snapshot: &Snapshot, backup: &Path) -> Result<(), String> {
    let alternate = remote::join(&snapshot.primary, "info/alternates");
    if let Some(t) = remote::parse(&alternate) {
        let p = remote::sh_path(&t.path);
        let b = remote::sh_path(&remote::parse(backup).ok_or("Invalid backup host")?.path);
        let request = remote::Request {
            script: format!(
                "[ ! -L {p} ] && [ -f {p} ] || exit 1\n[ ! -e {b} ] || exit 1\ncmp -s -- {p} - || {{ echo 'Alternates changed; borrowing retained' >&2; exit 1; }}\nln -- {p} {b} || exit 1\n[ {p} -ef {b} ] || exit 1\nrm -- {p}"
            ),
            stdin: Some(snapshot.alternate.clone()),
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        };
        let out = remote::exec(&t.host, &request)?;
        if out.code != 0 {
            return Err(format!(
                "Could not remove the guarded alternate link; inspect {} and {}. {}",
                remote::display(&alternate),
                remote::display(backup),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    } else {
        if std::fs::symlink_metadata(&alternate)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Alternate link changed to a symlink".into());
        }
        if files::read(&alternate, 64 * 1024, &|| true)?.as_ref() != Some(&snapshot.alternate) {
            return Err("Alternates changed; borrowing retained".into());
        }
        std::fs::hard_link(&alternate, backup).map_err(|e| e.to_string())?;
        if files::read(backup, 64 * 1024, &|| true)?.as_ref() != Some(&snapshot.alternate)
            || files::read(&alternate, 64 * 1024, &|| true)?.as_ref() != Some(&snapshot.alternate)
        {
            return Err("Alternates changed; borrowing retained and backup preserved".into());
        }
        std::fs::remove_file(alternate).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn restore(snapshot: &Snapshot, backup: &Path) -> Result<(), String> {
    let alternate = remote::join(&snapshot.primary, "info/alternates");
    if let Some(t) = remote::parse(&alternate) {
        let b = remote::sh_path(&remote::parse(backup).ok_or("Invalid backup host")?.path);
        let out = remote::exec(
            &t.host,
            &remote::Request {
                script: format!("ln -- {b} {}", remote::sh_path(&t.path)),
                timeout: Some(Duration::from_secs(20)),
                ..Default::default()
            },
        )?;
        if out.code != 0 {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
    } else {
        std::fs::hard_link(backup, alternate).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn make_independent(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    make_with_checkpoints(snapshot, active, &mut |_| Ok(()))
}
pub(super) fn make_with_checkpoints(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
    checkpoint: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    let _lock = Lock::acquire(&snapshot.primary)?;
    current(snapshot, active)?;
    let before = required(&snapshot.repo, active)?;
    let imported = copy_pack(&snapshot.repo)?;
    let index = remote::join(&snapshot.primary, &format!("pack/pack-{imported}.idx"));
    let rows = pack_rows(&super::text(
        &snapshot.repo,
        &["verify-pack", "-v", "--", &remote::git_path(&index)],
        active,
    )?)?;
    if !before.iter().all(|oid| rows.contains_key(oid)) {
        return Err("The copied self-contained pack does not contain every required object. Borrowing is preserved; copied objects remain safe to keep.".into());
    }
    checkpoint("copied")?;
    current(snapshot, active)?;
    if required(&snapshot.repo, active)? != before {
        return Err(
            "Required objects changed during copying; borrowing is preserved. Inspect again."
                .into(),
        );
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let backup = remote::join(
        &snapshot.primary,
        &format!(
            "info/alternates.gitcraft-backup-{nonce}-{}",
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
    );
    detach(snapshot, &backup)?;
    // Verify the clone with the alternate gone. Cancellation after detaching also restores it.
    let proof = (|| {
        checkpoint("detached")?;
        if required(&snapshot.repo, active)? != before
            || references(&snapshot.repo, active)? != snapshot.refs
        {
            return Err("Refs/index/reflogs changed during the final proof".to_owned());
        }
        // Read-only full connectivity check includes all refs/index/reflogs. No repair/prune.
        super::bytes(
            &snapshot.repo,
            &["fsck", "--connectivity-only", "--no-dangling"],
            active,
        )?;
        Ok(())
    })();
    if let Err(error) = proof {
        let recovery = match restore(snapshot, &backup) {
            Ok(()) => "Borrowing was restored.".into(),
            Err(e) => format!(
                "Automatic restore failed ({e}). Restore objects/info/alternates from this retained backup before continuing: {}",
                remote::display(&backup)
            ),
        };
        return Err(format!(
            "Independence proof failed: {error}. {recovery} Backup: {}",
            remote::display(&backup)
        ));
    }
    Ok(format!(
        "Clone is independent: {} required objects copied and verified. Donors were not modified. Recovery backup: {}",
        before.len(),
        remote::display(&backup)
    ))
}
