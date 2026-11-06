//! Restacking: replaying each layer's own commits onto its parent's tip, parents before the
//! branches on them (`git rebase --onto <parent's tip> <old base> <branch>`, the old base
//! being the parent's commit the layer was last based on). A conflict stops it; what is left
//! to do is kept in the repository's config (section `gitcraft-restack`), so it can be resumed
//! (or aborted, every branch put back) later, after a restart too.
//!
//! A layer checked out in another worktree is rebased there (`--update-refs` would skip it);
//! the others are rebased here, which checks them out in turn, so this worktree must have no
//! changes, and the branch checked out here before is checked out again at the end.

use std::path::{Path, PathBuf};

use super::{BASE_KEY, config_key, local_ref, set_op};
use crate::git::{self, count_apart, merge_base, resolve, run, run_ok_codes};

/// The config section the restack under way is kept in.
pub(crate) const SECTION: &str = "gitcraft-restack";

/// One layer to replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub branch: String,
    pub parent: String,
    /// The parent's commit the branch is based on now: its commits after it are replayed.
    pub old_base: String,
}

/// A step as it will run, for the preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub step: Step,
    /// The parent's tip now. A parent restacked first moves before its layer is replayed.
    pub new_base: String,
    /// The commits that will be replayed.
    pub commits: u32,
    /// The worktree the branch is checked out in, other than this one.
    pub worktree: Option<PathBuf>,
}

/// A branch as it was before the restack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Before {
    pub branch: String,
    pub tip: String,
    pub base: Option<String>,
}

/// The restack under way, as kept in the config.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub steps: Vec<Step>,
    /// How many of `steps` are done.
    pub done: usize,
    /// What was checked out here: `refs/heads/<branch>`, or a commit.
    pub head: String,
    pub before: Vec<Before>,
    /// The other worktree the current step's rebase runs in (`None`: this one). Mid-rebase,
    /// that worktree's branch reads as detached, so it can't be found by its branch then.
    pub at: Option<PathBuf>,
}

impl Pending {
    /// The step it stopped at.
    pub fn current(&self) -> Option<&Step> {
        self.steps.get(self.done)
    }
}

/// Where a branch is checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// In this worktree.
    Here,
    /// In another worktree.
    Worktree(PathBuf),
    /// Nowhere.
    Nowhere,
}

/// A branch the restack moved, for undoing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub branch: String,
    pub before: String,
    pub after: String,
    pub base_before: Option<String>,
    pub base_after: String,
    /// Where it is checked out now.
    pub place: Place,
}

/// How a restack (or the part of it run now) ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Every layer is on its parent's tip; these moved.
    Done(Vec<Moved>),
    /// A layer's rebase stopped on conflicts, in this worktree or the one named.
    Stopped {
        branch: String,
        worktree: Option<PathBuf>,
        message: String,
    },
}

/// Works out the steps for `layers` (branch, parent, recorded base), parents before the
/// branches on them, and what each will do.
pub fn prepare(
    repo: &Path,
    layers: &[(String, String, Option<String>)],
) -> Result<Vec<Preview>, String> {
    let worktrees = git::worktree_list(repo, None).unwrap_or_default();
    let mut out = Vec::new();
    for (branch, parent, base) in layers {
        let old_base = old_base(repo, branch, parent, base.as_deref())?;
        let new_base = resolve(repo, &local_ref(parent))
            .map_err(|_| format!("{parent}, which {branch} is based on, is gone"))?;
        let (_, commits) = count_apart(repo, &old_base, &local_ref(branch))?;
        out.push(Preview {
            step: Step {
                branch: branch.clone(),
                parent: parent.clone(),
                old_base,
            },
            new_base,
            commits,
            worktree: match place(&worktrees, branch) {
                Place::Worktree(path) => Some(path),
                _ => None,
            },
        });
    }
    Ok(out)
}

/// The commit after which `branch`'s own commits start: the base recorded, while the branch
/// still has it; else where git's reflog says it forked from its parent; else where the two
/// meet.
fn old_base(repo: &Path, branch: &str, parent: &str, base: Option<&str>) -> Result<String, String> {
    let (b, p) = (local_ref(branch), local_ref(parent));
    if let Some(base) = base
        && is_ancestor(repo, base, &b)
    {
        return Ok(base.to_owned());
    }
    if let Ok(out) = run(Some(repo), &["merge-base", "--fork-point", &p, &b])
        && !out.trim().is_empty()
    {
        return Ok(out.trim().to_owned());
    }
    merge_base(repo, &p, &b).map_err(|_| {
        format!("{branch} and {parent} have no commit in common: it can't be restacked")
    })
}

fn is_ancestor(repo: &Path, a: &str, b: &str) -> bool {
    run(Some(repo), &["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// Where `branch` is checked out, by `worktrees`.
fn place(worktrees: &[git::Worktree], branch: &str) -> Place {
    match worktrees
        .iter()
        .find(|w| w.branch.as_deref() == Some(branch))
    {
        Some(w) if w.current => Place::Here,
        Some(w) => Place::Worktree(w.path.clone()),
        None => Place::Nowhere,
    }
}

/// The restack under way, if one is.
pub fn pending(repo: &Path) -> Result<Option<Pending>, String> {
    let out = run_ok_codes(
        Some(repo),
        &[
            "config",
            "--local",
            "-z",
            "--get-regexp",
            &format!(r"^{SECTION}\."),
        ],
        &[0, 1],
    )?;
    Ok(parse_pending(&out))
}

/// `git config -z --get-regexp` output of the section → the restack it keeps.
pub(crate) fn parse_pending(out: &str) -> Option<Pending> {
    let mut pending = Pending::default();
    let mut any = false;
    for entry in out.split('\0').filter(|e| !e.is_empty()) {
        let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
        let Some(name) = key.strip_prefix(&format!("{SECTION}.")) else {
            continue;
        };
        let fields: Vec<&str> = value.split('\t').collect();
        any = true;
        match (name, fields.as_slice()) {
            ("head", [head]) => pending.head = (*head).to_owned(),
            ("at", _) if !value.is_empty() => pending.at = Some(PathBuf::from(value)),
            ("done", [n]) => pending.done = n.parse().unwrap_or(0),
            ("step", [branch, parent, old_base]) => pending.steps.push(Step {
                branch: (*branch).to_owned(),
                parent: (*parent).to_owned(),
                old_base: (*old_base).to_owned(),
            }),
            ("before", [branch, tip, base]) => pending.before.push(Before {
                branch: (*branch).to_owned(),
                tip: (*tip).to_owned(),
                base: (!base.is_empty()).then(|| (*base).to_owned()),
            }),
            _ => {}
        }
    }
    (any && !pending.steps.is_empty()).then_some(pending)
}

/// Starts restacking `steps` (in that order), and runs them until done or stopped.
pub fn start(repo: &Path, steps: Vec<Step>) -> Result<Outcome, String> {
    if steps.is_empty() {
        return Ok(Outcome::Done(Vec::new()));
    }
    if pending(repo)?.is_some() {
        return Err("A restack is already under way: continue or abort it first.".to_owned());
    }
    if git::status(repo)?.in_progress.is_some() {
        return Err("Finish or abort the operation in progress first.".to_owned());
    }
    let worktrees = git::worktree_list(repo, None)?;
    let here = steps.iter().any(|s| !in_other(&worktrees, &s.branch));
    if here && has_changes(repo)? {
        return Err(
            "Commit or stash your changes first: restacking checks the branches out here."
                .to_owned(),
        );
    }
    let head = run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?
        .trim()
        .to_owned();
    let head = if head.is_empty() {
        git::head(repo)
    } else {
        head
    };
    let recorded = super::recorded(repo)?;
    let mut before = Vec::new();
    for step in &steps {
        before.push(Before {
            branch: step.branch.clone(),
            tip: resolve(repo, &local_ref(&step.branch))?,
            base: recorded.get(&step.branch).and_then(|r| r.base.clone()),
        });
    }
    let pending = Pending {
        steps,
        done: 0,
        head,
        before,
        at: None,
    };
    save(repo, &pending)?;
    run_from(repo, pending)
}

/// Whether `branch` is checked out in a worktree other than this one.
fn in_other(worktrees: &[git::Worktree], branch: &str) -> bool {
    matches!(place(worktrees, branch), Place::Worktree(_))
}

/// Whether this worktree has changes to tracked files.
fn has_changes(repo: &Path) -> Result<bool, String> {
    Ok(!run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?
    .trim()
    .is_empty())
}

/// Keeps `pending` in the config, in place of whatever was kept. The steps go last: a restack
/// is kept only with steps, so one cut short while being written is either not kept at all or
/// kept with every branch's tip from before (what aborting puts back).
fn save(repo: &Path, pending: &Pending) -> Result<(), String> {
    clear(repo)?;
    let key = |name: &str| format!("{SECTION}.{name}");
    let mut steps = vec![
        set_op(&key("head"), &pending.head),
        set_op(&key("done"), &pending.done.to_string()),
    ];
    for b in &pending.before {
        let base = b.base.as_deref().unwrap_or("");
        steps.push(add_op(
            &key("before"),
            &format!("{}\t{}\t{base}", b.branch, b.tip),
        ));
    }
    for s in &pending.steps {
        steps.push(add_op(
            &key("step"),
            &format!("{}\t{}\t{}", s.branch, s.parent, s.old_base),
        ));
    }
    for step in steps {
        run_op(repo, &step)?;
    }
    Ok(())
}

/// Runs one step as built by `set_op` and friends.
fn run_op(repo: &Path, step: &[String]) -> Result<(), String> {
    let args: Vec<&str> = step.iter().map(String::as_str).collect();
    run(Some(repo), &args).map(drop)
}

fn add_op(key: &str, value: &str) -> Vec<String> {
    ["config", "--local", "--add", key, value]
        .map(str::to_owned)
        .to_vec()
}

/// Forgets the restack under way (none kept: nothing to do).
fn clear(repo: &Path) -> Result<(), String> {
    // Exit code 128: no such section.
    run_ok_codes(
        Some(repo),
        &["config", "--local", "--remove-section", SECTION],
        &[0, 128],
    )
    .map(drop)
}

/// Runs the steps of `pending` from the first not done.
fn run_from(repo: &Path, mut pending: Pending) -> Result<Outcome, String> {
    // Which branch is checked out where doesn't change while it runs: listed once.
    let worktrees = git::worktree_list(repo, None)?;
    while let Some(step) = pending.current().cloned() {
        let new_base = resolve(repo, &local_ref(&step.parent)).map_err(|_| {
            format!(
                "{}, which {} is based on, is gone",
                step.parent, step.branch
            )
        })?;
        let tip = local_ref(&step.branch);
        if !is_ancestor(repo, &new_base, &tip) {
            let at = match place(&worktrees, &step.branch) {
                Place::Worktree(path) => Some(path),
                _ => None,
            };
            keep_at(repo, at.as_deref())?;
            if let Err(e) = rebase(repo, at.as_deref(), &step, &new_base) {
                let stopped = rebase_stopped(repo, at.as_deref())?;
                if stopped {
                    return Ok(Outcome::Stopped {
                        branch: step.branch,
                        worktree: at,
                        message: e,
                    });
                }
                return Err(format!("Restacking {} failed: {e}", step.branch));
            }
        }
        finish_step(repo, &mut pending, &new_base)?;
    }
    finish(repo, &pending)
}

/// Keeps which worktree the current step runs in.
fn keep_at(repo: &Path, at: Option<&Path>) -> Result<(), String> {
    let key = format!("{SECTION}.at");
    match at {
        Some(path) => run_op(repo, &set_op(&key, &path.to_string_lossy())),
        None => run_ok_codes(
            Some(repo),
            &["config", "--local", "--unset-all", &key],
            &[0, 5],
        )
        .map(drop),
    }
}

/// Replays `step` onto `new_base`: here, or in the worktree `at` that has it checked out.
fn rebase(repo: &Path, at: Option<&Path>, step: &Step, new_base: &str) -> Result<String, String> {
    // A `rebase.updateRefs` of the user's would move other branches along.
    let mut args = vec![
        "-c",
        "rebase.updateRefs=false",
        "rebase",
        "--onto",
        new_base,
    ];
    match at {
        Some(dir) => {
            args.extend(["--autostash", "--end-of-options", &step.old_base]);
            run(Some(dir), &args)
        }
        None => {
            args.extend(["--end-of-options", &step.old_base, &step.branch]);
            run(Some(repo), &args)
        }
    }
}

/// Whether a rebase is under way (stopped) in this worktree or in `at`.
fn rebase_stopped(repo: &Path, at: Option<&Path>) -> Result<bool, String> {
    Ok(git::status(at.unwrap_or(repo))?.in_progress.is_some())
}

/// Records the step at the top of `pending` done: its branch is based on `new_base` now.
fn finish_step(repo: &Path, pending: &mut Pending, new_base: &str) -> Result<(), String> {
    let Some(step) = pending.current() else {
        return Ok(());
    };
    run_op(repo, &set_op(&config_key(&step.branch, BASE_KEY), new_base))?;
    pending.done += 1;
    let done = pending.done.to_string();
    run(
        Some(repo),
        &["config", "--local", &format!("{SECTION}.done"), &done],
    )
    .map(drop)
}

/// Checks out again what was checked out here, forgets the restack, and says what moved.
fn finish(repo: &Path, pending: &Pending) -> Result<Outcome, String> {
    checkout_back(repo, &pending.head)?;
    let moved = moved(repo, pending)?;
    clear(repo)?;
    Ok(Outcome::Done(moved))
}

/// Checks `head` (`refs/heads/<branch>` or a commit) out here, unless it is already.
fn checkout_back(repo: &Path, head: &str) -> Result<(), String> {
    let now = run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let now = now.trim();
    if let Some(branch) = head.strip_prefix("refs/heads/") {
        if now != head {
            // A branch's name never starts with `-`.
            run(Some(repo), &["checkout", "-q", branch])?;
        }
    } else if !head.is_empty() && (!now.is_empty() || git::head(repo) != head) {
        run(Some(repo), &["checkout", "-q", "--detach", head])?;
    }
    Ok(())
}

/// The branches `pending` moved, where they are now.
fn moved(repo: &Path, pending: &Pending) -> Result<Vec<Moved>, String> {
    let worktrees = git::worktree_list(repo, None)?;
    let recorded = super::recorded(repo)?;
    let mut out = Vec::new();
    for b in &pending.before {
        let after = resolve(repo, &local_ref(&b.branch))?;
        if after == b.tip {
            continue;
        }
        out.push(Moved {
            branch: b.branch.clone(),
            before: b.tip.clone(),
            after,
            base_before: b.base.clone(),
            base_after: recorded
                .get(&b.branch)
                .and_then(|r| r.base.clone())
                .unwrap_or_default(),
            place: place(&worktrees, &b.branch),
        });
    }
    Ok(out)
}

/// Goes on with the restack under way: finishes the rebase it stopped in (`rebase
/// --continue`), or takes in that it was finished meanwhile, then runs the steps left.
pub fn resume(repo: &Path) -> Result<Outcome, String> {
    let pending = pending(repo)?.ok_or("No restack is under way.")?;
    let Some(step) = pending.current().cloned() else {
        return finish(repo, &pending);
    };
    let at = pending.at.clone();
    if rebase_stopped(repo, at.as_deref())?
        && let Err(e) = run(
            Some(at.as_deref().unwrap_or(repo)),
            &["rebase", "--continue"],
        )
    {
        return Ok(Outcome::Stopped {
            branch: step.branch,
            worktree: at,
            message: e,
        });
    }
    // Finished (here or in the terminal), or aborted: then it runs again.
    run_from(repo, pending)
}

/// Gives up the restack under way: aborts the rebase it stopped in, puts every branch back
/// where it was, with its recorded base, and checks out again what was checked out here.
pub fn abort(repo: &Path) -> Result<(), String> {
    let pending = pending(repo)?.ok_or("No restack is under way.")?;
    if pending.current().is_some() {
        let at = pending.at.as_deref();
        if rebase_stopped(repo, at)? {
            run(Some(at.unwrap_or(repo)), &["rebase", "--abort"])?;
        }
    }
    let worktrees = git::worktree_list(repo, None)?;
    for b in &pending.before {
        let now = resolve(repo, &local_ref(&b.branch)).unwrap_or_default();
        if now != b.tip {
            match place(&worktrees, &b.branch) {
                Place::Here => run(Some(repo), &["reset", "-q", "--keep", &b.tip])?,
                Place::Worktree(path) => run(Some(&path), &["reset", "-q", "--keep", &b.tip])?,
                Place::Nowhere => run(
                    Some(repo),
                    &["update-ref", &local_ref(&b.branch), &b.tip, &now],
                )?,
            };
        }
        let key = config_key(&b.branch, BASE_KEY);
        match &b.base {
            Some(base) => run_op(repo, &set_op(&key, base))?,
            None => run_ok_codes(
                Some(repo),
                &["config", "--local", "--unset-all", &key],
                &[0, 5],
            )
            .map(drop)?,
        };
    }
    checkout_back(repo, &pending.head)?;
    clear(repo)
}
