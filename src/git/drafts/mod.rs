//! Opt-in local history of working changes. Roots are trees (not WIP commits); separate
//! index/work trees keep staged contents distinct and make every saved blob GC-reachable.
mod context;
pub mod deferred;
pub(in crate::git) mod index_transaction;
pub mod lists;
mod native_files;
mod permissions;
pub mod publication;
mod restore;
#[cfg(test)]
mod tests;
pub mod turn;
pub mod virtuals;
mod working_transaction;

use crate::{git, remote};
pub(in crate::git) use context::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub use context::{
    configure as set_enabled, configured as enabled, configured_with as enabled_while,
};
pub use restore::{RestoreHunk, RestorePreview, preview, restore};
const MAX_INDEX: usize = 20_000;
const MAX_FILES: usize = 2_000;
pub const FILE_LIMIT: usize = 8 * 1024 * 1024;
const TOTAL_LIMIT: usize = 32 * 1024 * 1024;
const PREFIX: &str = "refs/gitcraft/drafts/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id {
    pub reference: String,
    pub tree: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub mode: String,
    pub oid: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub index: Option<Entry>,
    pub work: Option<Entry>,
    pub untracked: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub version: u32,
    pub created: i64,
    pub reason: String,
    pub worktree: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub index_tree: String,
    pub work_tree: String,
    pub files: Vec<File>,
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub id: Id,
    pub metadata: Metadata,
}
#[derive(Clone, Copy)]
pub struct Retention {
    pub count: usize,
    pub days: i64,
}
impl Default for Retention {
    fn default() -> Self {
        Self {
            count: 50,
            days: 14,
        }
    }
}

#[derive(PartialEq, Eq)]
struct Source {
    raw: Vec<u8>,
    cache: Vec<u8>,
    cache_snapshot: Option<std::sync::Arc<index_transaction::IndexSnapshot>>,
    shared: Option<(String, Vec<u8>)>,
    index: BTreeMap<String, Entry>,
    skip: BTreeSet<String>,
    head: Option<String>,
    branch: Option<String>,
}

pub fn list(repo: &Path, active: &dyn Fn() -> bool) -> Result<Vec<Draft>, String> {
    let ctx = Context::new(repo, active)?;
    let drafts = list_in(&ctx, active)?;
    let now = git::now();
    let policy = Retention::default();
    cleanup_in(&ctx, &drafts, now, policy, None, active)?;
    Ok(drafts
        .into_iter()
        .enumerate()
        .filter(|(i, d)| {
            *i < policy.count && now.saturating_sub(d.metadata.created) <= policy.days * 86400
        })
        .map(|(_, d)| d)
        .collect())
}

pub fn capture(
    repo: &Path,
    reason: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    capture_preserving(repo, reason, None, None, active)
}
fn capture_preserving(
    repo: &Path,
    reason: &str,
    keep: Option<&Id>,
    force_path: Option<&str>,
    requested: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    let paths = force_path.map(|p| vec![p.to_owned()]).unwrap_or_default();
    capture_preserving_paths(repo, reason, keep, &paths, requested)
}
fn capture_preserving_paths(
    repo: &Path,
    reason: &str,
    keep: Option<&Id>,
    force_paths: &[String],
    requested: &dyn Fn() -> bool,
) -> Result<Option<Draft>, String> {
    let start = std::time::Instant::now();
    let active = || requested() && start.elapsed() < std::time::Duration::from_secs(120);
    let ctx = Context::new(repo, &active)?;
    let before = source(&ctx, &active)?;
    let index_tree = tree(&ctx, &before.index, &active)?;
    cached_index(&ctx, &before, &active)?;
    let initial_changes = changed(&ctx, &before, &index_tree, &active)?;
    let mut candidates = initial_changes.clone();
    let untracked = untracked_paths(ctx.source(
        &["ls-files", "--others", "--exclude-standard", "-z"],
        vec![],
        &[0],
        &active,
    )?)?;
    candidates.extend(untracked.iter().cloned());
    for path in force_paths {
        valid_path(path)?;
        candidates.insert(path.to_owned());
    }
    // Materialized sparse entries can have edits hidden by skip-worktree.
    for path in &before.skip {
        if read_file(&ctx, path, &before.index[path].mode, &active)?.is_some() {
            candidates.insert(path.clone());
        }
    }
    if candidates.len() > MAX_FILES {
        return Err(crate::i18n::text(
            "backend-draft-has-over-2000-changed-untracked-files-make-a-manual-backup-b502fa",
        )
        .into());
    }
    if candidates.is_empty() {
        cleanup_in(
            &ctx,
            &list_in(&ctx, &active)?,
            git::now(),
            Retention::default(),
            keep,
            &active,
        )?;
        return Ok(None);
    }
    let mut work = before.index.clone();
    let mut files = Vec::new();
    let mut observed = BTreeMap::new();
    let mut total = 0;
    for path in candidates {
        let index = before.index.get(&path).cloned();
        if index.as_ref().is_some_and(|e| e.mode == "160000") {
            return Err(crate::i18n::text(
                "backend-drafts-exclude-submodule-contents-back-up-submodule-changes-separ-a309a6",
            )
            .into());
        }
        let data = read_file(
            &ctx,
            &path,
            index.as_ref().map_or("100644", |e| e.mode.as_str()),
            &active,
        )?;
        let entry = if let Some((mode, bytes)) = &data {
            total += bytes.len();
            if total > TOTAL_LIMIT {
                return Err(
                    crate::i18n::text("backend-draft-content-exceeds-32-mib-make-a-manual-backup-before-continui-5603e0").into(),
                );
            }
            Some(Entry {
                mode: mode.clone(),
                oid: ctx.blob(bytes.clone(), &active)?,
            })
        } else {
            None
        };
        match &entry {
            Some(e) => {
                work.insert(path.clone(), e.clone());
            }
            None => {
                work.remove(&path);
            }
        }
        files.push(File {
            path: path.clone(),
            index,
            work: entry,
            untracked: untracked.contains(&path),
        });
        observed.insert(path, data);
    }
    let work_tree = tree(&ctx, &work, &active)?;
    if source(&ctx, &active)? != before {
        return Err(crate::i18n::text(
            "backend-head-or-index-changed-while-making-a-draft-it-will-retry-after-a-c7f805",
        )
        .into());
    }
    tree(&ctx, &before.index, &active)?;
    cached_index(&ctx, &before, &active)?;
    if changed(&ctx, &before, &index_tree, &active)? != initial_changes
        || untracked_paths(ctx.source(
            &["ls-files", "--others", "--exclude-standard", "-z"],
            vec![],
            &[0],
            &active,
        )?)? != untracked
    {
        return Err(crate::i18n::text(
            "backend-working-changes-moved-while-making-a-draft-it-will-retry-after-a-1f8fd4",
        )
        .into());
    }
    for (path, expected) in observed {
        if read_file(
            &ctx,
            &path,
            before
                .index
                .get(&path)
                .map_or("100644", |e| e.mode.as_str()),
            &active,
        )? != expected
        {
            return Err(crate::i18n::text(
                "backend-a-working-file-changed-while-making-a-draft-it-will-retry-after-a-3a7281",
            )
            .into());
        }
    }
    let existing = list_in(&ctx, &active)?;
    if let Some(last) = existing.first().filter(|d| {
        git::now().saturating_sub(d.metadata.created) <= Retention::default().days * 86400
            && d.metadata.index_tree == index_tree
            && d.metadata.work_tree == work_tree
            && d.metadata.head == before.head
            && d.metadata.branch == before.branch
            && d.metadata.files == files
    }) {
        cleanup_in(
            &ctx,
            &existing,
            git::now(),
            Retention::default(),
            keep,
            &active,
        )?;
        return Ok(Some(last.clone()));
    }
    let metadata = Metadata {
        version: 1,
        created: git::now(),
        reason: reason
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect(),
        worktree: ctx.id.clone(),
        head: before.head,
        branch: before.branch,
        index_tree,
        work_tree,
        files,
    };
    let bytes = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::text(
            "backend-draft-metadata-exceeds-1-mib-make-a-manual-backup-91327e",
        )
        .into());
    }
    let meta = ctx.blob(bytes, &active)?;
    let input = format!(
        "100644 blob {meta}\tmetadata.json\0\
        040000 tree {}\tindex\0\
        040000 tree {}\twork\0",
        metadata.index_tree, metadata.work_tree
    )
    .into_bytes();
    let root = ctx.text(&["mktree", "-z"], input, &active)?;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let reference = format!(
        "{PREFIX}{}/{nanos:039}-{:010}-{:010}",
        ctx.id,
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let id = Id {
        reference,
        tree: root,
    };
    ctx.source(
        &["update-ref", "--no-deref", "--stdin"],
        format!("create {} {}\n", id.reference, id.tree).into_bytes(),
        &[0],
        &active,
    )?;
    let draft = Draft { id, metadata };
    cleanup_in(
        &ctx,
        &existing,
        git::now(),
        Retention {
            count: Retention::default().count.saturating_sub(1),
            ..Default::default()
        },
        keep,
        &active,
    )?;
    Ok(Some(draft))
}

fn source(ctx: &Context, active: &dyn Fn() -> bool) -> Result<Source, String> {
    let raw = ctx.source(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)?;
    let (mut index, mut skip) = (BTreeMap::new(), BTreeSet::new());
    for record in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = std::str::from_utf8(record)
            .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
        let (info, path) = record
            .split_once('\t')
            .ok_or(crate::i18n::text("backend-cannot-read-index-entry-6979bd"))?;
        valid_path(path)?;
        let parts: Vec<_> = info.split_whitespace().collect();
        let [tag, mode, oid, stage] = parts.as_slice() else {
            return Err(crate::i18n::text("backend-cannot-read-index-entry-6979bd").into());
        };
        if *stage != "0" {
            return Err(crate::i18n::text(
                "backend-drafts-are-unavailable-while-the-index-contains-conflicts-resolve-6a5dd9",
            )
            .into());
        }
        if !matches!(*mode, "100644" | "100755" | "120000" | "160000") || !oid_valid(oid) {
            return Err(
                crate::i18n::text("backend-unsupported-index-entry-for-drafts-ed55d1").into(),
            );
        }
        if tag.eq_ignore_ascii_case("S") {
            skip.insert(path.to_owned());
        }
        index.insert(
            path.to_owned(),
            Entry {
                mode: (*mode).into(),
                oid: (*oid).into(),
            },
        );
        if index.len() > MAX_INDEX {
            return Err(crate::i18n::text(
                "backend-drafts-support-at-most-20000-index-entries-bd3f0a",
            )
            .into());
        }
    }
    let text = |args: &[&str]| -> Result<Option<String>, String> {
        let bytes = ctx.source(args, vec![], &[0, 1], active)?;
        let s = String::from_utf8(bytes)
            .map_err(|_| crate::i18n::text("backend-invalid-head-e1661b"))?;
        Ok((!s.trim().is_empty()).then(|| s.trim().to_owned()))
    };
    let cache = internal_file(ctx, &ctx.index_file, active)?.unwrap_or_default();
    let cache_snapshot =
        if !remote::is_remote(&ctx.repo) && cfg!(any(target_os = "macos", target_os = "linux")) {
            let snapshot = index_transaction::IndexSnapshot::capture(Path::new(&ctx.index_file))?;
            if snapshot.as_ref().map(|s| s.bytes()).unwrap_or_default() != cache {
                return Err(crate::i18n::text(
                    "backend-index-policy-changed-while-reading-source-3a58e5",
                )
                .into());
            }
            snapshot.map(std::sync::Arc::new)
        } else {
            None
        };
    Ok(Source {
        cache,
        cache_snapshot,
        shared: if ctx.shared_file.is_empty() {
            None
        } else {
            let name = ctx
                .shared_file
                .rsplit(['/', '\\'])
                .next()
                .ok_or(crate::i18n::text(
                    "backend-invalid-shared-index-path-cecec6",
                ))?;
            if !name.strip_prefix("sharedindex.").is_some_and(oid_valid) {
                return Err(crate::i18n::text("backend-invalid-shared-index-path-cecec6").into());
            }
            Some((
                name.into(),
                internal_file(ctx, &ctx.shared_file, active)?
                    .ok_or(crate::i18n::text("backend-shared-index-disappeared-8b3784"))?,
            ))
        },
        raw,
        index,
        skip,
        head: text(&["rev-parse", "--verify", "-q", "HEAD"])?,
        branch: text(&["symbolic-ref", "-q", "HEAD"])?,
    })
}

/// Copy metadata only, without following an index/cache link or persisting its contents
/// in the draft's object graph. The copied stat cache avoids hashing clean large files.
fn internal_file(
    ctx: &Context,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<Vec<u8>>, String> {
    const LIMIT: u64 = 32 * 1024 * 1024;
    if let Some(target) = remote::parse(&ctx.repo) {
        let (parent, name) = path.rsplit_once('/').ok_or(crate::i18n::text(
            "backend-invalid-git-metadata-path-f4a8cd",
        ))?;
        let script = read_script(parent, name, LIMIT)?;
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script,
                ..Default::default()
            },
            active,
        )?;
        if out.code == 0 && out.stdout == b"M\n" {
            return Ok(None);
        }
        if out.code == 0
            && out.stdout.len() >= 8
            && matches!(&out.stdout[..8], b"F100644\n" | b"F100755\n")
            && out.stdout.len() as u64 <= LIMIT + 8
        {
            return Ok(Some(out.stdout[8..].to_vec()));
        }
        return Err(crate::i18n::text(
            "backend-cannot-read-a-regular-index-cache-file-within-32-mib-136f8d",
        )
        .into());
    }
    let path = Path::new(path);
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err(crate::i18n::text(
                "backend-index-cache-links-and-special-files-are-not-supported-fcc001",
            )
            .into());
        }
        _ => {}
    }
    let parent = path
        .parent()
        .ok_or(crate::i18n::text("backend-invalid-index-path-d4427c"))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(crate::i18n::text("backend-index-path-is-not-utf-8-4ef0dd"))?;
    crate::safe_read::read_inside(parent, name, LIMIT)
        .map(Some)
        .ok_or_else(|| {
            crate::i18n::text("backend-index-cache-file-is-unreadable-or-exceeds-32-mib-0ad77d")
                .into()
        })
}

fn private_index_file(
    ctx: &Context,
    name: &str,
    source_path: &str,
    bytes: &[u8],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let path = ctx.path.join(name);
    if let Some(target) = remote::parse(&path) {
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script: format!(
                    "umask 077; cat > {target} && test -f {source} && test ! -L {source} && touch -r {source} {target}",
                    target = remote::sh_path(&target.path),
                    source = remote::sh_path(source_path),
                ),
                stdin: Some(bytes.to_vec()),
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err(
                crate::i18n::text("backend-cannot-copy-the-private-index-cache-6185c5").into(),
            );
        }
    } else {
        let metadata = std::fs::symlink_metadata(source_path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(crate::i18n::text(
                "backend-index-cache-links-and-special-files-are-not-supported-fcc001",
            )
            .into());
        }
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_times(std::fs::FileTimes::new().set_modified(modified)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Concrete read-only subtree consumer: copy the existing index and split dependency
/// into the empty private context, using the same stat-cache checks as draft capture.
pub(in crate::git) fn inspection_index(
    ctx: &Context,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    cached_index(ctx, &source(ctx, active)?, active)
}

fn cached_index(ctx: &Context, state: &Source, active: &dyn Fn() -> bool) -> Result<(), String> {
    if !state.cache.is_empty() {
        private_index_file(ctx, "index", &ctx.index_file, &state.cache, active)?;
    } else if !state.raw.is_empty() {
        return Err(crate::i18n::text(
            "backend-index-disappeared-while-preparing-the-draft-1afbba",
        )
        .into());
    }
    if let Some((name, bytes)) = &state.shared {
        private_index_file(ctx, name, &ctx.shared_file, bytes, active)?;
    }
    if ctx.run(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)? != state.raw {
        return Err(crate::i18n::text(
            "backend-index-moved-while-copying-its-private-stat-cache-3454e3",
        )
        .into());
    }
    // Preserve the source index's mtime when copying: advancing it makes Git skip
    // its racy-stat content check for same-size edits made in the original tick.
    // The source cache is immutable. Drop daemon/UC state and materialize any split
    // index only in the private copy before asking about working changes.
    ctx.run(
        &[
            "update-index",
            "--no-fsmonitor",
            "--no-untracked-cache",
            "--no-split-index",
        ],
        vec![],
        &[0],
        active,
    )?;
    let mut assume = Vec::new();
    for record in state.raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        if record.first().is_some_and(u8::is_ascii_lowercase) {
            let record = std::str::from_utf8(record)
                .map_err(|_| crate::i18n::text("backend-invalid-index-bb1331"))?;
            let (_, path) = record
                .split_once('\t')
                .ok_or(crate::i18n::text("backend-invalid-index-bb1331"))?;
            assume.extend_from_slice(path.as_bytes());
            assume.push(0);
        }
    }
    if !assume.is_empty() {
        ctx.run(
            &["update-index", "--no-assume-unchanged", "-z", "--stdin"],
            assume,
            &[0],
            active,
        )?;
    }
    Ok(())
}

fn tree(
    ctx: &Context,
    entries: &BTreeMap<String, Entry>,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    ctx.run(&["read-tree", "--empty"], vec![], &[0], active)?;
    let input = entries
        .iter()
        .flat_map(|(p, e)| format!("{} {}\t{p}\0", e.mode, e.oid).into_bytes())
        .collect();
    ctx.run(&["update-index", "-z", "--index-info"], input, &[0], active)?;
    ctx.text(&["write-tree", "--missing-ok"], vec![], active)
}
fn changed(
    ctx: &Context,
    state: &Source,
    index_tree: &str,
    active: &dyn Fn() -> bool,
) -> Result<BTreeSet<String>, String> {
    let mut result = paths(ctx.run(
        &[
            "diff-files",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
        ],
        vec![],
        &[0],
        active,
    )?)?;
    if let Some(head) = &state.head {
        result.extend(paths(ctx.run(
            &[
                "diff-tree",
                "--no-commit-id",
                "-r",
                "--name-only",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                head,
                index_tree,
            ],
            vec![],
            &[0],
            active,
        )?)?);
    } else {
        result.extend(state.index.keys().cloned());
    }
    Ok(result)
}
fn paths(bytes: Vec<u8>) -> Result<BTreeSet<String>, String> {
    bytes
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|p| {
            let p = std::str::from_utf8(p)
                .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
            valid_path(p)?;
            Ok(p.to_owned())
        })
        .collect()
}
// Native ls-files lists an untracked nested working repository as a directory
// marker. Exclude its contents, rather than descending into another repository.
fn untracked_paths(bytes: Vec<u8>) -> Result<BTreeSet<String>, String> {
    let mut result = BTreeSet::new();
    for name in bytes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let name = std::str::from_utf8(name)
            .map_err(|_| crate::i18n::text("backend-draft-paths-must-be-utf-8-7ae280"))?;
        if let Some(directory) = name.strip_suffix('/') {
            valid_path(directory)?;
        } else {
            valid_path(name)?;
            result.insert(name.to_owned());
        }
    }
    Ok(result)
}

/// Missing is a distinct, verified result, not a failed read through a link or
/// unsearchable parent. The final read still opens and verifies its own handle.
fn read_script(root: &str, rel: &str, limit: u64) -> Result<String, String> {
    valid_path(rel)?;
    let names: Vec<_> = rel.split('/').collect();
    let mut script = format!(
        "cd -P -- {} || exit 1\nroot=$(pwd -P) || exit 1\n[ \"$root\" = / ] && root=\n",
        remote::sh_path(root)
    );
    let mut at = String::new();
    for (i, name) in names.iter().enumerate() {
        let q = remote::sh_quote(name);
        script.push_str(&format!("[ -x . ] || exit 1\n[ -L {q} ] && exit 1\nif [ ! -e {q} ]; then printf 'M\\n'; exit 0; fi\n"));
        if i + 1 < names.len() {
            at.push('/');
            at.push_str(name);
            script.push_str(&format!(
                "cd -P -- {q} || exit 1\n[ \"$(pwd -P)\" = \"$root\"{} ] || exit 1\n",
                remote::sh_quote(&at)
            ));
        } else {
            script.push_str(&format!(
                "if [ -x {q} ]; then printf 'F100755\\n'; else printf 'F100644\\n'; fi\n"
            ));
        }
    }
    script.push_str(
        &remote::read_within_script(root, rel, limit)
            .ok_or(crate::i18n::text("backend-unsafe-draft-path-12307a"))?,
    );
    Ok(script)
}

#[cfg(unix)]
fn missing_inside(root: &Path, rel: &str) -> Result<(), String> {
    use rustix::fs::{AtFlags, Mode, OFlags, open, openat, statat};
    let names: Vec<_> = rel.split('/').collect();
    let (last, folders) = names
        .split_last()
        .ok_or(crate::i18n::text("backend-invalid-draft-path-58bc4d"))?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = open(root, flags - OFlags::NOFOLLOW, Mode::empty()).map_err(|e| e.to_string())?;
    for folder in folders {
        match openat(&dir, *folder, flags, Mode::empty()) {
            Ok(next) => dir = next,
            Err(rustix::io::Errno::NOENT) => return Ok(()),
            Err(e) => {
                return Err(crate::i18n::message(
                    "backend-cannot-verify-missing-draft-path-e-599e53",
                    &[("detail", (e).to_string().into())],
                ));
            }
        }
    }
    match statat(&dir, *last, AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err(
            crate::i18n::text("backend-draft-path-changed-or-cannot-be-safely-read-7b72b8").into(),
        ),
    }
}
#[cfg(not(unix))]
fn missing_inside(root: &Path, rel: &str) -> Result<(), String> {
    let mut path = root.to_path_buf();
    for component in rel.split('/') {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err(crate::i18n::text(
                            "backend-draft-path-crosses-a-reparse-point-dd9a8e",
                        )
                        .into());
                    }
                }
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(crate::i18n::text(
                        "backend-draft-path-changed-or-crosses-a-link-7acb39",
                    )
                    .into());
                }
            }
        }
    }
    Err(crate::i18n::text("backend-draft-path-changed-retry-after-a-pause-e68c36").into())
}

fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || path
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | "..") || p.eq_ignore_ascii_case(".git"))
    {
        Err(crate::i18n::message(
            "backend-drafts-cannot-safely-read-this-path-path-6f6fed",
            &[("path", format!("{:?}", path).into())],
        ))
    } else {
        Ok(())
    }
}
fn oid_valid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

fn read_file(
    ctx: &Context,
    path: &str,
    fallback: &str,
    active: &dyn Fn() -> bool,
) -> Result<Option<(String, Vec<u8>)>, String> {
    read_file_limited(ctx, path, fallback, FILE_LIMIT, active)
}
fn read_file_limited(
    ctx: &Context,
    path: &str,
    fallback: &str,
    limit: usize,
    active: &dyn Fn() -> bool,
) -> Result<Option<(String, Vec<u8>)>, String> {
    valid_path(path)?;
    let limit = limit.min(FILE_LIMIT);
    if !active() {
        return Err("Draft cancelled".into());
    }
    if let Some(target) = remote::parse(&ctx.repo) {
        let script = read_script(&ctx.worktree, path, limit as u64)?;
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                cwd: Some(ctx.worktree.clone()),
                script,
                ..Default::default()
            },
            active,
        )?;
        if out.code == 0 && out.stdout == b"M\n" {
            return Ok(None);
        }
        if out.code == 0
            && out.stdout.len() >= 8
            && out.stdout.len() - 8 <= limit
            && (out.stdout.starts_with(b"F100644\n") || out.stdout.starts_with(b"F100755\n"))
        {
            return Ok(Some((
                String::from_utf8_lossy(&out.stdout[1..7]).into_owned(),
                out.stdout[8..].to_vec(),
            )));
        }
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-only-regular-files-within-8-mib-are-supported-l-80a0a0",
            &[("path", (path).to_string().into())],
        ));
    }
    let root = Path::new(&ctx.worktree);
    let meta = match std::fs::symlink_metadata(root.join(path)) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return missing_inside(root, path).map(|()| None);
        }
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-symbolic-links-submodules-and-special-files-are-b6ff13",
            &[("path", (path).to_string().into())],
        ));
    }
    if meta.len() > limit as u64 {
        return Err(crate::i18n::message(
            "backend-cannot-draft-path-exceeds-remaining-snapshot-allowance-of-limit-b-dc54e5",
            &[
                ("path", (path).to_string().into()),
                ("limit", (limit).to_string().into()),
            ],
        ));
    }
    let bytes = crate::safe_read::read_inside(root, path, limit as u64).ok_or_else(|| {
        crate::i18n::message(
            "backend-cannot-draft-path-file-changed-is-linked-or-exceeds-its-remaining-294df6",
            &[("path", (path).to_string().into())],
        )
    })?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        }
    };
    #[cfg(not(unix))]
    let mode = fallback;
    #[cfg(unix)]
    let _ = fallback;
    Ok(Some((mode.into(), bytes)))
}

fn list_in(ctx: &Context, active: &dyn Fn() -> bool) -> Result<Vec<Draft>, String> {
    let namespace = format!("{PREFIX}{}/", ctx.id);
    let refs = ctx.source(
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            &namespace,
        ],
        vec![],
        &[0],
        active,
    )?;
    let refs = std::str::from_utf8(&refs)
        .map_err(|_| crate::i18n::text("backend-invalid-draft-refs-3d3d89"))?;
    if refs.lines().count() > 200 {
        return Err(crate::i18n::text(
            "backend-too-many-draft-refs-inspect-refs-gitcraft-drafts-in-the-terminal-814e26",
        )
        .into());
    }
    let mut drafts = Vec::new();
    for line in refs.lines() {
        let Some((reference, tree)) = line.split_once(' ') else {
            continue;
        };
        let id = Id {
            reference: reference.into(),
            tree: tree.into(),
        };
        if let Ok(metadata) = metadata(ctx, &id, active) {
            drafts.push(Draft { id, metadata });
        }
        if drafts.len() > 200 {
            return Err(crate::i18n::text(
                "backend-too-many-draft-refs-inspect-refs-gitcraft-drafts-in-the-terminal-814e26",
            )
            .into());
        }
    }
    drafts.sort_by(|a, b| {
        b.metadata
            .created
            .cmp(&a.metadata.created)
            .then_with(|| b.id.reference.cmp(&a.id.reference))
    });
    Ok(drafts)
}
fn metadata(ctx: &Context, id: &Id, active: &dyn Fn() -> bool) -> Result<Metadata, String> {
    let namespace = format!("{PREFIX}{}/", ctx.id);
    let leaf = id
        .reference
        .strip_prefix(&namespace)
        .ok_or(crate::i18n::text("backend-invalid-draft-identity-be5f3f"))?;
    let parts: Vec<_> = leaf.split('-').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
        || !oid_valid(&id.tree)
    {
        return Err(crate::i18n::text("backend-invalid-draft-identity-be5f3f").into());
    }
    if ctx.text(&["cat-file", "-t", &id.tree], vec![], active)? != "tree" {
        return Err(crate::i18n::text("backend-draft-root-is-not-a-tree-cf96bf").into());
    }
    let bytes = ctx.run(
        &["cat-file", "blob", &format!("{}:metadata.json", id.tree)],
        vec![],
        &[0],
        active,
    )?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::text("backend-draft-metadata-is-too-large-a9a01f").into());
    }
    let meta: Metadata = serde_json::from_slice(&bytes)
        .map_err(|_| crate::i18n::text("backend-incompatible-draft-metadata-ea032a"))?;
    if meta.version != 1
        || meta.worktree != ctx.id
        || !oid_valid(&meta.index_tree)
        || !oid_valid(&meta.work_tree)
        || meta.files.len() > MAX_FILES
        || meta.reason.chars().count() > 160
        || meta.reason.chars().any(char::is_control)
        || meta.head.as_ref().is_some_and(|oid| !oid_valid(oid))
        || meta
            .branch
            .as_ref()
            .is_some_and(|branch| branch.len() > 4096 || branch.chars().any(char::is_control))
    {
        return Err(crate::i18n::text(
            "backend-unsupported-draft-metadata-version-or-contents-7828f5",
        )
        .into());
    }
    for (name, oid) in [("index", &meta.index_tree), ("work", &meta.work_tree)] {
        if ctx.text(
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}:{name}", id.tree),
            ],
            vec![],
            active,
        )? != *oid
        {
            return Err(crate::i18n::text(
                "backend-draft-metadata-does-not-match-its-protected-trees-a37158",
            )
            .into());
        }
    }
    let mut paths = BTreeSet::new();
    for file in &meta.files {
        if !paths.insert(&file.path) {
            return Err(crate::i18n::text("backend-duplicate-draft-file-entry-414dd7").into());
        }
        valid_path(&file.path)?;
        for entry in [&file.index, &file.work].into_iter().flatten() {
            if !oid_valid(&entry.oid)
                || !matches!(
                    entry.mode.as_str(),
                    "100644" | "100755" | "120000" | "160000"
                )
            {
                return Err(crate::i18n::text("backend-invalid-draft-file-entry-ad48eb").into());
            }
        }
    }
    Ok(meta)
}
fn current(ctx: &Context, id: &Id, active: &dyn Fn() -> bool) -> Result<(), String> {
    metadata(ctx, id, active)?;
    let now = ctx.source(
        &["rev-parse", "--verify", "--end-of-options", &id.reference],
        vec![],
        &[0],
        active,
    )?;
    if std::str::from_utf8(&now)
        .map_err(|_| crate::i18n::text("backend-invalid-draft-ref-f6f5f2"))?
        .trim()
        != id.tree
    {
        return Err(crate::i18n::text(
            "backend-this-draft-ref-changed-reopen-earlier-versions-a2e9f9",
        )
        .into());
    }
    Ok(())
}
fn cleanup_in(
    ctx: &Context,
    drafts: &[Draft],
    now: i64,
    policy: Retention,
    keep: Option<&Id>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    for (i, draft) in drafts.iter().enumerate() {
        if keep.is_some_and(|id| *id == draft.id) {
            continue;
        }
        if i >= policy.count
            || now.saturating_sub(draft.metadata.created) > policy.days.saturating_mul(86400)
        {
            // Never delete a ref replaced since it was listed, nor another feature/version.
            metadata(ctx, &draft.id, active)?;
            ctx.source(
                &[
                    "update-ref",
                    "--no-deref",
                    "-d",
                    &draft.id.reference,
                    &draft.id.tree,
                ],
                vec![],
                &[0],
                active,
            )?;
        }
    }
    Ok(())
}
