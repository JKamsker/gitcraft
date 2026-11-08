//! What the constellation reads, in the background: the local branches, the worktrees and
//! their uncommitted changes, the agent tasks and their checkpoint turns, the recorded stack
//! parents, which branches the default branch contains, and how much disk each worktree's
//! folder takes. All of it read-only; git through [`crate::git`], so a repository on another
//! machine is read there.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{
    BranchIn, MAX_BRANCHES, MAX_TASKS, MAX_WORKTREES, Snapshot, Target, TaskIn, WorktreeIn,
};
use crate::git;
use crate::insights::Provenance;

/// Every local branch, the most recently committed first.
pub fn local_branches(repo: &Path) -> Result<Vec<BranchIn>, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)%1f%(objectname)%1f%(upstream:short)%1f%(upstream:track)%1f%(committerdate:unix)",
            "refs/heads",
        ],
    )?;
    Ok(parse_branches(&out))
}

/// What [`local_branches`]' `for-each-ref` printed.
pub fn parse_branches(out: &str) -> Vec<BranchIn> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            let name = parts.next()?.strip_prefix("refs/heads/")?.to_owned();
            let tip = parts.next()?.to_owned();
            let upstream = parts.next().unwrap_or_default().to_owned();
            let track = parts.next().unwrap_or_default();
            let committed = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let branch = git::Branch {
                name: name.clone(),
                target: tip.clone(),
                is_head: false,
                upstream: upstream.clone(),
                track: track.to_owned(),
            };
            let (ahead, behind) = branch.ahead_behind();
            Some(BranchIn {
                name,
                tip,
                upstream,
                ahead,
                behind,
                gone: branch.upstream_gone(),
                committed,
            })
        })
        .collect()
}

/// The default branch: the one `origin/HEAD` names (the local branch of that name when there
/// is one), else `main` or `master`.
pub fn default_branch(repo: &Path, local: &[BranchIn]) -> Option<Target> {
    let local_target = |name: &str| {
        local.iter().find(|b| b.name == name).map(|b| Target {
            name: b.name.clone(),
            rev: format!("refs/heads/{}", b.name),
            tip: b.tip.clone(),
        })
    };
    if let Ok(head) = git::run(
        Some(repo),
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    ) {
        let remote = head.trim();
        if let Some(name) = remote.strip_prefix("origin/")
            && let Some(t) = local_target(name)
        {
            return Some(t);
        }
        let rev = format!("refs/remotes/{remote}");
        if !remote.is_empty()
            && let Ok(tip) = git::resolve(repo, &rev)
        {
            return Some(Target {
                name: remote.to_owned(),
                rev,
                tip,
            });
        }
    }
    ["main", "master"].iter().find_map(|n| local_target(n))
}

/// The local branches whose tip `tip` contains.
fn merged_into(repo: &Path, tip: &str) -> Result<HashSet<String>, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            &format!("--merged={tip}"),
            "--format=%(refname)",
            "refs/heads",
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix("refs/heads/"))
        .map(str::to_owned)
        .collect())
}

/// `all` cut to [`MAX_BRANCHES`]: those in `keep` (checked out, in a task, the default
/// branch) first, then the most recently committed. Whether some were left out.
pub fn bound_branches(all: Vec<BranchIn>, keep: &HashSet<String>) -> (Vec<BranchIn>, bool) {
    if all.len() <= MAX_BRANCHES {
        return (all, false);
    }
    let (mut kept, rest): (Vec<BranchIn>, Vec<BranchIn>) =
        all.into_iter().partition(|b| keep.contains(&b.name));
    kept.truncate(MAX_BRANCHES);
    let room = MAX_BRANCHES - kept.len();
    kept.extend(rest.into_iter().take(room));
    (kept, true)
}

/// Reads what the constellation is made of at `now`. Fails only when the branches can't be
/// listed; what else can't be read is said in the snapshot.
pub fn snapshot(repo: &Path, now: i64) -> Result<Snapshot, String> {
    let all = local_branches(repo)?;
    let total = all.len();
    let mut partial = Vec::new();

    let mut worktrees: Vec<WorktreeIn> = match git::worktrees(repo, None) {
        Ok(list) => list
            .into_iter()
            .filter(|w| !w.bare)
            .map(|w| WorktreeIn {
                path: w.path.to_string_lossy().into_owned(),
                branch: w.branch,
                head: w.head,
                changes: w.changes,
                prunable: w.prunable,
                locked: w.locked,
                main: w.main,
                current: w.current,
            })
            .collect(),
        Err(e) => {
            partial.push(format!("worktrees couldn't be listed: {e}"));
            Vec::new()
        }
    };
    if worktrees.len() > MAX_WORKTREES {
        partial.push(format!(
            "{MAX_WORKTREES} of {} worktrees shown",
            worktrees.len()
        ));
        worktrees.truncate(MAX_WORKTREES);
    }

    let (mut tasks, tasks_error) = match crate::agent::tasks::load(repo) {
        Ok(list) => (list, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    if tasks.len() > MAX_TASKS {
        partial.push(format!("{MAX_TASKS} of {} agent tasks shown", tasks.len()));
        // The newest are kept.
        tasks.drain(..tasks.len() - MAX_TASKS);
    }
    let tasks: Vec<TaskIn> = tasks
        .into_iter()
        .map(|t| {
            let alive = worktrees
                .iter()
                .any(|w| super::same_path(&w.path, &t.path) && w.prunable.is_none());
            let (turns, last_turn) = if alive {
                turns_of(repo, &t.path)
            } else {
                (0, None)
            };
            TaskIn {
                agent: crate::agent::tasks::agent_label(&t.agent),
                branch: t.branch,
                path: t.path,
                prompt: t.prompt,
                started: t.started,
                turns,
                last_turn,
            }
        })
        .collect();

    let target = default_branch(repo, &all);
    let mut keep: HashSet<String> = worktrees.iter().filter_map(|w| w.branch.clone()).collect();
    keep.extend(tasks.iter().map(|t| t.branch.clone()));
    keep.extend(target.iter().map(|t| t.name.clone()));
    let (branches, cut) = bound_branches(all, &keep);
    if cut {
        partial.push(format!(
            "{} of {total} local branches shown (the most recently committed)",
            branches.len()
        ));
    }

    let stack_parents = match git::stack::recorded(repo) {
        Ok(map) => map.into_iter().map(|(b, r)| (b, r.parent)).collect(),
        Err(e) => {
            partial.push(format!("stack parents couldn't be read: {e}"));
            Default::default()
        }
    };

    let (merged, bases) = match &target {
        Some(t) => {
            let merged = match merged_into(repo, &t.tip) {
                Ok(m) => Some(m),
                Err(e) => {
                    partial.push(format!("merged branches couldn't be told: {e}"));
                    None
                }
            };
            let mut bases = HashMap::new();
            for b in &branches {
                if b.name == t.name || merged.as_ref().is_some_and(|m| m.contains(&b.name)) {
                    continue;
                }
                if let Ok(base) = git::merge_base(repo, &t.tip, &b.tip) {
                    bases.insert(b.name.clone(), base);
                }
            }
            (merged, bases)
        }
        None => (None, HashMap::new()),
    };

    let tips: Vec<(String, String)> = branches
        .iter()
        .map(|b| (b.name.clone(), b.tip.clone()))
        .collect();
    let mut provenance = Provenance::new(
        "git for-each-ref, worktree list, agent tasks",
        format!(
            "{} branches, {} worktrees, {} agent tasks",
            branches.len(),
            worktrees.len(),
            tasks.len()
        ),
    );
    provenance.collected = now;
    if let Some(t) = &target {
        provenance = provenance.tip(t.name.clone(), t.tip.clone());
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Ok(Snapshot {
        branches,
        worktrees,
        tasks,
        tasks_error,
        stack_parents,
        target,
        merged,
        bases,
        tips,
        remote: crate::remote::is_remote(repo),
        provenance,
    })
}

/// How many checkpoint turns the worktree at `path` saved, and when the last one was.
fn turns_of(repo: &Path, path: &str) -> (usize, Option<i64>) {
    // Turns are marked with the worktree's top folder as git names it there.
    let Ok(top) = git::run(Some(Path::new(path)), &["rev-parse", "--show-toplevel"]) else {
        return (0, None);
    };
    match crate::agent::checkpoint::turns(repo, top.trim_end_matches(['\n', '\r'])) {
        Ok(turns) => (turns.len(), turns.iter().map(|t| t.time).max()),
        Err(_) => (0, None),
    }
}

/// How much a worktree's folder takes on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Disk {
    pub bytes: u64,
    pub files: u64,
    /// The walk stopped at its limit: there is at least this much.
    pub partial: bool,
}

/// Most entries one worktree's walk looks at.
pub const DISK_ENTRIES: usize = 200_000;

/// What the folder at `dir` takes, its `.git` aside, looking at no more than `budget` entries.
/// Symbolic links are counted as themselves, not followed; a folder with a `.git` of its own
/// inside it (another worktree kept there, a submodule, a nested repository) is counted on its
/// own, not again here.
pub fn disk_usage(dir: &Path, budget: usize) -> Result<Disk, String> {
    let mut disk = Disk::default();
    let mut seen = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    let mut top = true;
    while let Some(folder) = stack.pop() {
        let entries = match std::fs::read_dir(&folder) {
            Ok(e) => e,
            Err(e) if top => return Err(e.to_string()),
            // A folder it may not read: what it holds isn't counted.
            Err(_) => {
                disk.partial = true;
                continue;
            }
        };
        for entry in entries.flatten() {
            if top && entry.file_name() == ".git" {
                continue;
            }
            seen += 1;
            if seen > budget {
                disk.partial = true;
                return Ok(disk);
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                if entry.path().join(".git").symlink_metadata().is_err() {
                    stack.push(entry.path());
                }
            } else {
                disk.files += 1;
                disk.bytes += meta.len();
            }
        }
        top = false;
    }
    Ok(disk)
}

/// A size in bytes for people: `12.3 MB`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
