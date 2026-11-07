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
        plan::checkout(&self.info(), &remotes, &self.setup(), locals, "main", None)
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
            id: 0,
            agent: None,
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
    let plan = plan::plan_rebase(
        &w.me,
        &w.info(),
        &git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
        &w.setup(),
        None,
        None,
        None,
    );
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
    let plan = plan::plan_rebase(
        &w.me,
        &w.info(),
        &git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
        &w.setup(),
        None,
        None,
        None,
    );
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

// Recording-only planner: captures the selected base before the job is queued. Its gate
// makes switch-before-delivery deterministic and never invokes a forge or network transport.
type RecordedPlan = Box<dyn FnOnce() -> Result<plan::Rebase, String> + Send>;
type PlanRecording = (std::sync::Arc<std::sync::Mutex<String>>, RecordedPlan);
thread_local! {
    static RECORD_PLAN: std::cell::RefCell<Option<PlanRecording>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn take_plan_recording(upstream: &str) -> Option<RecordedPlan> {
    RECORD_PLAN
        .with(|slot| slot.borrow_mut().take())
        .map(|(seen, work)| {
            *seen.lock().unwrap_or_else(|e| e.into_inner()) = upstream.into();
            work
        })
}
fn recorded_tab(w: &World, ctx: &egui::Context) -> crate::repo::RepoTab {
    use crate::forge::{Forge, Hosting};
    let mut tab = crate::repo::RepoTab::new_deferred(w.me.clone(), ctx);
    tab.remotes = ["origin", "secondary"]
        .map(|name| git::Remote {
            name: name.into(),
            url: format!("https://forge.example/{name}/app"),
        })
        .into();
    tab.forge_remotes = tab
        .remotes
        .iter()
        .map(|r| {
            (
                r.name.clone(),
                Hosting {
                    forge: Forge::Forgejo,
                    host: "forge.example".into(),
                    path: format!("{}/app", r.name),
                    web: "https://forge.example".into(),
                },
            )
        })
        .collect();
    tab.take_forge_selection(false, ctx);
    tab.forks.info.insert(7, w.info());
    tab
}
fn synthetic_plan(w: &World) -> plan::Rebase {
    plan::Rebase {
        local: "fix".into(),
        fork: "ann".into(),
        branch: "fix".into(),
        onto: "refs/remotes/secondary/main".into(),
        head: git::head(&w.me),
        seen: w.info().sha,
        tracking: "refs/remotes/ann/fix".into(),
        behind: 1,
        destinations: Vec::new(),
        base_head: git::head(&w.me),
    }
}
fn record_plan(
    w: &World,
) -> (
    std::sync::Arc<std::sync::Mutex<String>>,
    std::sync::mpsc::Sender<()>,
) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let (tx, rx) = std::sync::mpsc::channel();
    let p = synthetic_plan(w);
    RECORD_PLAN.with(|slot| {
        *slot.borrow_mut() = Some((
            seen.clone(),
            Box::new(move || {
                rx.recv().map_err(|e| e.to_string())?;
                Ok(p)
            }),
        ))
    });
    (seen, tx)
}
fn drain_until(
    tab: &mut crate::repo::RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&crate::repo::RepoTab) -> bool,
) {
    for _ in 0..300 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        done(tab),
        "async continuation did not settle: {:?}",
        tab.status
    );
}
#[test]
fn review14_rebase_planning_captures_primary_without_changing_transport() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("primary-plan");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    tab.forks.info.insert(7, w.info());
    let config = std::fs::read(w.me.join(".git/config")).unwrap_or_default();
    let (seen, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    assert_eq!(*seen.lock().unwrap_or_else(|e| e.into_inner()), "secondary");
    assert_eq!(tab.default_remote().as_deref(), Some("origin"));
    assert_eq!(
        std::fs::read(w.me.join(".git/config")).unwrap_or_default(),
        config
    );
}
#[test]
fn review14_queued_rebase_plan_cannot_repopulate_new_forge() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("queued-plan");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    let _ = gate.send(());
    let msg = tab.rx.recv_timeout(std::time::Duration::from_secs(5));
    assert!(msg.is_ok());
    if let Ok(msg) = msg {
        tab.handle(msg, &ctx);
    }
    assert!(tab.dialog.is_none(), "stale plan installed a confirmation");
    assert!(
        tab.forks.running.is_none(),
        "stale plan repopulated running"
    );
}
#[test]
fn review14_primary_switch_discards_rebase_confirmation() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("confirm-plan");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    assert!(
        tab.dialog.is_none(),
        "stale confirmation can still rebase and force-push"
    );
    assert!(tab.forks.running.is_none());
}

type StepRecording = Box<dyn FnMut(&[String])>;
thread_local! {
    static BEFORE_STEP: std::cell::RefCell<Option<StepRecording>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn before_step(op: &[String]) {
    BEFORE_STEP.with(|slot| {
        if let Some(record) = slot.borrow_mut().as_mut() {
            record(op);
        }
    });
}
fn rebase_ready(w: &World) -> plan::Rebase {
    run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
    commit(&w.ann, "upstream.txt");
    run(&w.ann, &["push", "-q", "origin", "up:main"]);
    let steps = w
        .checkout(&["main".into()])
        .map(|p| p.steps)
        .unwrap_or_default();
    assert!(run_steps(&w.me, &steps, OpKind::plain(), None).0.is_ok());
    let p = plan::plan_rebase(
        &w.me,
        &w.info(),
        &git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
        &w.setup(),
        None,
        None,
        None,
    );
    assert!(p.is_ok(), "{p:?}");
    p.unwrap_or_else(|_| synthetic_plan(w))
}
fn confirmation_id(tab: &crate::repo::RepoTab) -> u64 {
    tab.forks.scope.as_ref().map_or(0, |s| s.id)
}
#[test]
fn review14_divergent_primary_base_reaches_only_the_owned_fork() {
    let w = World::new("divergent");
    let secondary = w.root.join("secondary.git");
    run(
        &w.root,
        &["clone", "--bare", "-q", &arg(&w.upstream), &arg(&secondary)],
    );
    // Origin and the selected repository advance independently from the fork point.
    run(&w.ann, &["checkout", "-q", "main"]);
    commit(&w.ann, "origin-only.txt");
    run(&w.ann, &["push", "-q", "origin", "main"]);
    run(&w.ann, &["checkout", "-q", "-B", "selected", "HEAD~1"]);
    commit(&w.ann, "selected-only.txt");
    run(&w.ann, &["push", "-q", &arg(&secondary), "HEAD:main"]);
    run(&w.me, &["remote", "add", "secondary", &arg(&secondary)]);
    let steps = w
        .checkout(&["main".into()])
        .map(|p| p.steps)
        .unwrap_or_default();
    assert!(run_steps(&w.me, &steps, OpKind::plain(), None).0.is_ok());
    let primary = git::Remote {
        name: "secondary".into(),
        url: arg(&secondary),
    };
    let p = plan::plan_rebase(&w.me, &w.info(), &primary, &w.setup(), None, None, None);
    assert!(p.is_ok(), "{p:?}");
    let Ok(p) = p else { return };
    let origin = git::head(&w.upstream);
    let config = std::fs::read(w.me.join(".git/config")).unwrap_or_default();
    assert_eq!(p.onto, "refs/remotes/secondary/main");
    assert!(
        run_steps(
            &w.me,
            &p.steps(),
            normal(p.undo_steps(), p.redo_steps()),
            None
        )
        .0
        .is_ok()
    );
    assert!(w.me.join("selected-only.txt").exists());
    assert!(!w.me.join("origin-only.txt").exists());
    assert_eq!(
        git::head(&w.me),
        git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default()
    );
    assert_eq!(git::head(&w.upstream), origin);
    assert_eq!(
        std::fs::read(w.me.join(".git/config")).unwrap_or_default(),
        config
    );
}
#[test]
fn review14_pending_and_confirmed_plans_expire_on_rename_url_edit_removal_and_aba() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for pending in [true, false] {
        for change in ["rename", "url", "remove", "aba", "unrelated"] {
            let w = World::new(&format!("expire-{pending}-{change}"));
            let ctx = egui::Context::default();
            let mut tab = recorded_tab(&w, &ctx);
            let (_, gate) = record_plan(&w);
            tab.plan_fork_rebase(7, &ctx);
            let id = confirmation_id(&tab);
            if !pending {
                let _ = gate.send(());
                drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
            }
            match change {
                "rename" => {
                    tab.remotes[0].name = "renamed".into();
                    tab.forge_remotes[0].0 = "renamed".into();
                    tab.take_forge_selection(true, &ctx);
                }
                "url" => {
                    tab.remotes[0].url = "https://forge.example:4000/origin/app".into();
                    tab.forge_remotes[0].1.web = "https://forge.example:4000".into();
                    tab.take_forge_selection(true, &ctx);
                }
                "remove" => {
                    tab.remotes.remove(0);
                    tab.forge_remotes.remove(0);
                    tab.take_forge_selection(true, &ctx);
                }
                "aba" => {
                    tab.set_primary_forge(Some("secondary".into()), &ctx);
                    tab.set_primary_forge(Some("origin".into()), &ctx);
                }
                _ => {
                    tab.remotes.push(git::Remote {
                        name: "unrelated".into(),
                        url: "/owned/local".into(),
                    });
                    tab.take_forge_selection(true, &ctx);
                }
            }
            if pending {
                let _ = gate.send(());
                let msg = tab.rx.recv_timeout(std::time::Duration::from_secs(5));
                assert!(msg.is_ok(), "planner did not deliver");
                if let Ok(msg) = msg {
                    tab.handle(msg, &ctx);
                }
            }
            assert!(tab.dialog.is_none(), "{pending}/{change}");
            assert!(tab.forks.running.is_none(), "{pending}/{change}");
            tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
            assert!(
                tab.busy_op.is_none(),
                "queued confirmation survived {change}"
            );
            assert_eq!(git::head(&w.me), git::head(&w.upstream));
        }
    }
}
#[test]
fn review14_disk_remote_changes_refuse_before_any_rewrite_or_push() {
    for change in ["base-url", "fork-url", "pushurl", "remove", "rename"] {
        let w = World::new(&format!("disk-{change}"));
        let p = rebase_ready(&w);
        let before = git::head(&w.me);
        let fork_before = git::resolve(&w.fork, "fix").unwrap_or_default();
        match change {
            "base-url" => {
                run(&w.me, &["remote", "set-url", "origin", &arg(&w.fork)]);
            }
            "fork-url" => {
                run(&w.me, &["remote", "set-url", "ann", &arg(&w.upstream)]);
            }
            "pushurl" => {
                run(&w.me, &["config", "remote.ann.pushurl", &arg(&w.upstream)]);
            }
            "remove" => {
                run(&w.me, &["remote", "remove", "origin"]);
            }
            _ => {
                run(&w.me, &["remote", "rename", "origin", "renamed"]);
            }
        }
        let result = run_steps(&w.me, &p.steps(), OpKind::plain(), None).0;
        assert!(result.is_err(), "{change}: {result:?}");
        assert_eq!(git::head(&w.me), before, "{change}");
        assert_eq!(
            git::resolve(&w.fork, "fix").unwrap_or_default(),
            fork_before,
            "{change}"
        );
    }
}
#[test]
fn review14_running_rebase_stops_before_push_and_records_local_undo() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("running-switch");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let scope = super::guard::Scope::new(
        tab.forge_generation,
        tab.hosting.clone().unwrap_or(crate::forge::Hosting {
            forge: crate::forge::Forge::Forgejo,
            host: "forge.example".into(),
            web: "https://forge.example".into(),
            path: "origin/app".into(),
        }),
        git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
    );
    let invalidated = scope.clone();
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                invalidated.invalidate();
            }
        }))
    });
    let (result, kind, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
        false,
        None,
        Some(&scope),
    );
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("primary forge changed")),
        "{result:?}"
    );
    assert_ne!(git::head(&w.me), p.head);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    let label = "Rebase #7 onto main and push".to_owned();
    tab.busy_op = Some(label.clone());
    tab.forks.scope = Some(scope);
    tab.forks.scope_label = Some(label.clone());
    tab.forks.running = Some((label.clone(), p.clone()));
    tab.clear_forge_forks();
    assert!(
        tab.forks.running.is_some(),
        "retain active execution for completion cleanup"
    );
    tab.handle(
        crate::repo::Msg::OpDone {
            label,
            result,
            kind,
            refresh: crate::repo::Refresh::All,
            quiet: false,
            stopped: 3,
        },
        &ctx,
    );
    drain_until(&mut tab, &ctx, |t| {
        t.timeline
            .entries
            .iter()
            .any(|e| e.label.ends_with("(here only)"))
    });
    assert!(tab.busy_op.is_none());
    assert!(tab.forks.running.is_none());
    assert!(tab.forks.after_conflict.is_none());
    assert!(tab.last_op.is_none());
    let (undo, _) = p.local_undo(&git::head(&w.me));
    assert!(run_steps(&w.me, &undo, OpKind::plain(), None).0.is_ok());
    assert_eq!(git::head(&w.me), p.head);
}
#[test]
fn review14_disk_edit_between_rebase_and_push_stops_push() {
    let w = World::new("running-edit");
    let p = rebase_ready(&w);
    let repo = w.me.clone();
    let other = arg(&w.upstream);
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                run(&repo, &["config", "remote.ann.pushurl", &other]);
            }
        }))
    });
    let result = run_steps(&w.me, &p.steps(), OpKind::plain(), None).0;
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(result.is_err(), "{result:?}");
    assert_ne!(git::head(&w.me), p.head);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert!(git::resolve(&w.upstream, "fix").is_err());
}

type ApprovalRecording = Box<dyn FnMut(u64) -> Result<(), String> + Send>;
thread_local! {
    static RECORD_APPROVAL: std::cell::RefCell<Option<ApprovalRecording>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn take_approval_recording() -> Option<ApprovalRecording> {
    RECORD_APPROVAL.with(|slot| slot.borrow_mut().take())
}
#[test]
fn review14_approval_blocks_selection_and_external_rebind_stops_remaining_runs() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("approval");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    run(&w.me, &["remote", "set-url", "origin", &tab.remotes[0].url]);
    run(&w.me, &["remote", "add", "secondary", &tab.remotes[1].url]);
    tab.forks.waiting.insert(w.info().sha, vec![11, 12]);
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = calls.clone();
    let (arrived, first) = std::sync::mpsc::channel();
    let (gate, wait) = std::sync::mpsc::channel();
    RECORD_APPROVAL.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |run| {
            seen.lock().unwrap_or_else(|e| e.into_inner()).push(run);
            arrived.send(()).map_err(|e| e.to_string())?;
            wait.recv().map_err(|e| e.to_string())?;
            Ok(())
        }))
    });
    tab.approve_runs(7, &ctx);
    tab.approve_runs(7, &ctx);
    assert!(
        first
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    );
    assert!(tab.busy_op.is_some());
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    assert_eq!(
        tab.primary_forge_remote(),
        Some("origin"),
        "UI switching must wait for mutation"
    );
    // External config/catalog changes still invalidate the remaining captured work.
    tab.remotes[0].url = "https://forge.example:4000/origin/app".into();
    tab.forge_remotes[0].1.web = "https://forge.example:4000".into();
    tab.take_forge_selection(true, &ctx);
    let _ = gate.send(());
    let msg = tab.rx.recv_timeout(std::time::Duration::from_secs(5));
    assert!(msg.is_ok());
    if let Ok(msg) = msg {
        tab.handle(msg, &ctx);
    }
    assert_eq!(*calls.lock().unwrap_or_else(|e| e.into_inner()), [11]);
    assert!(tab.busy_op.is_none());
    assert!(!tab.forks.approving);
    assert!(tab.forks.scope.is_none());
    assert!(tab.forks.running.is_none());
}
#[test]
fn review14_queued_fork_actions_and_replaced_confirmations_cannot_run() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("queued-actions");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let old = tab.forge_generation;
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    tab.forks.info.insert(7, w.info());
    for act in [
        super::ForkAct::Checkout(7, old),
        super::ForkAct::Rebase(7, old),
        super::ForkAct::Approve(7, old),
    ] {
        tab.fork_act(act, &ctx);
        assert!(tab.busy_op.is_none());
        assert!(tab.forks.scope.is_none());
        assert!(tab.forks.approve_armed.is_none());
    }
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    let first = confirmation_id(&tab);
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    assert_ne!(first, confirmation_id(&tab));
    tab.fork_act(super::ForkAct::ConfirmRebase(first), &ctx);
    assert!(
        tab.busy_op.is_none(),
        "old confirmation must not execute a replacement plan"
    );
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    // Simulate Cancel; the next frame clears pending running and invalidates its token.
    tab.dialog = None;
    tab.forks_frame(&ctx);
    assert!(tab.forks.running.is_none());
    assert!(tab.forks.scope.is_none());
}
#[test]
fn review14_effective_destination_validation_uses_shared_canonical_identity() {
    use crate::forge::{Forge, Hosting};
    for forge in Forge::ALL {
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            web: "https://forge.example:4000/proxy".into(),
            path: "Group/Project".into(),
        };
        for (url, valid) in [
            ("https://forge.example:4000/proxy/Group/Project.git", true),
            ("ssh://git@forge.example:2222/Group/Project.git", true),
            ("https://forge.example:3000/proxy/Group/Project.git", false),
            ("https://forge.example:4000/Group/Project.git", false),
            ("https://forge.example:4000/PROXY/Group/Project.git", false),
            (
                "https://forge.example:4000/proxy/group/project.git",
                forge != Forge::GitLab,
            ),
        ] {
            let d = super::guard::Destination {
                name: "selected".into(),
                fetch: vec![url.into()],
                push: vec![url.into()],
            };
            assert_eq!(
                d.validate(Some(&h), false).is_ok(),
                valid,
                "{forge:?} / {url}"
            );
            assert_eq!(
                d.validate(Some(&h), true).is_ok(),
                valid,
                "{forge:?} / {url}"
            );
        }
    }
}

#[test]
fn review14_confirmed_owned_rebase_executes_and_cleans_running() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("confirm-execution");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    // This fixture assigns web metadata to owned local transports; no forge is called.
    tab.remotes = git::remotes(&w.me).unwrap_or_default();
    tab.forge_remotes.truncate(1);
    tab.take_forge_selection(true, &ctx);
    let primary = tab.remotes.iter().find(|r| r.name == "origin").cloned();
    assert!(primary.is_some());
    let Some(primary) = primary else { return };
    assert!(tab.hosting.is_some());
    let Some(hosting) = tab.hosting.clone() else {
        return;
    };
    let scope = super::guard::Scope::new(tab.forge_generation, hosting, primary);
    let id = scope.id;
    tab.forks.scope = Some(scope);
    let label = "Rebase #7 onto main and push".to_owned();
    tab.forks.scope_label = Some(label.clone());
    tab.forks.running = Some((label, p.clone()));
    tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
    assert!(tab.busy_op.is_some());
    drain_until(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.loading && !t.timeline.entries.is_empty()
    });
    assert!(tab.forks.running.is_none());
    assert_eq!(
        git::resolve(&w.fork, "fix").unwrap_or_default(),
        git::head(&w.me)
    );
    assert_ne!(git::head(&w.me), p.head);
    assert!(tab.timeline.entries.iter().any(|e| e.undo.is_some()));
}

#[test]
fn review14_checkout_rejects_other_instance_pushurl_before_fetch_or_config_changes() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("checkout-pushurl");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    run(&w.me, &["remote", "set-url", "origin", &tab.remotes[0].url]);
    let url = "https://forge.example/ann/app.git";
    run(&w.me, &["remote", "add", "ann", url]);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "https://forge.example:4000/ann/app.git",
        ],
    );
    tab.remotes = git::remotes(&w.me).unwrap_or_default();
    let mut info = w.info();
    info.clone_https = url.into();
    tab.forks.info.insert(7, info);
    let head = git::head(&w.me);
    let config = std::fs::read(w.me.join(".git/config")).unwrap_or_default();
    tab.checkout_fork(7, &ctx);
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert_eq!(git::head(&w.me), head);
    assert_eq!(
        std::fs::read(w.me.join(".git/config")).unwrap_or_default(),
        config
    );
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "main");
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(s, _)| s.contains("matching push destination")),
        "{:?}",
        tab.status
    );
}

#[test]
fn review14_stale_conflict_and_auth_completion_cannot_offer_a_fork_push() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for error in ["CONFLICT: could not apply", "fatal: Authentication failed"] {
        let w = World::new(if error.starts_with("CONFLICT") {
            "stale-conflict"
        } else {
            "stale-auth"
        });
        let p = rebase_ready(&w);
        let ctx = egui::Context::default();
        let mut tab = recorded_tab(&w, &ctx);
        assert!(tab.hosting.is_some());
        let Some(hosting) = tab.hosting.clone() else {
            return;
        };
        let scope = super::guard::Scope::new(tab.forge_generation, hosting, tab.remotes[0].clone());
        let label = "Rebase #7 onto main and push".to_owned();
        tab.forks.scope = Some(scope);
        tab.forks.scope_label = Some(label.clone());
        tab.forks.running = Some((label.clone(), p.clone()));
        tab.busy_op = Some(label.clone());
        tab.clear_forge_forks();
        tab.handle(
            crate::repo::Msg::OpDone {
                label,
                result: Err(error.into()),
                refresh: crate::repo::Refresh::All,
                kind: normal(p.undo_steps(), p.redo_steps()),
                quiet: false,
                stopped: 2,
            },
            &ctx,
        );
        drain_until(&mut tab, &ctx, |t| !t.loading);
        assert!(tab.busy_op.is_none());
        assert!(tab.forks.running.is_none());
        assert!(tab.forks.after_conflict.is_none());
        assert!(tab.last_op.is_none());
        assert!(!matches!(
            tab.dialog,
            Some(crate::repo::Dialog::Credentials { .. })
        ));
        assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    }
}
