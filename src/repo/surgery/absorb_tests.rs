//! Distributing staged fixes on real temporary repositories, through the tab.

use super::absorb::Match;
use super::tests::{finished, git_out, opened, pump, repo, sh, subjects};
use crate::repo::RepoTab;

/// `main` has f and g (ten lines each); the branch `feat` has A (f's 2nd line) and B (g's
/// 5th line and f's 9th).
const HISTORY: &str = "printf '1\\n2\\n3\\n4\\n5\\n6\\n7\\n8\\n9\\n10\\n' > 'f é.txt' \
     && printf 'a\\nb\\nc\\nd\\ne\\nf\\ng\\nh\\ni\\nj\\n' > g.txt && git add -A \
     && git commit -q -m base && git checkout -q -b feat \
     && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\n9\\n10\\n' > 'f é.txt' && git commit -q -am A \
     && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'f é.txt' \
     && printf 'a\\nb\\nc\\nd\\nE\\nf\\ng\\nh\\ni\\nj\\n' > g.txt && git commit -q -am B";

fn open(tab: &mut RepoTab, ctx: &egui::Context) {
    tab.open_absorb(ctx);
    pump(tab, ctx, |t| {
        t.absorb.is_some() || t.status.as_ref().is_some_and(|s| s.1)
    });
    assert!(tab.absorb.is_some(), "{:?}", tab.status);
}

fn check(tab: &mut RepoTab, ctx: &egui::Context) {
    tab.check_absorb(ctx);
    pump(tab, ctx, |t| t.absorb.as_ref().is_some_and(|a| !a.busy));
    let error = tab.absorb.as_ref().and_then(|a| a.error.clone());
    assert!(error.is_none(), "{error:?}");
}

/// What each hunk matched, by the summary of the commit.
fn found(tab: &RepoTab) -> Vec<String> {
    let Some(a) = tab.absorb.as_ref() else {
        return Vec::new();
    };
    let name = |i: usize| {
        a.targets
            .get(i)
            .map(|t| t.summary.clone())
            .unwrap_or_default()
    };
    a.hunks
        .iter()
        .map(|h| match h.found {
            Match::Clear(i) => format!("clear {}", name(i)),
            Match::Several(i) => format!("several {}", name(i)),
            Match::None => "none".to_owned(),
        })
        .collect()
}

/// Draws the absorb editor one frame, as the window does; how many actions it asked for.
fn draw(tab: &mut RepoTab, ctx: &egui::Context) -> usize {
    crate::theme::install_fonts(ctx);
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| tab.absorb_view(ui, &mut acts));
    out.textures_delta.clear();
    acts.len()
}

fn show(dir: &std::path::Path, spec: &str) -> String {
    git_out(dir, &["show", spec])
}

#[test]
fn staged_fixes_go_to_the_commit_that_last_touched_their_lines_and_undo_puts_them_back() {
    let dir = repo("absorb-clear", HISTORY);
    // Fixes to A's line and B's line, a new file, and an unstaged change elsewhere.
    sh(
        &dir,
        "printf '1\\nZWEI\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'f é.txt' \
         && printf 'a\\nb\\nc\\nd\\nEE\\nf\\ng\\nh\\ni\\nj\\n' > g.txt && echo new > n.txt \
         && git add -A && printf 'a\\nb\\nc\\nd\\nEE\\nf\\ng\\nh\\ni\\nJ\\n' > g.txt",
    );
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    let staged_before = git_out(&dir, &["diff", "--cached"]);
    open(&mut tab, &ctx);
    assert_eq!(found(&tab), ["clear A", "clear B"]);
    assert_eq!(draw(&mut tab, &ctx), 0);
    check(&mut tab, &ctx);
    assert_eq!(draw(&mut tab, &ctx), 0);
    tab.run_absorb(&ctx);
    let status = finished(&mut tab, &ctx);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(subjects(&dir), ["B", "A", "base", "root"]);
    assert!(show(&dir, "HEAD~1:f é.txt").contains("ZWEI"));
    assert_eq!(show(&dir, "HEAD:g.txt").lines().nth(4), Some("EE"));
    // The new file stays staged, the unstaged change unstaged.
    assert_eq!(git_out(&dir, &["diff", "--cached", "--name-only"]), "n.txt");
    assert_eq!(git_out(&dir, &["diff", "--name-only"]), "g.txt");
    assert!(git_out(&dir, &["stash", "list"]).is_empty());

    tab.undo(&ctx);
    let undone = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let staged = git_out(&dir, &["diff", "--cached"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.as_ref().is_some_and(|s| !s.1), "{undone:?}");
    assert_eq!(head, before);
    assert_eq!(staged, staged_before);
}

#[test]
fn lines_of_several_commits_are_suggested_and_older_ones_stay_staged() {
    let dir = repo("absorb-several", HISTORY);
    // Lines 2 to 9 replaced at once (A's, base's and B's), and base's line 4 of g alone.
    sh(
        &dir,
        "printf '1\\nall\\n10\\n' > 'f é.txt' \
         && printf 'a\\nb\\nc\\nD\\nE\\nf\\ng\\nh\\ni\\nj\\n' > g.txt && git add -A",
    );
    let (mut tab, ctx) = opened(&dir);
    open(&mut tab, &ctx);
    let hunks = found(&tab);
    // Leaving nothing chosen: a problem, nothing to check.
    let problem = tab.absorb.as_ref().and_then(|a| a.problem());
    // Take the suggestion for the first.
    let suggested = tab.absorb.as_ref().map(|a| match a.hunks[0].found {
        Match::Several(i) => Some(i),
        _ => None,
    });
    if let (Some(a), Some(s)) = (tab.absorb.as_mut(), suggested.flatten()) {
        a.choose(0, Some(s));
    }
    check(&mut tab, &ctx);
    tab.run_absorb(&ctx);
    let status = finished(&mut tab, &ctx);
    let top = show(&dir, "HEAD:f é.txt");
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(hunks, ["several B", "none"]);
    assert!(problem.is_some_and(|p| p.contains("Pick")));
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(top, "1\nall\n10");
    assert_eq!(staged, "g.txt");
}

#[test]
fn a_change_staged_after_checking_stops_it_and_changes_nothing() {
    let dir = repo("absorb-moved", HISTORY);
    sh(
        &dir,
        "printf '1\\nZWEI\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'f é.txt' && git add -A",
    );
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    open(&mut tab, &ctx);
    check(&mut tab, &ctx);
    sh(&dir, "echo more > m.txt && git add m.txt");
    tab.run_absorb(&ctx);
    let status = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let staged = git_out(
        &dir,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--cached",
            "--name-only",
        ],
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| s.1), "{status:?}");
    assert_eq!(head, before);
    assert_eq!(staged, "f é.txt\nm.txt");
}

#[test]
fn a_branch_without_a_remote_or_other_branch_is_all_its_own_but_its_first_commit() {
    let dir = repo(
        "absorb-alone",
        "echo x > a && git add -A && git commit -q -m base",
    );
    sh(&dir, "echo y > a && git add -A");
    let (mut tab, ctx) = opened(&dir);
    open(&mut tab, &ctx);
    let targets: Vec<String> = tab
        .absorb
        .as_ref()
        .map(|a| a.targets.iter().map(|t| t.summary.clone()).collect())
        .unwrap_or_default();
    let hunks = found(&tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(targets, ["base"]);
    assert_eq!(hunks, ["clear base"]);
}

/// With `git absorb` there (here a stand-in: an alias committing what is staged), it is
/// offered and runs as one undoable step.
#[test]
fn an_installed_git_absorb_is_offered_and_undone_as_one_step() {
    let dir = repo(
        "absorb-tool",
        "echo x > a && git add -A && git commit -q -m base \
         && git config alias.absorb '!f() { test \"$1\" = --version && exit 0; git commit -q -m absorbed; }; f'",
    );
    sh(&dir, "echo y > a && git add -A");
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    open(&mut tab, &ctx);
    let offered = tab.absorb.as_ref().is_some_and(|a| a.git_absorb);
    tab.run_git_absorb(&ctx);
    let status = finished(&mut tab, &ctx);
    let top = git_out(&dir, &["log", "-1", "--format=%s"]);
    tab.undo(&ctx);
    let undone = finished(&mut tab, &ctx);
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(offered);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(top, "absorbed");
    assert!(undone.as_ref().is_some_and(|s| !s.1), "{undone:?}");
    assert_eq!(head, before);
    assert_eq!(staged, "a");
}

/// Commits the upstream has are offered, marked pushed; nothing staged is said before reading.
#[test]
fn pushed_commits_are_marked_and_nothing_staged_is_said() {
    let dir = repo("absorb-pushed", HISTORY);
    let remote = dir.with_extension("remote");
    let _ = std::fs::remove_dir_all(&remote);
    sh(
        &dir,
        &format!(
            "git init -q --bare '{r}' && git remote add origin '{r}' \
             && git push -q -u origin HEAD~1:refs/heads/feat && git branch -q -u origin/feat",
            r = remote.display()
        ),
    );
    let (mut tab, ctx) = opened(&dir);
    tab.open_absorb(&ctx);
    let nothing = tab.status.clone();
    sh(
        &dir,
        "printf '1\\nZWEI\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'f é.txt' && git add -A",
    );
    tab.work = crate::git::status(&dir).unwrap_or_default();
    open(&mut tab, &ctx);
    let targets: Vec<(String, bool)> = tab
        .absorb
        .as_ref()
        .map(|a| {
            a.targets
                .iter()
                .map(|t| (t.summary.clone(), t.pushed))
                .collect()
        })
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
    assert!(nothing.is_some_and(|s| s.1 && s.0.contains("nothing is staged")));
    assert_eq!(targets, [("B".to_owned(), false), ("A".to_owned(), true)]);
}
