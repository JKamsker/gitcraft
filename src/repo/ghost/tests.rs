//! Each operation's ghost run against a real repository: what it says is what doing the
//! operation then gives (where the branch ends up, its tree, the commits left on no ref, the
//! commit and files that conflict), and the repository is just as it was after the ghost run.

use super::op::{Facts, GhostOp};
use super::plan::{Outcome, plan};
use crate::git::dry_run::GitVersion;
use crate::git::dry_run::tests::TestRepo;
use crate::git::{self};
use crate::repo::pull::PullMode;

fn facts(repo: &TestRepo) -> Result<Facts, String> {
    let status = git::status(&repo.dir)?;
    Ok(Facts {
        refs: git::refs(&repo.dir)?,
        remotes: git::remotes(&repo.dir)?
            .into_iter()
            .map(|r| r.name)
            .collect(),
        dirty: !status.is_clean(),
        credentials: None,
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
    assert!(o.notes.iter().any(|n| n.contains("isn't merged")));
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

/// A clone of a bare `origin`, with main pushed, and another clone that pushes to it.
struct Remote {
    _origin: TestRepo,
    other: TestRepo,
    here: TestRepo,
}

fn remote(name: &str) -> Result<Remote, String> {
    let origin = TestRepo::new(&format!("{name}-origin"))?;
    origin.git(&["config", "core.bare", "true"])?;
    let url = origin.dir.join(".git").to_string_lossy().into_owned();
    let here = TestRepo::new(name)?;
    here.commit(&[("a", "1\n")], "base")?;
    here.git(&["remote", "add", "origin", &url])?;
    here.git(&["push", "-q", "-u", "origin", "main"])?;
    let other = TestRepo::new(&format!("{name}-other"))?;
    other.git(&["remote", "add", "origin", &url])?;
    other.git(&["fetch", "-q", "origin"])?;
    other.git(&["reset", "-q", "--hard", "origin/main"])?;
    // Fetching with these must still change no ref.
    here.git(&["config", "fetch.prune", "true"])?;
    here.git(&["config", "remote.origin.tagOpt", ""])?;
    Ok(Remote {
        _origin: origin,
        other,
        here,
    })
}

/// A pull's ghost run fetches what the remote has (into no ref) and shows each mode's result:
/// a fast-forward, the merge (with its tree), the rebase; as pulling then does.
#[test]
fn a_pull_ghost_is_what_pulling_does() -> Result<(), String> {
    let r = remote("ghost-pull")?;
    r.other
        .commit(&[("b", "theirs\n")], "pushed by someone else")?;
    r.other.git(&["tag", "v1"])?;
    r.other.git(&["push", "-q", "origin", "main", "v1"])?;
    let theirs = r.other.rev("HEAD")?;
    // Up to date as far as this clone knows: only the remote has the new commit.
    let ff = ghost(&r.here, &GhostOp::Pull(PullMode::FastForwardOnly))?;
    assert_eq!(moved_to(&ff, "refs/heads/main"), Some(theirs.clone()));
    assert_eq!(
        moved_to(&ff, "refs/remotes/origin/main"),
        Some(theirs.clone())
    );
    assert_eq!(ff.created.len(), 1);
    assert!(
        r.here
            .git(&["rev-parse", "-q", "--verify", "refs/tags/v1"])
            .is_err()
    );
    // Diverged: each mode as it would go.
    r.here.commit(&[("c", "mine\n")], "mine")?;
    let refused = ghost(&r.here, &GhostOp::Pull(PullMode::FastForwardOnly))?;
    assert!(refused.refused.is_some());
    let merged = ghost(&r.here, &GhostOp::Pull(PullMode::Merge))?;
    let rebased = ghost(&r.here, &GhostOp::Pull(PullMode::Rebase))?;
    let old = r.here.rev("HEAD")?;
    r.here.git(&["pull", "-q", "--no-rebase", "--no-edit"])?;
    let merge_tree = r.here.rev("HEAD^{tree}")?;
    r.here.git(&["reset", "-q", "--hard", &old])?;
    r.here.git(&["pull", "-q", "--rebase"])?;
    let rebase_tree = r.here.rev("HEAD^{tree}")?;
    let files: Vec<&str> = merged
        .files
        .iter()
        .flatten()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(files, ["b"]);
    assert_eq!(merged.created[0].parents, [old.clone(), theirs.clone()]);
    assert_eq!(merged.tree, Some(merge_tree));
    assert_eq!(rebased.tree, Some(rebase_tree.clone()));
    let tip = moved_to(&rebased, "refs/heads/main").ok_or("main doesn't move")?;
    assert_eq!(r.here.rev(&format!("{tip}^{{tree}}"))?, rebase_tree);
    assert_eq!(lost(&rebased), really_lost(&r.here, &old)?);
    Ok(())
}

/// A force push's ghost run lists what leaves the remote's branch, with who made it.
#[test]
fn a_force_push_ghost_lists_what_leaves_the_remote() -> Result<(), String> {
    let r = remote("ghost-push")?;
    r.other.commit(&[("b", "theirs\n")], "pushed by Bo")?;
    r.other.git(&["config", "user.name", "Bo"])?;
    r.other
        .git(&["commit", "-q", "--amend", "--reset-author", "--no-edit"])?;
    r.other.git(&["push", "-q", "origin", "main"])?;
    r.here.git(&["fetch", "-q", "origin"])?;
    let theirs = r.here.rev("origin/main")?;
    r.here.commit(&[("c", "mine\n")], "mine")?;
    let o = ghost(
        &r.here,
        &GhostOp::ForcePush {
            branch: "main".to_owned(),
        },
    )?;
    assert!(o.from_remote);
    assert_eq!(
        moved_to(&o, "refs/remotes/origin/main"),
        Some(r.here.rev("main")?)
    );
    assert_eq!(lost(&o), std::slice::from_ref(&theirs));
    assert_eq!(o.lost[0].author, "Bo");
    r.here.git(&[
        "push",
        "-q",
        &format!("--force-with-lease=refs/heads/main:{theirs}"),
        "origin",
        "main",
    ])?;
    assert_eq!(really_lost(&r.here, &theirs)?, [theirs]);
    Ok(())
}

/// A hover that ends mid-way stops the work.
#[test]
fn a_cancelled_ghost_run_stops() -> Result<(), String> {
    let repo = diverged("ghost-cancel")?;
    let facts = facts(&repo)?;
    let op = GhostOp::Rebase {
        onto: "refs/heads/main".to_owned(),
        label: "main".to_owned(),
    };
    let out = plan(&repo.dir, &op, &facts, GitVersion(2, 41, 0), &|| true);
    assert_eq!(out.err().as_deref(), Some(crate::git::dry_run::CANCELLED));
    Ok(())
}

/// Waits for the tab's background work to come in, until `done`.
fn wait(
    tab: &mut crate::repo::RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&crate::repo::RepoTab) -> bool,
) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Offers `op` as a menu entry under the pointer does, for the frame `ghost_frame` runs in.
fn offer_now(ctx: &egui::Context, op: &GhostOp) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(super::offer_id(), (pass, op.clone())));
}

/// The pointer leaves the entry (a later frame offers nothing).
fn leave(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<(u64, GhostOp)>(super::offer_id()));
}

/// Hovering an entry starts nothing until the delay has passed; then the ghost graph comes in
/// the background, with the rebase's copies on top; once the pointer leaves, it goes and the
/// work is told to stop. The repository is as it was.
#[test]
fn a_hover_shows_the_ghost_graph_and_leaving_ends_it() -> Result<(), String> {
    let repo = diverged("ghost-hover")?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(repo.dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.history_gen > 0 && !t.loading);
    let rows = tab.commits.len();
    let before = repo.snapshot()?;
    let op = GhostOp::Rebase {
        onto: "refs/heads/main".to_owned(),
        label: "main".to_owned(),
    };
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    assert!(tab.ghost.job.is_none(), "started before the delay");
    // The pointer has rested on it long enough.
    if let Some((_, at)) = &mut tab.ghost.hover {
        *at -= super::HOVER_DELAY;
    }
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    let cancel = tab
        .ghost
        .job
        .as_ref()
        .map(|j| std::sync::Arc::clone(&j.cancel));
    assert!(cancel.is_some(), "no ghost run started");
    wait(&mut tab, &ctx, |t| t.ghost.shown.is_some());
    assert!(matches!(
        tab.ghost_strip_state(),
        Some(super::Strip::Ready(..))
    ));
    let (mut commits, mut graph, mut index) = (
        tab.commits.clone(),
        std::mem::take(&mut tab.graph),
        tab.index.clone(),
    );
    assert!(tab.ghost.swap_in(&mut commits, &mut graph, &mut index));
    assert_eq!(commits.len(), rows + 2);
    assert_eq!(tab.ghost.kind(0), super::Kind::New);
    tab.ghost.swap_out(&mut commits, &mut graph, &mut index);
    assert_eq!(commits.len(), rows);
    tab.graph = graph;
    // The same entry again: from the cache, nothing worked out again.
    let key = tab.ghost_key(op.clone());
    assert!(tab.ghost.cached(&key).is_some());
    // The pointer leaves.
    leave(&ctx);
    tab.ghost_frame(&ctx);
    assert!(tab.ghost.shown.is_none() && tab.ghost.hover.is_none());
    assert_eq!(repo.snapshot()?, before);
    Ok(())
}

/// A hover that ends while the work runs tells it to stop, and what it finds isn't shown.
#[test]
fn a_hover_that_ends_early_stops_its_work() -> Result<(), String> {
    let repo = diverged("ghost-hover-cancel")?;
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new(repo.dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.history_gen > 0 && !t.loading);
    let op = GhostOp::ResetHard {
        to: repo.rev("HEAD~1")?,
    };
    offer_now(&ctx, &op);
    ctx.input_mut(|i| i.modifiers.alt = true);
    tab.ghost_frame(&ctx);
    if let Some((_, at)) = &mut tab.ghost.hover {
        *at -= super::ALT_DELAY;
    }
    offer_now(&ctx, &op);
    tab.ghost_frame(&ctx);
    let cancel = tab
        .ghost
        .job
        .as_ref()
        .map(|j| std::sync::Arc::clone(&j.cancel));
    leave(&ctx);
    tab.ghost_frame(&ctx);
    assert!(cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)));
    std::thread::sleep(std::time::Duration::from_millis(300));
    tab.poll(&ctx);
    assert!(tab.ghost.shown.is_none());
    Ok(())
}
