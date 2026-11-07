//! Opt-in local history of working changes. Roots are trees (not WIP commits); separate
//! index/work trees keep staged contents distinct and make every saved blob GC-reachable.
mod context;
mod restore;
#[cfg(test)]
mod tests;

use crate::{git, remote};
use context::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub use context::{configure as set_enabled, configured as enabled};
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
    if let Some(path) = force_path {
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
        return Err(
            "Draft has over 2000 changed/untracked files; make a manual backup before continuing"
                .into(),
        );
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
            return Err(
                "Drafts exclude submodule contents; back up submodule changes separately".into(),
            );
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
                    "Draft content exceeds 32 MiB; make a manual backup before continuing".into(),
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
        return Err(
            "HEAD or index changed while making a draft; it will retry after a pause".into(),
        );
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
        return Err(
            "Working changes moved while making a draft; it will retry after a pause".into(),
        );
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
            return Err(
                "A working file changed while making a draft; it will retry after a pause".into(),
            );
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
        reason: reason.chars().take(160).collect(),
        worktree: ctx.id.clone(),
        head: before.head,
        branch: before.branch,
        index_tree,
        work_tree,
        files,
    };
    let bytes = serde_json::to_vec(&metadata).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Draft metadata exceeds 1 MiB; make a manual backup".into());
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
        let record = std::str::from_utf8(record).map_err(|_| "Draft paths must be UTF-8")?;
        let (info, path) = record.split_once('\t').ok_or("Cannot read index entry")?;
        valid_path(path)?;
        let parts: Vec<_> = info.split_whitespace().collect();
        let [tag, mode, oid, stage] = parts.as_slice() else {
            return Err("Cannot read index entry".into());
        };
        if *stage != "0" {
            return Err("Drafts are unavailable while the index contains conflicts; resolve or back up the conflict first".into());
        }
        if !matches!(*mode, "100644" | "100755" | "120000" | "160000") || !oid_valid(oid) {
            return Err("Unsupported index entry for drafts".into());
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
            return Err("Drafts support at most 20000 index entries".into());
        }
    }
    let text = |args: &[&str]| -> Result<Option<String>, String> {
        let bytes = ctx.source(args, vec![], &[0, 1], active)?;
        let s = String::from_utf8(bytes).map_err(|_| "Invalid HEAD")?;
        Ok((!s.trim().is_empty()).then(|| s.trim().to_owned()))
    };
    Ok(Source {
        cache: internal_file(ctx, &ctx.index_file, active)?.unwrap_or_default(),
        shared: if ctx.shared_file.is_empty() {
            None
        } else {
            let name = ctx
                .shared_file
                .rsplit(['/', '\\'])
                .next()
                .ok_or("Invalid shared index path")?;
            if !name.strip_prefix("sharedindex.").is_some_and(oid_valid) {
                return Err("Invalid shared index path".into());
            }
            Some((
                name.into(),
                internal_file(ctx, &ctx.shared_file, active)?.ok_or("Shared index disappeared")?,
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
        let (parent, name) = path.rsplit_once('/').ok_or("Invalid Git metadata path")?;
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
        return Err("Cannot read a regular index/cache file within 32 MiB".into());
    }
    let path = Path::new(path);
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err("Index/cache links and special files are not supported".into());
        }
        _ => {}
    }
    let parent = path.parent().ok_or("Invalid index path")?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Index path is not UTF-8")?;
    crate::safe_read::read_inside(parent, name, LIMIT)
        .map(Some)
        .ok_or_else(|| "Index/cache file is unreadable or exceeds 32 MiB".into())
}

fn private_index_file(
    ctx: &Context,
    name: &str,
    bytes: &[u8],
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let path = ctx.path.join(name);
    if let Some(target) = remote::parse(&path) {
        let out = git::forecast_remote(
            &target.host,
            remote::Request {
                script: format!("umask 077; cat > {}", remote::sh_path(&target.path)),
                stdin: Some(bytes.to_vec()),
                ..Default::default()
            },
            active,
        )?;
        if out.code != 0 {
            return Err("Cannot copy the private index cache".into());
        }
    } else {
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cached_index(ctx: &Context, state: &Source, active: &dyn Fn() -> bool) -> Result<(), String> {
    if !state.cache.is_empty() {
        private_index_file(ctx, "index", &state.cache, active)?;
    } else if !state.raw.is_empty() {
        return Err("Index disappeared while preparing the draft".into());
    }
    if let Some((name, bytes)) = &state.shared {
        private_index_file(ctx, name, bytes, active)?;
    }
    if ctx.run(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)? != state.raw {
        return Err("Index moved while copying its private stat cache".into());
    }
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
            let record = std::str::from_utf8(record).map_err(|_| "Invalid index")?;
            let (_, path) = record.split_once('\t').ok_or("Invalid index")?;
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
            let p = std::str::from_utf8(p).map_err(|_| "Draft paths must be UTF-8")?;
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
        let name = std::str::from_utf8(name).map_err(|_| "Draft paths must be UTF-8")?;
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
    script.push_str(&remote::read_within_script(root, rel, limit).ok_or("Unsafe draft path")?);
    Ok(script)
}

#[cfg(unix)]
fn missing_inside(root: &Path, rel: &str) -> Result<(), String> {
    use rustix::fs::{AtFlags, Mode, OFlags, open, openat, statat};
    let names: Vec<_> = rel.split('/').collect();
    let (last, folders) = names.split_last().ok_or("Invalid draft path")?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = open(root, flags - OFlags::NOFOLLOW, Mode::empty()).map_err(|e| e.to_string())?;
    for folder in folders {
        match openat(&dir, *folder, flags, Mode::empty()) {
            Ok(next) => dir = next,
            Err(rustix::io::Errno::NOENT) => return Ok(()),
            Err(e) => return Err(format!("Cannot verify missing draft path: {e}")),
        }
    }
    match statat(&dir, *last, AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err("Draft path changed or cannot be safely read".into()),
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
                        return Err("Draft path crosses a reparse point".into());
                    }
                }
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err("Draft path changed or crosses a link".into());
                }
            }
        }
    }
    Err("Draft path changed; retry after a pause".into())
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
        Err(format!("Drafts cannot safely read this path: {path:?}"))
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
    valid_path(path)?;
    if !active() {
        return Err("Draft cancelled".into());
    }
    if let Some(target) = remote::parse(&ctx.repo) {
        let script = read_script(&ctx.worktree, path, FILE_LIMIT as u64)?;
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
            && out.stdout.len() - 8 <= FILE_LIMIT
            && (out.stdout.starts_with(b"F100644\n") || out.stdout.starts_with(b"F100755\n"))
        {
            return Ok(Some((
                String::from_utf8_lossy(&out.stdout[1..7]).into_owned(),
                out.stdout[8..].to_vec(),
            )));
        }
        return Err(format!(
            "Cannot draft {path}: only regular files within 8 MiB are supported; links are not followed"
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
        return Err(format!(
            "Cannot draft {path}: symbolic links, submodules and special files are not followed"
        ));
    }
    let bytes = crate::safe_read::read_inside(root, path, FILE_LIMIT as u64)
        .ok_or_else(|| format!("Cannot draft {path}: file changed, is linked, or exceeds 8 MiB"))?;
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
    let refs = std::str::from_utf8(&refs).map_err(|_| "Invalid draft refs")?;
    if refs.lines().count() > 200 {
        return Err("Too many draft refs; inspect refs/gitcraft/drafts in the terminal".into());
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
            return Err("Too many draft refs; inspect refs/gitcraft/drafts in the terminal".into());
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
        .ok_or("Invalid draft identity")?;
    let parts: Vec<_> = leaf.split('-').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
        || !oid_valid(&id.tree)
    {
        return Err("Invalid draft identity".into());
    }
    if ctx.text(&["cat-file", "-t", &id.tree], vec![], active)? != "tree" {
        return Err("Draft root is not a tree".into());
    }
    let bytes = ctx.run(
        &["cat-file", "blob", &format!("{}:metadata.json", id.tree)],
        vec![],
        &[0],
        active,
    )?;
    if bytes.len() > 1024 * 1024 {
        return Err("Draft metadata is too large".into());
    }
    let meta: Metadata =
        serde_json::from_slice(&bytes).map_err(|_| "Incompatible draft metadata")?;
    if meta.version != 1
        || meta.worktree != ctx.id
        || !oid_valid(&meta.index_tree)
        || !oid_valid(&meta.work_tree)
        || meta.files.len() > MAX_FILES
    {
        return Err("Unsupported draft metadata version or contents".into());
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
            return Err("Draft metadata does not match its protected trees".into());
        }
    }
    for file in &meta.files {
        valid_path(&file.path)?;
        for entry in [&file.index, &file.work].into_iter().flatten() {
            if !oid_valid(&entry.oid)
                || !matches!(
                    entry.mode.as_str(),
                    "100644" | "100755" | "120000" | "160000"
                )
            {
                return Err("Invalid draft file entry".into());
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
        .map_err(|_| "Invalid draft ref")?
        .trim()
        != id.tree
    {
        return Err("This draft ref changed; reopen Earlier versions".into());
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
