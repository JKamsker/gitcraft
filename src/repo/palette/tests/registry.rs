// Included in the parent test module to preserve qualified test names.

/// Pure supplied-state regression for REG-R2-1: actual entries reach the production
/// Action::Key adapter and clipboard output, with no OS clipboard, Git or background read.
#[test]
fn registry_palette_selected_copy_dispatches_and_unowned_contexts_are_absent() -> Result<(), String>
{
    use crate::keymap::Command;
    let ctx = egui::Context::default();
    let mut tab = tab_with_branches(0);
    let hash = tab.refs.head.clone();
    tab.commits = vec![Arc::new(Commit {
        hash: hash.clone(),
        summary: "selected ü message".into(),
        refs: vec!["main".into()],
        ..Default::default()
    })];
    tab.selected = Some(0);
    for (command, expected) in [
        (Command::GraphCopyHash, hash.as_str()),
        (Command::GraphCopyMessage, "selected ü message"),
        (Command::GraphCopyBranch, "main"),
    ] {
        let item = tab
            .palette_items(&ctx)
            .into_iter()
            .find(|i| matches!(&i.run, PaletteRun::Action(Action::Key(c)) if *c == command))
            .ok_or("missing applicable selected-commit command")?;
        let PaletteRun::Action(action) = item.run else {
            return Err("not an action".into());
        };
        let mut action = Some(action);
        let out = ctx.run_ui(egui::RawInput::default(), |_| {
            if let Some(action) = action.take() {
                tab.apply(action, &ctx);
            }
        });
        assert_eq!(
            out.platform_output
                .commands
                .iter()
                .filter(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == expected))
                .count(),
            1
        );
        out.drop_without_applying_deltas();
    }
    // No generic palette entries for widget-owned find, selection, file-row or rebase
    // commands. Their keyboard handlers retain the actual view/focus and target ownership.
    for item in tab.palette_items(&ctx) {
        if let PaletteRun::Action(Action::Key(c)) = item.run {
            assert!(
                !matches!(tab.key_action(c), Ok(None)),
                "inert palette entry: {c:?}"
            );
            assert!(!matches!(
                c.scope(),
                crate::keymap::Scope::Find
                    | crate::keymap::Scope::Selection
                    | crate::keymap::Scope::Rebase
            ));
        }
    }
    tab.selected = None;
    assert!(tab.palette_items(&ctx).iter().all(|i| !matches!(
        &i.run,
        PaletteRun::Action(Action::Key(
            Command::GraphCopyHash | Command::GraphCopyMessage | Command::GraphCopyBranch
        ))
    )));
    assert!(matches!(
        tab.key_action(Command::Reload),
        Ok(Some(Action::Reload))
    ));
    tab.apply(Action::Key(Command::RebasePick), &ctx);
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(why, bad)| *bad && why.contains("owning view"))
    );
    Ok(())
}

#[test]
fn registry_palette_effective_undo_redo_search_keys_are_unique_under_override_unbind_reset() {
    use crate::keymap::{self, Command, Stroke};
    let ctx = egui::Context::default();
    let tab = tab();
    let mut settings = crate::settings::Settings::default();
    for mode in 0..3 {
        for (command, key) in [
            (Command::Undo, egui::Key::F8),
            (Command::Redo, egui::Key::F9),
            (Command::Search, egui::Key::F10),
        ] {
            match mode {
                0 => {
                    settings.key_bindings.insert(
                        command.id().into(),
                        vec![Stroke::new(key, egui::Modifiers::NONE)],
                    );
                }
                1 => {
                    settings.key_bindings.insert(command.id().into(), vec![]);
                }
                _ => {
                    settings.key_bindings.remove(command.id());
                }
            }
        }
        settings.put_for_test(&ctx);
        let items = tab.palette_items(&ctx);
        for command in [Command::Undo, Command::Redo, Command::Search] {
            let matches: Vec<_> = items
                .iter()
                .filter(
                    |i| matches!(&i.run, PaletteRun::Action(a) if a.key_command() == Some(command)),
                )
                .collect();
            assert_eq!(matches.len(), 1, "mode {mode}: {command:?}");
            let label = keymap::label(&ctx, command);
            assert_eq!(matches[0].shortcut, (!label.is_empty()).then_some(label));
            assert!(matches!(&matches[0].run, PaletteRun::Action(Action::Key(c)) if *c == command));
        }
    }
}

#[test]
fn registry_palette_search_dispatches_the_same_visible_target_as_the_key() -> Result<(), String> {
    use crate::keymap::Command;
    let ctx = egui::Context::default();
    let mut tab = tab();
    for lines in [false, true] {
        tab.search.open = false;
        tab.search.focus = false;
        tab.find.close();
        tab.diff = lines.then(|| {
            DiffView::new(DiffKey::Commit {
                hash: "recorded".into(),
                path: "file".into(),
            })
        });
        let item = tab
            .palette_items(&ctx)
            .into_iter()
            .find(|i| matches!(&i.run, PaletteRun::Action(Action::Key(Command::Search))))
            .ok_or("missing Search entry")?;
        let PaletteRun::Action(action) = item.run else {
            return Err("not an action".into());
        };
        tab.apply(action, &ctx);
        assert_eq!(tab.find.open, lines);
        assert_eq!(tab.find.focus, lines);
        assert_eq!(tab.search.open, !lines);
        assert_eq!(tab.search.focus, !lines);
    }
    Ok(())
}

#[test]
fn area_d_hidden_optional_tools_stay_in_palette_with_selected_commit_scope() {
    let mut tab = tab_with_branches(1);
    let head = "a".repeat(40);
    let selected = "b".repeat(40);
    let root = "c".repeat(40);
    tab.commits = vec![
        std::sync::Arc::new(Commit {
            hash: head.clone(),
            parents: vec![selected.clone()],
            ..Default::default()
        }),
        std::sync::Arc::new(Commit {
            hash: selected.clone(),
            parents: vec![root.clone()],
            ..Default::default()
        }),
        std::sync::Arc::new(Commit {
            hash: root,
            ..Default::default()
        }),
    ];
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.history = tab.commits.clone();
    tab.history_index = tab.index.clone();
    tab.selected = Some(1);
    let items = tab.palette_items(&egui::Context::default());
    for key in [
        "commit:split",
        "commit:move",
        "commit:bisect",
        "commit:parallel-bisect",
        "commit:test",
        "commit:release",
        "pick:rehearse",
    ] {
        assert!(
            items.iter().any(|i| i.key == key),
            "{key} missing with default (hidden) menu options"
        );
    }
    assert!(items.iter().any(
        |i| matches!(&i.run, PaletteRun::Action(Action::OpenSplit(hash)) if hash == &selected)
    ));
    assert!(items.iter().any(
        |i| matches!(&i.run, PaletteRun::Action(Action::OpenMove(hash)) if hash == &selected)
    ));
    tab.refs.current = None;
    assert!(
        !tab.palette_items(&egui::Context::default())
            .iter()
            .any(|i| matches!(
                &i.run,
                PaletteRun::Action(Action::OpenSplit(_) | Action::OpenMove(_))
            ))
    );
}
