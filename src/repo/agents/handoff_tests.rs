//! The tray through the tab, on a real repository and a real terminal.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::TrayAct;
use crate::pull_request::DiffSide;
use crate::repo::agents::AgentAct;
use crate::repo::pr_review::LineTarget;
use crate::repo::{Action, RepoTab};

fn repo(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-tray-tab-{name}-{} ü", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    assert!(crate::git::run(Some(&dir), &["init", "-q"]).is_ok());
    dir
}

fn pump(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn add_note(tab: &mut RepoTab, ctx: &egui::Context, path: &str, line: u64, text: &str) {
    tab.apply(
        Action::Agent(AgentAct::OpenNote(LineTarget {
            path: path.to_owned(),
            line,
            side: DiffSide::Right,
        })),
        ctx,
    );
    tab.agents.tray.text = text.to_owned();
    tab.apply(
        Action::Agent(AgentAct::Tray(TrayAct::Add("let x = 1;".to_owned()))),
        ctx,
    );
}

#[test]
fn notes_collect_in_the_tray_mark_their_lines_and_outlive_the_tab() {
    let dir = repo("keep");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.load_tray(&ctx);
    add_note(&mut tab, &ctx, "src/a b.rs", 3, "Name this ü");
    add_note(&mut tab, &ctx, "src/a b.rs", 3, "And test it");
    let review = tab.note_review("src/a b.rs");
    let marked = review.marks.get(&(DiffSide::Right, 3)).copied();
    let other = tab.note_review("other.rs").marks.is_empty();
    // Saved in the background; a new tab of the worktree reads them back.
    let saved = pump(&mut tab, &ctx, |_| {
        dir.join(".git/gitcraft-handoff.json").is_file()
    });
    let mut again = RepoTab::new_deferred(dir.clone(), &ctx);
    again.load_tray(&ctx);
    let read = pump(&mut again, &ctx, |t| t.agents.tray.notes.len() == 2);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(marked, Some(2));
    assert!(other && review.local);
    assert!(saved && read);
    assert_eq!(again.agents.tray.notes[0].text, "Name this ü");
    assert_eq!(again.agents.tray.notes[0].code, "let x = 1;");
}

/// Sending types the notes into the agent's terminal as one paste, never pressing Enter, and
/// only into a program that takes pastes: a shell would run each line.
#[cfg(unix)]
#[test]
fn the_tray_is_typed_into_the_agent_terminal_as_one_paste() {
    use crate::terminal::{ShellSpec, Terminal};
    let dir = repo("send");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.agents.tray.loaded_for_tests();
    let got = dir.join("got.txt");
    let agent = |label: &str, script: String| ShellSpec {
        label: label.to_owned(),
        program: "/bin/sh".to_owned(),
        args: vec!["-c".to_owned(), script],
    };
    // A program that never turned on bracketed paste: refused, the notes stay.
    let plain = agent("Claude Code", format!("exec cat > '{}'", got.display()));
    if let Ok(term) = Terminal::spawn(&dir, &plain, &ctx) {
        tab.terminals.push(term);
    }
    add_note(&mut tab, &ctx, "a.rs", 1, "Fix\nthis");
    std::thread::sleep(Duration::from_millis(300));
    tab.apply(Action::Agent(AgentAct::Tray(TrayAct::SendNotes)), &ctx);
    let refused = tab
        .status
        .as_ref()
        .is_some_and(|(s, err)| *err && s.contains("doesn't take a pasted message"));
    let kept = tab.agents.tray.notes.len() == 1;
    tab.terminals.clear();

    // One that did, as agent CLIs do.
    let pasting = agent(
        "Claude Code",
        format!(
            "stty raw -echo; printf '\\033[?2004h'; exec cat > '{}'",
            got.display()
        ),
    );
    if let Ok(term) = Terminal::spawn(&dir, &pasting, &ctx) {
        tab.terminals.push(term);
    }
    let start = Instant::now();
    let mut sent = false;
    while start.elapsed() < Duration::from_secs(10) && !sent {
        // Until the program's output (turning bracketed paste on) has been read.
        std::thread::sleep(Duration::from_millis(100));
        tab.apply(Action::Agent(AgentAct::Tray(TrayAct::SendNotes)), &ctx);
        sent = tab.agents.tray.notes.is_empty();
    }
    let arrived = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        std::fs::read_to_string(&got).is_ok_and(|s| s.contains("\u{1b}[201~"))
    });
    let typed = std::fs::read_to_string(&got).unwrap_or_default();
    tab.terminals.clear();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused && kept, "{:?}", tab.status);
    assert!(sent && arrived, "{:?} {typed:?}", tab.status);
    assert!(typed.starts_with("\u{1b}[200~Review notes"), "{typed:?}");
    assert!(
        typed.contains("1. a.rs:1\r   > let x = 1;\r   Fix\r   this\r"),
        "{typed:?}"
    );
    assert!(
        !typed.ends_with('\r'),
        "no Enter after the paste: {typed:?}"
    );
}
