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
