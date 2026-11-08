//! What the conflict radar reads, in the background: the active branches, and for each pair
//! of their tips not worked out yet, their merge-base, the files each changed since, and a
//! dry-run merge in memory ([`crate::collision`]: `git merge-tree`, nothing checked out, no
//! working tree touched). And, for a pair looked at, the conflicted regions that merge writes.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{
    Cache, Candidate, Hunk, MAX_PATHS, PairResult, Radar, assemble, overlap, parse_hunks, select,
};
use crate::collision;
use crate::git;
use crate::insights::Provenance;
use crate::insights::constellation::read::{default_branch, local_branches};

/// Most pairs merged at once.
const AT_ONCE: usize = 4;
/// Most conflicted files whose regions are read for a pair.
pub const HUNK_FILES: usize = 20;

/// The local branches as candidates: their tips, when they were committed, whether a worktree
/// has them checked out; and the default branch, when there is one.
pub fn candidates(repo: &Path) -> Result<(Vec<Candidate>, Option<Candidate>), String> {
    let all = local_branches(repo)?;
    let checked_out: HashSet<String> = git::worktree_list(repo, None)
        .map(|list| list.into_iter().filter_map(|w| w.branch).collect())
        .unwrap_or_default();
    let target = default_branch(repo, &all).map(|t| Candidate {
        committed: all
            .iter()
            .find(|b| b.name == t.name)
            .map_or(0, |b| b.committed),
        worktree: checked_out.contains(&t.name),
        name: t.name,
        tip: t.tip,
    });
    let list = all
        .into_iter()
        .map(|b| Candidate {
            worktree: checked_out.contains(&b.name),
            name: b.name,
            tip: b.tip,
            committed: b.committed,
        })
        .collect();
    Ok((list, target))
}

/// The radar of the repository at `now`, the pairs `cache` holds at the same tips taken from
/// it: the radar, and the cache with every pair of it (and none other).
pub fn radar(repo: &Path, mut cache: Cache, now: i64) -> Result<(Radar, Cache), String> {
    let (all, target) = candidates(repo)?;
    let (mut nodes, cut) = select(&all, target.as_ref().map(|t| t.name.as_str()), now);
    let has_target = target.is_some();
    nodes.extend(target);
    let tips: Vec<String> = nodes.iter().map(|n| n.tip.clone()).collect();
    let missing = cache.missing(&tips);
    let results = compute(repo, &nodes, &missing);
    let mut partial: Vec<String> = cut.into_iter().collect();
    for (&(i, j), result) in missing.iter().zip(results) {
        cache.insert(&tips[i], &tips[j], result);
    }
    cache.keep_only(&tips);
    let fresh: HashSet<(usize, usize)> = missing.iter().copied().collect();
    let computed = missing.len();
    let (edges, order) = assemble(nodes.clone(), has_target, &cache, &fresh);
    let reused = edges.len().saturating_sub(computed);
    let cut_pairs = edges.iter().filter(|e| e.result.partial.is_some()).count();
    if cut_pairs > 0 {
        partial.push(format!(
            "{cut_pairs} pairs changed more than {MAX_PATHS} files on a side: overlap read in part"
        ));
    }
    let failed = edges
        .iter()
        .filter(|e| e.result.merge_error().is_some())
        .count();
    if failed > 0 {
        partial.push(format!("{failed} pairs couldn't be merged in memory"));
    }
    let branches = nodes.len() - usize::from(has_target);
    let scope = match nodes.last().filter(|_| has_target) {
        Some(t) => format!(
            "{branches} active branches (in worktrees, or committed in the last {} days) and {}",
            super::RECENT_DAYS,
            t.name
        ),
        None => format!("{branches} active branches; no default branch found"),
    };
    let mut provenance = Provenance::new("git merge-tree (in memory), git diff", scope);
    provenance.collected = now;
    if let Some(t) = nodes.last().filter(|_| has_target) {
        provenance = provenance.tip(t.name.clone(), t.tip.clone());
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Ok((
        Radar {
            nodes,
            has_target,
            edges,
            order,
            computed,
            reused,
            provenance,
        },
        cache,
    ))
}

/// Works out the pairs `todo` of `nodes`, a few at a time, in their order.
fn compute(repo: &Path, nodes: &[Candidate], todo: &[(usize, usize)]) -> Vec<PairResult> {
    let next = AtomicUsize::new(0);
    // What a tip changed since a merge-base, read once for every pair sharing them.
    let changed: Changed = Mutex::new(HashMap::new());
    let mut done: Vec<(usize, PairResult)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..AT_ONCE.min(todo.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(i, j)) = todo.get(k) else {
                            break;
                        };
                        mine.push((k, pair(repo, &nodes[i], &nodes[j], &changed)));
                    }
                    mine
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    });
    done.sort_by_key(|(k, _)| *k);
    // A worker that panicked left its pairs out: each says so rather than being dropped.
    let mut out: Vec<PairResult> = Vec::with_capacity(todo.len());
    let mut it = done.into_iter().peekable();
    for k in 0..todo.len() {
        match it.next_if(|(d, _)| *d == k) {
            Some((_, r)) => out.push(r),
            None => out.push(PairResult {
                base: None,
                changed: HashMap::new(),
                overlap: Vec::new(),
                merge: Err("Not worked out".to_owned()),
                partial: Some("not worked out".to_owned()),
            }),
        }
    }
    out
}

/// What a tip changed since a merge-base (by both), and whether more than was kept.
pub type Changed = Mutex<HashMap<(String, String), Result<(Vec<String>, bool), String>>>;

/// What Git says of `a` and `b`: their merge-base, the files each changed since, and the
/// dry-run merge.
pub fn pair(repo: &Path, a: &Candidate, b: &Candidate, changed: &Changed) -> PairResult {
    let merge = dry_run(repo, a, b);
    let Ok(base) = git::merge_base(repo, &a.tip, &b.tip) else {
        return PairResult {
            base: None,
            changed: HashMap::new(),
            overlap: Vec::new(),
            merge,
            partial: None,
        };
    };
    let files = |tip: &str| -> Result<(Vec<String>, bool), String> {
        let k = (base.clone(), tip.to_owned());
        if let Some(found) = lock(changed).get(&k) {
            return found.clone();
        }
        let found = changed_files(repo, &base, tip);
        lock(changed).insert(k, found.clone());
        found
    };
    let (sides, cut) = match (files(&a.tip), files(&b.tip)) {
        (Ok((fa, ca)), Ok((fb, cb))) => ((fa, fb), ca || cb),
        (Err(e), _) | (_, Err(e)) => {
            return PairResult {
                base: Some(base),
                changed: HashMap::new(),
                overlap: Vec::new(),
                merge,
                partial: Some(format!("what changed couldn't be read: {e}")),
            };
        }
    };
    PairResult {
        changed: HashMap::from([
            (a.tip.clone(), sides.0.len()),
            (b.tip.clone(), sides.1.len()),
        ]),
        overlap: overlap(&sides.0, &sides.1),
        base: Some(base),
        merge,
        partial: cut.then(|| format!("more than {MAX_PATHS} files changed on a side")),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The dry-run merge of the two tips (as commits, so the answer is of those tips whatever
/// the branches do meanwhile).
fn dry_run(repo: &Path, a: &Candidate, b: &Candidate) -> collision::Cell {
    let branch = |c: &Candidate| collision::Branch {
        name: c.name.clone(),
        git: c.tip.clone(),
    };
    let done = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    match collision::compute(repo, &[branch(a)], &branch(b), &done, &cancel) {
        Ok(m) => m
            .with_target
            .into_iter()
            .next()
            .unwrap_or_else(|| Err("Not merged".to_owned())),
        Err(e) => Err(e),
    }
}

/// The files `tip` changed since `base` (renames as a removal and an addition), at most
/// [`MAX_PATHS`]; whether there were more.
fn changed_files(repo: &Path, base: &str, tip: &str) -> Result<(Vec<String>, bool), String> {
    let out = git::run(
        Some(repo),
        &["diff", "--name-only", "-z", "--no-renames", base, tip, "--"],
    )?;
    let mut files: Vec<String> = out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    let cut = files.len() > MAX_PATHS;
    files.truncate(MAX_PATHS);
    Ok((files, cut))
}

/// The conflicted regions of a conflicted file, as the dry-run merge wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHunks {
    pub path: String,
    /// Its regions (none when the conflict isn't in its text: a file deleted on one side, a
    /// binary one), or why it couldn't be read.
    pub hunks: Result<Vec<Hunk>, String>,
    /// More regions than kept.
    pub more: bool,
}

/// The conflicted regions of `paths` (at most [`HUNK_FILES`]) when tips `a` and `b` are merged
/// in memory.
pub fn conflict_hunks(
    repo: &Path,
    a: &str,
    b: &str,
    paths: &[String],
) -> Result<Vec<FileHunks>, String> {
    for rev in [a, b] {
        if rev.starts_with('-') {
            return Err(format!("{rev} is not a commit"));
        }
    }
    let out = git::run_ok_codes(
        Some(repo),
        &["merge-tree", "--write-tree", "--no-messages", "-z", a, b],
        &[0, 1],
    )?;
    let tree = out.split('\0').next().unwrap_or_default().trim();
    if tree.is_empty() {
        return Err("The dry-run merge wrote no tree".to_owned());
    }
    Ok(paths
        .iter()
        .take(HUNK_FILES)
        .map(|path| {
            let text = git::run(Some(repo), &["cat-file", "-p", &format!("{tree}:{path}")]);
            let (hunks, more) = match &text {
                Ok(text) => parse_hunks(text),
                Err(_) => (Vec::new(), false),
            };
            FileHunks {
                path: path.clone(),
                hunks: text.map(|_| hunks),
                more,
            }
        })
        .collect())
}
