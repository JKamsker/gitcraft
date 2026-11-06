//! What merges resolved, through the tab as the window drives it: a real repository with a
//! resolved conflict, an extra edit, a clean merge and an octopus merge.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable as _;

use super::badge::RowBadge;
use super::panel::Mode;
use crate::git::{self, DiffOptions, Remerge, RemergeFile};
use crate::repo::{Action, DiffKey, RepoTab, diff};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default()
}

fn commit(dir: &Path, files: &[(&str, &str)], message: &str) -> String {
    for (path, text) in files {
        let written = std::fs::write(dir.join(path), text);
        assert!(written.is_ok(), "writing {path}: {written:?}");
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
    git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}

/// The merges of the repository made by [`merges_repo`].
struct Merges {
    dir: PathBuf,
    /// A conflict in a file with spaces and accents in its name resolved, and another file
    /// edited beyond the merge.
    resolved: String,
    /// Another file edited beyond a merge that had no conflict.
    evil: String,
    clean: String,
    octopus: String,
}

const CONFLICTED: &str = "dossier é/ünï côdé.txt";

fn merges_repo(name: &str) -> Merges {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-tab-remerge-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("dossier é"));
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args);
    }
    commit(
        &dir,
        &[(CONFLICTED, "a\nb\nc\n"), ("other.txt", "x\n")],
        "base",
    );
    let branch = |dir: &Path, name: &str, file: &str, text: &str| {
        git(dir, &["checkout", "-q", "-b", name, "main"]);
        commit(dir, &[(file, text)], name);
        git(dir, &["checkout", "-q", "main"]);
    };
    let merge = |dir: &Path, branches: &[&str]| {
        let mut args = vec!["merge", "-q", "--no-ff", "--no-edit"];
        args.extend(branches);
        let _ = git::run(Some(dir), &args);
    };

    branch(&dir, "side", CONFLICTED, "a\nSIDE\nc\n");
    commit(&dir, &[(CONFLICTED, "a\nMAIN\nc\n")], "main");
    merge(&dir, &["side"]);
    let resolved = commit(
        &dir,
        &[(CONFLICTED, "a\nRESOLVED\nc\n"), ("other.txt", "x\nalso\n")],
        "resolved",
    );

    branch(&dir, "quiet", "quiet.txt", "q\n");
    merge(&dir, &["quiet"]);
    // Edited into the merge itself.
    let _ = std::fs::write(dir.join("other.txt"), "x\nalso\nevil\n");
    git(&dir, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let evil = git(&dir, &["rev-parse", "HEAD"]).trim().to_owned();

    branch(&dir, "plain", "plain.txt", "p\n");
    merge(&dir, &["plain"]);
    let clean = git(&dir, &["rev-parse", "HEAD"]).trim().to_owned();

    branch(&dir, "o1", "o1.txt", "1\n");
    branch(&dir, "o2", "o2.txt", "2\n");
    merge(&dir, &["o1", "o2"]);
    let octopus = git(&dir, &["rev-parse", "HEAD"]).trim().to_owned();
    Merges {
        dir,
        resolved,
        evil,
        clean,
        octopus,
    }
}

type Harness<'a> = egui_kittest::Harness<'a, RepoTab>;

/// Frames until `done` says so, or a while.
fn run_until(harness: &mut Harness<'_>, done: impl Fn(&RepoTab) -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        harness.step();
        if done(harness.state()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn row_of(tab: &RepoTab, hash: &str) -> usize {
    tab.commits
        .iter()
        .position(|c| c.hash == hash)
        .unwrap_or(usize::MAX)
}

fn select(harness: &mut Harness<'_>, hash: &str) {
    let row = row_of(harness.state(), hash);
    harness.state_mut().queue(Action::Select(row));
    let hash = hash.to_owned();
    assert!(run_until(harness, |t| t
        .detail
        .as_ref()
        .is_some_and(|d| d.hash == hash)));
}

#[test]
fn the_window_shows_what_merges_resolved() {
    let m = merges_repo("window");
    if git::remerge_support(&m.dir).is_err() {
        let _ = std::fs::remove_dir_all(&m.dir);
        return;
    }
    let ctx = egui::Context::default();
    let tab = RepoTab::new_deferred(m.dir.clone(), &ctx);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1400.0, 860.0])
        .build_ui_state(|ui, tab: &mut RepoTab| tab.ui(ui), tab);
    crate::theme::install_fonts(&harness.ctx);
    crate::theme::apply(&harness.ctx, crate::theme::ThemeKind::Dark);
    assert!(run_until(&mut harness, |t| !t.opening()
        && t.settled()
        && !t.commits.is_empty()));

    // The badges of the merges shown, read in the background.
    let read = |t: &RepoTab| {
        [&m.resolved, &m.evil, &m.clean]
            .iter()
            .all(|h| t.remerges.of(h).is_some())
    };
    let ok = run_until(&mut harness, read);
    let t = harness.state();
    assert!(
        ok,
        "the badges were never read: support {:?}, found {:?}, asked {:?}, running {}, merges {:?}",
        t.remerges.support,
        t.remerges.found.keys().collect::<Vec<_>>(),
        t.remerges.asked,
        t.remerges.batch_running,
        t.commits
            .iter()
            .filter(|c| c.parents.len() == 2)
            .map(|c| &c.hash)
            .collect::<Vec<_>>()
    );
    let tab = harness.state();
    let badge = |hash: &str| {
        tab.commits
            .get(row_of(tab, hash))
            .and_then(|c| tab.remerge_badge(c))
    };
    assert_eq!(
        badge(&m.resolved),
        Some(RowBadge {
            conflicts: 1,
            edits: 1
        })
    );
    assert_eq!(
        badge(&m.evil),
        Some(RowBadge {
            conflicts: 0,
            edits: 1
        })
    );
    assert_eq!(badge(&m.clean), None);
    // Never redone: git can't.
    assert_eq!(badge(&m.octopus), None);
    assert!(tab.remerges.of(&m.octopus).is_none());
    // Read out with what they mean.
    for label in ["resolved", "resolved + edited", "edited"] {
        eprintln!("DBG {label}: {}", harness.query_by_label(label).is_some());
    }
    for n in harness.query_all_by_label_contains("differs") {
        eprintln!("DBG node: {:?}", n);
    }
    assert!(
        harness
            .query_by_label_contains("conflicts resolved in 1 file")
            .is_some()
    );
    // The resolved merge's and the evil one's.
    assert_eq!(
        harness
            .query_all_by_label_contains("1 file changed beyond")
            .count(),
        2
    );

    // The resolved merge: the box, ticked, lists the conflict and the extra edit apart.
    select(&mut harness, &m.resolved);
    harness.run_steps(2);
    assert!(harness.query_by_label("What this merge resolved").is_some());
    let tab = harness.state();
    assert_eq!(
        tab.detail.as_ref().map(|d| tab.remerge_mode(d)),
        Some(Mode::Off)
    );
    harness.state_mut().queue(Action::ShowRemerge(true));
    harness.run_steps(3);
    assert!(harness.query_by_label("CONFLICTS RESOLVED · 1").is_some());
    assert!(
        harness
            .query_by_label("EDITED BEYOND THE MERGE · 1")
            .is_some()
    );
    assert!(harness.query_by_label("View all files").is_none());

    // Its conflicted file's diff, as the list opens it.
    let key = DiffKey::Remerge {
        hash: m.resolved.clone(),
        path: CONFLICTED.to_owned(),
        old_path: None,
        conflicts: vec![format!(
            "CONFLICT (content): Merge conflict in {CONFLICTED}"
        )],
    };
    harness.state_mut().queue(Action::OpenDiff(key));
    assert!(run_until(&mut harness, |t| t
        .diff
        .as_ref()
        .is_some_and(|d| d.data.is_some())));
    let data = harness.state().diff.as_ref().and_then(|d| d.data.as_ref());
    assert!(
        matches!(data, Some(Ok(_))),
        "no diff: {:?}",
        data.map(|r| r.as_ref().err())
    );
    let lines: Vec<String> = match data {
        Some(Ok(data)) => data.lines.iter().map(|l| l.text.clone()).collect(),
        _ => Vec::new(),
    };
    assert!(lines[0].contains("CONFLICT (content)"), "{lines:?}");
    assert!(
        lines.iter().any(|l| l.ends_with("resolves a conflict")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l == "RESOLVED"), "{lines:?}");

    // The clean merge, the box still ticked: said to be clean.
    select(&mut harness, &m.clean);
    assert!(run_until(&mut harness, |t| t
        .remerges
        .of(&m.clean)
        .is_some()));
    harness.run_steps(2);
    assert!(harness.query_by_label("A clean merge").is_some());

    // The octopus merge: its files, and why the box can't be ticked.
    select(&mut harness, &m.octopus);
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label_contains("An octopus merge (3 parents)")
            .is_some()
    );
    let tab = harness.state();
    assert!(matches!(
        tab.detail.as_ref().map(|d| tab.remerge_mode(d)),
        Some(Mode::Unavailable(_))
    ));
    // Its files as for any merge, against the first parent.
    assert!(harness.query_by_label("View all files").is_some());

    // An ordinary commit has no box.
    let plain = git(&m.dir, &["rev-parse", "main~1^2"]).trim().to_owned();
    select(&mut harness, &plain);
    harness.run_steps(2);
    assert!(harness.query_by_label("What this merge resolved").is_none());
    let _ = std::fs::remove_dir_all(&m.dir);
}

#[test]
fn the_extra_edit_of_a_merge_is_labelled_as_such() {
    let m = merges_repo("evil-diff");
    if git::remerge_support(&m.dir).is_err() {
        let _ = std::fs::remove_dir_all(&m.dir);
        return;
    }
    let key = DiffKey::Remerge {
        hash: m.evil.clone(),
        path: "other.txt".to_owned(),
        old_path: None,
        conflicts: Vec::new(),
    };
    let data = diff::load(&m.dir, &key, DiffOptions::default(), true);
    let found = git::remerge(&m.dir, &m.evil);
    let _ = std::fs::remove_dir_all(&m.dir);
    assert!(data.is_ok(), "{:?}", data.as_ref().err());
    let Ok(data) = data else { return };
    let texts: Vec<&str> = data.lines.iter().map(|l| l.text.as_str()).collect();
    assert!(
        texts.iter().any(|l| l.ends_with("edited beyond the merge")),
        "{texts:?}"
    );
    assert!(texts.contains(&"evil"), "{texts:?}");
    assert!(!data.partial, "a remerge diff is not staged from");
    let found = found.unwrap_or_default();
    assert_eq!(
        found,
        Remerge {
            files: vec![RemergeFile {
                status: 'M',
                path: "other.txt".to_owned(),
                added: 1,
                edited_hunks: 1,
                ..Default::default()
            }]
        }
    );
}

#[test]
fn badges_are_read_a_few_at_a_time() {
    let mut remerges = super::Remerges {
        support: Some(Ok(())),
        ..Default::default()
    };
    let shown: Vec<String> = (0..10).map(|n| format!("h{n}")).collect();
    let first = remerges.next_batch(&shown);
    assert_eq!(first.len(), super::BADGE_BATCH);
    // Nothing more while that batch runs.
    assert!(remerges.next_batch(&shown).is_empty());
    for hash in first {
        remerges.arrived(hash, Ok(Remerge::default()));
    }
    remerges.batch_running = false;
    let second = remerges.next_batch(&shown);
    assert_eq!(second, ["h4", "h5", "h6", "h7"]);
    // A git that can't: nothing is asked.
    let mut old = super::Remerges {
        support: Some(Err("old".to_owned())),
        ..Default::default()
    };
    assert!(old.next_batch(&shown).is_empty());
    assert_eq!(old.unsupported(), Some("old"));
}

#[test]
fn failures_are_tried_again_and_drivers_stop_the_badges() {
    let mut remerges = super::Remerges {
        support: Some(Err("could not reach the machine".to_owned())),
        ..Default::default()
    };
    remerges.arrived("bad".to_owned(), Err("connection lost".to_owned()));
    remerges.arrived("good".to_owned(), Ok(Remerge::default()));
    remerges.forget_failures();
    assert!(remerges.support.is_none());
    assert!(remerges.of("bad").is_none());
    assert!(remerges.of("good").is_some());
    // Merge drivers of the repository's own: nothing redone unasked.
    remerges.support = Some(Ok(()));
    remerges.drivers = true;
    assert!(remerges.next_batch(&["h".to_owned()]).is_empty());
}

#[test]
fn unticking_the_box_closes_a_diff_of_what_the_merge_resolved() {
    let m = merges_repo("untick");
    let ctx = egui::Context::default();
    let tab = RepoTab::new_deferred(m.dir.clone(), &ctx);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1400.0, 860.0])
        .build_ui_state(|ui, tab: &mut RepoTab| tab.ui(ui), tab);
    crate::theme::install_fonts(&harness.ctx);
    crate::theme::apply(&harness.ctx, crate::theme::ThemeKind::Dark);
    let opened = run_until(&mut harness, |t| !t.opening() && t.settled());
    let key = DiffKey::Remerge {
        hash: m.resolved.clone(),
        path: CONFLICTED.to_owned(),
        old_path: None,
        conflicts: Vec::new(),
    };
    let tab = harness.state_mut();
    tab.diff = Some(crate::repo::DiffView::new(key));
    tab.queue(Action::ShowRemerge(true));
    tab.queue(Action::ShowRemerge(false));
    harness.run_steps(2);
    let _ = std::fs::remove_dir_all(&m.dir);
    assert!(opened);
    assert!(!harness.state().remerges.on);
    assert!(harness.state().diff.is_none());
}
