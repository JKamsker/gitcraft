use std::collections::HashSet;

use egui::{Event, Key, Modifiers, RawInput};

use super::*;

/// What a key press is, whatever the platform: on macOS Cmd and Ctrl are two keys, elsewhere
/// Cmd is Ctrl, so two bindings that differ only there would clash outside macOS.
fn normalised(b: &Binding) -> (Key, bool, bool, bool) {
    let m = b.modifiers;
    (b.key, m.command || m.ctrl, m.shift, m.alt)
}

/// The keys of a manual row written as the views' own shortcuts are ("Cmd+Shift+Z", "Esc"),
/// when it is one key with modifiers.
fn parse(combo: &str) -> Option<(Key, bool, bool, bool)> {
    let mut parts: Vec<&str> = combo.split('+').collect();
    let key = Key::from_name(parts.pop()?)?;
    let (mut cmd, mut shift, mut alt) = (false, false, false);
    for m in parts {
        match m {
            "Cmd" | "Ctrl" => cmd = true,
            "Shift" => shift = true,
            "Alt" => alt = true,
            _ => return None,
        }
    }
    Some((key, cmd, shift, alt))
}

const EVERY_COMMAND: &[Command] = &[
    Command::NewTab,
    Command::CloseTab,
    Command::ReopenTab,
    Command::NextTab,
    Command::PreviousTab,
    Command::GoToTab(1),
    Command::GoToTab(2),
    Command::GoToTab(3),
    Command::GoToTab(4),
    Command::GoToTab(5),
    Command::GoToTab(6),
    Command::GoToTab(7),
    Command::GoToTab(8),
    Command::LastTab,
    Command::Palette,
    Command::Settings,
    Command::Reload,
    Command::Manual,
    Command::Fetch,
    Command::Pull,
    Command::Push,
    Command::Stash,
    Command::NewBranch,
    Command::Terminal,
    Command::StageAll,
    Command::Commit,
];

#[test]
fn no_two_bindings_share_a_key() {
    let mut seen = HashSet::new();
    for b in BINDINGS {
        assert!(seen.insert(normalised(b)), "{:?} is bound twice", b.key);
    }
}

#[test]
fn no_binding_takes_a_key_a_view_already_uses() {
    let bound: HashSet<_> = BINDINGS.iter().map(normalised).collect();
    for (group, rows) in crate::manual::SHORTCUTS {
        for (keys, what) in *rows {
            for combo in keys.split(" / ") {
                if let Some(parsed) = parse(combo) {
                    assert!(!bound.contains(&parsed), "{combo} ({group}: {what})");
                }
            }
        }
    }
}

#[test]
fn every_command_has_one_key_and_finds_its_way_back() {
    for &command in EVERY_COMMAND {
        let bindings: Vec<_> = BINDINGS.iter().filter(|b| b.command == command).collect();
        assert_eq!(bindings.len(), 1, "{command:?}");
        let b = bindings[0];
        assert_eq!(find(b.key, b.modifiers), Some(command));
        assert!(!label(command).is_empty());
    }
    assert_eq!(BINDINGS.len(), EVERY_COMMAND.len());
}

#[test]
fn every_command_is_in_the_manual_once() {
    let rows = WINDOW_ROWS.iter().chain(REPOSITORY_ROWS);
    let listed: Vec<Command> = rows.flat_map(|r| r.commands.iter().copied()).collect();
    for command in EVERY_COMMAND {
        let times = listed.iter().filter(|c| *c == command).count();
        assert_eq!(times, 1, "{command:?}");
    }
    for r in REPOSITORY_ROWS {
        assert!(r.commands.iter().all(|c| c.scope() == Scope::Repository));
    }
    for r in WINDOW_ROWS {
        assert!(r.commands.iter().all(|c| c.scope() == Scope::Window));
    }
}

#[test]
fn keys_are_written_for_this_platform() {
    let cmd = crate::manual::command();
    assert_eq!(label(Command::Fetch), format!("{cmd}+Shift+F"));
    assert_eq!(label(Command::Commit), format!("{cmd}+Enter"));
    assert_eq!(label(Command::Settings), format!("{cmd}+,"));
    // Ctrl, also on macOS.
    assert_eq!(label(Command::PreviousTab), "Ctrl+Shift+Tab");
    assert_eq!(label(Command::Reload), "F5");
    assert_eq!(tip("Push", Command::Push), format!("Push ({cmd}+Shift+K)"));
    let tabs = row_keys(&WINDOW_ROWS[4]);
    assert_eq!(tabs, format!("{cmd}+1…8"));
    assert_eq!(row_keys(&WINDOW_ROWS[3]), "Ctrl+Tab / Ctrl+Shift+Tab");
}

#[test]
fn extra_modifiers_make_another_key() {
    let shift = Modifiers::COMMAND | Modifiers::SHIFT;
    assert_eq!(find(Key::T, Modifiers::COMMAND), Some(Command::NewTab));
    assert_eq!(find(Key::T, shift), Some(Command::ReopenTab));
    assert_eq!(find(Key::F, Modifiers::COMMAND), None);
    assert_eq!(find(Key::F, Modifiers::NONE), None);
    assert_eq!(find(Key::B, Modifiers::COMMAND | Modifiers::ALT), None);
    assert_eq!(find(Key::Tab, Modifiers::NONE), None);
    assert_eq!(find(Key::Num0, Modifiers::COMMAND), None);
}

fn key(key: Key, modifiers: Modifiers, repeat: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    }
}

/// The commands of `scope` pressed in one frame with these events.
fn pressed_with(events: Vec<Event>, scope: Scope) -> Vec<Command> {
    let ctx = egui::Context::default();
    let mut got = Vec::new();
    let input = RawInput {
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |ui| got = ui.input(|i| pressed(i, scope)))
        .textures_delta
        .clear();
    got
}

#[test]
fn presses_are_read_per_scope_and_repeats_only_go_through_tabs() {
    let ctrl = if cfg!(target_os = "macos") {
        Modifiers::CTRL
    } else {
        Modifiers::CTRL | Modifiers::COMMAND
    };
    let events = vec![
        key(Key::J, Modifiers::COMMAND, false),
        key(Key::Tab, ctrl, false),
        key(Key::Tab, ctrl, true),
        key(Key::J, Modifiers::COMMAND, true),
        // Released keys and typed text do nothing.
        Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Text("b".into()),
    ];
    assert_eq!(
        pressed_with(events.clone(), Scope::Repository),
        [Command::Terminal]
    );
    assert_eq!(
        pressed_with(events, Scope::Window),
        [Command::NextTab, Command::NextTab]
    );
}

#[test]
fn hints_that_are_keys_are_told_from_categories() {
    for keys in ["Ctrl+B", "Cmd+Shift+F", "F5", "F12", "Ctrl+,", "Ctrl+Enter"] {
        assert!(is_keys(keys), "{keys}");
    }
    for word in [
        "branch",
        "remote",
        "F",
        "Fix",
        "origin",
        "",
        "+",
        "my+remote",
    ] {
        assert!(!is_keys(word), "{word}");
    }
}

/// On AZERTY the key under 4 reads as `'` without Shift: the tab keys go by the key's place.
#[test]
fn tab_digits_go_by_their_place_on_the_keyboard() {
    let azerty_4 = Event::Key {
        key: Key::Quote,
        physical_key: Some(Key::Num4),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    // Elsewhere the logical key wins: Ctrl+J on a layout where J sits elsewhere is still J.
    let moved_j = Event::Key {
        key: Key::J,
        physical_key: Some(Key::Num4),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    assert_eq!(
        pressed_with(vec![azerty_4], Scope::Window),
        [Command::GoToTab(4)]
    );
    assert_eq!(
        pressed_with(vec![moved_j.clone()], Scope::Repository),
        [Command::Terminal]
    );
    assert!(pressed_with(vec![moved_j], Scope::Window).is_empty());
}
