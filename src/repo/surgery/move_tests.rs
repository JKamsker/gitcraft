//! Moving changes between commits on real temporary repositories, through the tab.

use super::moving::Dest;
use super::tests::{check, draw, finished, git_out, line, opened, pump, repo, sh, subjects};
use crate::repo::RepoTab;

/// Ten lines, then `A` changes the 2nd and the 9th ("zwei", "neun") on top of `B`, which adds
/// a file; `later` comes after.
const HISTORY: &str = "printf '1\\n2\\n3\\n4\\n5\\n6\\n7\\n8\\n9\\n10\\n' > 'f i.txt' && git add -A \
     && git commit -q -m base && echo b > b.txt && git add -A && git commit -q -m B \
     && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'f i.txt' \
     && git commit -q -am A -m 'A body' && echo later > later.txt && git add -A \
     && git commit -q -m later";

fn open_move(tab: &mut RepoTab, ctx: &egui::Context, rev: &str) {
    let hash = git_out(&tab.path, &["rev-parse", rev]);
    tab.open_move(hash, ctx);
    pump(tab, ctx, |t| t.split.is_some());
}

/// Moves the "neun" hunk (its two lines) of the open editor to `dest`.
fn move_neun(tab: &mut RepoTab, dest: Dest) {
    let (nine, neun) = (line(tab, 0, "9"), line(tab, 0, "neun"));
    if let Some(s) = tab.split.as_mut() {
        s.assign_line(0, nine, 1);
        s.assign_line(0, neun, 1);
        s.set_dest(dest);
    }
}

/// The target `summary` is in the list of the open editor.
fn target(tab: &RepoTab, summary: &str) -> Dest {
    let Some(super::split::Mode::Move(m)) = tab.split.as_ref().map(|s| s.mode.clone()) else {
        return Dest::Work;
    };
    Dest::Commit(
        m.targets
            .iter()
            .position(|t| t.summary == summary)
            .unwrap_or(usize::MAX),
    )
}

fn file_at(dir: &std::path::Path, rev: &str) -> String {
    git_out(dir, &["show", &format!("{rev}:f i.txt")])
}

#[test]
fn a_hunk_moves_into_an_older_commit_and_undo_brings_it_back() {
    let dir = repo("move-older", HISTORY);
    let (mut tab, ctx) = opened(&dir);
    let (before, tree) = (
        git_out(&dir, &["rev-parse", "HEAD"]),
        git_out(&dir, &["rev-parse", "HEAD^{tree}"]),
    );
    open_move(&mut tab, &ctx, "HEAD~1");
    let dest = target(&tab, "B");
    move_neun(&mut tab, dest);
    assert_eq!(draw(&mut tab, &ctx), 0);
    check(&mut tab, &ctx);
    assert_eq!(draw(&mut tab, &ctx), 0);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(subjects(&dir), ["later", "A", "B", "base", "root"]);
    // B has the 9th line now, A only the 2nd; together as before.
    assert_eq!(file_at(&dir, "HEAD~2"), "1\n2\n3\n4\n5\n6\n7\n8\nneun\n10");
    assert_eq!(
        file_at(&dir, "HEAD~1"),
        "1\nzwei\n3\n4\n5\n6\n7\n8\nneun\n10"
    );
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(
        git_out(&dir, &["log", "-1", "--format=%B", "HEAD~1"]),
        "A\n\nA body"
    );
    assert_eq!(git_out(&dir, &["log", "-1", "--format=%B", "HEAD~2"]), "B");
    tab.undo(&ctx);
    let undone = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.as_ref().is_some_and(|s| !s.1), "{undone:?}");
    assert_eq!(head, before);
}

#[test]
fn everything_of_a_commit_moved_into_a_newer_one_drops_it() {
    let dir = repo("move-newer", HISTORY);
    let (mut tab, ctx) = opened(&dir);
    let tree = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    open_move(&mut tab, &ctx, "HEAD~1");
    let dest = target(&tab, "later");
    if let Some(s) = tab.split.as_mut() {
        s.assign_file(0, 1);
        s.set_dest(dest);
    }
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let subjects = subjects(&dir);
    let top = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    let under = file_at(&dir, "HEAD~1");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(subjects, ["later", "B", "base", "root"]);
    assert_eq!(top, tree);
    assert_eq!(under, "1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
}

#[test]
fn a_hunk_moved_back_to_the_working_changes_is_staged_and_undo_takes_it_back() {
    let dir = repo("move-work", HISTORY);
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    open_move(&mut tab, &ctx, "HEAD~1");
    move_neun(&mut tab, Dest::Work);
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(file_at(&dir, "HEAD"), "1\nzwei\n3\n4\n5\n6\n7\n8\n9\n10");
    assert_eq!(
        git_out(&dir, &["diff", "--cached", "--name-only"]),
        "f i.txt"
    );
    assert_eq!(git_out(&dir, &["diff", "--name-only"]), "");
    assert!(std::fs::read_to_string(dir.join("f i.txt")).is_ok_and(|t| t.contains("neun")));
    tab.undo(&ctx);
    let undone = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let clean = git_out(&dir, &["status", "--porcelain"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.as_ref().is_some_and(|s| !s.1), "{undone:?}");
    assert_eq!(head, before);
    assert_eq!(clean, "");
}

#[test]
fn moving_to_working_changes_keeps_other_changes_to_the_file() {
    let dir = repo("move-work-dirty", HISTORY);
    let (mut tab, ctx) = opened(&dir);
    open_move(&mut tab, &ctx, "HEAD~1");
    move_neun(&mut tab, Dest::Work);
    check(&mut tab, &ctx);
    sh(&dir, "echo more >> 'f i.txt'");
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let text = std::fs::read_to_string(dir.join("f i.txt")).unwrap_or_default();
    let staged = git_out(&dir, &["diff", "--cached"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert!(text.contains("neun") && text.ends_with("more\n"), "{text}");
    assert!(staged.contains("+neun") && !staged.contains("more"));
}

/// Some lines of a file the commit adds, moved elsewhere: both parts are made one on the
/// other, so neither adds the file again (which conflicted).
#[test]
fn some_lines_of_a_new_file_move_into_an_older_commit_and_back() {
    let dir = repo(
        "move-new-file",
        "echo b > b.txt && git add -A && git commit -q -m B          && printf 'one\ntwo\nthree\n' > 'new f.txt' && git add -A && git commit -q -m A",
    );
    let (mut tab, ctx) = opened(&dir);
    let tree = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    open_move(&mut tab, &ctx, "HEAD");
    let dest = target(&tab, "B");
    let one = line(&tab, 0, "one");
    if let Some(s) = tab.split.as_mut() {
        s.assign_line(0, one, 1);
        s.set_dest(dest);
    }
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let in_b = git_out(&dir, &["show", "HEAD~1:new f.txt"]);
    let top = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(in_b, "one");
    assert_eq!(top, tree);
}

/// X adds a line, A removes it: moving that removal under X would leave the line in the end.
/// Played through first, the move is refused and nothing changes.
#[test]
fn a_move_a_commit_in_between_would_undo_is_refused() {
    let dir = repo(
        "move-undone",
        "printf 'keep\\n' > x.txt && git add -A && git commit -q -m base && echo b > b.txt \
         && git add -A && git commit -q -m B && printf 'keep\\nextra\\n' > x.txt \
         && git commit -q -am X && printf 'keep\\n' > x.txt && git commit -q -am A",
    );
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    open_move(&mut tab, &ctx, "HEAD");
    let dest = target(&tab, "B");
    if let Some(s) = tab.split.as_mut() {
        s.assign_file(0, 1);
        s.set_dest(dest);
    }
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    pump(&mut tab, &ctx, |t| {
        t.split.as_ref().is_some_and(|s| !s.busy)
    });
    let error = tab.split.as_ref().and_then(|s| s.error.clone());
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(error.is_some_and(|e| e.contains("undoes or redoes")));
    assert_eq!(head, before);
}

/// Everything of HEAD back to the working changes: a soft reset of it, in effect.
#[test]
fn all_of_the_latest_commit_moved_to_the_working_changes() {
    let dir = repo("move-all-head", HISTORY);
    let (mut tab, ctx) = opened(&dir);
    let parent = git_out(&dir, &["rev-parse", "HEAD~1"]);
    open_move(&mut tab, &ctx, "HEAD");
    if let Some(s) = tab.split.as_mut() {
        s.assign_file(0, 1);
        s.set_dest(Dest::Work);
    }
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(head, parent);
    assert_eq!(staged, "later.txt");
}

/// Moved to the working changes, the rebase stopping on a conflict on the way: continued, the
/// moved changes are a commit on top, not lost.
#[test]
fn a_conflict_on_the_way_to_the_working_changes_loses_nothing() {
    let dir = repo(
        "move-work-conflict",
        &format!(
            "{HISTORY} && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\nzehn\\n' > 'f i.txt' \
             && git commit -q -am ten"
        ),
    );
    let (mut tab, ctx) = opened(&dir);
    open_move(&mut tab, &ctx, "HEAD~2");
    move_neun(&mut tab, Dest::Work);
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let stopped = crate::git::status(&dir).map(|w| w.in_progress);
    // Resolved as the old content, and continued in the terminal.
    sh(
        &dir,
        "for i in 1 2 3; do printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\nzehn\\n' > 'f i.txt' \
         && git add -A && GIT_EDITOR=true git rebase --continue && break; done; \
         test ! -d .git/rebase-merge",
    );
    let top = git_out(&dir, &["show", "HEAD:f i.txt"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| s.1), "{status:?}");
    assert_eq!(stopped, Ok(Some(crate::git::InProgress::Rebase)));
    assert!(top.contains("neun"), "{top}");
}

#[test]
fn a_move_that_conflicts_stops_in_the_rebase_and_abort_goes_back() {
    // "later" changes the 9th line again: moving A's "neun" under it into B conflicts with
    // nothing, but moving it to the top after "later" changed it does.
    let dir = repo(
        "move-conflict",
        &format!(
            "{HISTORY} && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nnine!\\n10\\n' > 'f i.txt' \
             && git commit -q -am retouch"
        ),
    );
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    open_move(&mut tab, &ctx, "HEAD~2");
    let dest = target(&tab, "retouch");
    move_neun(&mut tab, dest);
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let state = crate::git::status(&dir).map(|w| w.in_progress);
    sh(&dir, "git rebase --abort");
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| s.1), "{status:?}");
    assert_eq!(state, Ok(Some(crate::git::InProgress::Rebase)));
    assert_eq!(head, before);
}
