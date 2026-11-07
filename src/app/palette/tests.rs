//! The palette in the window, on real repositories: typed into and run with the keyboard as
//! someone would, and what git did checked.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Key};

use super::Palette;
use crate::app::{App, Saved, Tab};
use crate::git;
use crate::remote::askpass::asking;
use crate::repo::{Action, DiffKey};

type H<'a> = egui_kittest::Harness<'a, App>;

/// A repository on `main` with a commit of its own and a branch `side` with another, in a
/// fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-palette-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(std::fs::create_dir_all(&dir).is_ok());
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["checkout", "-q", "-b", "side"],
    ] {
        git_ok(&dir, args);
    }
    write(&dir, "side.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side work"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "main.txt", "main\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "main work"]);
    dir
}

fn write(dir: &Path, name: &str, text: &str) {
    assert!(std::fs::write(dir.join(name), text).is_ok());
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default().trim().to_owned()
}

/// The window with `dir` open, once it is read. The caller holds [`asking`] while it runs:
/// no other test's ssh question comes to it.
fn window(dir: &Path) -> H<'static> {
    let path = dir.to_path_buf();
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(move |cc| App::new(cc, Some(path)));
    settle(&mut harness);
    harness
}

/// Frames until the repository tab has nothing running.
fn settle(harness: &mut H<'_>) {
    let until = Instant::now() + Duration::from_secs(30);
    // A frame or two for what was queued to start.
    for _ in 0..3 {
        harness.step();
    }
    let settled = |harness: &H<'_>| {
        let state = harness.state();
        matches!(state.tabs.get(state.active),
            Some(Tab::Repo(r)) if !r.opening() && r.settled())
    };
    while Instant::now() < until && !settled(harness) {
        harness.step();
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(settled(harness), "the repository tab never settled");
    harness.step();
}

fn queue(harness: &mut H<'_>, action: Action) {
    let state = harness.state_mut();
    let active = state.active;
    if let Some(Tab::Repo(r)) = state.tabs.get_mut(active) {
        r.queue(action);
    }
    settle(harness);
}

fn palette<'a>(harness: &'a H<'_>) -> Option<&'a Palette> {
    harness.state().palette.as_ref()
}

/// The verb whose branch is being picked.
fn step_title<'a>(harness: &'a H<'_>) -> Option<&'a str> {
    palette(harness)?.step.as_ref().map(|s| s.title.as_str())
}

fn query<'a>(harness: &'a H<'_>) -> Option<&'a str> {
    palette(harness).map(|p| p.query.as_str())
}

/// Opens the palette with its key and types `text`.
fn open_and_type(harness: &mut H<'_>, text: &str) {
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::P);
    harness.step();
    harness.step();
    assert!(
        harness.state().palette.is_some(),
        "Ctrl+P opened no palette"
    );
    type_text(harness, text);
}

fn type_text(harness: &mut H<'_>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.step();
    harness.step();
}

/// The labels listed, best first.
fn listed(harness: &H<'_>) -> Vec<String> {
    let Some(p) = palette(harness) else {
        return Vec::new();
    };
    let Some((_, shown)) = &p.shown else {
        return Vec::new();
    };
    shown
        .iter()
        .map(|s| p.list()[s.index].label.clone())
        .collect()
}

fn enter(harness: &mut H<'_>) {
    harness.key_press(Key::Enter);
    harness.step();
    harness.step();
}

#[test]
fn typing_reb_ranks_rebase_first_and_its_branch_is_picked_next() {
    let dir = repo("rebase");
    let _asking = asking();
    let mut harness = window(&dir);
    open_and_type(&mut harness, "reb");
    let got = listed(&harness);
    assert_eq!(
        got.first().map(String::as_str),
        Some("Rebase main onto…"),
        "{got:?}"
    );
    assert!(got.contains(&"Pull (rebase)".to_owned()), "{got:?}");
    enter(&mut harness);
    assert_eq!(step_title(&harness), Some("Rebase main onto…"));
    assert_eq!(query(&harness), Some(""));
    assert_eq!(listed(&harness), ["side"]);
    enter(&mut harness);
    assert!(harness.state().palette.is_none());
    settle(&mut harness);
    let log = git_ok(&dir, &["log", "--format=%s", "main"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        ["main work", "side work", "base"]
    );
    // Remembered: the verb and the branch, first when nothing is typed next time.
    let recent = &harness.state().palette_recent;
    assert_eq!(recent.rank("rebase:side"), Some(0));
    assert_eq!(recent.rank("pick:rebase"), Some(1));
}

/// A letter typed and Enter pressed in the same frame run the best match for the whole query.
#[test]
fn enter_with_the_last_letter_in_the_same_frame_runs_the_new_best_match() {
    let dir = repo("same-frame");
    let _asking = asking();
    let mut harness = window(&dir);
    open_and_type(&mut harness, "pu");
    harness.event(Event::Text("ll (r".to_owned()));
    harness.key_press(Key::Enter);
    harness.step();
    harness.step();
    let _ = std::fs::remove_dir_all(&dir);
    // "pu" alone ranks Push or Pull first; "pull (r" only Pull (rebase), which runs and closes.
    assert!(harness.state().palette.is_none());
    let recent = &harness.state().palette_recent;
    assert_eq!(recent.rank("Pull (rebase)"), Some(0));
}

#[test]
fn the_selected_commit_is_cherry_picked_from_the_palette() {
    let dir = repo("cherry");
    let side = git_ok(&dir, &["rev-parse", "side"]);
    let _asking = asking();
    let mut harness = window(&dir);
    queue(&mut harness, Action::SelectHash(side.clone()));
    open_and_type(&mut harness, "");
    let short = &side[..8];
    assert_eq!(
        listed(&harness).first().map(String::as_str),
        Some(format!("Cherry-pick {short}").as_str())
    );
    type_text(&mut harness, "cp");
    assert_eq!(
        listed(&harness).first().map(String::as_str),
        Some(format!("Cherry-pick {short}").as_str())
    );
    enter(&mut harness);
    settle(&mut harness);
    let log = git_ok(&dir, &["log", "--format=%s", "-2"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(log.lines().collect::<Vec<_>>(), ["side work", "main work"]);
}

#[test]
fn going_back_from_the_second_step_restores_what_was_typed() {
    let dir = repo("back");
    let _asking = asking();
    let mut harness = window(&dir);
    open_and_type(&mut harness, "merge");
    enter(&mut harness);
    assert_eq!(step_title(&harness), Some("Merge into main…"));
    type_text(&mut harness, "zzz");
    assert!(listed(&harness).is_empty());
    // Esc in the second step goes back to the first, not out.
    harness.key_press(Key::Escape);
    harness.step();
    // Only looked at: the verb is not remembered.
    assert_eq!(harness.state().palette_recent.rank("pick:merge"), None);
    assert_eq!(step_title(&harness), None);
    assert_eq!(query(&harness), Some("merge"));
    // Backspace in the second step's empty field goes back too, but not a held one that
    // emptied it.
    enter(&mut harness);
    type_text(&mut harness, "x");
    for _ in 0..3 {
        harness.event(Event::Key {
            key: Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    harness.event(Event::Key {
        key: Key::Backspace,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    assert_eq!(step_title(&harness), Some("Merge into main…"));
    assert_eq!(query(&harness), Some(""));
    harness.key_press(Key::Backspace);
    harness.step();
    assert_eq!(step_title(&harness), None);
    assert!(palette(&harness).is_some());
    harness.key_press(Key::Escape);
    harness.step();
    harness.step();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(harness.state().palette.is_none());
}

#[test]
fn the_open_file_with_spaces_and_unicode_is_staged_from_the_palette() {
    let dir = repo("stage");
    let name = "notes ü 文件.txt";
    write(&dir, name, "new\n");
    let _asking = asking();
    let mut harness = window(&dir);
    // The window saw the new file: open its diff, as clicking it does.
    queue(&mut harness, Action::Reload);
    let file = git::StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: '?',
        area: git::Area::Untracked,
    };
    queue(&mut harness, Action::OpenDiff(DiffKey::Work(file)));
    open_and_type(&mut harness, "stage ü");
    assert_eq!(
        listed(&harness).first().map(String::as_str),
        Some(format!("Stage {name}").as_str())
    );
    enter(&mut harness);
    settle(&mut harness);
    let staged = git_ok(
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
    assert_eq!(staged, name);
}

#[test]
fn thousands_of_branches_open_and_filter_quickly() {
    let dir = repo("many");
    let head = git_ok(&dir, &["rev-parse", "HEAD"]);
    // Packed, as a large repository keeps them.
    let packed: String = (0..3000)
        .map(|k| format!("{head} refs/heads/team/feature-{k}\n"))
        .collect();
    write(
        &dir.join(".git"),
        "packed-refs",
        &format!("# pack-refs with: peeled fully-peeled sorted\n{packed}"),
    );
    let _asking = asking();
    let mut harness = window(&dir);
    let started = Instant::now();
    open_and_type(&mut harness, "");
    let first_step = listed(&harness).len();
    assert!(first_step < 100, "{first_step} entries with nothing typed");
    type_text(&mut harness, "feature-2999");
    assert_eq!(
        listed(&harness).first().map(String::as_str),
        Some("Check out team/feature-2999")
    );
    assert!(listed(&harness).len() <= 10);
    harness.key_press(Key::Escape);
    harness.step();
    harness.step();
    assert!(
        harness.state().palette.is_none(),
        "Esc left the palette open"
    );
    open_and_type(&mut harness, "merge into");
    enter(&mut harness);
    assert_eq!(listed(&harness).len(), 3001);
    type_text(&mut harness, "2999");
    assert_eq!(
        listed(&harness).first().map(String::as_str),
        Some("team/feature-2999")
    );
    let took = started.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(took < Duration::from_secs(10), "took {took:?}");
}

#[test]
fn what_was_run_lately_survives_a_restart() -> Result<(), serde_json::Error> {
    let mut saved = Saved::default();
    saved.palette_recent.used("pick:merge");
    saved.palette_recent.used("Fetch");
    let text = serde_json::to_string(&saved)?;
    let back: Saved = serde_json::from_str(&text)?;
    assert_eq!(back.palette_recent, saved.palette_recent);
    // Saved before the palette remembered anything: nothing remembered.
    let old: Saved = serde_json::from_str("{}")?;
    assert_eq!(old.palette_recent.rank("Fetch"), None);
    Ok(())
}
