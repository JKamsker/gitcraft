//! The reflog view driven as the window drives it: a commit lost in the terminal is found in
//! HEAD's reflog, shown, and made a branch of; the scan finds what an expired reflog no longer
//! has, and a scan cancelled leaves what was found before.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ReflogAct, Source, menu};
use crate::git::{self, ReflogKind};
use crate::repo::{Action, RepoTab};

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-reflog-view-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.0), args)
            .map(|out| out.trim().to_owned())
            .map_err(|e| format!("git {args:?}: {e}"))
    }

    /// A commit of a file of its own named `file`.
    fn commit(&self, file: &str) -> Result<String, String> {
        std::fs::write(self.0.join(file), format!("{file}\n")).map_err(|e| e.to_string())?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "-m", file])?;
        Ok(git::head(&self.0))
    }
}

/// Polls the tab until `done` says so (or 20 seconds have gone by).
fn until(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(20);
    while Instant::now() < end {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    false
}

fn idle(tab: &RepoTab) -> bool {
    !tab.opening()
        && !tab.loading
        && tab.busy_op.is_none()
        && tab
            .reflog
            .as_ref()
            .is_none_or(|v| !v.loading && v.scan.is_none())
}

fn act(tab: &mut RepoTab, ctx: &egui::Context, action: Action) -> Result<(), String> {
    tab.apply(action, ctx);
    if until(tab, ctx, idle) {
        Ok(())
    } else {
        Err("the tab never settled".to_owned())
    }
}

#[test]
fn a_commit_lost_in_the_terminal_comes_back_as_a_branch() -> Result<(), String> {
    let repo = Repo::new("reset")?;
    repo.commit("base.txt")?;
    let lost = repo.commit("lost.txt")?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_owned(), &ctx);
    if !until(&mut tab, &ctx, idle) {
        return Err("the repository never opened".to_owned());
    }
    assert!(!tab.index.contains_key(&lost));

    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::head())),
    )?;
    let view = tab.reflog.as_ref().ok_or("no view")?;
    let reset = view.entries.first().ok_or("no entries")?;
    assert_eq!(reset.message.kind, ReflogKind::Reset);
    assert_eq!(reset.old.as_deref(), Some(lost.as_str()));
    let at = view
        .entries
        .iter()
        .position(|e| e.new == lost)
        .ok_or("the lost commit isn't listed")?;

    // Selected: it shows in the detail pane, though the graph doesn't have it.
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::Select(at)))?;
    assert!(until(&mut tab, &ctx, |t| t.detail.is_some()));
    assert_eq!(
        tab.detail.as_ref().map(|d| d.hash.clone()),
        Some(lost.clone())
    );
    assert_eq!(tab.selected, None);
    assert_eq!(tab.reflog_commit_off_graph(), Some(lost.clone()));

    // Made a branch of (what the dialog runs once a name is typed): now in the graph, and
    // selected there; the reflog is read again.
    let pick = tab
        .reflog
        .as_ref()
        .and_then(|v| v.selected_pick())
        .ok_or("nothing selected")?;
    assert!(matches!(
        menu::create_branch(&pick),
        Action::Dialog(crate::repo::Dialog::CreateBranch { at, checkout: false, .. }) if at == lost
    ));
    act(
        &mut tab,
        &ctx,
        Action::Ops {
            label: "Create branch recovered".to_owned(),
            ops: crate::repo::ops(&[&["branch", "recovered", &lost]]),
            refresh: crate::repo::Refresh::All,
        },
    )?;
    assert!(until(&mut tab, &ctx, |t| t.index.contains_key(&lost)));
    until(&mut tab, &ctx, idle);
    assert_eq!(repo.git(&["rev-parse", "recovered"])?, lost);
    let row = tab.index.get(&lost).copied();
    assert!(row.is_some() && tab.selected == row);
    // Selecting the row reads its detail again, in the background.
    assert!(until(&mut tab, &ctx, |t| t
        .detail
        .as_ref()
        .is_some_and(|d| d.hash == lost)));

    // The branch's own reflog.
    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::branch("recovered"))),
    )?;
    let view = tab.reflog.as_ref().ok_or("no view")?;
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.entries[0].message.kind, ReflogKind::Branch);
    assert_eq!(view.selected, None);
    Ok(())
}

#[test]
fn the_scan_finds_what_an_expired_reflog_lost_and_a_cancelled_scan_keeps_it() -> Result<(), String>
{
    let repo = Repo::new("scan")?;
    repo.commit("base.txt")?;
    repo.git(&["switch", "-q", "-c", "topic"])?;
    let lost = repo.commit("topic.txt")?;
    repo.git(&["switch", "-q", "main"])?;
    repo.git(&["branch", "-q", "-D", "topic"])?;
    repo.git(&[
        "reflog",
        "expire",
        "--expire=now",
        "--expire-unreachable=now",
        "--all",
    ])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_owned(), &ctx);
    if !until(&mut tab, &ctx, idle) {
        return Err("the repository never opened".to_owned());
    }
    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::Lost)),
    )?;
    // The tab only explains the scan: it reads every object, so it waits to be asked.
    assert!(
        tab.reflog
            .as_ref()
            .is_some_and(|v| v.lost.is_none() && v.scan.is_none())
    );
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::FindLost))?;
    let found = |tab: &RepoTab| -> Vec<String> {
        tab.reflog
            .as_ref()
            .and_then(|v| v.lost_commits())
            .map(|c| c.iter().map(|c| c.hash.clone()).collect())
            .unwrap_or_default()
    };
    assert_eq!(found(&tab), std::slice::from_ref(&lost));

    // Another scan, cancelled: it stops and what was found stays.
    tab.apply(Action::Reflog(ReflogAct::Reload), &ctx);
    assert!(tab.reflog.as_ref().is_some_and(|v| v.scan.is_some()));
    tab.apply(Action::Reflog(ReflogAct::CancelScan), &ctx);
    assert!(until(&mut tab, &ctx, idle));
    std::thread::sleep(Duration::from_millis(200));
    tab.poll(&ctx);
    assert_eq!(found(&tab), std::slice::from_ref(&lost));

    // Cherry-picked back onto main, as the view's Cherry-pick does.
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::Select(0)))?;
    let pick = tab
        .reflog
        .as_ref()
        .and_then(|v| v.selected_pick())
        .ok_or("nothing selected")?;
    assert_eq!(pick.hash, lost);
    act(&mut tab, &ctx, menu::cherry_pick(&pick))?;
    assert!(repo.path().join("topic.txt").exists());
    assert_eq!(repo.git(&["log", "-1", "--format=%s"])?, "topic.txt");
    // Checked out detached, as the view's Check out does.
    act(&mut tab, &ctx, menu::checkout(&pick))?;
    assert_eq!(git::head(repo.path()), lost);
    Ok(())
}

/// Closing the view stops a scan still running.
#[test]
fn closing_the_view_stops_its_scan() -> Result<(), String> {
    let repo = Repo::new("close")?;
    repo.commit("base.txt")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_owned(), &ctx);
    if !until(&mut tab, &ctx, idle) {
        return Err("the repository never opened".to_owned());
    }
    tab.apply(Action::Reflog(ReflogAct::FindLost), &ctx);
    let cancel = tab
        .reflog
        .as_ref()
        .and_then(|v| v.scan.as_ref())
        .map(|s| s.cancel.clone())
        .ok_or("no scan")?;
    tab.apply(Action::CloseReflog, &ctx);
    assert!(cancel.is_cancelled());
    // Its answer, when it comes, goes nowhere.
    std::thread::sleep(Duration::from_millis(200));
    tab.poll(&ctx);
    assert!(tab.reflog.is_none());
    Ok(())
}

/// Leaving the view (closing it, or another list) takes the detail pane off a commit the
/// graph doesn't have; a lost commit made a branch of drops off the lost list; an operation
/// that leaves conflicts still goes to the changes row.
#[test]
fn leaving_an_off_graph_commit_and_recovering_one() -> Result<(), String> {
    let repo = Repo::new("leave")?;
    repo.commit("base.txt")?;
    let lost = repo.commit("lost.txt")?;
    repo.git(&["reset", "-q", "--hard", "HEAD~1"])?;
    let head = git::head(repo.path());
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_owned(), &ctx);
    if !until(&mut tab, &ctx, idle) {
        return Err("the repository never opened".to_owned());
    }
    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::head())),
    )?;
    let at = tab
        .reflog
        .as_ref()
        .and_then(|v| v.entries.iter().position(|e| e.new == lost))
        .ok_or("not listed")?;
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::Select(at)))?;
    assert_eq!(tab.detail_hash.as_deref(), Some(lost.as_str()));
    // Another list: the entries of the one before are not shown under its name.
    tab.apply(
        Action::Reflog(ReflogAct::Show(Source::branch("main"))),
        &ctx,
    );
    assert!(tab.reflog.as_ref().is_some_and(|v| v.entries.is_empty()));
    assert_eq!(tab.detail_hash.as_deref(), Some(head.as_str()));
    until(&mut tab, &ctx, idle);

    // An operation left conflicts: the changes row, not the lost commit.
    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::head())),
    )?;
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::Select(at)))?;
    std::fs::write(repo.path().join("dirty.txt"), "x").map_err(|e| e.to_string())?;
    tab.focus_wip = true;
    act(&mut tab, &ctx, Action::Reload)?;
    assert!(until(&mut tab, &ctx, |t| t
        .selected
        .is_some_and(|r| t.is_wip(r))));
    assert!(!tab.focus_wip);

    // Found by the scan, made a branch of: no longer lost.
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::FindLost))?;
    let listed = |t: &RepoTab| {
        t.reflog
            .as_ref()
            .and_then(|v| v.lost_commits())
            .map(<[_]>::len)
    };
    assert_eq!(listed(&tab), Some(1));
    act(
        &mut tab,
        &ctx,
        Action::Ops {
            label: "Create branch back".to_owned(),
            ops: crate::repo::ops(&[&["branch", "back", &lost]]),
            refresh: crate::repo::Refresh::All,
        },
    )?;
    assert!(until(&mut tab, &ctx, |t| listed(t) == Some(0)));

    // Closing the view with a lost commit shown goes back to the graph's selection.
    act(
        &mut tab,
        &ctx,
        Action::Reflog(ReflogAct::Show(Source::head())),
    )?;
    let reset_at = tab
        .reflog
        .as_ref()
        .and_then(|v| v.entries.iter().position(|e| e.new == lost))
        .ok_or("not listed")?;
    act(&mut tab, &ctx, Action::Reflog(ReflogAct::Select(reset_at)))?;
    act(&mut tab, &ctx, Action::CloseReflog)?;
    assert!(tab.reflog.is_none());
    Ok(())
}
