//! A search driven as the window drives it: the dialog opened from a branch's menu, the
//! search run in the background with its progress, what it found, and Stop.

use std::time::{Duration, Instant};

use super::*;
use crate::git::Found;
use crate::git::fixture::Repo;

/// `a` (checked out) and `b` off `main`, three commits each; `a`'s second adds `flag`, `b`'s
/// third `uses-flag`.
fn two_branches(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(&format!("tab-interaction-{name}"))?;
    repo.commit("base", &[("README.md", Some("hi\n"))])?;
    for (branch, special, file) in [("b", 3, "uses-flag"), ("a", 2, "flag")] {
        repo.git(&["checkout", "-q", "-b", branch, "main"])?;
        for i in 1..=3 {
            let own = format!("{branch}{i}.txt");
            let path = if i == special { file } else { own.as_str() };
            repo.commit(&format!("{branch} step {i}"), &[(path, Some("x\n"))])?;
        }
    }
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
        if tab.settled() && !tab.interactions.going() && tab.history_gen > 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Err("the tab never settled".to_owned())
}

fn dialog_spec(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    other: &str,
) -> Result<InteractionSpec, String> {
    tab.apply(
        Action::Interaction(InteractionAct::Open {
            other: other.to_owned(),
        }),
        ctx,
    );
    match tab.dialog.take() {
        Some(Dialog::FindInteraction { spec, .. }) => Ok(spec),
        _ => Err("no dialog".to_owned()),
    }
}

#[test]
fn the_search_finds_the_pair_from_a_branch_s_menu() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = two_branches("pair")?;
    let mut tab = open(&repo, &ctx)?;
    let mut spec = dialog_spec(&mut tab, &ctx, "b")?;
    assert_eq!((spec.a.as_str(), spec.b.as_str()), ("a", "b"));
    assert_eq!(spec.budget, git::DEFAULT_BUDGET);
    assert_eq!(spec.reruns, 1);
    spec.command = "! { test -e flag && test -e uses-flag; }".to_owned();
    tab.apply(Action::Interaction(InteractionAct::Start(spec)), &ctx);
    settle(&mut tab, &ctx)?;
    let search = tab.interactions.search.as_ref().ok_or("no search")?;
    let found = search.ended.clone().ok_or("not ended")??;
    let Found::Together { commits, minimal } = &found.found else {
        return Err(format!("{:?}", found.found));
    };
    assert!(minimal);
    let names: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(names, ["a step 2", "b step 3"]);
    assert!(search.last.is_some());
    tab.apply(Action::Interaction(InteractionAct::Close), &ctx);
    assert!(tab.interactions.search.is_none());
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn the_palette_offers_every_other_branch() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = two_branches("palette")?;
    let tab = open(&repo, &ctx)?;
    let mut items: Vec<String> = tab
        .interaction_palette_items()
        .into_iter()
        .map(|i| i.0)
        .collect();
    items.sort();
    assert_eq!(
        items,
        [
            "Find the interaction of a and b…",
            "Find the interaction of a and main…"
        ]
    );
    Ok(())
}

/// Stopped, it ends with the reason and leaves no worktree.
#[cfg(unix)]
#[test]
fn stop_ends_the_search() -> Result<(), String> {
    let ctx = egui::Context::default();
    let repo = two_branches("stop")?;
    let mut tab = open(&repo, &ctx)?;
    let mut spec = dialog_spec(&mut tab, &ctx, "b")?;
    spec.command = "sleep 30".to_owned();
    tab.apply(Action::Interaction(InteractionAct::Start(spec)), &ctx);
    std::thread::sleep(Duration::from_millis(600));
    tab.apply(Action::Interaction(InteractionAct::Stop), &ctx);
    settle(&mut tab, &ctx)?;
    let ended = tab
        .interactions
        .search
        .as_ref()
        .and_then(|s| s.ended.clone())
        .ok_or("not ended")?;
    assert_eq!(ended.err().as_deref(), Some("stopped"));
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}
