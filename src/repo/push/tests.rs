//! Pushes refused for real: two clones of a bare repository, one pushing behind the other's
//! back, and what the check, the lease, Sync's pull and a first push make of it.

use std::path::{Path, PathBuf};

use super::check::{self, Checked, Question, Rejection, Shape, Target};
use super::porcelain::{self, RefLine, Refusal};
use super::{Attempt, Chain, force_push_ops, publish_args, pull_from, refused_target};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::pull::PullMode;
use crate::repo::{OpKind, ops};

/// A bare repository with `main`, and two clones of it, `mine` and `theirs`; gone when dropped.
struct World {
    root: PathBuf,
    bare: PathBuf,
    mine: PathBuf,
    theirs: PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {args:?} in {}: {e}", dir.display()))
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn world(name: &str) -> Result<World, String> {
    let root = std::env::temp_dir().join(format!("gitcraft-push-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let w = World {
        bare: root.join("remote.git"),
        mine: root.join("mine"),
        theirs: root.join("theirs"),
        root,
    };
    run(
        &w.root,
        &["init", "-q", "--bare", "-b", "main", &path(&w.bare)],
    )?;
    for dir in [&w.mine, &w.theirs] {
        run(&w.root, &["clone", "-q", &path(&w.bare), &path(dir)])?;
        run(dir, &["config", "user.name", "t"])?;
        run(dir, &["config", "user.email", "t@t"])?;
        run(dir, &["checkout", "-q", "-B", "main"])?;
    }
    commit(&w.mine, "base")?;
    run(&w.mine, &["push", "-q", "-u", "origin", "main"])?;
    run(&w.theirs, &["pull", "-q", "origin", "main"])?;
    run(&w.theirs, &["branch", "-q", "-u", "origin/main"])?;
    Ok(w)
}

fn commit(dir: &Path, message: &str) -> Result<String, String> {
    run(dir, &["commit", "-q", "--allow-empty", "-m", message])?;
    Ok(git::head(dir))
}

/// A file `f.txt` saying `text`, committed.
fn commit_file(dir: &Path, text: &str) -> Result<String, String> {
    std::fs::write(dir.join("f.txt"), text).map_err(|e| e.to_string())?;
    run(dir, &["add", "f.txt"])?;
    commit(dir, text.trim())
}

fn amend(dir: &Path, message: &str) -> Result<String, String> {
    run(
        dir,
        &["commit", "-q", "--amend", "--allow-empty", "-m", message],
    )
}

fn remote_main(w: &World) -> String {
    git::resolve(&w.bare, "refs/heads/main").unwrap_or_default()
}

/// Pushes as the toolbar does, from `dir`: the error, if it failed.
fn plain_push(dir: &Path) -> Option<String> {
    run_steps(
        dir,
        &ops(&[&["push", "--porcelain"]]),
        OpKind::plain(),
        None,
    )
    .0
    .err()
}

fn question(branch: &str, target: Option<Target>, to: &str, remotes: &[&str]) -> Question {
    Question {
        branch: branch.to_owned(),
        target,
        to: to.to_owned(),
        remotes: remotes.iter().map(|r| (*r).to_owned()).collect(),
        publish: false,
        moved: false,
    }
}

fn origin(branch: &str) -> Target {
    Target {
        remote: "origin".to_owned(),
        branch: branch.to_owned(),
    }
}

/// The refused line of a push of `main`, and why.
fn refused_line(error: &str) -> Result<(RefLine, Refusal), String> {
    porcelain::refused(error, "refs/heads/main", None)
        .ok_or_else(|| format!("no ref line: {error}"))
}

/// Pushes `mine` as the toolbar does, and checks the refusal as the tab does.
fn push_and_check(w: &World) -> Result<Box<Rejection>, String> {
    let error = plain_push(&w.mine).ok_or("the push went through")?;
    let (line, why) = refused_line(&error)?;
    if why != Refusal::Behind {
        return Err(format!("refused otherwise: {why:?}"));
    }
    let q = question("main", None, &line.to, &["origin"]);
    rejection(check::check(&w.mine, &q, None)?)
}

fn rejection(checked: Checked) -> Result<Box<Rejection>, String> {
    match checked {
        Checked::Rejected(r) => Ok(r),
        other => Err(format!("not a rejection: {other:?}")),
    }
}

#[test]
fn a_branch_only_behind_is_told_so() -> Result<(), String> {
    let w = world("behind")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    // `mine` has nothing new, but is behind: pushing its main would take the remote back.
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (0, 1));
    assert_eq!(r.shape, Shape::Behind);
    assert_eq!(r.remote, remote_main(&w));
    assert_eq!(r.local, git::head(&w.mine));
    assert_eq!(r.missing.len(), 1);
    assert_eq!(r.missing[0].subject, "theirs");
    assert_eq!(r.target, origin("main"));
    Ok(())
}

#[test]
fn both_moving_on_is_diverged_not_rewritten() -> Result<(), String> {
    let w = world("diverged")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (1, 1));
    assert_eq!(r.shape, Shape::Diverged);
    Ok(())
}

#[test]
fn an_amend_after_pushing_is_a_rewrite() -> Result<(), String> {
    let w = world("rewritten")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    amend(&w.mine, "first, better")?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (1, 1));
    assert_eq!(r.shape, Shape::Rewritten);
    assert_eq!(r.missing[0].subject, "first");
    Ok(())
}

/// Pushed, then someone else pushed on top, then amended here: their commit was never here, so
/// it is not called a rewrite of mine (the force push would drop their work).
#[test]
fn a_commit_by_someone_else_on_top_is_not_a_rewrite() -> Result<(), String> {
    let w = world("rewritten-and-theirs")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    run(&w.theirs, &["pull", "-q"])?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    amend(&w.mine, "first, better")?;
    let r = push_and_check(&w)?;
    assert_eq!(r.shape, Shape::Diverged);
    let subjects: Vec<&str> = r.missing.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["theirs", "first"]);
    Ok(())
}

/// Someone pushes after the check: the force push is refused and their commit stays; checked
/// again, it is named, and the force push goes.
#[test]
fn a_lease_that_moved_refuses_the_force_push() -> Result<(), String> {
    let w = world("lease")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    amend(&w.mine, "amended")?;
    let r = push_and_check(&w)?;
    // Meanwhile.
    run(&w.theirs, &["pull", "-q"])?;
    let theirs = commit(&w.theirs, "theirs, after the check")?;
    run(&w.theirs, &["push", "-q"])?;
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    let error = forced.err().ok_or("the lease must refuse")?;
    let why = porcelain::refused(&error, "refs/heads/main", Some(&r.local)).map(|(_, why)| why);
    assert_eq!(why, Some(Refusal::Moved), "{error}");
    assert_eq!(remote_main(&w), theirs);
    let mut q = question(
        "main",
        Some(r.target.clone()),
        "refs/heads/main",
        &["origin"],
    );
    q.moved = true;
    let again = rejection(check::check(&w.mine, &q, None)?)?;
    assert!(again.moved);
    assert_eq!(again.remote, theirs);
    let named = again.missing.iter().map(|c| c.subject.as_str());
    assert!(named.into_iter().any(|s| s == "theirs, after the check"));
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&again), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    assert_eq!(remote_main(&w), git::head(&w.mine));
    Ok(())
}

/// The branch here moves after the check: the force push pushes nothing.
#[test]
fn a_branch_that_moved_here_refuses_the_force_push() -> Result<(), String> {
    let w = world("local-moved")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    let before = remote_main(&w);
    commit(&w.mine, "mine, after the check")?;
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_err());
    assert_eq!(remote_main(&w), before);
    Ok(())
}

#[test]
fn pull_then_push_joins_the_two() -> Result<(), String> {
    for mode in [PullMode::Rebase, PullMode::Merge] {
        let w = world(&format!("pull-push-{mode:?}"))?;
        let theirs = commit(&w.theirs, "theirs")?;
        run(&w.theirs, &["push", "-q"])?;
        commit(&w.mine, "mine")?;
        let r = push_and_check(&w)?;
        let steps = [
            pull_from(mode, &r.target),
            ops(&[&["push", "--porcelain"]]).remove(0),
        ];
        let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
        assert!(result.is_ok(), "{mode:?}: {result:?}");
        let remote = remote_main(&w);
        assert_eq!(remote, git::head(&w.mine));
        run(&w.mine, &["merge-base", "--is-ancestor", &theirs, &remote])?;
        let subjects = run(&w.mine, &["log", "--format=%s", &remote])?;
        assert!(subjects.lines().any(|s| s == "mine"), "{mode:?}");
    }
    Ok(())
}

/// A pull that conflicts stops there: nothing is pushed.
#[test]
fn a_conflicting_pull_pushes_nothing() -> Result<(), String> {
    let w = world("conflict")?;
    commit_file(&w.theirs, "theirs\n")?;
    run(&w.theirs, &["push", "-q"])?;
    commit_file(&w.mine, "mine\n")?;
    let before = remote_main(&w);
    let steps = [
        pull_from(PullMode::Merge, &origin("main")),
        ops(&[&["push", "--porcelain"]]).remove(0),
    ];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    let error = result.err().ok_or("the pull must conflict")?;
    assert!(error.contains("CONFLICT"), "{error}");
    assert_eq!(remote_main(&w), before);
    Ok(())
}

#[test]
fn a_branch_with_no_upstream_is_published_and_tracks_it() -> Result<(), String> {
    let w = world("publish")?;
    run(&w.mine, &["checkout", "-q", "-b", "feature"])?;
    let tip = commit(&w.mine, "feature")?;
    let steps = [publish_args("feature", "origin")];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::resolve(&w.bare, "refs/heads/feature"), Ok(tip));
    let upstream = run(
        &w.mine,
        &["rev-parse", "--abbrev-ref", "feature@{upstream}"],
    )?;
    assert_eq!(upstream.trim(), "origin/feature");
    Ok(())
}

/// A first push refused because the remote has a branch of that name with other commits: it
/// is checked where it went, and the force push sets the upstream as the first push would have.
#[test]
fn a_refused_first_push_is_checked_where_it_went() -> Result<(), String> {
    let w = world("publish-refused")?;
    run(&w.theirs, &["checkout", "-q", "-b", "feature"])?;
    commit(&w.theirs, "their feature")?;
    run(&w.theirs, &["push", "-q", "-u", "origin", "feature"])?;
    run(&w.mine, &["checkout", "-q", "-b", "feature"])?;
    commit(&w.mine, "my feature")?;
    let steps = [publish_args("feature", "origin")];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    let error = result.err().ok_or("the first push must be refused")?;
    let (line, why) =
        porcelain::refused(&error, "refs/heads/feature", None).ok_or("no ref line")?;
    assert_eq!(why, Refusal::Behind);
    let mut q = question("feature", Some(origin("feature")), &line.to, &["origin"]);
    q.publish = true;
    let r = rejection(check::check(&w.mine, &q, None)?)?;
    assert_eq!(r.shape, Shape::Diverged);
    assert!(r.publish);
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    let upstream = run(
        &w.mine,
        &["rev-parse", "--abbrev-ref", "feature@{upstream}"],
    )?;
    assert_eq!(upstream.trim(), "origin/feature");
    Ok(())
}

/// A hook on the remote says no: the refusal is the remote's, with its words.
#[cfg(unix)]
#[test]
fn a_hook_refusal_is_the_remotes_and_keeps_its_message() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let w = world("hook")?;
    let hook = w.bare.join("hooks").join("pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'main is protected here' >&2\nexit 1\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    commit(&w.mine, "mine")?;
    let error = plain_push(&w.mine).ok_or("the hook must refuse")?;
    let (_, why) = refused_line(&error)?;
    assert!(matches!(why, Refusal::Remote(_)), "{error}");
    assert!(error.contains("main is protected here"), "{error}");
    Ok(())
}

#[test]
fn a_remote_branch_deleted_is_gone() -> Result<(), String> {
    let w = world("gone")?;
    let q = question(
        "main",
        Some(origin("elsewhere")),
        "refs/heads/elsewhere",
        &["origin"],
    );
    let checked = check::check(&w.mine, &q, None)?;
    let gone = Checked::Gone {
        branch: "main".to_owned(),
        target: origin("elsewhere"),
        publish: false,
    };
    assert_eq!(checked, gone);
    Ok(())
}

/// With two remotes, the check goes where git pushes: `branch.<b>.pushRemote` first.
#[test]
fn the_push_remote_is_the_one_checked() -> Result<(), String> {
    let w = world("push-remote")?;
    let fork = path(&w.root.join("fork.git"));
    run(&w.root, &["clone", "-q", "--bare", &path(&w.bare), &fork])?;
    run(&w.mine, &["remote", "add", "fork", &fork])?;
    run(&w.mine, &["config", "branch.main.pushRemote", "fork"])?;
    // Their commit on the fork only.
    run(&w.theirs, &["remote", "add", "fork", &fork])?;
    commit(&w.theirs, "on the fork")?;
    run(&w.theirs, &["push", "-q", "fork", "main"])?;
    commit(&w.mine, "mine")?;
    let error = plain_push(&w.mine).ok_or("the fork must refuse")?;
    let (line, _) = refused_line(&error)?;
    let q = question("main", None, &line.to, &["origin", "fork"]);
    let r = rejection(check::check(&w.mine, &q, None)?)?;
    assert_eq!(r.target.remote, "fork");
    assert_eq!(r.missing[0].subject, "on the fork");
    Ok(())
}

/// A commit pushed, then undone here: the branch is only behind, but what the remote has was
/// here, so it is told as a rewrite (with the force push), not "pull to catch up".
#[test]
fn an_undone_commit_after_pushing_is_a_rewrite() -> Result<(), String> {
    let w = world("undone")?;
    commit(&w.mine, "to undo")?;
    run(&w.mine, &["push", "-q"])?;
    run(&w.mine, &["reset", "-q", "--keep", "HEAD~1"])?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (0, 1));
    assert_eq!(r.shape, Shape::Rewritten);
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    assert_eq!(remote_main(&w), git::head(&w.mine));
    Ok(())
}

/// Fetched from one repository, pushed to another (`pushurl`): what is checked is the
/// repository the push was refused by.
#[test]
fn the_push_url_is_the_one_checked() -> Result<(), String> {
    let w = world("push-url")?;
    let fork = path(&w.root.join("fork.git"));
    run(&w.root, &["clone", "-q", "--bare", &path(&w.bare), &fork])?;
    run(&w.mine, &["config", "remote.origin.pushurl", &fork])?;
    run(&w.theirs, &["remote", "add", "fork", &fork])?;
    commit(&w.theirs, "only on the push url")?;
    run(&w.theirs, &["push", "-q", "fork", "main"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    assert_eq!(r.target, origin("main"));
    assert_eq!(r.missing[0].subject, "only on the push url");
    assert_eq!(
        git::resolve(Path::new(&fork), "refs/heads/main"),
        Ok(r.remote.clone())
    );
    Ok(())
}

#[test]
fn a_refused_push_is_looked_at_where_the_line_says() {
    // A publish sent to `me/main` by the remote's push refspec: that is what is checked.
    assert_eq!(
        refused_target(Some(origin("main")), "refs/heads/me/main"),
        Some(Some(origin("me/main")))
    );
    // A plain push: the remote is left to git's config.
    assert_eq!(refused_target(None, "refs/heads/main"), Some(None));
    // Gerrit's review refs are no branch to look at or replace.
    assert_eq!(refused_target(Some(origin("main")), "refs/for/main"), None);
}

/// Sync's push waits for what started after its pull (updating the submodules), then runs.
#[test]
fn syncs_push_waits_for_the_submodule_update_after_the_pull() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/sync-chain".into(), &ctx);
    let push = Attempt {
        label: "Sync: push".to_owned(),
        branch: "main".to_owned(),
        target: None,
        commit: None,
        publish: false,
        done: None,
    };
    tab.push_flow.then_push = Some(Chain {
        after: "Sync: pull".to_owned(),
        push,
        pulled: false,
    });
    // The pull ended, and the submodule update started right after it.
    tab.busy_op = Some("Update submodules".to_owned());
    tab.push_op_ended("Sync: pull", &Ok(String::new()), &ctx);
    let waiting = tab
        .push_flow
        .then_push
        .as_ref()
        .map(|c| (c.after.clone(), c.pulled));
    assert_eq!(waiting, Some(("Update submodules".to_owned(), true)));
    // It ended (even failing): the push is started. With no branch read, it says HEAD is
    // detached, which shows it ran.
    tab.busy_op = None;
    tab.push_op_ended("Update submodules", &Err("failed".to_owned()), &ctx);
    assert!(tab.push_flow.then_push.is_none());
    assert!(matches!(
        tab.push_flow.banner,
        Some(super::Banner::Detached)
    ));
}

/// A failed pull stops Sync: nothing is pushed, and the status says so.
#[test]
fn a_failed_sync_pull_pushes_nothing() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/sync-stop".into(), &ctx);
    let push = Attempt {
        label: "Sync: push".to_owned(),
        branch: "main".to_owned(),
        target: None,
        commit: None,
        publish: false,
        done: None,
    };
    tab.push_flow.then_push = Some(Chain {
        after: "Sync: pull".to_owned(),
        push,
        pulled: false,
    });
    tab.push_op_ended(
        "Sync: pull",
        &Err("CONFLICT (content): f.txt".to_owned()),
        &ctx,
    );
    assert!(tab.push_flow.then_push.is_none());
    assert!(tab.push_flow.banner.is_none());
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, err)| (s.contains("nothing was pushed"), err));
    assert_eq!(status, Some((true, true)));
}
