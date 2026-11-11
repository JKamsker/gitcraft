// Included in the parent test module to preserve qualified test names.

#[test]
fn text_terminal_palette_modal_and_context_menu_own_their_keys() -> Result<(), String> {
    let mut t = Test::new("focus-guards")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    t.tab.keys_blocked = true;
    t.key(Key::Space);
    t.pane(Key::Num2);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Detail));
    t.tab.keys_blocked = false;
    let mut typed = String::new();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
        events: keys(Key::Space, Modifiers::NONE, false),
        ..Default::default()
    };
    t.ctx
        .run_ui(input, |ui| {
            egui::Panel::top("test-typing").show(ui, |ui| {
                ui.text_edit_singleline(&mut typed).request_focus();
            });
            t.tab.ui(ui);
        })
        .drop_without_applying_deltas();
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    t.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    t.tab.dialog = Some(Dialog::confirm(
        "Test modal",
        "No operation should run".to_owned(),
        "Confirm",
        vec![],
        None,
    ));
    t.key(Key::Space);
    assert!(t.tab.busy_op.is_none());
    t.tab.dialog = None;
    t.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    let shapes = t.frame(vec![]);
    let point = text_center(&shapes, "a.txt");
    assert!(point.is_some());
    if let Some(point) = point {
        t.click(point, egui::PointerButton::Secondary);
    }
    assert!(egui::Popup::is_any_open(&t.ctx));
    t.key(Key::Space);
    assert!(t.tab.busy_op.is_none());
    egui::Popup::close_all(&t.ctx);
    #[cfg(unix)]
    {
        t.tab.terminals.apply(
            crate::repo::terminals::TerminalAct::New {
                shell: Some(crate::terminal::ShellSpec {
                    label: "test".to_owned(),
                    program: "sh".to_owned(),
                    args: vec![],
                }),
                cwd: None,
            },
            &t.tab.path,
            &t.ctx,
        );
        t.frame(vec![]);
        t.click(egui::pos2(500.0, 700.0), egui::PointerButton::Primary);
        assert!(
            t.ctx.egui_wants_keyboard_input(),
            "the actual PTY terminal must hold focus"
        );
        for key in [Key::Space, Key::Enter, Key::S, Key::U, Key::D] {
            t.key(key);
        }
        t.pane(Key::Num3);
        assert!(t.tab.busy_op.is_none() && t.tab.dialog.is_none());
    }
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    Ok(())
}

#[test]
fn stale_status_path_diff_and_conflict_targets_never_execute() -> Result<(), String> {
    let mut t = Test::new("stale-targets")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    let selected = t.tab.keyboard.borrow().cursor.clone();
    assert!(selected.is_some());
    let Some((scope, key)) = selected else {
        return Err("no file cursor".to_owned());
    };
    let request = Request {
        path: t.path.clone(),
        read: t.tab.work_read,
        epoch: t.tab.work_epoch,
        target: Target::File {
            scope,
            key: key.clone(),
            open: false,
        },
    };
    t.tab.work_read += 1;
    t.tab.apply_keyboard(request.clone(), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    let mut moved = request.clone();
    moved.read = t.tab.work_read;
    moved.path = PathBuf::from("another repository");
    t.tab.apply_keyboard(moved, &t.ctx);
    assert!(t.tab.busy_op.is_none());
    t.tab.work_pending = 99;
    t.key(Key::Space);
    assert!(t.tab.busy_op.is_none());
    t.tab.work_pending = 0;
    t.tab.work.unstaged.clear();
    t.tab.work.conflicted.push(crate::git::StatusFile {
        path: "conflict".to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    });
    t.pane(Key::Num3);
    t.key(Key::Space);
    assert!(t.tab.busy_op.is_none());
    t.tab.apply_keyboard(request, &t.ctx);
    assert!(t.tab.busy_op.is_none());
    // A confirmed discard from an older diff read cannot act on its replacement.
    t.tab.work = crate::git::status(&t.path)?;
    t.tab.work_read += 1;
    t.tab.apply(Action::OpenDiff(key.clone()), &t.ctx);
    t.wait()?;
    let Some(view) = &t.tab.diff else {
        return Err("no diff".to_owned());
    };
    let Some(Ok(data)) = &view.data else {
        return Err("no diff data".to_owned());
    };
    let hunk = view.keyboard_hunk.unwrap_or(0);
    let stale = Request {
        path: t.path.clone(),
        read: t.tab.work_read,
        epoch: t.tab.work_epoch,
        target: Target::Hunk {
            key,
            request: view.request + 1,
            pick: diff::Pick::Discard,
            lines: diff::hunk_changes(&data.lines, hunk),
            confirmed: true,
        },
    };
    t.tab.apply_keyboard(stale, &t.ctx);
    assert!(t.tab.busy_op.is_none());
    if let Some(view) = &mut t.tab.diff {
        view.next_request();
    }
    t.pane(Key::Num2);
    t.key(Key::S);
    assert!(
        t.tab.busy_op.is_none(),
        "retained data from a previous diff read must not be actionable"
    );
    Ok(())
}

#[test]
fn tree_navigation_reveals_collapsed_folders_scrolls_and_preserves_path_selection()
-> Result<(), String> {
    let mut t = Test::new("tree-scroll")?;
    std::fs::create_dir_all(t.path.join("nested/deep")).map_err(|e| e.to_string())?;
    for i in 0..80 {
        std::fs::write(t.path.join(format!("nested/deep/{i:03}.txt")), "new\n")
            .map_err(|e| e.to_string())?;
    }
    t.tab.reload_status(&t.ctx);
    t.wait()?;
    Settings {
        files_as_tree: true,
        ..Default::default()
    }
    .set(&t.ctx);
    let id = Id::new(("file-folder", t.tab.repo_key(), "unstaged"));
    t.ctx
        .data_mut(|d| d.insert_persisted(id.with("nested/deep"), true));
    t.pane(Key::Num3);
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "nested/deep/000.txt")
    );
    assert!(
        !t.ctx
            .data_mut(|d| d.get_persisted::<bool>(id.with("nested/deep")))
            .unwrap_or(false)
    );
    for _ in 0..45 {
        t.key(Key::J);
    }
    let shapes = t.frame(vec![]);
    let selected = t.tab.keyboard.borrow().cursor.clone();
    assert!(matches!(&selected, Some((_, DiffKey::Work(f))) if f.path == "nested/deep/045.txt"));
    let point = text_center(&shapes, "045.txt");
    assert!(
        point.is_some_and(|p| p.y > 80.0 && p.y < 600.0),
        "keyboard selection must scroll into the visible file area: point={point:?}, cursor={selected:?}"
    );
    t.tab.reload_status(&t.ctx);
    t.wait()?;
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "nested/deep/045.txt")
    );
    // A removed path clears the cursor instead of silently arming the row that takes its place.
    std::fs::remove_file(t.path.join("nested/deep/045.txt")).map_err(|e| e.to_string())?;
    t.tab.reload_status(&t.ctx);
    t.wait()?;
    assert!(t.tab.keyboard.borrow().cursor.is_none());
    t.key(Key::Space);
    assert!(t.tab.busy_op.is_none());
    Ok(())
}

#[test]
fn rename_reload_preserves_identity_and_invalidates_pre_reload_file_commands() -> Result<(), String>
{
    let mut t = Test::new("rename-owner")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    let selected = t.tab.keyboard.borrow().cursor.clone();
    let Some((scope, key)) = selected else {
        return Err("missing initial file cursor".to_owned());
    };
    let stale = Request {
        path: t.path.clone(),
        read: t.tab.work_read,
        epoch: t.tab.work_epoch,
        target: Target::File {
            scope,
            key,
            open: false,
        },
    };
    t.git(&["mv", "a.txt", "renamed.txt"])?;
    t.tab.reload_status(&t.ctx);
    t.wait()?;
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(file))) if file.path == "renamed.txt")
    );
    let before = t.git(&["diff", "--cached"])?;
    t.tab.apply_keyboard(stale, &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert_eq!(t.git(&["diff", "--cached"])?, before);
    Ok(())
}

#[test]
fn tabbing_to_a_file_updates_the_keyboard_target_before_space_stages() -> Result<(), String> {
    let mut t = Test::new("tab-file-owner")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    t.frame(vec![]);
    for _ in 0..40 {
        t.key(Key::Tab);
        if t.tab
            .keyboard
            .borrow()
            .cursor
            .as_ref()
            .is_some_and(|(_, key)| key.path() == "b.txt")
        {
            break;
        }
    }
    assert!(
        t.tab
            .keyboard
            .borrow()
            .cursor
            .as_ref()
            .is_some_and(|(_, key)| key.path() == "b.txt"),
        "Tab must follow real focus to the second file"
    );
    t.key(Key::Space);
    t.wait()?;
    assert_eq!(t.git(&["diff", "--cached", "--name-only"])?.trim(), "b.txt");
    Ok(())
}
