//! The whole of a run on a merge put aside, through a real tab: _Resolve all with agent_ puts
//! the merge aside (the worktree clean, its row in the graph), the user checks out another
//! branch meanwhile, the agent (a stand-in script) ends and its work is kept and announced,
//! _Bring it back_ puts the merge back on its branch and opens the review, and _Apply_ marks
//! the file resolved.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;

use super::{Phase, ResolverAct};
use crate::ai::run::CommandLine;
use crate::git::fixture::Repo;
use crate::repo::{Action, RepoTab};
use crate::settings::Settings;

mod diagnostics;
mod stand_in;
pub(super) use stand_in::command_for;
use stand_in::register;

/// What the stand-in writes in place of the conflicted file.
const RESOLVED: &str = "a both";

/// A stand-in agent: writes `a.txt` without its conflict, says it is done as Claude Code does.
fn stand_in() -> CommandLine {
    slow_stand_in(0)
}

/// [`stand_in`], taking `secs` seconds first.
fn slow_stand_in(secs: u32) -> CommandLine {
    let end = r#"{"type":"result","subtype":"success","is_error":false,"result":"kept both"}"#;
    CommandLine::Script {
        sh: format!("sleep {secs}; printf '%s\\n' '{RESOLVED}' > a.txt; printf '%s\\n' '{end}'"),
        cmd: format!(
            "ping -n {} 127.0.0.1 >NUL & (echo {RESOLVED})> a.txt & echo {end}",
            secs + 1
        ),
    }
}

/// A repository in the middle of a merge of `side` into `main` that conflicts in `a.txt`,
/// and a branch `other` to go to meanwhile.
fn conflicted(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(name)?;
    repo.commit("base", &[("a.txt", Some("a base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[("a.txt", Some("a side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("a main\n"))])?;
    repo.git(&["branch", "other"])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    Ok(repo)
}

type H<'a> = Harness<'a, Option<RepoTab>>;

fn harness(path: PathBuf) -> H<'static> {
    Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(
            move |ui, tab: &mut Option<RepoTab>| {
                let tab = tab.get_or_insert_with(|| {
                    Settings {
                        avatars: false,
                        auto_fetch_minutes: 0,
                        ..Settings::default()
                    }
                    .set(ui.ctx());
                    crate::theme::install_fonts(ui.ctx());
                    RepoTab::new(path.clone(), ui.ctx())
                });
                tab.ui(ui);
            },
            None,
        )
}

/// Steps the tab until `until` holds of it (or 60 s pass): whether it did.
fn wait(h: &mut H<'_>, until: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        h.step();
        if h.state().as_ref().is_some_and(&until) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    diagnostics::report(h.state().as_ref());
    false
}

fn act(h: &mut H<'_>, act: ResolverAct) {
    if let Some(tab) = h.state_mut().as_mut() {
        tab.queued.push(Action::Resolver(act));
    }
}

fn merging(dir: &Path) -> bool {
    crate::git::run(Some(dir), &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok()
}

fn phase(tab: &RepoTab) -> Option<Phase> {
    tab.resolver.run.as_ref().map(|r| r.phase)
}

#[test]
fn a_merge_put_aside_is_resolved_brought_back_and_applied() -> Result<(), String> {
    let repo = Repo::new("resolver flow")?;
    repo.commit("base", &[("a.txt", Some("a base\n"))])?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit("side", &[("a.txt", Some("a side\n"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main", &[("a.txt", Some("a main\n"))])?;
    repo.git(&["branch", "other"])?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    let _stand_in = register(repo.path(), stand_in());
    let dir = repo.path().to_path_buf();
    let mut h = harness(dir.clone());
    assert!(
        wait(&mut h, |t| !t.opening() && t.work.conflicted.len() == 1),
        "the tab read"
    );

    // Resolve all: the merge is put aside, the worktree free, its row in the graph.
    act(&mut h, ResolverAct::Start(None));
    assert!(wait(&mut h, |t| !t.resolver.parked.is_empty()), "put aside");
    assert!(!merging(&dir), "no merge in the worktree meanwhile");
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    assert!(
        wait(&mut h, |t| t
            .commits
            .iter()
            .any(|c| c.hash.starts_with("\u{0}parked:"))),
        "its row in the graph"
    );
    // At the top, and shown: the user is left looking at it, not sent down to HEAD.
    assert!(
        wait(&mut h, |t| t.selected == Some(0)
            && t.commits
                .first()
                .is_some_and(|c| c.hash.starts_with("\u{0}parked:"))),
        "its row on top, selected"
    );

    // The user goes elsewhere while the agent works; it ends, its work is kept and announced.
    repo.git(&["checkout", "-q", "other"])?;
    assert!(
        wait(&mut h, |t| phase(t) == Some(Phase::Ready)),
        "the agent's work kept"
    );
    let tab = h.state().as_ref().ok_or("no tab")?;
    let run = tab.resolver.run.as_ref().ok_or("no run")?;
    assert!(run.parked.is_some());
    assert_eq!(run.results.len(), 1);
    assert!(run.results[0].1.applies(), "{:?}", run.results);
    assert!(
        tab.finished
            .iter()
            .any(|notice| notice.raw().0.contains("resolved 1 file")),
        "the user is told: {:?}",
        tab.finished
    );
    let recovery = crate::resolver::park::list(&dir)?
        .into_iter()
        .next()
        .ok_or("missing parked merge")?;
    let id = run.id.clone();
    let kept = crate::agent::repo_file(&dir, &format!("resolver/{id}"))?;
    assert!(kept.join("run.json").is_file() && kept.join("resolved/a.txt").is_file());

    // gitcraft is closed and opened again: the merge is still put aside, the agent's work
    // with it, and the run comes back to bring it back.
    drop(h);
    assert!(
        kept.join("run.json").is_file(),
        "closing gitcraft keeps the agent's work"
    );
    let mut h = harness(dir.clone());
    assert!(
        wait(&mut h, |t| t
            .resolver
            .run
            .as_ref()
            .is_some_and(|r| r.id == id
                && r.parked.is_some()
                && r.phase == Phase::Ready)),
        "the run comes back in the next session"
    );

    // Bring it back: its branch, its merge as it was, and the review opened.
    act(&mut h, ResolverAct::BringBack(id.clone()));
    assert!(
        wait(&mut h, |t| t.resolver.reviewing()
            && t.resolver.parked.is_empty()),
        "brought back and shown"
    );
    // The panel on the right shows its changes again (conflicted and staged files), not the
    // commit looked at meanwhile.
    assert!(
        wait(&mut h, |t| t
            .selected
            .and_then(|i| t.commits.get(i))
            .is_some_and(|c| c.hash == crate::repo::WIP_HASH)),
        "its changes selected"
    );
    assert!(merging(&dir));
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
        "main"
    );
    assert!(
        repo.git(&["ls-files", "-u"])?.contains("a.txt"),
        "the conflict is back"
    );

    // Apply: the agent's file, marked resolved.
    act(&mut h, ResolverAct::Apply);
    assert!(
        wait(&mut h, |t| phase(t) == Some(Phase::Applied)
            && t.busy_op.is_none()),
        "applied"
    );
    assert!(
        wait(&mut h, |_| repo
            .git(&["ls-files", "-u"])
            .is_ok_and(|u| u.is_empty())),
        "marked resolved"
    );
    let a = std::fs::read_to_string(dir.join("a.txt")).map_err(|e| e.to_string())?;
    assert_eq!(a.trim_end(), RESOLVED);
    assert!(merging(&dir), "the merge goes on, to be committed");
    // Closing removes the agent workspace; byte-exact recovery remains discoverable.
    act(&mut h, ResolverAct::Dismiss);
    assert!(wait(&mut h, |t| t.resolver.run.is_none()));
    let start = Instant::now();
    while kept.exists() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!kept.exists(), "the agent's kept work is removed");
    assert_eq!(crate::resolver::park::list(&dir)?, vec![recovery.clone()]);
    let archive = crate::resolver::park::recovery_location(&dir, &recovery)?;
    assert!(archive.ends_with("parked"));
    assert!(archive.join("index").is_file());
    assert!(archive.join("raw/a.txt").is_file());
    assert!(
        repo.git(&["rev-parse", "--verify", &recovery.ref_name()])
            .is_err()
    );
    assert_eq!(
        repo.git(&[
            "rev-parse",
            "--verify",
            &format!("refs/gitcraft/parked-recovery/{}", recovery.id)
        ])?
        .trim(),
        recovery.snapshot
    );
    Ok(())
}

/// Brought back while its agent still works: the merge is back at once, the agent goes on in
/// its own worktree, and its work is then checked against the merge in the worktree.
#[test]
fn a_merge_brought_back_while_its_agent_works_gets_its_work_after() -> Result<(), String> {
    let repo = conflicted("resolver flow early")?;
    let _stand_in = register(repo.path(), slow_stand_in(8));
    let dir = repo.path().to_path_buf();
    let mut h = harness(dir.clone());
    assert!(wait(&mut h, |t| !t.opening() && t.work.conflicted.len() == 1));
    act(&mut h, ResolverAct::Start(None));
    assert!(wait(&mut h, |t| !t.resolver.parked.is_empty()
        && phase(t) == Some(Phase::Working)));
    let id = h
        .state()
        .as_ref()
        .and_then(|t| t.resolver.run.as_ref())
        .map(|r| r.id.clone())
        .ok_or("no run")?;
    // The user checks out another branch meanwhile, as gitcraft does it, the agent at work.
    if let Some(tab) = h.state_mut().as_mut() {
        tab.queued.push(Action::Checkout("other".into()));
    }
    assert!(
        wait(&mut h, |t| t.busy_op.is_none()
            && repo
                .git(&["symbolic-ref", "--short", "HEAD"])
                .is_ok_and(|b| b.trim() == "other")),
        "checked out"
    );
    assert_eq!(
        h.state().as_ref().and_then(phase),
        Some(Phase::Working),
        "while the agent works"
    );

    let recovery = crate::resolver::park::list(&dir)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("missing parked merge")?;
    let archive = crate::resolver::park::recovery_location(&dir, &recovery)?;
    let index = std::fs::read(archive.join("index")).map_err(|e| e.to_string())?;
    let raw = std::fs::read(archive.join("raw/a.txt")).map_err(|e| e.to_string())?;
    act(&mut h, ResolverAct::BringBack(id));
    assert!(
        wait(&mut h, |t| t.busy_op.is_none()
            && t.resolver
                .run
                .as_ref()
                .is_some_and(|r| r.parked.is_none())),
        "brought back at once"
    );
    assert!(wait(&mut h, |t| !t.resolver.reading_parked
        && t.resolver.parked.contains(&recovery)));
    let archive = crate::resolver::park::recovery_location(&dir, &recovery)?;
    assert!(archive.ends_with("parked"));
    assert_eq!(
        std::fs::read(archive.join("index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(
        std::fs::read(archive.join("raw/a.txt")).map_err(|e| e.to_string())?,
        raw
    );
    assert!(merging(&dir));
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
        "main"
    );

    assert!(
        wait(&mut h, |t| phase(t) == Some(Phase::Ready)),
        "the agent ends"
    );
    let tab = h.state().as_ref().ok_or("no tab")?;
    let run = tab.resolver.run.as_ref().ok_or("no run")?;
    assert!(run.results[0].1.applies(), "{:?}", run.results);
    Ok(())
}

/// A conflicted file behind the agent fence never reaches the agent: asked for alone it is
/// refused, the merge left as it is; _Resolve all_ leaves it to the user and resolves the rest.
#[test]
fn a_file_behind_the_agent_fence_is_left_to_the_user() -> Result<(), String> {
    let repo = Repo::new("resolver fence")?;
    repo.commit(
        "base",
        &[("a.txt", Some("a base\n")), ("b.lock", Some("b base\n"))],
    )?;
    repo.git(&["checkout", "-q", "-b", "side"])?;
    repo.commit(
        "side",
        &[("a.txt", Some("a side\n")), ("b.lock", Some("b side\n"))],
    )?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit(
        "main",
        &[("a.txt", Some("a main\n")), ("b.lock", Some("b main\n"))],
    )?;
    assert!(repo.git(&["merge", "-q", "side"]).is_err());
    repo.git(&["config", "--local", "--add", "gitcraft.fence", "*.lock"])?;
    let _stand_in = register(repo.path(), stand_in());
    let dir = repo.path().to_path_buf();
    let mut h = harness(dir.clone());
    assert!(wait(&mut h, |t| !t.opening() && t.work.conflicted.len() == 2));

    act(&mut h, ResolverAct::Start(Some(vec!["b.lock".into()])));
    assert!(wait(&mut h, |t| phase(t) == Some(Phase::Failed)), "refused");
    let error = h
        .state()
        .as_ref()
        .and_then(|t| t.resolver.run.as_ref())
        .and_then(|r| r.error.clone())
        .unwrap_or_default();
    assert!(error.contains("agent fence"), "{error}");
    assert!(merging(&dir));
    act(&mut h, ResolverAct::Dismiss);
    assert!(wait(&mut h, |t| t.resolver.run.is_none()));

    act(&mut h, ResolverAct::Start(None));
    assert!(
        wait(&mut h, |t| phase(t) == Some(Phase::Ready)),
        "the rest resolved"
    );
    let tab = h.state().as_ref().ok_or("no tab")?;
    let run = tab.resolver.run.as_ref().ok_or("no run")?;
    assert_eq!(run.files, ["a.txt"]);
    let results: Vec<&str> = run.results.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(results, ["a.txt"]);
    // The merge was put aside whole, the fenced file with it.
    assert!(run.parked.as_ref().is_some_and(|p| p.files.len() == 2));
    Ok(())
}

#[test]
fn uncertain_cleanup_preserves_workspace_record_and_blocks_apply() -> Result<(), String> {
    let repo = conflicted("resolver-uncertain-proof")?;
    let ws = std::sync::Arc::new(crate::resolver::workspace::Workspace::prepare(
        repo.path(),
        &["a.txt".into()],
    )?);
    let dir = ws.dir().to_owned();
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    let mut run = super::look_tests::run(Phase::Ready, Instant::now());
    run.id = "uncertain-proof".into();
    run.workspace = Some(ws.clone());
    run.agent = Some(crate::resolver::process::Running::fake(vec![]));
    let artifact = dir.with_extension("mcp.json");
    std::fs::write(&artifact, b"retain me").map_err(|e| e.to_string())?;
    run.mcp_file = Some(artifact.clone());
    super::clean_up(&mut run);
    assert_eq!(run.phase, Phase::Failed);
    assert!(run.picked.is_empty());
    assert!(run.workspace.is_some() && run.mcp_file.is_some());
    let recovery = crate::agent::repo_file(repo.path(), "resolver/uncertain-proof")?;
    assert!(recovery.join("uncertain-run.json").is_file());
    assert!(recovery.join("cleanup-unconfirmed").is_file());
    tab.resolver.run = Some(run);
    tab.apply(Action::Resolver(ResolverAct::Apply), &ctx);
    assert!(tab.busy_op.is_none());
    drop(tab);
    drop(ws);
    assert!(dir.is_dir() && artifact.is_file());
    assert!(dir.with_extension("preserved").is_file());
    assert!(dir.with_extension("uncertain-run.json").is_file());
    // Explicit test-owned cleanup only, after verifying preservation.
    repo.git(&[
        "worktree",
        "remove",
        "--force",
        "--force",
        &dir.to_string_lossy(),
    ])?;
    std::fs::remove_file(dir.with_extension("preserved")).map_err(|e| e.to_string())?;
    std::fs::remove_file(artifact).map_err(|e| e.to_string())?;
    std::fs::remove_file(dir.with_extension("uncertain-run.json")).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn insights_action_closes_only_the_read_only_resolver_review() -> Result<(), String> {
    let repo = conflicted("resolver-insights-action")?;
    let ws = std::sync::Arc::new(crate::resolver::workspace::Workspace::prepare(
        repo.path(),
        &["a.txt".into()],
    )?);
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    let mut run = super::look_tests::run(Phase::Ready, Instant::now());
    run.workspace = Some(ws.clone());
    run.review = Some(super::ReviewState {
        files: Some(vec![]),
        selected: 0,
        as_diff: false,
    });
    let results = run.results.clone();
    let picked = run.picked.clone();
    let id = run.id.clone();
    tab.resolver.run = Some(run);
    assert!(tab.resolver.reviewing());
    tab.apply(
        Action::Insights(crate::repo::insights::InsightsAct::Open(
            crate::repo::insights::Kind::Evolution,
        )),
        &ctx,
    );
    assert!(!tab.resolver.reviewing());
    assert!(tab.insights.is_some());
    let run = tab.resolver.run.as_ref().ok_or("lost run")?;
    assert_eq!(run.id, id);
    assert_eq!(run.phase, Phase::Ready);
    assert_eq!(run.results, results);
    assert_eq!(run.picked, picked);
    assert!(std::sync::Arc::ptr_eq(
        run.workspace.as_ref().ok_or("lost workspace")?,
        &ws
    ));
    assert!(ws.dir().is_dir());
    Ok(())
}
