use super::*;

#[test]
fn selection_registry_runs_actual_file_view_and_preserves_text_ownership() {
    use crate::keymap::{self, Command, Scope, Stroke};
    use egui::{Event, Key, Modifiers};
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut settings = crate::settings::Settings::default();
    for (command, key) in [
        (Command::SelectionAll, Key::F8),
        (Command::SelectionCopy, Key::F9),
        (Command::SelectionHistory, Key::F10),
    ] {
        settings
            .key_bindings
            .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
    }
    settings.put_for_test(&ctx);
    let data = Ok(FileText::new(
        "one\n日本語\nthree\n".as_bytes(),
        "test.txt",
        true,
    ));
    let mut find = Find::default();
    let source_calls = std::cell::Cell::new(0);
    let mut frame = |key: Option<Key>, text_owned, history| {
        let mut asked = None;
        let out = ctx.run_ui(
            egui::RawInput {
                events: key
                    .into_iter()
                    .flat_map(|key| {
                        [true, false].map(|pressed| Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        })
                    })
                    .collect(),
                ..Default::default()
            },
            |ui| {
                keymap::active(
                    &ctx,
                    vec![Scope::Window, Scope::Repository, Scope::Selection],
                );
                if text_owned {
                    ui.text_edit_singleline(&mut String::new()).request_focus();
                }
                asked = show(
                    ui,
                    "registry-file",
                    Some(&data),
                    &mut find,
                    || {
                        source_calls.set(source_calls.get() + 1);
                        Source::DiffFile("recorded".into())
                    },
                    history,
                    None,
                    None,
                );
            },
        );
        let copied: Vec<_> = out
            .platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        out.drop_without_applying_deltas();
        (asked, copied)
    };
    frame(None, false, Ok(()));
    frame(Some(Key::F8), false, Ok(()));
    assert_eq!(
        frame(Some(Key::F9), false, Ok(())).1,
        ["one\n日本語\nthree"]
    );
    assert_eq!(frame(Some(Key::F10), false, Ok(())).0, Some((1, 3)));
    assert_eq!(
        frame(Some(Key::F10), false, Err("No committed version")).0,
        None
    );
    assert!(frame(Some(Key::F9), true, Ok(())).1.is_empty());
    assert_eq!(source_calls.get(), 0);
}

#[test]
fn lines_are_picked_by_click_and_shift_click() {
    let one = LinePick::clicked(None, 4, false);
    assert_eq!(one.map(LinePick::numbers), Some((5, 5)));
    // Shift+click goes from where the pick started, either way.
    let up = LinePick::clicked(one, 1, true);
    assert_eq!(up.map(LinePick::numbers), Some((2, 5)));
    assert!(up.is_some_and(|p| p.contains(3) && !p.contains(5)));
    // A click elsewhere starts again; on the one line picked, unpicks it.
    assert_eq!(
        LinePick::clicked(up, 9, false).map(LinePick::numbers),
        Some((10, 10))
    );
    assert_eq!(LinePick::clicked(one, 4, false), None);
    // Shift+click with nothing picked picks that line.
    assert_eq!(
        LinePick::clicked(None, 2, true).map(LinePick::numbers),
        Some((3, 3))
    );
}

#[test]
fn lines_lose_their_endings_and_tabs() {
    assert_eq!(split_lines("a\r\n\tb\nc"), ["a", "    b", "c"]);
    assert_eq!(split_lines("a\n\n"), ["a", ""]);
    assert!(split_lines("").is_empty());
}

#[test]
fn a_nul_makes_a_file_binary() {
    let text = FileText::new(b"PNG\0\x01", "a.png", true);
    assert_eq!(text.binary, Some(5));
    assert!(text.lines.is_empty());
    let text = FileText::new(b"fn main() {}\n", "main.rs", true);
    assert_eq!((text.binary, text.lines.len(), text.longest), (None, 1, 12));
    assert!(text.spans.is_some());
}

#[test]
fn a_diff_shows_the_file_after_the_change_or_before_its_deletion() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-file-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| git::run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(args)?;
    }
    std::fs::write(dir.join("a.txt"), "one\n").map_err(|e| e.to_string())?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "one"])?;
    std::fs::write(dir.join("a.txt"), "two\n").map_err(|e| e.to_string())?;
    git(&["commit", "-qam", "two"])?;
    git(&["rm", "-q", "a.txt"])?;
    git(&["commit", "-qm", "gone"])?;
    let commit = |rev: &str| -> Result<DiffKey, String> {
        Ok(DiffKey::Commit {
            hash: git(&["rev-parse", rev])?.trim().to_owned(),
            path: "a.txt".to_owned(),
        })
    };
    let changed = load_for_diff(&dir, &commit("HEAD~1")?, true)?;
    let deleted = load_for_diff(&dir, &commit("HEAD")?, true)?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (changed.lines, changed.before),
        (vec!["two".to_owned()], false)
    );
    assert_eq!(
        (deleted.lines, deleted.before),
        (vec!["two".to_owned()], true)
    );
    Ok(())
}
