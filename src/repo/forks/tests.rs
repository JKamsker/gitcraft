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
    assert!(plan.is_err_and(|e| e.contains("pull it first")));
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
    for _ in 0..3000 {
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
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
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
fn review14_confirmed_owned_rebase_executes_and_cleans_running()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("confirm-execution");
    let ctx = egui::Context::default();
    let (p, mut tab) = refresh_stable_rebase(&w, &ctx)?;
    let id = confirmation_id(&tab);
    tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
    assert!(tab.busy_op.is_some());
    drain_until(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.loading && !t.timeline.entries.is_empty()
    });
    // Rebase is a local operation. Its undo is retained before the distinct push
    // checklist is approved; a completed rewrite alone must not publish anything.
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert_ne!(git::head(&w.me), p.head);
    assert!(tab.timeline.entries.iter().any(|e| e.undo.is_some()));
    let approved = crate::repo::push_preflight::test_support::approve(&mut tab, &ctx);
    assert!(approved.is_ok(), "{approved:?}");
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert!(tab.forks.running.is_none());
    assert_eq!(
        git::resolve(&w.fork, "fix").unwrap_or_default(),
        git::head(&w.me)
    );
    assert_ne!(git::head(&w.me), p.head);
    assert!(tab.timeline.entries.iter().any(|e| e.undo.is_some()));
    Ok(())
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
            .is_some_and(|n| n.0.contains("matching push destination")),
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

#[test]
fn review14_other_tab_preference_aba_stops_worker_before_first_tab_polls() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("cross-tab-worker");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let first = recorded_tab(&w, &ctx);
    let mut second = recorded_tab(&w, &ctx);
    assert!(first.hosting.is_some());
    let Some(hosting) = first.hosting.clone() else {
        return;
    };
    let scope = super::guard::Scope::new(
        first.forge_generation,
        hosting,
        git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
    )
    .with_forge_lifetime(&ctx, first.repo_key());
    assert!(
        scope.check().is_ok(),
        "another tab reading the same preference must preserve the lifetime"
    );
    let generation = first.forge_generation;
    second.set_primary_forge(Some("secondary".into()), &ctx);
    second.set_primary_forge(Some("origin".into()), &ctx);
    assert_eq!(
        first.forge_generation, generation,
        "the running tab has not polled/rebound"
    );
    let before = git::head(&w.me);
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        OpKind::plain(),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("primary forge changed")),
        "{result:?}"
    );
    assert_eq!(git::head(&w.me), before);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
}

#[cfg(unix)]
#[test]
fn review19_first_checkout_accepts_canonical_https_to_ssh_rewrite() {
    use crate::forge::{Forge, Hosting};
    use std::os::unix::fs::PermissionsExt;
    let w = World::new("rewrite-first-checkout");
    let primary = git::Remote {
        name: "origin".into(),
        url: "https://github.com/acme/app.git".into(),
    };
    run(&w.me, &["remote", "set-url", "origin", &primary.url]);
    run(
        &w.me,
        &[
            "config",
            "url.git@github.com:ann/.insteadOf",
            "https://github.com/ann/",
        ],
    );
    // SSH is an owned local upload-pack fixture: it cannot connect to a forge.
    let quoted = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("local ssh");
    let log = w.root.join("ssh-recording");
    let script = format!(
        "#!/bin/sh\nallowed=no\nfor arg do\n  [ \"$arg\" = \"git-upload-pack 'ann/app.git'\" ] && allowed=yes\ndone\n[ \"$allowed\" = yes ] || exit 97\nprintf fetch >> {}\nexec git-upload-pack {}\n",
        quoted(&log),
        quoted(&w.fork)
    );
    assert!(std::fs::write(&ssh, script).is_ok());
    assert!(std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).is_ok());
    run(&w.me, &["config", "core.sshCommand", &quoted(&ssh)]);
    run(&w.me, &["config", "ssh.variant", "ssh"]);
    let h = Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        web: "https://github.com".into(),
        path: "acme/app".into(),
    };
    let mut info = w.info();
    info.clone_https = "https://github.com/ann/app.git".into();
    info.clone_ssh = "git@github.com:ann/app.git".into();
    let p = plan::checkout(
        &info,
        std::slice::from_ref(&primary),
        &w.setup(),
        &["main".into()],
        "main",
        Some(&h),
    );
    assert!(p.is_ok(), "{p:?}");
    let Ok(p) = p else { return };
    assert!(p.new_remote);
    assert_eq!(p.url, info.clone_https);
    let fork_hosting = Hosting {
        path: info.repo_path.clone(),
        ..h.clone()
    };
    let scope = super::guard::Scope::new(1, h, primary).with_fork(
        p.remote.clone(),
        p.url.clone(),
        fork_hosting,
        true,
    );
    let (result, _, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(
        run(&w.me, &["config", "branch.fix.pushRemote"]).trim(),
        "ann"
    );
    assert_eq!(
        run(&w.me, &["config", "remote.ann.gitcraftFork"]).trim(),
        "true"
    );
    assert_eq!(
        run(&w.me, &["remote", "get-url", "ann"]).trim(),
        "git@github.com:ann/app.git"
    );
    assert_eq!(std::fs::read_to_string(log).unwrap_or_default(), "fetch");
    assert_eq!(git::head(&w.me), w.info().sha);
}

// A real refresh must rediscover the same forge while these fixtures wait for the
// separate push checklist. The SSH spelling supplies that identity; this strict
// helper executes only the owned upstream's upload-pack, never a network client.
fn refresh_stable_rebase(
    w: &World,
    ctx: &egui::Context,
) -> Result<(plan::Rebase, crate::repo::RepoTab), Box<dyn std::error::Error>> {
    let _ = rebase_ready(w);
    let quoted = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("owned upstream ssh");
    let script = format!(
        "#!/bin/sh\nhost=no\ncommand=no\nfor arg do\n  [ \"$arg\" = \"git@forge.example\" ] && host=yes\n  [ \"$arg\" = \"git-upload-pack 'origin/app'\" ] && command=yes\ndone\n[ \"$host:$command\" = yes:yes ] || exit 97\nexec git-upload-pack {}\n",
        quoted(&w.upstream)
    );
    assert!(std::fs::write(&ssh, script).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert!(std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).is_ok());
    }
    run(&w.me, &["config", "core.sshCommand", &quoted(&ssh)]);
    run(&w.me, &["config", "ssh.variant", "ssh"]);
    run(&w.me, &["config", "gitcraft.forge", "forgejo"]);
    run(
        &w.me,
        &[
            "remote",
            "set-url",
            "origin",
            "git@forge.example:origin/app",
        ],
    );
    let remotes = git::remotes(&w.me)?;
    let primary = remotes.iter().find(|r| r.name == "origin").cloned();
    assert!(primary.is_some());
    let primary = primary.ok_or("owned origin is configured above")?;
    let p = plan::plan_rebase(&w.me, &w.info(), &primary, &w.setup(), None, None, None);
    assert!(p.is_ok(), "{p:?}");
    let p = p?;
    let mut tab = crate::repo::RepoTab::new_deferred(w.me.clone(), ctx);
    tab.forge_remotes = crate::forge::detect_remotes(&w.me, &remotes);
    tab.remotes = remotes;
    tab.take_forge_selection(false, ctx);
    let hosting = tab.hosting.clone();
    assert!(hosting.is_some());
    if let Some(hosting) = hosting {
        tab.forks.scope = Some(super::guard::Scope::new(
            tab.forge_generation,
            hosting,
            primary,
        ));
    }
    tab.forks.info.insert(7, w.info());
    let label = "Rebase #7 onto main and push".to_owned();
    tab.forks.scope_label = Some(label.clone());
    tab.forks.running = Some((label, p.clone()));
    Ok((p, tab))
}

fn execution_tab(w: &World, p: &plan::Rebase, ctx: &egui::Context) -> crate::repo::RepoTab {
    let mut tab = recorded_tab(w, ctx);
    tab.remotes = git::remotes(&w.me).unwrap_or_default();
    tab.forge_remotes.truncate(1);
    tab.take_forge_selection(true, ctx);
    assert!(tab.hosting.is_some());
    if let Some(hosting) = tab.hosting.clone() {
        let primary = git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        };
        tab.forks.scope = Some(super::guard::Scope::new(
            tab.forge_generation,
            hosting,
            primary,
        ));
        let label = "Rebase #7 onto main and push".to_owned();
        tab.forks.scope_label = Some(label.clone());
        tab.forks.running = Some((label, p.clone()));
    }
    tab
}
#[test]
fn review19_rejected_rebase_does_not_offer_undo_of_an_external_commit() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("external-before-confirm");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    commit(&w.me, "external.txt");
    let external = git::head(&w.me);
    let id = confirmation_id(&tab);
    tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    for _ in 0..50 {
        tab.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(git::head(&w.me), external);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert!(
        !tab.timeline.entries.iter().any(|e| e.undo.is_some()),
        "unrelated commit was offered as our rewrite: {:?}",
        tab.timeline
            .entries
            .iter()
            .map(|e| &e.label)
            .collect::<Vec<_>>()
    );
    assert!(tab.forks.after_conflict.is_none());
}
#[test]
fn review19_undo_lease_is_independent_of_fetch_refspecs_and_persists() {
    let w = World::new("undo-explicit-lease");
    let p = rebase_ready(&w);
    let (result, kind) = run_steps(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
    );
    assert!(result.is_ok(), "{result:?}");
    let entry = custom(kind);
    assert!(entry.is_some());
    let Some(entry) = entry else { return };
    let encoded = serde_json::to_string(&entry).unwrap_or_default();
    let restored: Result<UndoEntry, _> = serde_json::from_str(&encoded);
    assert!(restored.is_ok());
    let Ok(entry) = restored else { return };
    let rebased = git::head(&w.me);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.fetch",
            "+refs/heads/*:refs/remotes/ann-new/*",
        ],
    );
    run(&w.ann, &["fetch", "-q", "fork"]);
    run(&w.ann, &["checkout", "-q", "-B", "fix", "fork/fix"]);
    commit(&w.ann, "contributor-after-push.txt");
    run(&w.ann, &["push", "-q", "fork", "fix"]);
    let theirs = git::head(&w.ann);
    run(&w.me, &["fetch", "-q", "ann"]);
    assert_eq!(
        git::resolve(&w.me, &p.tracking).unwrap_or_default(),
        rebased
    );
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(
        result.is_err(),
        "undo replaced the contributor's new commit: {result:?}"
    );
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), theirs);
    assert_eq!(git::head(&w.me), rebased);
}

#[test]
fn review19_new_remote_validates_both_rewrites_and_pins_the_initial_effective_urls()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    for (name, rule, destination, valid) in [
        ("push-valid", "pushInsteadOf", "git@github.com:ann/", true),
        ("fetch-wrong", "insteadOf", "git@github.com:mallory/", false),
        (
            "push-wrong",
            "pushInsteadOf",
            "git@github.com:mallory/",
            false,
        ),
    ] {
        let w = World::new(name);
        run(
            &w.me,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/acme/app.git",
            ],
        );
        run(
            &w.me,
            &[
                "config",
                &format!("url.{destination}.{rule}"),
                "https://github.com/ann/",
            ],
        );
        let h = Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            web: "https://github.com".into(),
            path: "acme/app".into(),
        };
        let primary = git::remotes(&w.me)?.remove(0);
        let mut fork = h.clone();
        fork.path = "ann/app".into();
        let scope = super::guard::Scope::new(1, h, primary).with_fork(
            "ann".into(),
            "https://github.com/ann/app.git".into(),
            fork,
            true,
        );
        let steps = vec![
            vec![
                "remote",
                "add",
                "--",
                "ann",
                "https://github.com/ann/app.git",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            vec!["config", "remote.ann.gitcraftFork", "true"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ];
        let result = crate::repo::operations::run_steps_guarded(
            &w.me,
            &steps,
            OpKind::plain(),
            None,
            false,
            None,
            Some(&scope),
        )
        .0;
        assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
        let marker = git::run(Some(&w.me), &["config", "--get", "remote.ann.gitcraftFork"]);
        assert_eq!(marker.is_ok(), valid);
        if valid {
            assert!(scope.check_remote(&w.me).is_ok());
            // Same canonical fork, but a destination edit after its initial capture.
            run(
                &w.me,
                &[
                    "config",
                    "remote.ann.pushurl",
                    "ssh://git@github.com/ann/app.git",
                ],
            );
            assert!(scope.check_remote(&w.me).is_err());
            run(&w.me, &["config", "--unset", "remote.ann.pushurl"]);
            assert!(scope.check_remote(&w.me).is_ok());
            run(&w.me, &["remote", "remove", "ann"]);
            assert!(scope.check_remote(&w.me).is_err());
        }
    }
    Ok(())
}

#[test]
fn review19_recorded_rewrite_recovery_cannot_reset_a_later_external_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("owned-rewrite-later-commit");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
    let me = w.me.clone();
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                commit(&me, "external-after-rebase.txt");
            }
        }))
    });
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(result.is_err(), "{result:?}");
    let external = git::head(&w.me);
    let owned = scope.rewrite().ok_or("missing worker rewrite record")?;
    assert_ne!(owned.tip, p.head);
    assert_ne!(owned.tip, external);
    assert_eq!(git::resolve(&w.fork, "fix")?, p.seen);
    tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
    drain_until(&mut tab, &ctx, |t| {
        t.timeline.entries.iter().any(|e| {
            e.undo
                .as_ref()
                .is_some_and(|u| u.label.ends_with("(here only)"))
        })
    });
    let entry = tab
        .timeline
        .entries
        .iter()
        .find_map(|e| e.undo.clone().filter(|u| u.label.ends_with("(here only)")))
        .ok_or("missing local recovery entry")?;
    let undo = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(undo.is_err(), "{undo:?}");
    assert_eq!(git::head(&w.me), external);
    Ok(())
}

fn persisted(entry: UndoEntry) -> Result<UndoEntry, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&entry)?)
}
fn advanced_fork(w: &World, file: &str) -> String {
    run(&w.ann, &["fetch", "-q", "fork"]);
    run(&w.ann, &["checkout", "-q", "-B", "fix", "fork/fix"]);
    commit(&w.ann, file);
    run(&w.ann, &["push", "-q", "fork", "fix"]);
    run(&w.me, &["fetch", "-q", "ann"]);
    git::head(&w.ann)
}
#[test]
fn review19_persisted_redo_lease_rejects_a_fetched_contributor_advance()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("redo-explicit-lease");
    let p = rebase_ready(&w);
    let (result, kind) = run_steps(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
    );
    assert!(result.is_ok(), "{result:?}");
    let entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    let (result, kind) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = kind else {
        return Err("expected undo entry".into());
    };
    let entry = persisted(entry)?;
    assert!(entry.redo.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{}", p.head))
    }));
    run(
        &w.me,
        &[
            "config",
            "remote.ann.fetch",
            "+refs/heads/*:refs/remotes/ann-new/*",
        ],
    );
    let theirs = advanced_fork(&w, "contributor-before-redo.txt");
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, theirs);
    Ok(())
}

#[test]
fn review19_persisted_undo_redo_roundtrips_rebind_the_exact_pushed_tip()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("repeat-undo-redo");
    let p = rebase_ready(&w);
    let mut steps = p.steps();
    steps
        .iter_mut()
        .filter(|op| op.first().is_some_and(|s| s == "push"))
        .for_each(|op| op.insert(1, "--no-verify".into()));
    let (result, kind) = run_steps(&w.me, &steps, normal(p.undo_steps(), p.redo_steps()), None);
    assert!(result.is_ok(), "{result:?}");
    let mut entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    for _ in 0..2 {
        let tip = git::head(&w.me);
        assert!(entry.undo.iter().any(|op| {
            op.iter()
                .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{tip}"))
        }));
        let (result, kind) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(git::head(&w.me), p.head);
        assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
        let OpKind::Undo(undone) = kind else {
            return Err("expected undo entry".into());
        };
        entry = persisted(undone)?;
        let (result, kind) = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        let OpKind::Redo(redone) = kind else {
            return Err("expected redo entry".into());
        };
        entry = persisted(redone)?;
        assert_ne!(git::head(&w.me), p.head);
        assert_eq!(git::resolve(&w.fork, "fix")?, git::head(&w.me));
    }
    Ok(())
}

fn edit_base(repo: &Path, text: &str) -> Result<(), std::io::Error> {
    std::fs::write(repo.join("base.txt"), text)?;
    run(repo, &["add", "base.txt"]);
    run(repo, &["commit", "-q", "-m", text]);
    Ok(())
}
#[test]
fn review19_conflict_continuation_pins_its_tip_and_original_explicit_lease()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    for mode in ["push", "local-moved", "contributor-moved"] {
        let w = World::new(&format!("continued-{mode}"));
        edit_base(&w.ann, "contributor version")?;
        run(&w.ann, &["push", "-q", "fork", "fix"]);
        run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
        edit_base(&w.ann, "upstream version")?;
        run(&w.ann, &["push", "-q", "origin", "up:main"]);
        let ctx = egui::Context::default();
        let (p, mut tab) = refresh_stable_rebase(&w, &ctx)?;
        let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
        let result = crate::repo::operations::run_steps_guarded(
            &w.me,
            &p.steps(),
            normal(p.undo_steps(), p.redo_steps()),
            None,
            false,
            None,
            Some(&scope),
        )
        .0;
        assert!(
            result.as_ref().is_err_and(|e| e.contains("CONFLICT")),
            "{result:?}"
        );
        assert!(
            scope
                .rewrite()
                .ok_or("missing worker rewrite record")?
                .conflicts
        );
        tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
        assert!(tab.forks.after_conflict.is_some());
        std::fs::write(w.me.join("base.txt"), "resolved version")?;
        run(&w.me, &["add", "base.txt"]);
        run(&w.me, &["-c", "core.editor=true", "rebase", "--continue"]);
        assert_eq!(git::in_progress(&w.me), Ok(None));
        let rebased = git::head(&w.me);
        tab.refs = git::refs(&w.me)?;
        tab.offer_push_after_conflict();
        assert!(tab.dialog.is_some());
        assert_eq!(tab.forks.push_tip.as_deref(), Some(rebased.as_str()));
        let expected_fork = match mode {
            "local-moved" => {
                commit(&w.me, "after-continuation.txt");
                p.seen.clone()
            }
            "contributor-moved" => {
                run(
                    &w.me,
                    &[
                        "config",
                        "remote.ann.fetch",
                        "+refs/heads/*:refs/remotes/ann-new/*",
                    ],
                );
                advanced_fork(&w, "after-conflict-contributor.txt")
            }
            _ => rebased.clone(),
        };
        let expected_local = git::head(&w.me);
        tab.dialog = None;
        tab.fork_act(super::ForkAct::ConfirmPush(scope.id), &ctx);
        // The conflict confirmation leads to the same deliberate push checklist.
        // A stale contributor lease may refuse during inspection; a local tip
        // change must still refuse if its checklist reaches actual dispatch.
        assert_eq!(
            git::resolve(&w.fork, "fix")?,
            if mode == "contributor-moved" {
                expected_fork.clone()
            } else {
                p.seen.clone()
            }
        );
        let approved = crate::repo::push_preflight::test_support::approve(&mut tab, &ctx);
        if mode == "push" {
            assert!(approved.is_ok(), "{approved:?}");
        } else {
            assert!(
                approved.is_err(),
                "{mode}: a stale checklist must refuse publication"
            );
            // A declined checklist has no OpDone. Dismissing it and processing
            // the next ordinary frame retires the pending fork action.
            tab.dialog = None;
            tab.forks_frame(&ctx);
        }
        drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
        assert_eq!(git::head(&w.me), expected_local);
        assert_eq!(git::resolve(&w.fork, "fix")?, expected_fork);
        assert!(tab.forks.running.is_none(), "{mode}: {approved:?}");
        if mode == "push" {
            assert!(
                tab.timeline
                    .entries
                    .iter()
                    .filter_map(|e| e.undo.as_ref())
                    .any(|entry| entry.undo.iter().any(|op| {
                        op.iter().any(|arg| {
                            arg == &format!("--force-with-lease=refs/heads/fix:{rebased}")
                        })
                    }))
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn review19_auth_retry_pushes_only_the_recorded_rewrite_with_an_explicit_lease()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("auth-retry-owned");
    let p = rebase_ready(&w);
    let hook = w.fork.join("hooks/pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'fatal: Authentication failed' >&2\nexit 1\n",
    )?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))?;
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
    let steps = p.steps();
    let kind = normal(p.undo_steps(), p.redo_steps());
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &steps,
        kind.clone(),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    assert!(
        result.as_ref().is_err_and(|e| git::is_auth_error(e)),
        "{result:?}"
    );
    // Record the pending operation just as run_ops does. Keep owned-local forge metadata
    // until the completion callback is inspected, before reload redetects local transports.
    tab.last_op = Some(crate::repo::PendingOp {
        label: "Rebase #7 onto main and push".into(),
        ops: steps,
        refresh: crate::repo::Refresh::All,
        kind,
        quiet: false,
    });
    tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
    let rebased = git::head(&w.me);
    assert_ne!(rebased, p.head);
    let retry = tab.last_op.take().ok_or("missing authentication retry")?;
    assert!(retry.label.starts_with("Push "));
    assert!(retry.ops.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{}", p.seen))
    }));
    std::fs::remove_file(&hook)?;
    // A later local commit must not be included by retrying the original push.
    commit(&w.me, "after-auth-error.txt");
    let external = git::head(&w.me);
    let refused = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, p.seen);
    assert_eq!(git::head(&w.me), external);
    run(&w.me, &["reset", "--hard", &rebased]);
    let (result, kind) = run_steps(&w.me, &retry.ops, retry.kind, None);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, rebased);
    let entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    assert!(entry.undo.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{rebased}"))
    }));
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        git::head(&w.me),
        rebased,
        "fork-only undo preserves the completed local rewrite"
    );
    assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
    Ok(())
}

#[cfg(unix)]
fn push_window(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new(&format!("review20-{mode}-push-window"));
    let p = rebase_ready(&w);
    let mut kind = normal(p.undo_steps(), p.redo_steps());
    let mut steps = p.steps();
    if mode != "normal" {
        let (result, done) = run_steps(&w.me, &steps, kind, None);
        assert!(result.is_ok(), "{result:?}");
        let entry = persisted(custom(done).ok_or("missing completed rebase")?)?;
        if mode == "redo" {
            let (result, undone) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
            assert!(result.is_ok(), "{result:?}");
            let OpKind::Undo(entry) = undone else {
                return Err("missing undone entry".into());
            };
            let entry = persisted(entry)?;
            steps = entry.redo.clone();
            kind = OpKind::Redo(entry);
        } else {
            steps = entry.undo.clone();
            kind = OpKind::Undo(entry);
        }
    }
    let ready = w.root.join("hook-ready");
    let release = w.root.join("hook-release");
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let hook = format!(
        "#!/bin/sh\n: > {}\ni=0\nwhile [ ! -f {} ] && [ \"$i\" -lt 1500 ]; do\n i=$((i+1))\n sleep 0.01\ndone\n[ -f {} ]\n",
        quote(&ready),
        quote(&release),
        quote(&release)
    );
    let hook_path = w.me.join(".git/hooks/pre-push");
    std::fs::write(&hook_path, hook)?;
    std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o700))?;
    let me = w.me.clone();
    let worker = std::thread::spawn(move || run_steps(&me, &steps, kind, None));
    for _ in 0..1000 {
        if ready.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Release even on setup failure: only this owned Git child can wait on these files.
    if !ready.exists() {
        std::fs::write(&release, "release")?;
        let _ = worker.join();
        return Err("owned pre-push hook never became ready".into());
    }
    let rewritten = git::head(&w.me);
    commit(&w.me, "external-during-push.txt");
    let external = git::head(&w.me);
    std::fs::write(&release, "release")?;
    let (result, done) = worker.join().map_err(|_| "owned push worker panicked")?;
    std::fs::remove_file(hook_path)?;
    if mode == "undo" {
        assert!(
            result.is_err(),
            "undo reset a branch changed while pushing: {result:?}"
        );
        assert_eq!(git::head(&w.me), external);
        assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
        return Ok(());
    }
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        git::resolve(&w.fork, "fix")?,
        rewritten,
        "the push itself uses pinned R"
    );
    assert_eq!(git::head(&w.me), external);
    let entry = if mode == "normal" {
        custom(done).ok_or("missing normal undo")?
    } else {
        let OpKind::Redo(entry) = done else {
            return Err("missing redo result".into());
        };
        entry
    };
    let entry = persisted(entry)?;
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(
        result.is_err(),
        "undo accepted external C after a pinned R push: {result:?}"
    );
    assert_eq!(git::head(&w.me), external);
    assert_eq!(git::resolve(&w.fork, "fix")?, rewritten);
    Ok(())
}
#[cfg(unix)]
#[test]
fn review20_normal_push_cannot_bind_local_undo_to_a_commit_created_in_its_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("normal")
}
#[cfg(unix)]
#[test]
fn review20_redo_push_cannot_bind_local_undo_to_a_commit_created_in_its_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("redo")
}
#[cfg(unix)]
#[test]
fn review20_undo_cannot_reset_a_commit_created_in_its_push_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("undo")
}

#[cfg(unix)]
fn web_checkout(
    w: &World,
) -> Result<(plan::Checkout, super::guard::Scope), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    use std::os::unix::fs::PermissionsExt;
    let primary = git::Remote {
        name: "origin".into(),
        url: "https://github.com/acme/app.git".into(),
    };
    run(&w.me, &["remote", "set-url", "origin", &primary.url]);
    run(
        &w.me,
        &[
            "config",
            "url.git@github.com:ann/.insteadOf",
            "https://github.com/ann/",
        ],
    );
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("checkout-owned-ssh");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nallowed=no\nfor arg do\n [ \"$arg\" = \"git-upload-pack 'ann/app.git'\" ] && allowed=yes\ndone\n[ \"$allowed\" = yes ] || exit 97\nexec git-upload-pack {}\n",
            quote(&w.fork)
        ),
    )?;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))?;
    run(&w.me, &["config", "core.sshCommand", &quote(&ssh)]);
    run(&w.me, &["config", "ssh.variant", "ssh"]);
    let h = Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        web: "https://github.com".into(),
        path: "acme/app".into(),
    };
    let mut info = w.info();
    info.clone_https = "https://github.com/ann/app.git".into();
    info.clone_ssh = "git@github.com:ann/app.git".into();
    let p = plan::checkout(
        &info,
        std::slice::from_ref(&primary),
        &w.setup(),
        &["main".into()],
        "main",
        Some(&h),
    )?;
    let fork = Hosting {
        path: "ann/app".into(),
        ..h.clone()
    };
    let scope = super::guard::Scope::new(1, h, primary).with_fork(
        p.remote.clone(),
        p.url.clone(),
        fork,
        p.new_remote,
    );
    Ok((p, scope))
}
#[cfg(unix)]
#[test]
fn review20_checkout_redo_rejects_a_new_wrong_repository_rewrite_without_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-redo-checkout");
    let evil = World::new("review20-other-repository");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = done else {
        return Err("missing undone checkout".into());
    };
    let entry = persisted(entry)?;
    run(
        &w.me,
        &["config", "--unset", "url.git@github.com:ann/.insteadOf"],
    );
    run(
        &w.me,
        &[
            "config",
            &format!("url.{}.insteadOf", arg(&evil.fork)),
            "https://github.com/ann/app.git",
        ],
    );
    let before = git::head(&w.me);
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(
        result.is_err(),
        "checkout redo accepted a wrong repository: {result:?}"
    );
    assert_eq!(git::head(&w.me), before);
    assert!(git::resolve(&w.me, "refs/heads/fix").is_err());
    assert!(git::resolve(&w.me, "refs/remotes/ann/fix").is_err());
    assert!(git::run(Some(&w.me), &["config", "--get", "remote.ann.gitcraftFork"]).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_canonical_rewrite_roundtrips_without_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-valid-replay");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let mut entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    for _ in 0..2 {
        let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "main");
        assert!(git::remotes(&w.me)?.iter().all(|r| r.name != "ann"));
        let OpKind::Undo(undone) = done else {
            return Err("missing checkout undo result".into());
        };
        entry = persisted(undone)?;
        let (result, done) = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(git::head(&w.me), w.info().sha);
        assert_eq!(
            run(&w.me, &["remote", "get-url", "ann"]).trim(),
            "git@github.com:ann/app.git"
        );
        let OpKind::Redo(redone) = done else {
            return Err("missing checkout redo result".into());
        };
        entry = persisted(redone)?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_undo_rejects_push_url_changes_and_missing_remotes_before_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-undo-destinations");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let head = git::head(&w.me);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "https://github.com:444/ann/app.git",
        ],
    );
    let result = run_steps(&w.me, &entry.undo, OpKind::Undo(entry.clone()), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(git::head(&w.me), head);
    run(&w.me, &["remote", "remove", "ann"]);
    let result = run_steps(
        &w.me,
        &entry.undo.clone(),
        OpKind::Undo(persisted(entry)?),
        None,
    )
    .0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(git::head(&w.me), head);
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_redo_retry_retains_the_initial_effective_destination_pin()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new("review20-replay-retry");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = done else {
        return Err("missing undone checkout".into());
    };
    let entry = persisted(entry)?;
    let original_ssh = run(&w.me, &["config", "core.sshCommand"]);
    let failure = w.root.join("owned-auth-failure");
    let recording = w.root.join("retry-recording");
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    std::fs::write(
        &failure,
        format!(
            "#!/bin/sh\nprintf failed >> {}\necho 'fatal: Authentication failed' >&2\nexit 1\n",
            quote(&recording)
        ),
    )?;
    std::fs::set_permissions(&failure, std::fs::Permissions::from_mode(0o700))?;
    run(&w.me, &["config", "core.sshCommand", &quote(&failure)]);
    let ops = entry.redo.clone();
    let (result, kind, stopped) = crate::repo::operations::run_steps_counted(
        &w.me,
        &ops,
        OpKind::Redo(entry),
        None,
        false,
        None,
    );
    assert!(
        result.as_ref().is_err_and(|e| git::is_auth_error(e)),
        "{result:?}"
    );
    assert_eq!(std::fs::read_to_string(&recording)?, "failed");
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(w.me.clone(), &ctx);
    tab.last_op = Some(crate::repo::PendingOp {
        label: "Retry checkout".into(),
        ops,
        refresh: crate::repo::Refresh::All,
        kind,
        quiet: false,
    });
    tab.keep_unfinished(stopped);
    let retry = tab.last_op.take().ok_or("missing checkout retry")?;
    // Canonical SSH spelling, but changed after the initial effective URLs were pinned.
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "ssh://git@github.com/ann/app.git",
        ],
    );
    let result = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(
        std::fs::read_to_string(&recording)?,
        "failed",
        "changed destination was refused before retrying fetch"
    );
    assert!(git::resolve(&w.me, "refs/heads/fix").is_err());
    run(&w.me, &["config", "--unset", "remote.ann.pushurl"]);
    run(&w.me, &["remote", "remove", "ann"]);
    let result = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(
        result.is_err(),
        "a retry may not silently replace a removed remote: {result:?}"
    );
    run(
        &w.me,
        &["remote", "add", "ann", "https://github.com/ann/app.git"],
    );
    run(&w.me, &["config", "core.sshCommand", original_ssh.trim()]);
    let result = run_steps(&w.me, &retry.ops, retry.kind, None).0;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::head(&w.me), w.info().sha);
    Ok(())
}

#[test]
fn review20_saved_checkout_guard_preserves_authority_prefix_and_gitlab_case()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    for forge in Forge::ALL {
        let w = World::new(&format!("review20-saved-identity-{}", forge.key()));
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            web: "https://forge.example:4000/proxy".into(),
            path: "Group/Repo".into(),
        };
        run(
            &w.me,
            &[
                "remote",
                "add",
                "ann",
                "https://forge.example:4000/proxy/Group/Repo.git",
            ],
        );
        for (url, valid) in [
            ("https://forge.example:4000/proxy/Group/Repo.git", true),
            ("ssh://git@forge.example:2222/Group/Repo.git", true),
            ("https://forge.example:3000/proxy/Group/Repo.git", false),
            ("https://forge.example:4000/Group/Repo.git", false),
            ("https://forge.example:4000/PROXY/Group/Repo.git", false),
            (
                "https://forge.example:4000/proxy/group/repo.git",
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
            ),
        ] {
            run(&w.me, &["remote", "set-url", "ann", url]);
            let guard = super::guard::CheckoutDestination::new("ann", url, false, &h).step();
            let steps = vec![
                guard,
                vec!["config".into(), "gitcraft.fixtureRan".into(), "true".into()],
            ];
            let steps: Vec<Vec<String>> = serde_json::from_str(&serde_json::to_string(&steps)?)?;
            let result = run_steps(&w.me, &steps, OpKind::plain(), None).0;
            assert_eq!(result.is_ok(), valid, "{forge:?} {url}: {result:?}");
            let ran = git::run(Some(&w.me), &["config", "--get", "gitcraft.fixtureRan"]);
            assert_eq!(ran.is_ok(), valid);
            if valid {
                run(&w.me, &["config", "--unset", "gitcraft.fixtureRan"]);
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_legacy_saved_checkout_without_identity_is_refused_before_replay()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-legacy-checkout");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let mut entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    // Reconstruct the old on-disk record: it had the action label and plain commands.
    entry.label = "Check out #7 to push to ann".into();
    let mut legacy = entry.clone();
    for ops in [&mut legacy.undo, &mut legacy.redo] {
        ops.retain(|s| s.first().is_none_or(|m| m != "\0fork-checkout"));
    }
    let legacy = persisted(legacy)?;
    let head = git::head(&w.me);
    let result = run_steps(
        &w.me,
        &legacy.undo.clone(),
        OpKind::Undo(legacy.clone()),
        None,
    )
    .0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::head(&w.me), head);
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(undone) = done else {
        return Err("missing guarded undo result".into());
    };
    entry = persisted(undone)?;
    let before = git::head(&w.me);
    let result = run_steps(&w.me, &legacy.redo.clone(), OpKind::Redo(legacy), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::head(&w.me), before);
    assert!(git::remotes(&w.me)?.iter().all(|r| r.name != "ann"));
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(result.is_ok(), "{result:?}");
    Ok(())
}

#[test]
fn invalidated_fork_scope_keeps_raw_guard_while_error_paint_tracks_locale() -> Result<(), String> {
    let hosting = crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".into(),
        path: "opaque/project".into(),
        web: "https://github.com".into(),
    };
    let scope = super::guard::Scope::new(
        1234,
        hosting,
        git::Remote {
            name: "opaque 日本語".into(),
            url: "https://github.com/opaque/project".into(),
        },
    );
    scope.check()?;
    scope.invalidate();
    let raw = scope.check().err().ok_or("invalidated scope accepted")?;
    let expected = "The primary forge changed; plan this fork action again. Any completed local rewrite remains undoable.";
    assert_eq!(raw, expected);
    for (language, display) in [
        ("en-US", expected),
        (
            "de",
            "Die primäre Forge wurde geändert; plane diese Fork-Aktion erneut. Abgeschlossene lokale Umschreibungen bleiben rückgängig machbar.",
        ),
        ("en-US", expected),
    ] {
        crate::i18n::set_language(language);
        assert_eq!(git::display_error(&raw), display);
        assert_eq!(scope.check(), Err(expected.to_owned()));
        let opaque = format!("provider log: {expected}\ncommand: refs/日本語");
        assert_eq!(git::display_error(&opaque), opaque);
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
