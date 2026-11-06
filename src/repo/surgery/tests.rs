//! History surgery on real temporary repositories, through the tab as the window drives it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::git;
use crate::repo::RepoTab;

pub(super) fn sh(dir: &Path, script: &str) {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output();
    assert!(
        out.as_ref().is_ok_and(|o| o.status.success()),
        "{script}: {out:?}"
    );
}

/// A repository with `script` run in it, after a first commit.
pub(super) fn repo(name: &str, script: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-surgery-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    sh(
        &dir,
        "git init -q -b main && git config user.name 'Zoë' && git config user.email z@t \
         && git config commit.gpgSign false && git commit -q --allow-empty -m root",
    );
    sh(&dir, script);
    dir
}

/// Handles the tab's messages until `done` says so (or half a minute passes).
pub(super) fn pump(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !done(tab) && Instant::now() < until {
        if let Ok(msg) = tab.rx.recv_timeout(Duration::from_millis(200)) {
            tab.handle(msg, ctx);
        }
    }
    assert!(done(tab), "the tab never got there: {:?}", tab.status);
}

/// The tab open on `dir`, its first read in.
pub(super) fn opened(dir: &Path) -> (RepoTab, egui::Context) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.to_path_buf(), &ctx);
    pump(&mut tab, &ctx, |t| !t.commits.is_empty() && t.settled());
    (tab, ctx)
}

/// Waits for the operation running to end, and says how it did.
pub(super) fn finished(tab: &mut RepoTab, ctx: &egui::Context) -> Option<(String, bool)> {
    pump(tab, ctx, |t| t.busy_op.is_some());
    pump(tab, ctx, |t| t.busy_op.is_none());
    pump(tab, ctx, RepoTab::settled);
    tab.status.clone()
}

pub(super) fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn subjects(dir: &Path) -> Vec<String> {
    git_out(dir, &["log", "--format=%s", "HEAD"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// What the split editor gives each file and line of commit `rev` once it is open on it.
fn open(tab: &mut RepoTab, ctx: &egui::Context, rev: &str) {
    let hash = git_out(&tab.path, &["rev-parse", rev]);
    tab.open_split(hash, ctx);
    pump(tab, ctx, |t| t.split.is_some());
}

fn check(tab: &mut RepoTab, ctx: &egui::Context) {
    tab.check_split(ctx);
    pump(tab, ctx, |t| t.split.as_ref().is_some_and(|s| !s.busy));
    let error = tab.split.as_ref().and_then(|s| s.error.clone());
    assert!(error.is_none(), "{error:?}");
    assert!(tab.split.as_ref().is_some_and(|s| s.checked.is_some()));
}

/// Draws the split editor one frame, as the window does, and what it asked for.
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
    let mut out = ctx.run_ui(input, |ui| tab.split_view(ui, &mut acts));
    out.textures_delta.clear();
    acts.len()
}

/// The index of the line of file `f` in the editor whose text is `text`.
fn line(tab: &RepoTab, f: usize, text: &str) -> usize {
    tab.split
        .as_ref()
        .and_then(|s| s.files.get(f))
        .and_then(|file| file.lines.iter().position(|l| l.text == text))
        .unwrap_or(usize::MAX)
}

#[test]
fn a_split_commit_adds_up_to_the_old_one_and_undo_brings_it_back() {
    let dir = repo(
        "split-two",
        "printf '1\\n2\\n3\\n4\\n5\\n6\\n7\\n8\\n9\\n10\\n' > 'a file.txt' && git add -A \
         && git commit -q -m base \
         && printf '1\\nzwei\\n3\\n4\\n5\\n6\\n7\\n8\\nneun\\n10\\n' > 'a file.txt' \
         && printf 'héllo wörld\\n' > 'ünï cödé.txt' && git add -A \
         && git commit -q -m 'two things' -m 'the body' \
         && echo later > later.txt && git add -A && git commit -q -m later",
    );
    let (mut tab, ctx) = opened(&dir);
    let before = git_out(&dir, &["rev-parse", "HEAD"]);
    let tree = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    let old_tree = git_out(&dir, &["rev-parse", "HEAD~1^{tree}"]);
    open(&mut tab, &ctx, "HEAD~1");
    // "zwei" (and its removed line) into the first commit; "neun" and the new file stay in
    // the second.
    let (zwei, two) = (line(&tab, 0, "zwei"), line(&tab, 0, "2"));
    if let Some(s) = tab.split.as_mut() {
        s.assign_line(0, zwei, 0);
        s.assign_line(0, two, 0);
        s.groups[0].subject = "zwei".to_owned();
        s.groups[1].subject = "neun und ünïcode".to_owned();
    }
    assert_eq!(draw(&mut tab, &ctx), 0);
    check(&mut tab, &ctx);
    assert_eq!(draw(&mut tab, &ctx), 0);
    let first_patch = tab
        .split
        .as_ref()
        .and_then(|s| s.checked.as_ref())
        .map(|c| c.patches[0].0.clone())
        .unwrap_or_default();
    assert!(first_patch.contains("+zwei") && !first_patch.contains("neun"));
    // Nothing on disk changed by checking.
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), before);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(
        subjects(&dir),
        ["later", "neun und ünïcode", "zwei", "base", "root"]
    );
    // Together the same as before: the tree at the top and after the second commit.
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD~1^{tree}"]), old_tree);
    assert_eq!(
        git_out(&dir, &["show", "HEAD~2:a file.txt"]),
        "1\nzwei\n3\n4\n5\n6\n7\n8\n9\n10"
    );
    assert_eq!(
        git_out(&dir, &["log", "-1", "--format=%an", "HEAD~2"]),
        "Zoë"
    );
    assert_eq!(git_out(&dir, &["status", "--porcelain"]), "");

    tab.undo(&ctx);
    let undone = finished(&mut tab, &ctx);
    let after_undo = git_out(&dir, &["rev-parse", "HEAD"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.as_ref().is_some_and(|s| !s.1), "{undone:?}");
    assert_eq!(after_undo, before);
}

#[test]
fn three_ways_with_a_binary_a_deleted_and_a_new_file() {
    let dir = repo(
        "split-three",
        "printf 'x\\n' > gone.txt && printf '\\000\\001' > pic.bin && git add -A \
         && git commit -q -m base \
         && git rm -q gone.txt && printf '\\000\\002\\003' > pic.bin \
         && printf 'one\\ntwo\\n' > 'new one.txt' && git add -A && git commit -q -m mixed",
    );
    let (mut tab, ctx) = opened(&dir);
    let tree = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    open(&mut tab, &ctx, "HEAD");
    if let Some(s) = tab.split.as_mut() {
        let files: Vec<&str> = s.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["gone.txt", "new one.txt", "pic.bin"]);
        assert_eq!(s.files[0].whole, Some("deleted"));
        assert_eq!(s.files[2].whole, Some("a binary file"));
        s.add_group();
        s.assign_file(0, 0);
        // Only the first line of the new file in the second commit.
        let one = s.files[1]
            .lines
            .iter()
            .position(|l| l.text == "one")
            .unwrap_or(0);
        let two = s.files[1]
            .lines
            .iter()
            .position(|l| l.text == "two")
            .unwrap_or(0);
        s.assign_line(1, one, 1);
        s.assign_line(1, two, 2);
        s.assign_file(2, 2);
        for (g, name) in ["delete", "start new", "rest"].iter().enumerate() {
            s.groups[g].subject = (*name).to_owned();
        }
        assert_eq!(s.problem(), None);
    }
    check(&mut tab, &ctx);
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let new_one = git_out(&dir, &["show", "HEAD~1:new one.txt"]);
    let gone = git_out(&dir, &["ls-tree", "--name-only", "HEAD~2"]);
    let top = git_out(&dir, &["rev-parse", "HEAD^{tree}"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(new_one, "one");
    assert_eq!(gone, "pic.bin");
    assert_eq!(top, tree);
}

#[test]
fn a_commit_with_merges_after_it_is_refused() {
    let dir = repo(
        "split-merge",
        "echo a > a && git add -A && git commit -q -m a && echo b > b && git add -A \
         && git commit -q -m b && git checkout -q -b side HEAD~1 && echo c > c && git add -A \
         && git commit -q -m c && git checkout -q main && git merge -q --no-edit side",
    );
    let (mut tab, ctx) = opened(&dir);
    let hash = git_out(&dir, &["rev-parse", "HEAD~1"]);
    let refused = tab.surgery_refusal(&hash);
    tab.open_split(hash, &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.contains("Merges")));
    assert!(tab.split.is_none());
    assert!(tab.status.as_ref().is_some_and(|s| s.1));
}

#[test]
fn a_commit_made_after_checking_is_kept() {
    let dir = repo(
        "split-moved",
        "printf 'a\\nb\\n' > f && git add -A && git commit -q -m base \
         && printf 'A\\nB\\n' > f && git commit -q -am both",
    );
    let (mut tab, ctx) = opened(&dir);
    open(&mut tab, &ctx, "HEAD");
    let a = line(&tab, 0, "A");
    let old_a = line(&tab, 0, "a");
    if let Some(s) = tab.split.as_mut() {
        s.assign_line(0, a, 0);
        s.assign_line(0, old_a, 0);
        s.groups[0].subject = "A".to_owned();
        s.groups[1].subject = "B".to_owned();
    }
    check(&mut tab, &ctx);
    // Made in the terminal meanwhile: replayed on top, not dropped.
    sh(&dir, "echo new > n && git add n && git commit -q -m newer");
    tab.run_split(&ctx);
    let status = finished(&mut tab, &ctx);
    let subjects = subjects(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(status.as_ref().is_some_and(|s| !s.1), "{status:?}");
    assert_eq!(subjects, ["newer", "B", "A", "base", "root"]);
}
