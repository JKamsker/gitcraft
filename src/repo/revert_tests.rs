//! Reverting one file, one hunk or a few lines of a past commit, on real repositories, through
//! the actions the window sends.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{Action, DiffKey, RepoTab, diff};
use crate::git;

fn opened_tab(path: PathBuf, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(path, ctx);
    assert!(wait(&mut tab, ctx, |t| !t.loading && t.timeline.loaded).is_ok());
    tab
}

/// A fresh repository named after `name`, with `files` committed as "one".
pub(super) fn scratch(name: &str, files: &[(&str, &str)]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        must(&dir, args)?;
    }
    commit(&dir, files, "one")?;
    Ok(dir)
}

pub(super) fn must(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {args:?}: {e}"))
}

/// Writes `files` and commits them all as `message`; returns the commit.
pub(super) fn commit(dir: &Path, files: &[(&str, &str)], message: &str) -> Result<String, String> {
    for (path, text) in files {
        write(dir, path, text)?;
    }
    must(dir, &["add", "-A"])?;
    must(dir, &["commit", "-q", "-m", message])?;
    Ok(must(dir, &["rev-parse", "HEAD"])?.trim().to_owned())
}

pub(super) fn write(dir: &Path, path: &str, text: &str) -> Result<(), String> {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, text).map_err(|e| format!("{path}: {e}"))
}

pub(super) fn read(dir: &Path, path: &str) -> Result<String, String> {
    std::fs::read_to_string(dir.join(path)).map_err(|e| format!("{path}: {e}"))
}

/// Lines `1..=n`, one number per line, with `changes` (line, new text) made.
pub(super) fn numbered(n: u32, changes: &[(u32, &str)]) -> String {
    (1..=n)
        .map(|i| {
            changes
                .iter()
                .find(|(at, _)| *at == i)
                .map_or_else(|| i.to_string(), |(_, t)| (*t).to_owned())
                + "\n"
        })
        .collect()
}

/// Polls the tab until `done` holds (or gives up after ten seconds).
pub(super) fn wait(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Err("the tab never got there".to_owned())
}

/// Sends `action` and waits for the operation it started to end; returns the status shown.
fn run(tab: &mut RepoTab, ctx: &egui::Context, action: Action) -> Result<(String, bool), String> {
    tab.apply(action, ctx);
    wait(tab, ctx, |t| {
        t.busy_op.is_none() && !t.loading && t.queued.is_empty()
    })?;
    Ok(tab.status.clone().unwrap_or_default())
}

fn revert_file(hash: &str, path: &str) -> Action {
    Action::RevertFile {
        hash: hash.to_owned(),
        path: path.to_owned(),
    }
}

/// Opens commit `hash`'s diff of `path`, waits for its lines and returns them.
fn open_diff(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    hash: &str,
    path: &str,
) -> Result<Vec<diff::DiffLine>, String> {
    tab.apply(
        Action::OpenDiff(DiffKey::Commit {
            hash: hash.to_owned(),
            path: path.to_owned(),
        }),
        ctx,
    );
    wait(tab, ctx, |t| {
        t.diff.as_ref().is_some_and(|d| d.data.is_some())
    })?;
    match tab.diff.as_ref().and_then(|d| d.data.as_ref()) {
        Some(Ok(data)) if data.revertable => Ok(data.lines.clone()),
        Some(Ok(_)) => Err("a commit's diff should be revertable in pieces".to_owned()),
        Some(Err(e)) => Err(e.clone()),
        None => Err("no diff".to_owned()),
    }
}

fn status_short(dir: &Path) -> Result<String, String> {
    must(dir, &["-c", "core.quotePath=false", "status", "--short"])
}

fn hunks(lines: &[diff::DiffLine]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| lines[i].kind == diff::Kind::Hunk)
        .collect()
}

fn revert_picked(lines: BTreeSet<usize>) -> Action {
    Action::PickLines {
        pick: diff::Pick::Revert,
        lines,
    }
}

#[test]
fn a_file_is_reverted_into_the_working_tree_and_ctrl_z_brings_the_change_back() -> Result<(), String>
{
    let path = "dir with space/ünï cödé.txt";
    let dir = scratch("revert-file", &[(path, &numbered(10, &[]))])?;
    let two = commit(&dir, &[(path, &numbered(10, &[(5, "five")]))], "two")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&two, path))?;
    assert!(!error, "{said}");
    assert!(said.contains("done") && said.contains("Ctrl+Z"), "{said}");
    assert_eq!(read(&dir, path)?, numbered(10, &[]));
    // In the working tree only: nothing staged, nothing committed.
    assert_eq!(status_short(&dir)?.trim_end(), format!(" M \"{path}\""));
    assert_eq!(must(&dir, &["rev-parse", "HEAD"])?.trim(), two);

    let (said, error) = run(&mut tab, &ctx, Action::Undo)?;
    assert!(!error, "{said}");
    assert_eq!(read(&dir, path)?, numbered(10, &[(5, "five")]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_file_changed_since_is_merged_in_with_three_ways() -> Result<(), String> {
    let dir = scratch("revert-merged", &[("a.txt", &numbered(12, &[]))])?;
    let two = commit(&dir, &[("a.txt", &numbered(12, &[(6, "six")]))], "two")?;
    // Line 4 changed since: the patch's context no longer matches, but the changes don't touch.
    let since = numbered(12, &[(4, "FOUR"), (6, "six")]);
    commit(&dir, &[("a.txt", &since)], "three")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&two, "a.txt"))?;
    assert!(!error, "{said}");
    assert!(said.contains("3-way"), "{said}");
    assert_eq!(read(&dir, "a.txt")?, numbered(12, &[(4, "FOUR")]));
    // A 3-way merge stages what it did.
    assert_eq!(status_short(&dir)?.trim_end(), "M  a.txt");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_revert_that_conflicts_says_so_and_leaves_the_file_conflicted() -> Result<(), String> {
    let dir = scratch("revert-conflict", &[("c.txt", &numbered(8, &[]))])?;
    let two = commit(&dir, &[("c.txt", &numbered(8, &[(4, "four")]))], "two")?;
    commit(&dir, &[("c.txt", &numbered(8, &[(4, "FOUR")]))], "three")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&two, "c.txt"))?;
    assert!(error);
    assert!(
        said.contains("conflicts") && said.contains("c.txt") && said.contains("Conflicted"),
        "{said}"
    );
    assert!(read(&dir, "c.txt")?.contains("<<<<<<<"));
    let status = status_short(&dir)?;
    assert!(status.starts_with("UU c.txt"), "{status}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn unstaged_changes_in_the_way_of_a_merge_are_named() -> Result<(), String> {
    let dir = scratch("revert-dirty", &[("d.txt", &numbered(8, &[]))])?;
    let two = commit(&dir, &[("d.txt", &numbered(8, &[(4, "four")]))], "two")?;
    // The line itself changed on disk, not committed: the patch doesn't apply, and a 3-way
    // merge needs the file as staged.
    let edited = numbered(8, &[(4, "four, edited")]);
    write(&dir, "d.txt", &edited)?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&two, "d.txt"))?;
    assert!(error);
    assert!(
        said.contains("d.txt has changed since") && said.contains("Stage or stash"),
        "{said}"
    );
    // Left as it was.
    assert_eq!(read(&dir, "d.txt")?, edited);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_binary_file_is_refused_with_the_reason() -> Result<(), String> {
    let dir = scratch("revert-binary", &[("keep.txt", "x\n")])?;
    std::fs::write(dir.join("img.bin"), [0u8, 1, 2, 3, 0, 9]).map_err(|e| e.to_string())?;
    let hash = commit(&dir, &[], "binary")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&hash, "img.bin"))?;
    assert!(error);
    assert!(said.contains("binary file"), "{said}");
    assert!(dir.join("img.bin").exists());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn reverting_an_added_file_removes_it_and_a_rename_moves_it_back() -> Result<(), String> {
    let dir = scratch("revert-add-rename", &[("old name.txt", &numbered(20, &[]))])?;
    let added = commit(&dir, &[("new.txt", "fresh\n")], "add")?;
    must(&dir, &["mv", "old name.txt", "renamed é.txt"])?;
    let edited = numbered(20, &[(3, "three")]);
    let renamed = commit(&dir, &[("renamed é.txt", &edited)], "rename")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);

    let (said, error) = run(&mut tab, &ctx, revert_file(&added, "new.txt"))?;
    assert!(!error, "{said}");
    assert!(!dir.join("new.txt").exists());

    let (said, error) = run(&mut tab, &ctx, revert_file(&renamed, "renamed é.txt"))?;
    assert!(!error, "{said}");
    assert!(!dir.join("renamed é.txt").exists());
    assert_eq!(read(&dir, "old name.txt")?, numbered(20, &[]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn one_hunk_of_a_commit_is_reverted_and_the_others_stay() -> Result<(), String> {
    let dir = scratch("revert-hunk", &[("h.txt", &numbered(40, &[]))])?;
    let both = numbered(40, &[(5, "five"), (30, "thirty")]);
    let two = commit(&dir, &[("h.txt", &both)], "two")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let lines = open_diff(&mut tab, &ctx, &two, "h.txt")?;
    let hunks = hunks(&lines);
    assert_eq!(hunks.len(), 2);

    let second = diff::hunk_changes(&lines, hunks[1]);
    let (said, error) = run(&mut tab, &ctx, revert_picked(second))?;
    assert!(!error, "{said}");
    assert!(said.starts_with("Revert a hunk of h.txt"), "{said}");
    assert_eq!(read(&dir, "h.txt")?, numbered(40, &[(5, "five")]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn only_the_lines_picked_are_reverted() -> Result<(), String> {
    let dir = scratch("revert-lines", &[("l.txt", &numbered(10, &[]))])?;
    let three = numbered(10, &[(4, "four"), (5, "five"), (6, "six")]);
    let two = commit(&dir, &[("l.txt", &three)], "two")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let lines = open_diff(&mut tab, &ctx, &two, "l.txt")?;
    // The removal of "5" and the addition of "five": line 5 goes back, 4 and 6 stay.
    let picked: BTreeSet<usize> = (0..lines.len())
        .filter(|&i| lines[i].text == "5" || lines[i].text == "five")
        .collect();
    assert_eq!(picked.len(), 2);
    if let Some(view) = &mut tab.diff {
        view.chosen = picked.clone();
    }

    let (said, error) = run(&mut tab, &ctx, revert_picked(picked))?;
    assert!(!error, "{said}");
    assert_eq!(
        read(&dir, "l.txt")?,
        numbered(10, &[(4, "four"), (6, "six")])
    );
    // The lines picked are let go of once reverted.
    assert!(tab.diff.as_ref().is_some_and(|d| d.chosen.is_empty()));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_hunk_whose_lines_changed_since_conflicts() -> Result<(), String> {
    let dir = scratch("revert-hunk-conflict", &[("k.txt", &numbered(10, &[]))])?;
    let two = commit(&dir, &[("k.txt", &numbered(10, &[(5, "five")]))], "two")?;
    commit(&dir, &[("k.txt", &numbered(10, &[(5, "FIVE")]))], "three")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let lines = open_diff(&mut tab, &ctx, &two, "k.txt")?;
    let hunk = *hunks(&lines).first().ok_or("no hunk")?;

    let (said, error) = run(
        &mut tab,
        &ctx,
        revert_picked(diff::hunk_changes(&lines, hunk)),
    )?;
    assert!(error);
    assert!(said.contains("conflicts"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn reverting_works_with_no_context_lines_in_the_users_config() -> Result<(), String> {
    let dir = scratch("revert-no-context", &[("n.txt", &numbered(20, &[]))])?;
    must(&dir, &["config", "diff.context", "0"])?;
    let both = numbered(20, &[(4, "four"), (15, "fifteen")]);
    let two = commit(&dir, &[("n.txt", &both)], "two")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let lines = open_diff(&mut tab, &ctx, &two, "n.txt")?;
    let hunk = *hunks(&lines).first().ok_or("no hunk")?;
    let (said, error) = run(
        &mut tab,
        &ctx,
        revert_picked(diff::hunk_changes(&lines, hunk)),
    )?;
    assert!(!error, "{said}");
    assert_eq!(read(&dir, "n.txt")?, numbered(20, &[(15, "fifteen")]));
    let (said, error) = run(&mut tab, &ctx, Action::Undo)?;
    assert!(!error, "{said}");
    let (said, error) = run(&mut tab, &ctx, revert_file(&two, "n.txt"))?;
    assert!(!error, "{said}");
    assert_eq!(read(&dir, "n.txt")?, numbered(20, &[]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn ctrl_z_brings_back_a_revert_whatever_apply_whitespace_says() -> Result<(), String> {
    let dir = scratch("revert-whitespace", &[("s.txt", &numbered(6, &[]))])?;
    must(&dir, &["config", "apply.whitespace", "error"])?;
    let two = commit(
        &dir,
        &[("s.txt", &numbered(6, &[(3, "3 ")]))],
        "trailing space",
    )?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let (said, error) = run(&mut tab, &ctx, revert_file(&two, "s.txt"))?;
    assert!(!error, "{said}");
    assert_eq!(read(&dir, "s.txt")?, numbered(6, &[]));
    let (said, error) = run(&mut tab, &ctx, Action::Undo)?;
    assert!(!error, "{said}");
    assert_eq!(read(&dir, "s.txt")?, numbered(6, &[(3, "3 ")]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_submodule_change_offers_no_pieces_to_revert() -> Result<(), String> {
    let dir = scratch("revert-submodule", &[("x.txt", "x\n")])?;
    let a = must(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    must(
        &dir,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{a},lib"),
        ],
    )?;
    must(&dir, &["commit", "-q", "-m", "submodule"])?;
    let sub = must(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let key = DiffKey::Commit {
        hash: sub.clone(),
        path: "lib".to_owned(),
    };
    let data = diff::load(&dir, &key, git::DiffOptions::default(), true)?;
    assert!(!data.revertable);
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let (said, error) = run(&mut tab, &ctx, revert_file(&sub, "lib"))?;
    assert!(error && said.contains("submodule"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn lines_picked_stay_picked_when_another_operation_holds_the_revert_up() -> Result<(), String> {
    let dir = scratch("revert-busy", &[("b.txt", &numbered(6, &[]))])?;
    let two = commit(&dir, &[("b.txt", &numbered(6, &[(2, "two")]))], "two")?;
    let ctx = egui::Context::default();
    let mut tab = opened_tab(dir.clone(), &ctx);
    let lines = open_diff(&mut tab, &ctx, &two, "b.txt")?;
    let picked: BTreeSet<usize> = (0..lines.len())
        .filter(|&i| lines[i].text == "two")
        .collect();
    if let Some(view) = &mut tab.diff {
        view.chosen = picked.clone();
    }
    tab.busy_op = Some("Push".to_owned());
    tab.apply(revert_picked(picked.clone()), &ctx);
    let (said, error) = tab.status.clone().unwrap_or_default();
    assert!(error && said.contains("Push was still running"), "{said}");
    assert!(tab.diff.as_ref().is_some_and(|d| d.chosen == picked));
    assert_eq!(read(&dir, "b.txt")?, numbered(6, &[(2, "two")]));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_whitespace_blind_diff_offers_no_pieces_to_revert() -> Result<(), String> {
    let dir = scratch("revert-ws", &[("w.txt", "a\nb\n")])?;
    let two = commit(&dir, &[("w.txt", "a\nB\n")], "two")?;
    let opts = git::DiffOptions {
        ignore_ws: true,
        whole_file: false,
        ..Default::default()
    };
    let key = DiffKey::Commit {
        hash: two,
        path: "w.txt".to_owned(),
    };
    let data = diff::load(&dir, &key, opts, true)?;
    assert!(!data.revertable && !data.partial);
    let data = diff::load(&dir, &key, git::DiffOptions::default(), true)?;
    assert!(data.revertable && !data.partial);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
