//! Requests going through a real tab, as the window takes them from the channel: an operation
//! that runs at once and lands in the undo list as the agent's, and the approval card
//! approved, denied, or left to time out.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::git;
use crate::mcp::link::Incoming;
use crate::mcp::wire::{Outcome, Reply, WriteOp};
use crate::repo::RepoTab;

/// A repository with one commit and a branch `old`, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-flow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["branch", "old"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// A tab of `dir`, read.
fn tab(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_owned(), ctx);
    settle(&mut tab, ctx, |t| !t.loading && !t.refs.head.is_empty());
    tab
}

/// Polls the tab, as frames do, until `done` (or a few seconds pass).
fn settle(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Hands `op` to the tab as the window's inbox does; the agent's end of the answer.
fn ask(tab: &mut RepoTab, ctx: &egui::Context, op: WriteOp) -> Receiver<Reply> {
    let (reply, answer) = channel();
    let incoming = Incoming {
        repo: tab.path.to_string_lossy().into_owned(),
        agent: "Claude Code".to_owned(),
        op,
        reply,
        gone: Default::default(),
    };
    tab.agent_request(incoming, ctx);
    answer
}

/// The answer, polling the tab until it comes.
fn answer(tab: &mut RepoTab, ctx: &egui::Context, rx: &Receiver<Reply>) -> Option<Reply> {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        tab.poll(ctx);
        if let Ok(reply) = rx.try_recv() {
            settle(tab, ctx, |t| {
                !t.loading
                    && matches!(
                        t.timeline.awaiting,
                        crate::repo::timeline::Awaiting::Nothing
                    )
            });
            return Some(reply);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn delete_old() -> WriteOp {
    WriteOp::DeleteBranch {
        name: "old".into(),
        force: false,
    }
}

#[test]
fn an_undoable_request_runs_at_once_and_is_marked_the_agents() {
    let ctx = egui::Context::default();
    let dir = repo("runs");
    let mut tab = tab(&dir, &ctx);
    let rx = ask(
        &mut tab,
        &ctx,
        WriteOp::CreateBranch {
            name: "feature".into(),
            start: None,
            checkout: false,
        },
    );
    let reply = answer(&mut tab, &ctx, &rx);
    let made = git::resolve(&dir, "refs/heads/feature").is_ok();
    let top = tab
        .timeline
        .current_entry()
        .and_then(|e| e.undo.as_ref())
        .cloned();
    let card = tab.agent_card();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(card.is_none(), "an undoable operation asks nobody");
    assert_eq!(reply.as_ref().map(|r| r.outcome), Some(Outcome::Done));
    assert!(reply.is_some_and(|r| r.message.contains("Ctrl+Z")));
    assert!(made);
    let top = top.unwrap_or_else(|| crate::repo::UndoEntry {
        id: 0,
        label: String::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
        agent: None,
    });
    assert_eq!(top.label, "Create branch feature");
    assert_eq!(top.agent.as_deref(), Some("Claude Code"));
}

#[test]
fn an_approved_request_runs_and_stays_undoable() {
    let ctx = egui::Context::default();
    let dir = repo("approve");
    let mut tab = tab(&dir, &ctx);
    let rx = ask(&mut tab, &ctx, delete_old());
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let card = tab.agent_card();
    let before = git::resolve(&dir, "refs/heads/old").is_ok();
    tab.agent_decide(true, &ctx);
    let reply = answer(&mut tab, &ctx, &rx);
    let gone = git::resolve(&dir, "refs/heads/old").is_err();
    let top = tab
        .timeline
        .current_entry()
        .and_then(|e| e.undo.as_ref())
        .and_then(|e| e.agent.clone());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(before, "nothing runs before the user answers");
    let armed_at_once = card
        .as_ref()
        .is_some_and(|c| c.shown_for.as_millis() >= 700);
    assert!(
        !armed_at_once,
        "Approve takes no click the moment the card shows"
    );
    let card = card.map(|c| (c.agent, c.ask.title, c.commands, c.undoable));
    assert_eq!(
        card,
        Some((
            "Claude Code".to_owned(),
            "Delete branch old".to_owned(),
            vec!["git branch -d -- old".to_owned()],
            true,
        ))
    );
    assert_eq!(reply.map(|r| r.outcome), Some(Outcome::Done));
    assert!(gone);
    assert_eq!(top.as_deref(), Some("Claude Code"));
}

#[test]
fn a_denied_request_changes_nothing() {
    let ctx = egui::Context::default();
    let dir = repo("deny");
    let mut tab = tab(&dir, &ctx);
    let rx = ask(&mut tab, &ctx, delete_old());
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    tab.agent_decide(false, &ctx);
    let reply = answer(&mut tab, &ctx, &rx);
    let kept = git::resolve(&dir, "refs/heads/old").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(reply.map(|r| r.outcome), Some(Outcome::Denied));
    assert!(kept);
    assert!(tab.agent_card().is_none());
    assert!(tab.timeline.entries.iter().all(|e| e.undo.is_none()));
}

#[test]
fn a_card_nobody_answers_is_denied_by_itself() {
    let ctx = egui::Context::default();
    let dir = repo("timeout");
    let mut tab = tab(&dir, &ctx);
    tab.agent_calls.approval_wait = Duration::from_millis(50);
    let rx = ask(&mut tab, &ctx, WriteOp::Push { force: true });
    let reply = answer(&mut tab, &ctx, &rx);
    let _ = std::fs::remove_dir_all(&dir);
    // No remote: the plan fails before any card. With one, the card times out.
    assert_eq!(reply.map(|r| r.outcome), Some(Outcome::Failed));

    let dir = repo("timeout-card");
    let mut tab = self::tab(&dir, &ctx);
    tab.agent_calls.approval_wait = Duration::from_millis(50);
    let rx = ask(&mut tab, &ctx, delete_old());
    let reply = answer(&mut tab, &ctx, &rx);
    let kept = git::resolve(&dir, "refs/heads/old").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(reply.as_ref().map(|r| r.outcome), Some(Outcome::TimedOut));
    assert!(reply.is_some_and(|r| r.message.contains("nothing was done")));
    assert!(kept);
    assert!(tab.agent_card().is_none());
}

#[test]
fn requests_wait_their_turn_behind_the_card() {
    let ctx = egui::Context::default();
    let dir = repo("queue");
    let mut tab = tab(&dir, &ctx);
    let first = ask(&mut tab, &ctx, delete_old());
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let second = ask(
        &mut tab,
        &ctx,
        WriteOp::CreateBranch {
            name: "next".into(),
            start: None,
            checkout: false,
        },
    );
    // Planned, but not run while the card is open.
    settle(&mut tab, &ctx, |t| !t.agent_calls.queue.is_empty());
    let waited = git::resolve(&dir, "refs/heads/next").is_err();
    tab.agent_decide(false, &ctx);
    let first = answer(&mut tab, &ctx, &first);
    let second = answer(&mut tab, &ctx, &second);
    let made = git::resolve(&dir, "refs/heads/next").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(waited);
    assert_eq!(first.map(|r| r.outcome), Some(Outcome::Denied));
    assert_eq!(second.map(|r| r.outcome), Some(Outcome::Done));
    assert!(made);
}

#[test]
fn a_request_that_cant_be_planned_says_why() {
    let ctx = egui::Context::default();
    let dir = repo("refused");
    let mut tab = tab(&dir, &ctx);
    let rx = ask(
        &mut tab,
        &ctx,
        WriteOp::Checkout {
            branch: "missing".into(),
        },
    );
    let reply = answer(&mut tab, &ctx, &rx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(reply.as_ref().map(|r| r.outcome), Some(Outcome::Failed));
    assert!(reply.is_some_and(|r| r.message.contains("no local branch called missing")));
}

#[test]
fn a_request_is_planned_against_the_repository_it_runs_on() {
    let ctx = egui::Context::default();
    let dir = repo("planned-late");
    // `side` changes a.txt one way, `main` the other: picking it conflicts.
    for args in [
        &["switch", "-q", "-c", "side"][..],
        &["commit", "-q", "--allow-empty", "-m", "x"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("a.txt"), "side\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-q", "-m", "side"]).is_ok());
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    let _ = std::fs::write(dir.join("a.txt"), "main\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-q", "-m", "main"]).is_ok());
    let mut tab = tab(&dir, &ctx);
    // Sent together: the stage is clean when it comes in, conflicted when its turn comes.
    let pick = ask(
        &mut tab,
        &ctx,
        WriteOp::CherryPick {
            commit: "side".into(),
        },
    );
    let stage = ask(
        &mut tab,
        &ctx,
        WriteOp::Stage {
            paths: vec!["a.txt".into()],
        },
    );
    let picked = answer(&mut tab, &ctx, &pick);
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let card = tab.agent_card().map(|c| c.ask.title);
    tab.agent_decide(false, &ctx);
    let staged = answer(&mut tab, &ctx, &stage);
    let still = git::run(Some(&dir), &["ls-files", "-u"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        picked.map(|r| r.outcome),
        Some(Outcome::Failed),
        "the pick conflicts"
    );
    assert_eq!(card.as_deref(), Some("Mark conflicts resolved"));
    assert_eq!(staged.map(|r| r.outcome), Some(Outcome::Denied));
    assert!(!still.is_empty(), "the conflict is left for the user");
}

#[test]
fn an_approval_holds_only_for_what_the_card_showed() {
    let ctx = egui::Context::default();
    let dir = repo("approval-stale");
    let mut tab = tab(&dir, &ctx);
    let rx = ask(&mut tab, &ctx, delete_old());
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let first = tab.agent_card().map(|c| c.ask.body);
    // `old` gets a commit while the card is open.
    assert!(
        git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "later"]
        )
        .is_ok()
    );
    assert!(git::run(Some(&dir), &["branch", "-f", "old", "HEAD"]).is_ok());
    tab.agent_decide(true, &ctx);
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let second = tab.agent_card().map(|c| c.ask.body);
    let kept = git::resolve(&dir, "refs/heads/old").is_ok();
    tab.agent_decide(false, &ctx);
    let reply = answer(&mut tab, &ctx, &rx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(first.is_some() && second.is_some());
    assert_ne!(first, second, "the card shows the branch as it is now");
    assert!(kept, "not deleted on an approval of what it was before");
    assert_eq!(reply.map(|r| r.outcome), Some(Outcome::Denied));
}

#[test]
fn a_card_goes_away_once_the_agent_stops_waiting() {
    let ctx = egui::Context::default();
    let dir = repo("abandoned");
    let mut tab = tab(&dir, &ctx);
    let (reply, rx) = channel();
    let gone = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    tab.agent_request(
        Incoming {
            repo: tab.path.to_string_lossy().into_owned(),
            agent: "Codex".into(),
            op: delete_old(),
            reply,
            gone: gone.clone(),
        },
        &ctx,
    );
    settle(&mut tab, &ctx, |t| t.agent_card().is_some());
    let shown = tab.agent_card().is_some();
    // The call was cancelled (its connection closed).
    gone.store(true, std::sync::atomic::Ordering::SeqCst);
    settle(&mut tab, &ctx, |t| t.agent_card().is_none());
    let gone_card = tab.agent_card().is_none();
    tab.agent_decide(true, &ctx);
    settle(&mut tab, &ctx, |t| !t.is_busy());
    let kept = git::resolve(&dir, "refs/heads/old").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shown && gone_card);
    assert!(kept, "nothing runs for an agent that left");
    assert!(rx.try_recv().is_err());
}
