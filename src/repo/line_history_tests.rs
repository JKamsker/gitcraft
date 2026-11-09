//! The history of a few lines (`git log -L`) on real repositories: through later edits and
//! renames, from lines on disk or staged, and in the file view as the window opens it.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::revert_tests::{commit, must, numbered, scratch, wait, write};
use super::{Action, DiffKey, RepoTab, diff};
use crate::git::{self, LinesAt};

fn summaries(commits: &[git::FileCommit]) -> Vec<&str> {
    commits.iter().map(|c| c.summary.as_str()).collect()
}

fn head() -> LinesAt {
    LinesAt::Rev("HEAD".to_owned())
}

/// a.txt: 20 numbered lines; "five" on line 5; renamed to "ü dir/b c.txt"; a line added at
/// the top (lines move down by one) and line 15 changed (outside lines 4–7).
fn history_repo(name: &str) -> Result<PathBuf, String> {
    let dir = scratch(name, &[("a.txt", &numbered(20, &[]))])?;
    commit(&dir, &[("a.txt", &numbered(20, &[(5, "five")]))], "five")?;
    std::fs::create_dir_all(dir.join("ü dir")).map_err(|e| e.to_string())?;
    must(&dir, &["mv", "a.txt", "ü dir/b c.txt"])?;
    commit(&dir, &[], "rename")?;
    let moved = format!("top\n{}", numbered(20, &[(5, "five"), (15, "fifteen")]));
    commit(&dir, &[("ü dir/b c.txt", &moved)], "top and fifteen")?;
    Ok(dir)
}

const PATH: &str = "ü dir/b c.txt";

#[test]
fn lines_are_followed_through_later_edits_and_renames() -> Result<(), String> {
    let dir = history_repo("line-history")?;
    // Lines 5–8 now are lines 4–7 before the line added at the top.
    let commits = git::line_history(&dir, &head(), PATH, (5, 8))?;
    // The rename and the change of line 15 didn't touch them; the line at the top only moved
    // them.
    assert_eq!(summaries(&commits), ["five", "one"]);
    assert_eq!(commits[0].path, "a.txt");
    assert_eq!(commits[0].lines, vec![(4, 7)]);
    assert_eq!(commits[1].path, "a.txt");

    // The part of "five" that changed them, read again from what the list holds.
    let five = &commits[0];
    let patch = git::line_diff(&dir, &five.hash, &five.path, &five.lines)?;
    assert!(patch.contains("-5\n+five\n"), "{patch}");
    assert!(!patch.contains("fifteen"));

    // Line 16 (old 15) is touched by the last commit and the first only.
    let fifteen = git::line_history(&dir, &head(), PATH, (16, 16))?;
    assert_eq!(summaries(&fifteen), ["top and fifteen", "one"]);
    assert_eq!(fifteen[0].path, PATH);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn lines_on_disk_or_staged_are_found_in_head_first() -> Result<(), String> {
    let dir = history_repo("line-history-work")?;
    // Two more lines at the top, on disk only: line 8 on disk is line 6 in HEAD.
    let head_text = std::fs::read_to_string(dir.join(PATH)).map_err(|e| e.to_string())?;
    write(&dir, PATH, &format!("new 1\nnew 2\n{head_text}"))?;
    let on_disk = git::line_history(&dir, &LinesAt::WorkTree, PATH, (8, 8))?;
    assert_eq!(summaries(&on_disk), ["five", "one"]);
    // Lines only on disk have no history.
    let new = git::line_history(&dir, &LinesAt::WorkTree, PATH, (1, 2));
    assert!(new.is_err_and(|e| e.contains("aren't committed yet")));
    // Staged, the same.
    must(&dir, &["add", "-A"])?;
    let staged = git::line_history(&dir, &LinesAt::Index, PATH, (8, 8))?;
    assert_eq!(summaries(&staged), ["five", "one"]);
    // A new file has none at all.
    write(&dir, "fresh.txt", "x\n")?;
    must(&dir, &["add", "fresh.txt"])?;
    let fresh = git::line_history(&dir, &LinesAt::Index, "fresh.txt", (1, 1));
    assert!(fresh.is_err_and(|e| e.contains("isn't committed yet")));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn lines_of_a_file_renamed_but_not_committed_are_found_under_its_old_name() -> Result<(), String> {
    let dir = history_repo("line-history-renamed")?;
    must(&dir, &["mv", PATH, "moved.txt"])?;
    // Staged as it was, and edited on disk since: line 7 is HEAD's 6 (old 5, "five").
    let staged = git::line_history(&dir, &LinesAt::Index, "moved.txt", (6, 6))?;
    assert_eq!(summaries(&staged), ["five", "one"]);
    let text = std::fs::read_to_string(dir.join("moved.txt")).map_err(|e| e.to_string())?;
    write(&dir, "moved.txt", &format!("added\n{text}"))?;
    let on_disk = git::line_history(&dir, &LinesAt::WorkTree, "moved.txt", (7, 7))?;
    assert_eq!(summaries(&on_disk), ["five", "one"]);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn lines_past_the_end_and_files_not_there_say_so() -> Result<(), String> {
    let dir = scratch(
        "line-history-errors",
        &[("s.txt", "one\ntwo\n"), ("empty.txt", "")],
    )?;
    let past = git::line_history(&dir, &head(), "s.txt", (5, 9));
    assert!(past.is_err_and(|e| e.contains("aren't in s.txt")));
    let empty = git::line_history(&dir, &head(), "empty.txt", (1, 1));
    assert!(empty.is_err_and(|e| e.contains("empty.txt")));
    let missing = git::line_history(&dir, &head(), "nope.txt", (1, 1));
    assert!(missing.is_err_and(|e| e.contains("nope.txt")));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_huge_file_and_a_name_with_a_colon() -> Result<(), String> {
    // Windows allows no `:` in file names.
    let name = if cfg!(windows) {
        "big, file.txt"
    } else {
        "big: file.txt"
    };
    let dir = scratch("line-history-huge", &[(name, &numbered(30_000, &[]))])?;
    let near_end = numbered(30_000, &[(29_990, "near the end")]);
    commit(&dir, &[(name, &near_end)], "end")?;
    let commits = git::line_history(&dir, &head(), name, (29_985, 30_000))?;
    assert_eq!(summaries(&commits), ["end", "one"]);
    assert_eq!(commits[0].path, name);
    // Every line at once.
    let all = git::line_history(&dir, &head(), name, (1, 30_000))?;
    assert_eq!(all.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn the_window_opens_the_history_of_lines_and_the_diff_of_one_of_them() -> Result<(), String> {
    let dir = history_repo("line-history-ui")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.apply(
        Action::LineHistory {
            path: PATH.to_owned(),
            at: head(),
            // Picked from the bottom up.
            range: (8, 5),
        },
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        t.file_view.as_ref().is_some_and(|v| v.history.is_some())
    })?;
    let commits = match tab.file_view.as_ref().and_then(|v| v.history.clone()) {
        Some(Ok(c)) => c,
        other => return Err(format!("{other:?}")),
    };
    assert_eq!(summaries(&commits), ["five", "one"]);
    assert_eq!(
        tab.file_view.as_ref().and_then(|v| v.lines.clone()),
        Some((head(), (5, 8)))
    );

    // The whole file's history, read meanwhile, doesn't take the place of the lines'.
    let whole = super::Msg::FileHistory(
        tab.file_view
            .as_ref()
            .ok_or("missing file view")?
            .generation,
        PATH.to_owned(),
        Some("HEAD".to_owned()),
        Ok(Vec::new()),
    );
    let _ = tab.tx.send(whole);
    tab.poll(&ctx);
    let shown = tab.file_view.as_ref().and_then(|v| v.history.clone());
    assert!(matches!(&shown, Some(Ok(c)) if c.len() == 2));

    // Clicking "five" shows only what it did to those lines, and offers nothing to revert.
    let five = commits[0].clone();
    tab.apply(
        Action::OpenDiffOfLines {
            hash: five.hash.clone(),
            path: five.path.clone(),
            lines: five.lines.clone(),
        },
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        t.diff.as_ref().is_some_and(|d| d.data.is_some())
    })?;
    assert!(tab.history_beside_diff(), "the diff opens beside the list");
    let data = match tab.diff.as_ref().and_then(|d| d.data.as_ref()) {
        Some(Ok(d)) => d,
        other => return Err(format!("{:?}", other.map(|r| r.as_ref().err()))),
    };
    assert!(!data.revertable);
    let texts: Vec<&str> = data.lines.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&"five") && texts.contains(&"5"), "{texts:?}");
    assert!(texts.len() < 10, "only the lines followed: {texts:?}");

    // The whole file's history replaces it when asked for.
    tab.apply(
        Action::FileView {
            path: PATH.to_owned(),
            rev: Some("HEAD".to_owned()),
            tab: super::file_view::FileTab::History,
        },
        &ctx,
    );
    assert!(tab.file_view.as_ref().is_some_and(|v| v.lines.is_none()));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_selection_in_a_commit_diff_asks_for_the_lines_of_that_commit() -> Result<(), String> {
    let dir = history_repo("line-history-pick")?;
    let hash = must(&dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let key = DiffKey::Commit {
        hash: hash.clone(),
        path: PATH.to_owned(),
    };
    let data = diff::load(&dir, &key, git::DiffOptions::default(), true)?;
    // The added "fifteen".
    let chosen: BTreeSet<usize> = (0..data.lines.len())
        .filter(|&i| data.lines[i].text == "fifteen")
        .collect();
    let picked = diff::picked_lines(&key, None, &data.lines, None, &chosen)?;
    assert_eq!(picked.at, LinesAt::Rev(hash));
    assert_eq!(picked.range, (16, 16));
    let commits = git::line_history(&dir, &picked.at, &picked.path, picked.range)?;
    assert_eq!(summaries(&commits), ["top and fifteen", "one"]);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
