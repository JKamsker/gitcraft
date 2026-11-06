//! Rehearsals driven as the window drives them: the dialog opened from a menu, both
//! operations rehearsed and compared, one done for real (into the undo journal), and one
//! refused because a branch moved meanwhile.

use std::time::{Duration, Instant};

use super::*;
use crate::git::Played;
use crate::git::fixture::Repo;

/// `feature` (checked out) with three commits, the second changing `shared.txt` as `main`
/// does when `conflict`; else `main` changes another file.
fn diverged(name: &str, conflict: bool) -> Result<Repo, String> {
    let repo = Repo::new(&format!("tab-rehearse-{name}"))?;
    repo.commit("base", &[("shared.txt", Some("one\n"))])?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    for i in 1..=3 {
        let text = format!("{i}\n");
        let mut files = vec![("f.txt", Some(text.as_str()))];
        if i == 2 {
            files.push(("shared.txt", Some("feature's\n")));
        }
        repo.commit(&format!("step {i}"), &files)?;
    }
    repo.git(&["checkout", "-q", "main"])?;
    let file = if conflict { "shared.txt" } else { "main.txt" };
    repo.commit("main moves", &[(file, Some("main's\n"))])?;
    repo.git(&["checkout", "-q", "feature"])?;
    Ok(repo)
}

fn open(repo: &Repo, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(repo.path().to_path_buf(), ctx);
    settle(&mut tab, ctx)?;
    Ok(tab)
}

fn settle(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(90);
    while Instant::now() < until {
        tab.poll(ctx);
        let applying = tab.rehearsing.run.as_ref().is_some_and(|r| r.applying);
        if tab.settled() && !tab.rehearsing.going() && !applying && tab.history_gen > 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Err("the tab never settled".to_owned())
}

fn act(tab: &mut RepoTab, ctx: &egui::Context, act: RehearseAct) -> Result<(), String> {
    tab.apply(Action::Rehearse(act), ctx);
    settle(tab, ctx)
}

fn both(command: Option<&str>) -> RehearseAct {
    RehearseAct::Start {
        other: "main".to_owned(),
        remote: false,
        kinds: vec![RehearseKind::Rebase, RehearseKind::Merge],
        command: command.map(str::to_owned),
    }
}

fn played(tab: &RepoTab, kind: RehearseKind) -> Result<Played, String> {
    let run = tab.rehearsing.run.as_ref().ok_or("no rehearsal")?;
    let (_, result) = run
        .results
        .iter()
        .find(|(k, _)| *k == kind)
        .ok_or("not rehearsed")?;
    Ok(result.clone()?.played)
}

#[test]
fn the_dialog_offers_the_test_command() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = diverged("dialog", false)?;
    git::set_test_command(repo.path(), git::DEFAULT_TEST, "make check")?;
    let mut tab = open(&repo, &ctx)?;
    tab.test_lane.visible = true;
    tab.read_test_lane(&ctx);
    settle(&mut tab, &ctx)?;
    let until = Instant::now() + Duration::from_secs(10);
    while tab.test_lane.command().is_none() && Instant::now() < until {
        tab.poll(&ctx);
        std::thread::sleep(Duration::from_millis(15));
    }
    act(
        &mut tab,
        &ctx,
        RehearseAct::Open {
            other: "main".into(),
            remote: false,
        },
    )?;
    let Some(Dialog::Rehearse {
        other,
        rebase,
        compare,
        command,
        ..
    }) = &tab.dialog
    else {
        return Err("no dialog".to_owned());
    };
    assert_eq!(other, "main");
    assert!(*rebase && *compare);
    assert_eq!(command, "make check");
    // The palette offers it with the base branch.
    let items = tab.rehearse_palette_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "Rehearse merge or rebase of feature with main…");
    Ok(())
}

/// Both rehearsed: the rebase stops at the second commit, the merge has the same conflict;
/// nothing of the user's changes.
#[test]
fn both_rehearsed_with_their_conflicts() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = diverged("conflict", true)?;
    let tip = repo.rev("feature")?;
    let mut tab = open(&repo, &ctx)?;
    act(&mut tab, &ctx, both(Some("true")))?;
    let Played::Stopped {
        step,
        of,
        conflicts,
        ..
    } = played(&tab, RehearseKind::Rebase)?
    else {
        return Err("the rebase didn't stop".to_owned());
    };
    assert_eq!((step, of), (2, 3));
    assert_eq!(conflicts, ["shared.txt"]);
    assert!(matches!(
        played(&tab, RehearseKind::Merge)?,
        Played::Stopped { .. }
    ));
    assert_eq!(repo.rev("feature")?, tip);
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

/// A clean rehearsal done for real: the branch is rebased, and Ctrl+Z can undo it.
#[test]
fn applying_does_it_for_real_and_it_can_be_undone() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = diverged("apply", false)?;
    let mut tab = open(&repo, &ctx)?;
    act(&mut tab, &ctx, both(None))?;
    let Played::Clean { tree, .. } = played(&tab, RehearseKind::Rebase)? else {
        return Err("the rebase didn't go through".to_owned());
    };
    let undo_before = tab.undo_stack.len();
    act(&mut tab, &ctx, RehearseAct::Apply(RehearseKind::Rebase))?;
    assert!(tab.rehearsing.run.is_none());
    // The real rebase gives what the rehearsal said.
    assert_eq!(repo.rev("main")?, repo.rev("feature~3")?);
    assert_eq!(repo.tree("feature")?, tree);
    assert_eq!(tab.undo_stack.len(), undo_before + 1);
    Ok(())
}

/// A branch moved since the rehearsal: applying refuses, saying which.
#[test]
fn applying_after_a_branch_moved_is_refused() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = diverged("moved", false)?;
    let mut tab = open(&repo, &ctx)?;
    act(&mut tab, &ctx, both(None))?;
    let tip = repo.rev("feature")?;
    repo.git(&["branch", "-f", "main", "main~1"])?;
    act(&mut tab, &ctx, RehearseAct::Apply(RehearseKind::Merge))?;
    let (said, bad) = tab.status.clone().ok_or("nothing said")?;
    assert!(bad);
    assert!(said.starts_with("main moved since the rehearsal"), "{said}");
    assert_eq!(repo.rev("feature")?, tip);
    // The window stays, to rehearse again.
    assert!(tab.rehearsing.run.is_some());
    Ok(())
}
