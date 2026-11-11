//! Each operation's ghost run against a real repository: what it says is what doing the
//! operation then gives (where the branch ends up, its tree, the commits left on no ref, the
//! commit and files that conflict), and the repository is just as it was after the ghost run.

use super::op::{Facts, GhostOp};
use super::plan::{Outcome, plan};
use crate::git::dry_run::GitVersion;
use crate::git::dry_run::tests::TestRepo;
use crate::git::{self};
use crate::repo::pull::PullMode;

#[test]
fn recording_ending_timeline_hover_cancels_only_its_owned_read_scope() {
    let parent = crate::tasks::Cancellation::default();
    let token = parent.child();
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut ghost = super::Ghost {
        job: Some(super::Job {
            id: 1,
            key: super::Key {
                op: GhostOp::Timeline {
                    state: Default::default(),
                    label: "fixture".into(),
                },
                refs: 0,
            },
            cancel: stopped.clone(),
            _scope: crate::tasks::Scope(token.clone()),
        }),
        ..Default::default()
    };
    ghost.end();
    assert!(token.is_cancelled());
    assert!(!parent.is_cancelled());
    assert!(stopped.load(std::sync::atomic::Ordering::Relaxed));
}

fn facts(repo: &TestRepo) -> Result<Facts, String> {
    let status = git::status(&repo.dir)?;
    Ok(Facts {
        refs: git::refs(&repo.dir)?,
        remotes: git::remotes(&repo.dir)?
            .into_iter()
            .map(|r| r.name)
            .collect(),
        dirty: super::op::tracked_changes(&status),
    })
}

/// The ghost run of `op` on each engine's git version, checking it leaves the repository as
/// it was; the outcome on the git that runs the tests.
fn ghost(repo: &TestRepo, op: &GhostOp) -> Result<Outcome, String> {
    let facts = facts(repo)?;
    let before = repo.snapshot()?;
    let real = GitVersion::of(&repo.dir)?;
    for version in [GitVersion(2, 30, 0), GitVersion(2, 41, 0)] {
        plan(&repo.dir, op, &facts, version, &|| false)?;
        assert_eq!(
            repo.snapshot()?,
            before,
            "{op:?} on git {version} changed it"
        );
    }
    let outcome = plan(&repo.dir, op, &facts, real, &|| false)?;
    assert_eq!(repo.snapshot()?, before, "{op:?} changed the repository");
    Ok(outcome)
}

/// Where the ghost run says `full` ends up.
fn moved_to(o: &Outcome, full: &str) -> Option<String> {
    o.moves.iter().find(|m| m.full == full)?.to.clone()
}

fn lost(o: &Outcome) -> Vec<String> {
    let mut l: Vec<String> = o.lost.iter().map(|c| c.hash.clone()).collect();
    l.sort();
    l
}

/// What `old` has that no ref reaches any more, after the operation ran.
fn really_lost(repo: &TestRepo, old: &str) -> Result<Vec<String>, String> {
    let mut l: Vec<String> = repo
        .git(&["rev-list", old, "--not", "--all"])?
        .lines()
        .map(str::to_owned)
        .collect();
    l.sort();
    Ok(l)
}

/// main: base, m1 (a); feature (checked out): f1 (b), f2 (c), from base.
fn diverged(name: &str) -> Result<TestRepo, String> {
    let repo = TestRepo::new(name)?;
    let base = repo.commit(&[("a", "1\n")], "base")?;
    repo.commit(&[("a", "2\n")], "main changes a")?;
    repo.git(&["switch", "-q", "-c", "feature", &base])?;
    repo.commit(&[("b", "b\n")], "feature adds b")?;
    repo.commit(&[("c", "c\n")], "feature adds c")?;
    Ok(repo)
}

#[test]
fn a_rebase_ghost_is_what_rebasing_does() -> Result<(), String> {
    let repo = diverged("ghost-rebase")?;
    let old = repo.rev("HEAD")?;
    let o = ghost(
        &repo,
        &GhostOp::Rebase {
            onto: "refs/heads/main".to_owned(),
            label: "main".to_owned(),
        },
    )?;
    let tip = moved_to(&o, "refs/heads/feature").ok_or("feature doesn't move")?;
    assert_eq!(o.copies, 2);
    assert_eq!(o.created[0].hash, tip);
    assert_eq!(o.created[0].summary, "feature adds c");
    assert_eq!(o.created[1].parents, [repo.rev("main")?]);
    let files: Vec<&str> = o.files.iter().flatten().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["a"]);
    repo.git(&["rebase", "-q", "main"])?;
    assert_eq!(
        repo.rev(&format!("{tip}^{{tree}}"))?,
        repo.rev("HEAD^{tree}")?
    );
    assert_eq!(lost(&o), really_lost(&repo, &old)?);
    assert_eq!(o.replaced, 2);
    assert!(o.vanish.contains(&old));
    Ok(())
}

#[test]
fn a_conflicting_rebase_ghost_stops_where_rebasing_stops() -> Result<(), String> {
    let repo = TestRepo::new("ghost-rebase-conflict")?;
    let base = repo.commit(&[("a", "1\n")], "base")?;
    repo.commit(&[("a", "main\n")], "main changes a")?;
    repo.git(&["switch", "-q", "-c", "feature", &base])?;
    repo.commit(&[("b", "b\n")], "feature adds b")?;
    let second = repo.commit(&[("a", "feature\n")], "feature changes a")?;
    let o = ghost(
        &repo,
        &GhostOp::Rebase {
            onto: "refs/heads/main".to_owned(),
            label: "main".to_owned(),
        },
    )?;
    let conflict = o.conflict.clone().ok_or("no conflict")?;
    assert!(repo.git(&["rebase", "main"]).is_err());
    let stopped = repo.rev("REBASE_HEAD")?;
    let files = repo.git(&["diff", "--name-only", "--diff-filter=U"])?;
    repo.git(&["rebase", "--abort"])?;
    assert_eq!((conflict.step, conflict.of), (2, 2));
    assert_eq!(conflict.commit.map(|c| c.hash), Some(stopped.clone()));
    assert_eq!(stopped, second);
    assert_eq!(conflict.files, files.lines().collect::<Vec<_>>());
    assert!(o.moves.is_empty() && o.lost.is_empty());
    Ok(())
}

#[test]
fn a_hard_reset_ghost_is_what_resetting_does() -> Result<(), String> {
    let repo = diverged("ghost-reset")?;
    let old = repo.rev("HEAD")?;
    let to = repo.rev("HEAD~2")?;
    // Uncommitted work, which the reset throws away.
    std::fs::write(repo.dir.join("b"), "changed\n").map_err(|e| e.to_string())?;
    let o = ghost(&repo, &GhostOp::ResetHard { to: to.clone() })?;
    assert_eq!(moved_to(&o, "refs/heads/feature"), Some(to.clone()));
    let mut files: Vec<String> = o.files.iter().flatten().map(|f| f.path.clone()).collect();
    files.sort();
    let changed = repo.git(&["diff", "--name-only", &to])?;
    repo.git(&["reset", "-q", "--hard", &to])?;
    assert_eq!(files, changed.lines().collect::<Vec<_>>());
    assert_eq!(files, ["b", "c"]);
    // As the reset changes them: both go.
    assert!(
        o.files.iter().flatten().all(|f| f.status == 'D'),
        "{:?}",
        o.files
    );
    assert_eq!(lost(&o), really_lost(&repo, &old)?);
    assert_eq!(o.lost.len(), 2);
    assert_eq!(o.lost[0].author, "Ana");
    Ok(())
}

#[test]
fn a_delete_ghost_lists_what_deleting_loses() -> Result<(), String> {
    let repo = diverged("ghost-delete")?;
    repo.git(&["switch", "-q", "main"])?;
    let old = repo.rev("feature")?;
    let o = ghost(
        &repo,
        &GhostOp::DeleteBranch {
            branch: "feature".to_owned(),
            remote: false,
        },
    )?;
    assert_eq!(o.moves[0].to, None);
    assert!(o.warnings.iter().any(|n| n.contains("isn't merged")));
    repo.git(&["branch", "-q", "-D", "feature"])?;
    assert_eq!(lost(&o), really_lost(&repo, &old)?);
    assert_eq!(o.lost.len(), 2);
    Ok(())
}

#[test]
fn a_drop_ghost_is_what_dropping_does() -> Result<(), String> {
    let repo = diverged("ghost-drop")?;
    let dropped = repo.rev("HEAD~1")?;
    let old = repo.rev("HEAD")?;
    let o = ghost(
        &repo,
        &GhostOp::DropCommit {
            hash: dropped.clone(),
        },
    )?;
    let tip = moved_to(&o, "refs/heads/feature").ok_or("feature doesn't move")?;
    assert_eq!(o.copies, 1);
    // What the quick edit runs: the commits after it replayed onto its parent.
    repo.git(&["rebase", "-q", "--onto", &format!("{dropped}^"), &dropped])?;
    assert_eq!(
        repo.rev(&format!("{tip}^{{tree}}"))?,
        repo.rev("HEAD^{tree}")?
    );
    assert_eq!(lost(&o), really_lost(&repo, &old)?);
    assert!(o.lost.iter().any(|c| c.hash == dropped));
    assert_eq!(o.replaced, 1);
    Ok(())
}

// Keep fixture helpers and qualified test names in this module.
include!("tests/remote.rs");

// Keep fixture helpers and qualified test names in this module.
include!("tests/hover.rs");

/// An untracked file stops no rebase, and the ghost run doesn't say it would.
#[test]
fn an_untracked_file_does_not_refuse_a_rebase() -> Result<(), String> {
    let repo = diverged("ghost-untracked")?;
    std::fs::write(repo.dir.join("notes.txt"), "mine\n").map_err(|e| e.to_string())?;
    let o = ghost(
        &repo,
        &GhostOp::Rebase {
            onto: "refs/heads/main".to_owned(),
            label: "main".to_owned(),
        },
    )?;
    assert_eq!(o.refused, None);
    repo.git(&["rebase", "-q", "main"])?;
    // A change to a tracked file does refuse.
    std::fs::write(repo.dir.join("a"), "edited\n").map_err(|e| e.to_string())?;
    let o = ghost(
        &repo,
        &GhostOp::Rebase {
            onto: "refs/heads/main".to_owned(),
            label: "main".to_owned(),
        },
    )?;
    assert!(o.refused.is_some());
    Ok(())
}

/// Dropping a commit that a later one undoes leaves that later one empty: the interactive
/// rebase a quick edit runs stops there, and so does the ghost run.
#[test]
fn a_drop_that_empties_a_commit_stops_where_the_rebase_stops() -> Result<(), String> {
    let repo = TestRepo::new("ghost-drop-empty")?;
    repo.commit(&[("f", "1\n")], "base")?;
    let c = repo.commit(&[("f", "2\n")], "f to 2")?;
    let d = repo.commit(&[("f", "1\n")], "f back to 1")?;
    repo.commit(&[("g", "g\n")], "adds g")?;
    let o = ghost(&repo, &GhostOp::DropCommit { hash: c.clone() })?;
    let stop = o.conflict.clone().ok_or("no stop")?;
    let dropped = repo.git(&[
        "-c",
        "sequence.editor=sed -i -e 1s/^pick/drop/",
        "rebase",
        "-i",
        &format!("{c}^"),
    ]);
    let stopped = repo.dir.join(".git/rebase-merge").exists();
    let _ = repo.git(&["rebase", "--abort"]);
    assert!(dropped.is_err() && stopped, "{dropped:?}");
    assert_eq!((stop.step, stop.of), (1, 2));
    assert_eq!(stop.commit.map(|c| c.hash), Some(d));
    assert!(stop.files.is_empty() && o.moves.is_empty());
    Ok(())
}

/// A commit of the branch that upstream has cherry-picked (and changed since) is left out by
/// `git pull --rebase`, which compares with the new upstream: no conflict, the same tree.
#[test]
fn a_pull_rebase_leaves_out_what_upstream_picked() -> Result<(), String> {
    let r = remote("ghost-pull-picked")?;
    let mine = r.here.commit(&[("a", "2\n")], "mine")?;
    r.here
        .git(&["push", "-q", "origin", &format!("{mine}:refs/heads/picked")])?;
    r.other.git(&["fetch", "-q", "origin"])?;
    r.other.git(&["cherry-pick", "-x", &mine])?;
    r.other.commit(&[("a", "3\n")], "theirs on top")?;
    r.other.git(&["push", "-q", "origin", "main"])?;
    r.here.commit(&[("c", "c\n")], "more of mine")?;
    r.here.git(&["fetch", "-q", "origin"])?;
    let o = ghost(&r.here, &GhostOp::Pull(PullMode::Rebase))?;
    assert!(o.conflict.is_none(), "{:?}", o.conflict);
    r.here.git(&["pull", "-q", "--rebase"])?;
    assert_eq!(o.tree, Some(r.here.rev("HEAD^{tree}")?));
    Ok(())
}

/// With `rebase.updateRefs`, a branch on one of the rebased commits moves to its copy, as
/// git moves it; its old commit is then lost too.
#[test]
fn update_refs_moves_the_stacked_branch_along() -> Result<(), String> {
    let repo = diverged("ghost-update-refs")?;
    repo.git(&["config", "rebase.updateRefs", "true"])?;
    repo.git(&["branch", "stack", "HEAD~1"])?;
    let old = repo.rev("HEAD")?;
    let o = ghost(
        &repo,
        &GhostOp::Rebase {
            onto: "refs/heads/main".to_owned(),
            label: "main".to_owned(),
        },
    )?;
    let stack = moved_to(&o, "refs/heads/stack").ok_or("stack doesn't move")?;
    repo.git(&["rebase", "-q", "main"])?;
    assert_eq!(
        repo.rev(&format!("{stack}^{{tree}}"))?,
        repo.rev("stack^{tree}")?
    );
    assert_eq!(lost(&o), really_lost(&repo, &old)?);
    Ok(())
}

/// `git branch -d` checks a branch with an upstream against the upstream, not HEAD: merged
/// into the checked-out branch but not pushed, it refuses, and the ghost run says so.
#[test]
fn delete_says_unpushed_is_unmerged() -> Result<(), String> {
    let r = remote("ghost-delete-upstream")?;
    r.here.git(&["switch", "-q", "-c", "feat"])?;
    r.here.git(&["push", "-q", "-u", "origin", "feat"])?;
    r.here.commit(&[("f", "f\n")], "not pushed")?;
    r.here.git(&["switch", "-q", "main"])?;
    r.here.git(&["merge", "-q", "--ff-only", "feat"])?;
    let o = ghost(
        &r.here,
        &GhostOp::DeleteBranch {
            branch: "feat".to_owned(),
            remote: false,
        },
    )?;
    assert!(
        o.warnings.iter().any(|n| n.contains("isn't merged")),
        "{:?}",
        o.notes
    );
    assert!(r.here.git(&["branch", "-d", "feat"]).is_err());
    Ok(())
}

/// A hover that ends before its result is drawn takes back the scroll it would have made,
/// rather than leaving it for the graph's next frame.
#[test]
fn an_ended_hover_leaves_no_scroll_behind() {
    let mut ghost = super::Ghost {
        shift: Some(super::Shift::In(12)),
        ..Default::default()
    };
    ghost.end();
    assert_eq!(ghost.take_shift(), None);
}

/// The key of an outcome changes when a tracked file changes (a rebase then refuses), not when
/// an untracked file appears.
#[test]
fn the_key_follows_what_a_rebase_checks() -> Result<(), String> {
    let repo = diverged("ghost-key")?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(repo.dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.history_gen > 0 && !t.loading);
    let op = GhostOp::Rebase {
        onto: "refs/heads/main".to_owned(),
        label: "main".to_owned(),
    };
    let clean = tab.ghost_key(op.clone());
    let file = |path: &str, code, area| crate::git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area,
    };
    tab.work
        .unstaged
        .push(file("notes.txt", '?', crate::git::Area::Untracked));
    let with_untracked = tab.ghost_key(op.clone());
    tab.work
        .unstaged
        .push(file("b", 'M', crate::git::Area::Unstaged));
    let with_tracked = tab.ghost_key(op);
    assert_eq!(clean, with_untracked);
    assert_ne!(clean, with_tracked);
    Ok(())
}

#[test]
fn timeline_hover_restores_multiple_branch_positions_without_writing() -> Result<(), String> {
    let repo = TestRepo::new("undo-tree-ghost")?;
    repo.commit(&[("base.txt", "base\n")], "base")?;
    let old = crate::repo::timeline::RefState::of(&git::refs(&repo.dir)?);
    repo.commit(&[("later.txt", "later\n")], "later")?;
    let before = repo.snapshot()?;
    let outcome = ghost(
        &repo,
        &GhostOp::Timeline {
            state: old.clone(),
            label: "Before later".into(),
        },
    )?;
    let branch = old.branch;
    assert_eq!(
        moved_to(&outcome, &format!("refs/heads/{branch}")),
        Some(old.head)
    );
    assert!(
        outcome
            .files
            .as_ref()
            .is_some_and(|files| files.iter().any(|f| f.path == "later.txt"))
    );
    assert_eq!(repo.snapshot()?, before);
    Ok(())
}
