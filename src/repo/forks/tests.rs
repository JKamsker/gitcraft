//! Pull requests from forks against real repositories: an upstream and a contributor's fork as
//! bare repositories, and the maintainer's clone of the upstream.

use std::path::{Path, PathBuf};

use super::plan::{self, Setup};
use crate::forge::ForkInfo;
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, Undo, UndoEntry};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft forks-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?} failed: {out:?}");
    out.unwrap_or_default()
}

fn arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn identity(dir: &Path, name: &str) {
    run(dir, &["config", "user.name", name]);
    run(
        dir,
        &["config", "user.email", &format!("{name}@example.org")],
    );
}

fn commit(dir: &Path, file: &str) {
    let _ = std::fs::write(dir.join(file), format!("{file}\n"));
    run(dir, &["add", "--", file]);
    run(dir, &["commit", "-q", "-m", &format!("add {file}")]);
}

/// The upstream, Ann's fork of it with her branch `fix`, and the maintainer's clone.
struct World {
    root: PathBuf,
    upstream: PathBuf,
    fork: PathBuf,
    ann: PathBuf,
    me: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let root = folder(name);
        let upstream = root.join("upstream.git");
        let fork = root.join("ann fork.git");
        let ann = root.join("ann");
        let me = root.join("me");
        let _ = std::fs::create_dir_all(&upstream);
        run(&upstream, &["init", "-q", "--bare", "-b", "main"]);
        // The first commit, through Ann's clone.
        run(&root, &["clone", "-q", &arg(&upstream), &arg(&ann)]);
        identity(&ann, "ann");
        commit(&ann, "base.txt");
        run(&ann, &["push", "-q", "origin", "HEAD:main"]);
        run(
            &root,
            &["clone", "-q", "--bare", &arg(&upstream), &arg(&fork)],
        );
        run(&ann, &["remote", "add", "fork", &arg(&fork)]);
        run(&ann, &["checkout", "-q", "-b", "fix"]);
        commit(&ann, "fix ü.txt");
        run(&ann, &["push", "-q", "fork", "fix"]);
        run(&root, &["clone", "-q", &arg(&upstream), &arg(&me)]);
        identity(&me, "me");
        World {
            root,
            upstream,
            fork,
            ann,
            me,
        }
    }

    fn info(&self) -> ForkInfo {
        ForkInfo {
            number: 7,
            cross: true,
            owner: "ann".into(),
            repo_path: "ann/app".into(),
            clone_https: arg(&self.fork),
            clone_ssh: String::new(),
            branch: "fix".into(),
            base: "main".into(),
            sha: git::resolve(&self.fork, "refs/heads/fix").unwrap_or_default(),
            maintainers_can_edit: true,
            ..ForkInfo::default()
        }
    }

    fn setup(&self) -> Setup {
        plan::read_setup(&self.me).unwrap_or_default()
    }

    fn checkout(&self, locals: &[String]) -> Result<plan::Checkout, String> {
        let remotes = git::remotes(&self.me).unwrap_or_default();
        plan::checkout(&self.info(), &remotes, &self.setup(), locals, "main")
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn custom(kind: OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            label: "x".into(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    }
}

fn normal(plan_undo: Vec<Vec<String>>, redo: Vec<Vec<String>>) -> OpKind {
    OpKind::Normal(
        Undo::Custom {
            undo: plan_undo,
            redo,
        },
        String::new(),
    )
}

#[test]
fn a_forks_branch_is_checked_out_so_that_a_plain_push_goes_back_to_it_and_undo_tidies_up() {
    let w = World::new("checkout");
    let plan = w.checkout(&["main".to_owned()]);
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let (result, kind) = run_steps(
        &w.me,
        &plan.steps,
        normal(plan.undo.clone(), plan.steps.clone()),
        None,
    );
    assert!(result.is_ok(), "{result:?}");
    let current = run(&w.me, &["branch", "--show-current"]);
    let push_remote = run(&w.me, &["config", "branch.fix.pushRemote"]);
    // The maintainer's fix, pushed the way the toolbar's Push does.
    commit(&w.me, "maintainer.txt");
    run(&w.me, &["push", "-q"]);
    let on_fork = git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default();
    let mine = git::head(&w.me);
    let upstream_untouched = git::resolve(&w.upstream, "refs/heads/fix").is_err();
    // The setup is known, and the badge's remote is the fork's.
    let setup = w.setup();
    // Undo refuses: the branch has a commit made since. Without it, it goes through.
    let entry = custom(kind);
    let refused = entry
        .clone()
        .map(|e| run_steps(&w.me, &e.undo.clone(), OpKind::Undo(e), None).0);
    // The commit taken off again (it is on the fork): the branch is as the checkout left it.
    run(&w.me, &["reset", "-q", "--hard", "HEAD~1"]);
    let undone = entry.map(|e| run_steps(&w.me, &e.undo.clone(), OpKind::Undo(e), None).0);
    let remotes = git::remotes(&w.me).unwrap_or_default();
    assert_eq!(current.trim(), "fix");
    assert_eq!(push_remote.trim(), "ann");
    assert_eq!(on_fork, mine);
    assert!(upstream_untouched);
    assert!(setup.pushing_to("ann", "fix").is_some());
    assert_eq!(setup.fork_remotes, ["ann"]);
    assert!(matches!(refused, Some(Err(_))), "{refused:?}");
    assert!(matches!(undone, Some(Ok(_))), "{undone:?}");
    assert!(git::resolve(&w.me, "refs/heads/fix").is_err());
    assert!(!remotes.iter().any(|r| r.name == "ann"));
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "main");
}

#[test]
fn a_branch_name_taken_here_still_pushes_to_the_forks_name() {
    let w = World::new("taken");
    run(&w.me, &["branch", "fix"]);
    let plan = w.checkout(&["main".to_owned(), "fix".to_owned()]);
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let (result, _) = run_steps(&w.me, &plan.steps, OpKind::plain(), None);
    commit(&w.me, "maintainer.txt");
    // As the toolbar's Push does it for such a branch.
    let push = plan::renamed_push(&w.setup(), "ann/fix").unwrap_or_default();
    let args: Vec<&str> = push.iter().map(String::as_str).collect();
    let pushed = git::run(Some(&w.me), &args);
    let on_fork = git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default();
    let mine = git::head(&w.me);
    // Another branch on the same fork still pushes with a plain `git push`: no remote-wide
    // refspec was added.
    let config = git::run_ok_codes(
        Some(&w.me),
        &["config", "--get-all", "remote.ann.push"],
        &[0, 1],
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(plan.local, "ann/fix");
    assert_eq!(config.map(|c| c.trim().to_owned()), Ok(String::new()));
    assert_eq!(plan::renamed_push(&w.setup(), "main"), None);
    assert!(pushed.is_ok(), "{pushed:?}");
    assert_eq!(on_fork, mine);
}

#[test]
fn checking_out_again_reuses_what_is_set_up() {
    let w = World::new("again");
    let first = w.checkout(&["main".to_owned()]);
    let steps = first.map(|p| p.steps).unwrap_or_default();
    let (result, _) = run_steps(&w.me, &steps, OpKind::plain(), None);
    run(&w.me, &["checkout", "-q", "main"]);
    let locals = vec!["main".to_owned(), "fix".to_owned()];
    let second = w.checkout(&locals);
    assert!(result.is_ok(), "{result:?}");
    assert!(second.is_ok_and(|p| !p.new_remote && p.local == "fix" && p.remote == "ann"));
}

#[test]
fn rebasing_onto_the_base_pushes_to_the_fork_and_undo_puts_both_back() {
    let w = World::new("rebase");
    // The base moves on upstream.
    run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
    commit(&w.ann, "upstream.txt");
    run(&w.ann, &["push", "-q", "origin", "up:main"]);
    let steps = w
        .checkout(&["main".to_owned()])
        .map(|p| p.steps)
        .unwrap_or_default();
    let (checked_out, _) = run_steps(&w.me, &steps, OpKind::plain(), None);
    let before = git::head(&w.me);
    let plan = plan::plan_rebase(&w.me, &w.info(), "origin", &w.setup(), None);
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let (result, kind) = run_steps(
        &w.me,
        &plan.steps(),
        normal(plan.undo_steps(), plan.redo_steps()),
        None,
    );
    let rebased = git::head(&w.me);
    let on_fork = git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default();
    let has_base = git::run(
        Some(&w.me),
        &["merge-base", "--is-ancestor", "origin/main", "HEAD"],
    )
    .is_ok();
    let undone = custom(kind).map(|e| run_steps(&w.me, &e.undo.clone(), OpKind::Undo(e), None).0);
    let fork_back = git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default();
    assert!(checked_out.is_ok(), "{checked_out:?}");
    assert_eq!(plan.behind, 1);
    assert!(result.is_ok(), "{result:?}");
    assert_ne!(rebased, before);
    assert_eq!(on_fork, rebased);
    assert!(has_base);
    assert!(matches!(undone, Some(Ok(_))), "{undone:?}");
    assert_eq!(git::head(&w.me), before);
    assert_eq!(fork_back, before);
}

#[test]
fn a_rebase_is_refused_when_the_contributor_pushed_commits_not_here() {
    let w = World::new("contributor-ahead");
    run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
    commit(&w.ann, "upstream.txt");
    run(&w.ann, &["push", "-q", "origin", "up:main"]);
    let steps = w
        .checkout(&["main".to_owned()])
        .map(|p| p.steps)
        .unwrap_or_default();
    let (checked_out, _) = run_steps(&w.me, &steps, OpKind::plain(), None);
    // Ann pushes more to her branch.
    run(&w.ann, &["checkout", "-q", "fix"]);
    commit(&w.ann, "more.txt");
    run(&w.ann, &["push", "-q", "fork", "fix"]);
    let plan = plan::plan_rebase(&w.me, &w.info(), "origin", &w.setup(), None);
    assert!(checked_out.is_ok());
    assert!(plan.is_err_and(|e| e.contains("pull them first")));
}

#[test]
fn unused_fork_remotes_are_removed_and_undo_adds_them_back() {
    let w = World::new("tidy");
    let steps = w
        .checkout(&["main".to_owned()])
        .map(|p| p.steps)
        .unwrap_or_default();
    let (checked_out, _) = run_steps(&w.me, &steps, OpKind::plain(), None);
    let used = w.setup().unused_forks();
    run(&w.me, &["checkout", "-q", "main"]);
    run(&w.me, &["branch", "-D", "fix"]);
    let unused = w.setup().unused_forks();
    let remotes = git::remotes(&w.me).unwrap_or_default();
    let (tidy, undo) = plan::tidy(&remotes, &unused);
    let (removed, _) = run_steps(&w.me, &tidy, OpKind::plain(), None);
    let after = git::remotes(&w.me).unwrap_or_default();
    let (added, _) = run_steps(&w.me, &undo, OpKind::plain(), None);
    let back = w.setup();
    assert!(checked_out.is_ok());
    assert!(used.is_empty());
    assert_eq!(unused, ["ann"]);
    assert!(removed.is_ok() && added.is_ok());
    assert!(!after.iter().any(|r| r.name == "ann"));
    assert_eq!(back.fork_remotes, ["ann"]);
}
