//! Branch cleanup on real repositories: what is found merged, rebased, squash-merged, gone or
//! stale; what is never offered; deleting the ticked ones here and on the remote, undoing and
//! redoing that.

use std::path::{Path, PathBuf};

use super::Candidate;
use crate::forge::MergedPullRequest;
use crate::git;
use crate::repo::branch_health::{self, Inputs, Merged, Reason};
use crate::repo::operations::run_steps;
use crate::repo::sidebar::delete_many;
use crate::repo::{OpKind, RepoTab, Undo, UndoEntry};

const DAY: i64 = 24 * 60 * 60;

/// A scratch folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gitcraft-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
        .map(|out| out.trim().to_owned())
        .map_err(|e| format!("git {args:?}: {e}"))
}

/// A commit on the checked-out branch changing `file`.
fn commit(dir: &Path, file: &str, text: &str) -> Result<String, String> {
    let path = dir.join(file);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    git_ok(dir, &["add", "-A"])?;
    git_ok(dir, &["commit", "-q", "-m", text])?;
    git_ok(dir, &["rev-parse", "HEAD"])
}

/// A clone of a bare `remote.git` with `main` pushed, and branches that are merged in every
/// way, gone, open, and checked out in a worktree.
struct Fixture {
    _scratch: Scratch,
    dir: PathBuf,
    remote: PathBuf,
    /// The last commit of the squash-merged branch (what its pull request proposed).
    squashed_tip: String,
}

fn fixture(name: &str) -> Result<Fixture, String> {
    let scratch = Scratch::new(name);
    let (dir, remote) = (scratch.0.join("my repo"), scratch.0.join("remote.git"));
    let _ = std::fs::create_dir_all(&dir);
    git_ok(
        &scratch.0,
        &["init", "-q", "--bare", &remote.to_string_lossy()],
    )?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["remote", "add", "origin", &remote.to_string_lossy()],
    ] {
        git_ok(&dir, args)?;
    }
    commit(&dir, "base.txt", "base")?;
    // Merged with a merge commit; its name with folders and characters beyond ASCII.
    git_ok(&dir, &["checkout", "-q", "-b", "fix/ünïcode-✓"])?;
    commit(&dir, "u.txt", "unicode")?;
    git_ok(&dir, &["checkout", "-q", "main"])?;
    git_ok(
        &dir,
        &["merge", "-q", "--no-ff", "-m", "merge", "fix/ünïcode-✓"],
    )?;
    // Rebased: its commit picked onto main under another hash.
    git_ok(&dir, &["checkout", "-q", "-b", "rebased"])?;
    let picked = commit(&dir, "r.txt", "rebased work")?;
    git_ok(&dir, &["checkout", "-q", "main"])?;
    // Main moves first: picked onto the same parent in the same second, the commit would be
    // the very same one.
    commit(&dir, "m.txt", "main moves on")?;
    git_ok(&dir, &["cherry-pick", picked.as_str()])?;
    // Squash-merged: two commits, one squashed commit on main.
    git_ok(&dir, &["checkout", "-q", "-b", "squashed"])?;
    commit(&dir, "s.txt", "one")?;
    let squashed_tip = commit(&dir, "s.txt", "two")?;
    git_ok(&dir, &["checkout", "-q", "main"])?;
    git_ok(&dir, &["merge", "-q", "--squash", "squashed"])?;
    git_ok(&dir, &["commit", "-q", "-m", "squashed (#7)"])?;
    // Open work.
    git_ok(&dir, &["checkout", "-q", "-b", "wip"])?;
    commit(&dir, "w.txt", "wip")?;
    git_ok(&dir, &["checkout", "-q", "main"])?;
    // Merged, but checked out in a worktree.
    git_ok(&dir, &["branch", "in-worktree"])?;
    let wt = scratch.0.join("wt");
    git_ok(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            &wt.to_string_lossy(),
            "in-worktree",
        ],
    )?;
    // Pushed and tracked, then deleted on the remote and pruned here: gone.
    git_ok(&dir, &["checkout", "-q", "-b", "gone"])?;
    commit(&dir, "g.txt", "gone work")?;
    git_ok(&dir, &["checkout", "-q", "main"])?;
    git_ok(
        &dir,
        &[
            "push",
            "-q",
            "-u",
            "origin",
            "main",
            "gone",
            "fix/ünïcode-✓",
        ],
    )?;
    git_ok(
        &dir,
        &[
            "branch",
            "-q",
            "-u",
            "origin/fix/ünïcode-✓",
            "fix/ünïcode-✓",
        ],
    )?;
    git_ok(&remote, &["branch", "-D", "gone"])?;
    git_ok(&dir, &["fetch", "-q", "--prune", "origin"])?;
    Ok(Fixture {
        _scratch: scratch,
        dir,
        remote,
        squashed_tip,
    })
}

fn merged_pr(branch: &str, sha: &str, number: u64) -> MergedPullRequest {
    MergedPullRequest {
        number,
        branch: branch.to_owned(),
        sha: sha.to_owned(),
        base: "main".to_owned(),
    }
}

/// A tab on `dir` with what a reload reads, and which branches are merged read as the sidebar
/// has it read.
fn tab_on(dir: &Path, prs: Vec<MergedPullRequest>) -> RepoTab {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), &ctx);
    tab.refs = git::refs(dir).unwrap_or_default();
    tab.remotes = git::remotes(dir).unwrap_or_default();
    tab.worktrees = git::worktrees(dir, None).unwrap_or_default();
    tab.branch_health.merged_prs = prs;
    let bases = tab.health_bases();
    let inputs = Inputs::new(&tab.refs.local, bases, &tab.branch_health.merged_prs);
    let mut cache = branch_health::CherryCache::new();
    let read = branch_health::read(dir, &inputs, &mut cache).unwrap_or_default();
    tab.branch_health.health = Some(read);
    tab.branch_health.cache = cache;
    tab
}

fn names(list: &[Candidate]) -> Vec<&str> {
    list.iter().map(|c| c.name.as_str()).collect()
}

fn reasons_of<'a>(list: &'a [Candidate], name: &str) -> &'a [Reason] {
    list.iter()
        .find(|c| c.name == name)
        .map_or(&[][..], |c| c.reasons.as_slice())
}

#[test]
fn merged_rebased_squashed_and_gone_branches_are_found() -> Result<(), String> {
    let f = fixture("cleanup-found")?;
    let prs = vec![merged_pr("squashed", &f.squashed_tip, 7)];
    let tab = tab_on(&f.dir, prs);
    let found = tab.cleanup_candidates(0);
    // Never main (checked out and the default) nor the branch in a worktree; not open work.
    assert_eq!(
        names(&found),
        ["fix/ünïcode-✓", "gone", "rebased", "squashed"]
    );
    let into = Reason::Merged(Merged::Into("origin/main".to_owned()));
    assert_eq!(reasons_of(&found, "fix/ünïcode-✓"), [into]);
    assert_eq!(
        reasons_of(&found, "rebased"),
        [Reason::Merged(Merged::Rebased("origin/main".to_owned()))]
    );
    assert_eq!(
        reasons_of(&found, "squashed"),
        [Reason::Merged(Merged::Pr(7, "main".to_owned()))]
    );
    assert_eq!(reasons_of(&found, "gone"), [Reason::UpstreamGone]);
    // The merged one is on the remote; gone isn't any more.
    let remote_of = |name: &str| {
        found
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.remote.clone())
    };
    assert_eq!(
        remote_of("fix/ünïcode-✓").map(|(r, n, _)| (r, n)),
        Some(("origin".to_owned(), "fix/ünïcode-✓".to_owned()))
    );
    assert_eq!(remote_of("gone"), None);
    // Merged ones start ticked; one deleted on its remote alone doesn't.
    let ticked: Vec<&str> = found
        .iter()
        .filter(|c| c.preselected())
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(ticked, ["fix/ünïcode-✓", "rebased", "squashed"]);
    Ok(())
}

/// Without the pull request, a squash merge is not found: its commits are not in main, not
/// even by their changes (two squashed into one).
#[test]
fn a_squash_merge_needs_its_pull_request() -> Result<(), String> {
    let f = fixture("cleanup-squash")?;
    let tab = tab_on(&f.dir, Vec::new());
    assert!(!names(&tab.cleanup_candidates(0)).contains(&"squashed"));
    // A pull request merged before the branch got another commit: that commit is not in.
    git_ok(&f.dir, &["checkout", "-q", "squashed"])?;
    commit(&f.dir, "s.txt", "three")?;
    git_ok(&f.dir, &["checkout", "-q", "main"])?;
    let tab = tab_on(&f.dir, vec![merged_pr("squashed", &f.squashed_tip, 7)]);
    assert!(!names(&tab.cleanup_candidates(0)).contains(&"squashed"));
    // A pull request of another branch of that name, from a fork, says nothing.
    let tab = tab_on(&f.dir, vec![merged_pr("squashed", &"f".repeat(40), 8)]);
    assert!(!names(&tab.cleanup_candidates(0)).contains(&"squashed"));
    Ok(())
}

#[test]
fn stale_branches_go_by_their_last_commit() -> Result<(), String> {
    let f = fixture("cleanup-stale")?;
    let tab = tab_on(&f.dir, Vec::new());
    let health = tab.branch_health.health.as_ref().ok_or("no read")?;
    let wip = tab
        .refs
        .local
        .iter()
        .find(|b| b.name == "wip")
        .ok_or("no wip")?;
    let now = git::now();
    assert!(health.reasons(wip, now, 90).is_empty());
    let later = now + 120 * DAY;
    assert_eq!(health.reasons(wip, later, 90), [Reason::Stale(120)]);
    assert!(
        health.reasons(wip, later, 0).is_empty(),
        "0 days: never stale"
    );
    // Every local branch, dated.
    assert_eq!(health.by_date.len(), tab.refs.local.len());
    assert!(
        tab.refs
            .local
            .iter()
            .all(|b| health.dates.contains_key(&b.name))
    );
    Ok(())
}

/// The ticked branches go here and on the remote in one operation; undo brings both back, with
/// what they tracked; redo deletes them again.
#[test]
fn deleting_here_and_on_the_remote_is_undone_and_redone() -> Result<(), String> {
    let f = fixture("cleanup-delete")?;
    let tab = tab_on(&f.dir, vec![merged_pr("squashed", &f.squashed_tip, 7)]);
    let found = tab.cleanup_candidates(0);
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, undo) = delete_many(&chosen, true);
    let Undo::Custom { undo, redo } = undo else {
        return Err("not undoable".to_owned());
    };
    let here = |name: &str| git::resolve(&f.dir, &format!("refs/heads/{name}")).is_ok();
    let there = |name: &str| git::resolve(&f.remote, &format!("refs/heads/{name}")).is_ok();
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_ok(), "{done:?}");
    for c in &found {
        assert!(!here(&c.name), "{} is still here", c.name);
    }
    assert!(!there("fix/ünïcode-✓"));
    assert!(here("main") && here("wip") && here("in-worktree"));
    let entry = UndoEntry {
        id: 0,
        agent: None,
        label: "Delete branches".to_owned(),
        undo: undo.clone(),
        redo,
        heads: None,
    };
    let (undone, kind) = run_steps(&f.dir, &undo, OpKind::Undo(entry.clone()), None);
    assert!(undone.is_ok(), "{undone:?}");
    // The redo, expecting each branch where undoing put it.
    let OpKind::Undo(entry) = kind else {
        return Err("not an undo".to_owned());
    };
    for c in &found {
        let at = git::resolve(&f.dir, &format!("refs/heads/{}", c.name)).ok();
        assert_eq!(at.as_deref(), Some(c.target.as_str()), "{}", c.name);
    }
    assert!(there("fix/ünïcode-✓"));
    let upstream = git_ok(
        &f.dir,
        &["rev-parse", "--abbrev-ref", "fix/ünïcode-✓@{upstream}"],
    )?;
    assert_eq!(upstream, "origin/fix/ünïcode-✓");
    let (redone, _) = run_steps(&f.dir, &entry.redo.clone(), OpKind::Redo(entry), None);
    assert!(redone.is_ok(), "{redone:?}");
    assert!(!here("rebased") && !there("fix/ünïcode-✓"));
    Ok(())
}

/// A branch that got a commit after it was offered stays, and so does everything else: the
/// commit is not what was offered for deletion.
#[test]
fn nothing_is_deleted_once_a_branch_moved() -> Result<(), String> {
    let f = fixture("cleanup-moved")?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, false);
    git_ok(&f.dir, &["checkout", "-q", "rebased"])?;
    commit(&f.dir, "r2.txt", "more")?;
    git_ok(&f.dir, &["checkout", "-q", "main"])?;
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_err());
    for c in &found {
        assert!(git::resolve(&f.dir, &format!("refs/heads/{}", c.name)).is_ok());
    }
    Ok(())
}

/// Someone pushed to a remote branch since it was fetched: the atomic push is refused, and
/// nothing is deleted anywhere.
#[test]
fn a_remote_branch_pushed_to_since_keeps_everything() -> Result<(), String> {
    let f = fixture("cleanup-lease")?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, true);
    let theirs = git_ok(
        &f.dir,
        &["commit-tree", "-p", "HEAD", "-m", "theirs", "HEAD^{tree}"],
    )?;
    let to = format!("{theirs}:refs/heads/fix/ünïcode-✓");
    git_ok(&f.dir, &["push", "-q", &f.remote.to_string_lossy(), &to])?;
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_err());
    for c in &found {
        assert!(git::resolve(&f.dir, &format!("refs/heads/{}", c.name)).is_ok());
    }
    let remote = git::resolve(&f.remote, "refs/heads/fix/ünïcode-✓").ok();
    assert_eq!(remote.as_deref(), Some(theirs.as_str()));
    Ok(())
}

/// Hundreds of branches go in a few commands, each under the command line's length limit, and
/// come back with one undo.
#[test]
fn many_branches_are_deleted_in_batches() -> Result<(), String> {
    let s = Scratch::new("cleanup-many");
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git_ok(&s.0, args)?;
    }
    commit(&s.0, "a.txt", "a")?;
    git_ok(&s.0, &["checkout", "-q", "-b", "side"])?;
    commit(&s.0, "b.txt", "b")?;
    git_ok(&s.0, &["checkout", "-q", "main"])?;
    git_ok(&s.0, &["merge", "-q", "--no-ff", "-m", "m", "side"])?;
    for i in 0..320 {
        git_ok(&s.0, &["branch", &format!("old/branch-{i:03}"), "side"])?;
    }
    let tab = tab_on(&s.0, Vec::new());
    let found = tab.cleanup_candidates(0);
    assert_eq!(found.len(), 321);
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, undo) = delete_many(&chosen, true);
    let deletes: Vec<&Vec<String>> = steps
        .iter()
        .filter(|op| op.get(..2).is_some_and(|a| a == ["branch", "-D"]))
        .collect();
    assert_eq!(deletes.len(), 3);
    assert!(deletes.iter().all(|op| op.len() <= 3 + 150));
    assert!(
        !steps
            .iter()
            .any(|op| op.first().is_some_and(|a| a == "push")),
        "nothing is on a remote"
    );
    let (done, _) = run_steps(&s.0, &steps, OpKind::plain(), None);
    assert!(done.is_ok(), "{done:?}");
    let left = git_ok(&s.0, &["for-each-ref", "--format=%(refname)", "refs/heads"])?;
    assert_eq!(left, "refs/heads/main");
    let Undo::Custom { undo, redo } = undo else {
        return Err("not undoable".to_owned());
    };
    let entry = UndoEntry {
        id: 0,
        agent: None,
        label: "Delete branches".to_owned(),
        undo: undo.clone(),
        redo,
        heads: None,
    };
    let (undone, _) = run_steps(&s.0, &undo, OpKind::Undo(entry), None);
    assert!(undone.is_ok(), "{undone:?}");
    let back = git_ok(&s.0, &["for-each-ref", "--format=%(refname)", "refs/heads"])?;
    assert_eq!(back.lines().count(), 322);
    Ok(())
}

/// An empty repository, or one with only its default branch, offers nothing and reads fine.
#[test]
fn nothing_to_offer_in_a_new_repository() -> Result<(), String> {
    let s = Scratch::new("cleanup-empty");
    git_ok(&s.0, &["init", "-q", "-b", "main"])?;
    let tab = tab_on(&s.0, Vec::new());
    assert!(tab.cleanup_candidates(1).is_empty());
    assert!(tab.branch_health.health.is_some());
    Ok(())
}

/// A branch being bisected (or rebased) in another worktree shows there as detached, so it is
/// offered; git refuses to delete it. That is found before anything is deleted: the other
/// ticked branches, here and on the remote, stay, rather than going with no undo.
#[test]
fn a_branch_in_use_in_a_worktree_stops_everything_before_it_starts() -> Result<(), String> {
    let f = fixture("cleanup-bisect")?;
    let wt = f.dir.with_file_name("bisecting");
    git_ok(&f.dir, &["branch", "busy", "main~1"])?;
    git_ok(
        &f.dir,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "busy"],
    )?;
    git_ok(&wt, &["bisect", "start", "HEAD", "HEAD~2"])?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    assert!(
        names(&found).contains(&"busy"),
        "shown detached, it is offered"
    );
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, true);
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_err());
    for c in &found {
        assert!(
            git::resolve(&f.dir, &format!("refs/heads/{}", c.name)).is_ok(),
            "{}",
            c.name
        );
    }
    assert!(git::resolve(&f.remote, "refs/heads/fix/ünïcode-✓").is_ok());
    Ok(())
}

/// A branch made from a shared one (`checkout -b fix origin/release`) tracks it: deleting the
/// branch "on the remote" must not delete `release`. Only a remote branch of the same name, and
/// never the default branch, goes along.
#[test]
fn only_a_remote_branch_of_the_same_name_goes_along() -> Result<(), String> {
    let f = fixture("cleanup-shared")?;
    git_ok(&f.dir, &["push", "-q", "origin", "main:refs/heads/release"])?;
    git_ok(&f.dir, &["fetch", "-q", "origin"])?;
    git_ok(
        &f.dir,
        &["branch", "-q", "--track", "from-release", "origin/release"],
    )?;
    git_ok(
        &f.dir,
        &["branch", "-q", "--track", "from-main", "origin/main"],
    )?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    for name in ["from-release", "from-main"] {
        let c = found.iter().find(|c| c.name == name).ok_or(name)?;
        assert_eq!(c.remote, None, "{name}");
        assert!(c.preselected(), "{name} is merged");
    }
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, true);
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_ok(), "{done:?}");
    assert!(git::resolve(&f.remote, "refs/heads/release").is_ok());
    assert!(git::resolve(&f.remote, "refs/heads/main").is_ok());
    assert!(git::resolve(&f.remote, "refs/heads/fix/ünïcode-✓").is_err());
    Ok(())
}

/// A branch found merged, then given a commit: until a read sees the commit, it is not offered
/// as merged on the strength of the old read.
#[test]
fn a_branch_that_moved_since_the_read_is_not_merged() -> Result<(), String> {
    let f = fixture("cleanup-moved-read")?;
    let mut tab = tab_on(&f.dir, Vec::new());
    git_ok(&f.dir, &["checkout", "-q", "rebased"])?;
    commit(&f.dir, "new.txt", "new work")?;
    git_ok(&f.dir, &["checkout", "-q", "main"])?;
    // The tab's refs are read again; the health read is the old one still.
    tab.refs = git::refs(&f.dir)?;
    assert!(!names(&tab.cleanup_candidates(0)).contains(&"rebased"));
    Ok(())
}

/// The second read of an unchanged repository asks `git cherry` nothing new: its answers are
/// kept by branch and base commit.
#[test]
fn cherry_answers_are_kept_between_reads() -> Result<(), String> {
    let f = fixture("cleanup-cache")?;
    let tab = tab_on(&f.dir, Vec::new());
    let cached = tab.branch_health.cache.clone();
    assert!(
        cached.values().any(|all_in| *all_in),
        "rebased is cached as in"
    );
    // Answers that say the opposite of git: a read that used them shows it.
    let mut lying: branch_health::CherryCache = cached.into_iter().map(|(k, v)| (k, !v)).collect();
    let bases = tab.health_bases();
    let inputs = Inputs::new(&tab.refs.local, bases, &[]);
    let read = branch_health::read(&f.dir, &inputs, &mut lying)?;
    assert!(!read.merged.contains_key("rebased"));
    Ok(())
}
