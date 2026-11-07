//! Deliberate independence copies a self-contained pack; donors are read-only throughout.
//! The actual alternate inode is retained by a guarded create-only rename.
use super::super::{files, pack_rows, topology::Topology, valid_oid};
use crate::connections;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
#[path = "held.rs"]
mod held;
use held::{Ownership, Stage};

const MAX_PACK_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub repo: PathBuf,
    profile: connections::Context,
    primary: PathBuf,
    ownership: Arc<Ownership>,
    topology: Topology,
    refs: Vec<u8>,
}
fn references(
    repo: &Path,
    ownership: &Ownership,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
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
        snapshot.extend(ownership.run(repo, &args, Vec::new(), 16 * 1024 * 1024, None, active)?);
        snapshot.push(0xff);
    }
    Ok(snapshot)
}
pub(crate) fn prepare(
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
    let own = super::super::line(repo, &["rev-parse", "--absolute-git-dir"], active)?;
    let common = super::super::line(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        active,
    )?;
    if own != common {
        return Err("Make independent is unavailable in a linked worktree; its object store belongs to the common repository.".into());
    }
    let ownership = Arc::new(Ownership::capture(
        repo,
        Path::new(&own),
        primary,
        alternate,
    )?);
    for store in &topology.stores {
        for pack in files::packs(&store.path, 64, active)? {
            if files::size(&pack.with_extension("promisor"), active)?.is_some() {
                return Err("A promisor pack is present. Make independent requires complete local objects and never fetches them.".into());
            }
        }
    }
    Ok(Snapshot {
        repo: repo.into(),
        profile: connections::current(),
        primary: primary.into(),
        ownership: ownership.clone(),
        topology: topology.clone(),
        refs: references(repo, &ownership, active)?,
    })
}
fn current(snapshot: &Snapshot, active: &dyn Fn() -> bool) -> Result<(), String> {
    if snapshot.profile != connections::current() {
        return Err("The profile changed; prepare independence again.".into());
    }
    snapshot.ownership.current(true)?;
    let topology = super::super::topology::read(&snapshot.repo, &snapshot.primary, active)?;
    if topology != snapshot.topology
        || references(&snapshot.repo, &snapshot.ownership, active)? != snapshot.refs
    {
        return Err("Refs, index, reflogs, worktrees or alternate donors changed; inspect again before making this clone independent.".into());
    }
    snapshot.ownership.current(true)
}
fn required(
    repo: &Path,
    ownership: &Ownership,
    active: &dyn Fn() -> bool,
) -> Result<BTreeSet<String>, String> {
    let out = ownership.run(
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
        Vec::new(),
        16 * 1024 * 1024,
        None,
        active,
    )?;
    let out = String::from_utf8(out).map_err(|_| "Invalid required object list")?;
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
fn copy_pack(
    repo: &Path,
    ownership: &Ownership,
    stage: &Stage,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    // No --thin and no object/delta reuse: every base is inside the produced pack.
    let pack = ownership.run(
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
        None,
        active,
    )?;
    if !pack.starts_with(b"PACK") {
        return Err("Git did not produce a complete pack; borrowing is preserved.".into());
    }
    let indexed = ownership.run(
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
        Some(stage),
        active,
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

pub(crate) fn make_independent(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    make_with_checkpoints(snapshot, active, &mut |_| Ok(()))
}
pub(crate) fn make_with_checkpoints(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
    checkpoint: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    let owner = &snapshot.ownership;
    let _lock = owner.lock()?;
    current(snapshot, active)?;
    let before = required(&snapshot.repo, owner, active)?;
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stage_name = format!(
        "gitcraft-independent-stage-{nonce}-{}",
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let stage = owner.stage(&stage_name)?;
    let imported = copy_pack(&snapshot.repo, owner, &stage, active)?;
    let index = stage.index(&imported);
    let output = owner.run(
        &snapshot.repo,
        &[
            "verify-pack",
            "-v",
            "--",
            index.to_str().ok_or("Invalid stage path")?,
        ],
        Vec::new(),
        16 * 1024 * 1024,
        None,
        active,
    )?;
    let rows = pack_rows(std::str::from_utf8(&output).map_err(|_| "Invalid pack metadata")?)?;
    if !before.iter().all(|oid| rows.contains_key(oid)) {
        return Err("Copied pack does not contain every required object; borrowing is preserved and the private stage remains available.".into());
    }
    checkpoint("copied")?;
    current(snapshot, active)?;
    if required(&snapshot.repo, owner, active)? != before {
        return Err("Required objects changed during copying; borrowing is preserved.".into());
    }
    owner.install(&stage, &imported)?;
    current(snapshot, active)?;
    // Fixed-candidate tests can exercise ownership changes after final verification.
    checkpoint("verified")?;
    let backup_name = format!("alternates.gitcraft-backup-{nonce}");
    if let Err(error) = owner.detach(&backup_name) {
        let restored = owner.restore(&backup_name);
        return Err(format!(
            "Guarded alternate removal failed: {error}. Create-only recovery: {restored:?}. Recovery backup location (if created): {}",
            owner.recovery_path(&backup_name).display()
        ));
    }
    let proof = (|| {
        checkpoint("detached")?;
        owner.current(false)?;
        owner.verify_backup(&backup_name)?;
        if required(&snapshot.repo, owner, active)? != before
            || references(&snapshot.repo, owner, active)? != snapshot.refs
        {
            return Err("Refs/index/reflogs changed during final proof".into());
        }
        owner.run(
            &snapshot.repo,
            &["fsck", "--connectivity-only", "--no-dangling"],
            Vec::new(),
            16 * 1024 * 1024,
            None,
            active,
        )?;
        owner.current(false)?;
        owner.verify_backup(&backup_name)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = proof {
        let recovery = match owner.restore(&backup_name) {
            Ok(()) => "Borrowing was restored.".into(),
            Err(e) => format!("Create-only restoration did not overwrite a newer file: {e}"),
        };
        return Err(format!(
            "Independence proof failed: {error}. {recovery} Retained backup: {}",
            owner.recovery_path(&backup_name).display()
        ));
    }
    Ok(format!(
        "Clone is independent: {} required objects copied and verified. Donors were not modified. Recovery backup: {}. Private recovery stage: {}",
        before.len(),
        owner.recovery_path(&backup_name).display(),
        stage_name
    ))
}
