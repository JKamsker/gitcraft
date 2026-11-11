//! Context matrices exercise the composed palette, not its extraction boundaries.
use super::*;
use crate::keymap::Stroke;
use crate::repo::{offline, partial_stash, push, timeline};

fn shortcut_context(mode: usize) -> egui::Context {
    let ctx = egui::Context::default();
    let mut settings = crate::settings::Settings::default();
    for (command, key) in [
        (Command::Fetch, egui::Key::F6),
        (Command::Pull, egui::Key::F7),
        (Command::Push, egui::Key::F8),
        (Command::Undo, egui::Key::F9),
        (Command::Redo, egui::Key::F10),
        (Command::Timeline, egui::Key::F11),
        (Command::Stash, egui::Key::F12),
    ] {
        match mode {
            1 => {
                settings.key_bindings.insert(
                    command.id().into(),
                    vec![Stroke::new(key, egui::Modifiers::NONE)],
                );
            }
            2 => {
                settings
                    .key_bindings
                    .insert(command.id().into(), Vec::new());
            }
            _ => {}
        }
    }
    settings.put_for_test(&ctx);
    ctx
}

fn transport_case(review: bool, paused: bool, shortcuts: usize) {
    let ctx = shortcut_context(shortcuts);
    let entries = push::palette::items(&ctx, review);
    let mut expected = vec![
        "Fetch",
        "Pull",
        if review { "Push for review" } else { "Push" },
    ];
    if !review {
        expected.push("Sync (pull, then push)");
    }
    expected.push(if review {
        "Push for review, skipping hooks"
    } else {
        "Push, skipping hooks"
    });
    expected.extend(PullMode::ALL.map(|mode| mode.label()));
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    for ((_, hint, action), command) in
        entries
            .iter()
            .take(3)
            .zip([Command::Fetch, Command::Pull, Command::Push])
    {
        assert_eq!(hint, &label(&ctx, command));
        assert!(matches!(
            (command, action),
            (Command::Fetch, Action::Fetch)
                | (Command::Pull, Action::Pull)
                | (Command::Push, Action::Push)
        ));
    }
    assert_eq!(
        entries
            .iter()
            .filter(|(_, _, action)| matches!(action, Action::Sync))
            .count(),
        usize::from(!review)
    );
    let modes: Vec<_> = entries
        .iter()
        .filter_map(|(_, hint, action)| match action {
            Action::PullWith(mode) => {
                assert_eq!(hint, "remote");
                Some(*mode)
            }
            _ => None,
        })
        .collect();
    assert_eq!(modes, PullMode::ALL);
    let skipped: Vec<_> = entries
        .iter()
        .filter(|(_, _, action)| matches!(action, Action::PushSkippingHooks))
        .collect();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].1, "--no-verify");
    let item = offline::palette_item(paused);
    assert_eq!(
        (&*item.0, &*item.1),
        if paused {
            ("Go online", "network offline")
        } else {
            ("Go offline", "network offline metered")
        }
    );
    assert!(matches!(item.2, Action::SetOffline(next) if next != paused));
    for ((_, hint, _), command) in
        timeline::palette::items(&ctx)
            .iter()
            .zip([Command::Undo, Command::Redo, Command::Timeline])
    {
        assert_eq!(hint, &label(&ctx, command));
    }
    assert_eq!(
        partial_stash::palette::stash(&ctx)[0].1,
        label(&ctx, Command::Stash)
    );
    if shortcuts == 1 {
        assert_eq!(entries[0].1, "F6");
    }
    if shortcuts == 2 {
        assert!(entries.iter().take(3).all(|(_, hint, _)| hint.is_empty()));
    }
}

#[test]
fn transport_intents_and_shortcuts_follow_review_pause_and_keymap_contexts() {
    for review in [false, true] {
        for paused in [false, true] {
            for shortcuts in 0..3 {
                transport_case(review, paused, shortcuts);
            }
        }
    }
}

fn composed_case(review: bool, selected: bool, busy: bool, lfs: bool) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/palette-owners".into(), &ctx);
    tab.gerrit = review.then(|| crate::gerrit::Server {
        remote: "origin".into(),
        web: "https://review.example.invalid".into(),
        project: "project".into(),
        web_warning: None,
    });
    tab.refs.head = "a".repeat(40);
    tab.commits = vec![std::sync::Arc::new(crate::git::Commit {
        hash: tab.refs.head.clone(),
        summary: "recorded selection".into(),
        ..Default::default()
    })];
    tab.selected = selected.then_some(0);
    tab.busy_op = busy.then(|| "recorded operation".into());
    tab.lfs = lfs;
    let entries = tab.additional_palette_items(&ctx);
    let names: Vec<_> = entries.iter().map(|entry| entry.0.as_str()).collect();
    let ordered = [
        "DVC: Pull",
        "Code moved across files",
        "File migration map",
        "Three-state staging lens",
        "File versions across branches",
        "Fetch",
        "Pull",
        "Behaviour timeline\u{2026}",
        "Inspect local Git bundle\u{2026}",
        "Stash only what is staged\u{2026}",
        "Save snapshot (stash, keep the changes)\u{2026}",
        "Distribute staged fixes\u{2026}",
        "Undo",
        "Redo",
        "Timeline and undo tree",
        "Stash changes\u{2026}",
        "Stage all changes",
        "Commit",
        "Search commits",
        "Reflog",
        "Find lost commits",
        "Activity: operations and their output",
        "Reload",
        "Select stale or merged branches\u{2026}",
        "Conflicts that can wait: isolated rebase and restack\u{2026}",
        "All refs and namespace history\u{2026}",
        "Repository doctor",
    ];
    let positions: Vec<_> = ordered
        .iter()
        .map(|name| names.iter().position(|found| found == name))
        .collect();
    assert!(
        positions.iter().all(Option::is_some),
        "missing contribution"
    );
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "changed composition order"
    );
    assert_eq!(
        entries
            .iter()
            .filter(|(_, _, action)| matches!(action, Action::Sync))
            .count(),
        usize::from(!review)
    );
    assert_eq!(names.contains(&"LFS: Fetch"), lfs);
    assert_eq!(names.contains(&"LFS: Pull"), lfs);
    let final_items = tab.palette_items(&ctx);
    for name in [
        "Code moved across files",
        "Inspect state publication receipts\u{2026}",
        "Find lost commits",
    ] {
        assert_eq!(
            final_items.iter().filter(|item| item.label == name).count(),
            1
        );
    }
}

#[test]
fn owned_groups_keep_their_global_slots_across_repository_contexts() {
    for review in [false, true] {
        for selected in [false, true] {
            for busy in [false, true] {
                for lfs in [false, true] {
                    composed_case(review, selected, busy, lfs);
                }
            }
        }
    }
}
