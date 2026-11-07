//! Rewrites made in a tab keep both versions and offer to compare them; the comparison pairs
//! the commits and shows how a patch changed.

use std::path::{Path, PathBuf};

use super::VersionsAct;
use crate::git::{self, PairKind};
use crate::repo::{Action, RepoTab, rebase_action};

/// A folder removed when dropped.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map(|s| s.trim().to_owned())
}

/// Fifty numbered lines, some replaced (by numbers, so no hunk names a function).
fn lines(changes: &[(usize, &str)]) -> String {
    (1..=50)
        .map(|n| {
            changes
                .iter()
                .find(|(m, _)| *m == n)
                .map_or(n.to_string(), |(_, w)| (*w).to_owned())
                + "\n"
        })
        .collect()
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) -> Result<String, String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", message])?;
    git(dir, &["rev-parse", "HEAD"])
}

/// `main` with a commit `topic` doesn't have, `topic` (checked out) with two of its own.
fn repo(name: &str) -> Result<Scratch, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-versions-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let scratch = Scratch(dir);
    let dir = scratch.0.as_path();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(dir, args)?;
    }
    let base = commit(dir, "a.txt", &lines(&[]), "base")?;
    commit(dir, "m.txt", "main\n", "main moves")?;
    git(dir, &["checkout", "-q", "-b", "topic", &base])?;
    commit(dir, "a.txt", &lines(&[(5, "5555")]), "one")?;
    commit(dir, "a.txt", &lines(&[(5, "5555"), (25, "2525")]), "two")?;
    Ok(scratch)
}

/// Polls the tab until `done` says so (or a few seconds went by).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}

/// A tab on `dir` with its first read in.
fn tab(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_path_buf(), ctx);
    wait(&mut tab, ctx, |t| !t.loading && t.versions.rewrites_read);
    tab
}

#[test]
fn a_rebase_keeps_both_versions_and_offers_to_compare_them() -> Result<(), String> {
    let scratch = repo("rebase")?;
    let dir = scratch.0.as_path();
    let before = git(dir, &["rev-parse", "HEAD"])?;
    let ctx = egui::Context::default();
    let mut tab = tab(dir, &ctx);
    tab.apply(
        rebase_action(crate::repo::actions::BranchRef::local("main"), "topic"),
        &ctx,
    );
    assert!(wait(&mut tab, &ctx, |t| t.versions.fresh.is_some()));
    let after = git(dir, &["rev-parse", "HEAD"])?;
    let main = git(dir, &["rev-parse", "main"])?;
    let status = tab.status.clone();
    assert_eq!(
        status,
        Some(("Rebase topic onto main: done".to_owned(), false))
    );
    let kept = git::rewrites(dir)?;
    assert_eq!(kept.len(), 1);
    let rewrite = &kept[0];
    assert_eq!(rewrite.branch, "topic");
    assert_eq!(
        (rewrite.old.tip.as_str(), rewrite.new.tip.as_str()),
        (before.as_str(), after.as_str())
    );
    // The new version starts at what it went onto, not where the two met.
    assert_eq!(rewrite.new.base, main);
    assert_eq!(tab.versions.rewrites, kept);
    // The graph doesn't list the old commits.
    assert!(!tab.history.iter().any(|c| c.hash == before));

    // Compared: the same two commits, unchanged.
    tab.apply(
        Action::Versions(VersionsAct::Rewrite(rewrite.clone())),
        &ctx,
    );
    assert!(wait(&mut tab, &ctx, |t| t
        .versions
        .view
        .as_ref()
        .is_some_and(|v| v.data.is_some())));
    let view = tab.versions.view.as_ref().ok_or("no comparison")?;
    let data = view.data.clone().ok_or("not read")??;
    assert_eq!(data.counts().same, 2, "{data:#?}");
    assert!(data.counts().identical());
    tab.apply(Action::Versions(VersionsAct::Select(1)), &ctx);
    let view = tab.versions.view.as_ref().ok_or("no comparison")?;
    assert_eq!(view.selected, Some(1));
    assert!(matches!(&view.interdiff, Some(Ok(d)) if d.lines.is_empty()));
    tab.apply(Action::Versions(VersionsAct::Close), &ctx);
    assert!(tab.versions.view.is_none());

    // Fast-forwarding main to topic rewrites nothing: nothing more is kept.
    git(dir, &["checkout", "-q", "main"])?;
    tab.apply(
        crate::repo::fast_forward_action(crate::repo::actions::BranchRef::local("topic"), "main"),
        &ctx,
    );
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading));
    std::thread::sleep(std::time::Duration::from_millis(200));
    tab.poll(&ctx);
    assert_eq!(git::rewrites(dir)?.len(), 1);
    Ok(())
}

#[test]
fn a_rebase_continued_after_conflicts_is_kept_from_where_it_started() -> Result<(), String> {
    let scratch = repo("conflict")?;
    let dir = scratch.0.as_path();
    // main changes the line `two` changes too.
    git(dir, &["checkout", "-q", "main"])?;
    commit(dir, "a.txt", &lines(&[(25, "9999")]), "main changes 25")?;
    git(dir, &["checkout", "-q", "topic"])?;
    let before = git(dir, &["rev-parse", "HEAD"])?;
    let ctx = egui::Context::default();
    let mut tab = tab(dir, &ctx);
    tab.apply(
        rebase_action(crate::repo::actions::BranchRef::local("main"), "topic"),
        &ctx,
    );
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none()
        && !t.loading
        && t.work.in_progress.is_some()));
    assert!(tab.versions.pending.is_some());
    assert!(git::rewrites(dir)?.is_empty());

    // Resolved by hand, then continued from the tab.
    std::fs::write(dir.join("a.txt"), lines(&[(5, "5555"), (25, "2525")]))
        .map_err(|e| e.to_string())?;
    git(dir, &["add", "a.txt"])?;
    tab.apply(Action::Reload, &ctx);
    assert!(wait(&mut tab, &ctx, |t| !t.loading
        && t.work.conflicted.is_empty()));
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, |t| t.versions.fresh.is_some()));
    let kept = git::rewrites(dir)?;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].old.tip, before);
    assert_eq!(kept[0].new.tip, git(dir, &["rev-parse", "HEAD"])?);
    assert!(tab.versions.pending.is_none());

    // Compared: `one` is the same, `two` changed (its patch now goes over main's change).
    let rewrite = kept[0].clone();
    tab.apply(Action::Versions(VersionsAct::Rewrite(rewrite)), &ctx);
    assert!(wait(&mut tab, &ctx, |t| t
        .versions
        .view
        .as_ref()
        .is_some_and(|v| v.data.is_some())));
    let view = tab.versions.view.as_ref().ok_or("no comparison")?;
    let data = view.data.clone().ok_or("not read")??;
    let kinds: Vec<PairKind> = data.pairs.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PairKind::Same, PairKind::Changed], "{data:#?}");
    tab.apply(Action::Versions(VersionsAct::Select(1)), &ctx);
    let view = tab.versions.view.as_ref().ok_or("no comparison")?;
    let lines = match &view.interdiff {
        Some(Ok(d)) => d.lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
        other => {
            return Err(format!(
                "no interdiff: {:?}",
                other.as_ref().map(|r| r.is_ok())
            ));
        }
    };
    assert!(lines.iter().any(|l| l.contains("-9999")), "{lines:#?}");
    Ok(())
}

#[test]
fn an_added_commit_shows_its_whole_patch() -> Result<(), String> {
    let scratch = repo("added")?;
    let dir = scratch.0.as_path();
    let ctx = egui::Context::default();
    let mut tab = tab(dir, &ctx);
    let main = git(dir, &["rev-parse", "main"])?;
    let topic = git(dir, &["rev-parse", "topic"])?;
    let base = git(dir, &["merge-base", "main", "topic"])?;
    tab.open_versions(
        "test".to_owned(),
        ("old".to_owned(), "new".to_owned()),
        git::Series::new(&base, &main),
        git::Series::new(&base, &topic),
        &ctx,
    );
    assert!(wait(&mut tab, &ctx, |t| t
        .versions
        .view
        .as_ref()
        .is_some_and(|v| v.data.is_some())));
    let added = {
        let view = tab.versions.view.as_ref().ok_or("no comparison")?;
        let data = view.data.clone().ok_or("not read")??;
        data.pairs
            .iter()
            .position(|p| p.kind == PairKind::Added)
            .ok_or("nothing added")?
    };
    tab.apply(Action::Versions(VersionsAct::Select(added)), &ctx);
    assert!(wait(&mut tab, &ctx, |t| t
        .versions
        .view
        .as_ref()
        .is_some_and(|v| v.interdiff.is_some())));
    let view = tab.versions.view.as_ref().ok_or("no comparison")?;
    let Some(Ok(data)) = &view.interdiff else {
        return Err("no patch".to_owned());
    };
    assert!(
        data.lines
            .iter()
            .all(|l| l.kind == crate::repo::diff::Kind::Add)
    );
    Ok(())
}

#[test]
fn a_summary_counts_what_there_is() {
    let counts = git::Counts {
        same: 3,
        changed: 2,
        added: 1,
        dropped: 0,
        moved: 0,
    };
    assert_eq!(
        super::summary(&counts),
        "2 commits changed, 1 added, 3 identical"
    );
    let one = git::Counts {
        same: 0,
        changed: 0,
        added: 1,
        dropped: 2,
        moved: 1,
    };
    assert_eq!(super::summary(&one), "1 commit added, 2 dropped, 1 moved");
    assert_eq!(super::summary(&git::Counts::default()), "no commits");
}

/// A pull request compared is taken as seen once its comparison is shown, the very version
/// compared; one whose comparison fails stays as it was.
#[test]
fn a_pull_request_is_seen_only_once_its_comparison_shows() -> Result<(), String> {
    use super::Since;
    let scratch = repo("seen")?;
    let dir = scratch.0.as_path();
    let ctx = egui::Context::default();
    let mut tab = tab(dir, &ctx);
    tab.hosting = Some(crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".to_owned(),
        path: "acme/app".to_owned(),
        web: "https://github.com".to_owned(),
    });
    let key = tab
        .hosting
        .as_ref()
        .and_then(|h| h.review_repository_key())
        .ok_or("missing canonical review key")?;
    let main = git(dir, &["rev-parse", "main"])?;
    let topic = git(dir, &["rev-parse", "topic"])?;
    let base = git(dir, &["merge-base", "main", "topic"])?;
    let since = |tip: &str| Since {
        number: 4,
        old: git::Series::new(&base, &main),
        new: git::Series::new(&base, tip),
        counts: git::Counts::default(),
        rewritten: true,
    };

    // A version that can't be read: the comparison fails, nothing is taken as seen.
    tab.show_pr_since(since(&"f".repeat(40)), &ctx);
    assert!(wait(&mut tab, &ctx, |t| t
        .versions
        .view
        .as_ref()
        .is_some_and(|v| v.data.is_some())));
    std::thread::sleep(std::time::Duration::from_millis(100));
    tab.poll(&ctx);
    assert_eq!(git::reviewed(dir, &key, 4)?, None);

    tab.show_pr_since(since(&topic), &ctx);
    assert!(wait(&mut tab, &ctx, |_| git::reviewed(dir, &key, 4)
        .ok()
        .flatten()
        .is_some()));
    assert_eq!(
        git::reviewed(dir, &key, 4)?,
        Some(git::Series::new(&base, &topic))
    );
    assert_eq!(tab.versions.seen_heads.get(&4), Some(&topic));
    Ok(())
}
