//! Real keyboard frames and Git repositories: exercise the normal view/action/worker path.
use super::*;
use crate::theme::Tokens;
use egui::{Event, Modifiers, Pos2, Shape, Vec2, vec2};
use std::time::{Duration, Instant};

struct Test {
    ctx: egui::Context,
    tab: RepoTab,
    path: PathBuf,
    time: f64,
}

impl Drop for Test {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Test {
    fn new(name: &str) -> Result<Self, String> {
        let path =
            std::env::temp_dir().join(format!("gitcraft-keyboard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "t"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            crate::git::run(Some(&path), &args)?;
        }
        let original = (0..80)
            .map(|i| format!("original {i}\n"))
            .collect::<String>();
        std::fs::write(path.join("a.txt"), &original).map_err(|e| e.to_string())?;
        std::fs::write(path.join("b.txt"), "before\n").map_err(|e| e.to_string())?;
        crate::git::run(Some(&path), &["add", "-A"])?;
        crate::git::run(Some(&path), &["commit", "-qm", "base"])?;
        std::fs::write(
            path.join("a.txt"),
            original
                .replace("original 5\n", "first edit\n")
                .replace("original 65\n", "second edit\n"),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(path.join("b.txt"), "after\n").map_err(|e| e.to_string())?;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        Settings {
            files_as_tree: false,
            ..Default::default()
        }
        .set(&ctx);
        let mut tab = RepoTab::new_deferred(path.clone(), &ctx);
        tab.start_deferred(&ctx);
        let deadline = Instant::now() + Duration::from_secs(10);
        while tab.opening() || tab.loading || tab.watcher.is_none() || tab.work_pending != 0 {
            tab.poll(&ctx);
            if Instant::now() >= deadline {
                return Err(format!("repository did not load: {:?}", tab.load_error));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        // Explicit reload tests own their timing; filesystem notifications otherwise race the
        // simulated keyboard clock. Normal operation workers and status/diff refresh remain real.
        tab.watcher = None;
        tab.poller = None;
        tab.fs_rx = None;
        tab.fs_pending = None;
        tab.recheck_after_load = false;
        Ok(Self {
            ctx,
            tab,
            path,
            time: 0.0,
        })
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        crate::git::run(Some(&self.path), args)
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<egui::epaint::ClippedShape> {
        self.time += 0.05;
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                events,
                time: Some(self.time),
                ..Default::default()
            },
            |ui| {
                self.tab.ui(ui);
                crate::a11y::end_frame(ui.ctx());
            },
        );
        out.textures_delta.clear();
        std::mem::take(&mut out.shapes)
    }

    fn key(&mut self, key: Key) {
        self.frame(keys(key, Modifiers::NONE, false));
    }
    fn pane(&mut self, key: Key) {
        self.frame(keys(key, Modifiers::COMMAND | Modifiers::ALT, false));
    }

    fn wait(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.tab.poll(&self.ctx);
            let ready_diff = self
                .tab
                .diff
                .as_ref()
                .is_none_or(|v| v.data.is_some() && v.data_request == v.request);
            if self.tab.busy_op.is_none()
                && !self.tab.loading
                && self.tab.work_pending == 0
                && ready_diff
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "worker did not settle: busy={:?} status={:?}",
                    self.tab.busy_op, self.tab.status
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.frame(vec![]);
        Ok(())
    }

    fn click(&mut self, pos: Pos2, button: egui::PointerButton) {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![
            Event::PointerButton {
                pos,
                button,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]);
    }
}

fn keys(key: Key, modifiers: Modifiers, repeat: bool) -> Vec<Event> {
    vec![
        Event::ModifiersChanged(modifiers),
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        },
        Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        },
        Event::ModifiersChanged(Modifiers::NONE),
    ]
}

fn text_center(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<Pos2> {
    fn find(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(text) if text.galley.job.text == label => {
                Some(text.visual_bounding_rect().center())
            }
            Shape::Vec(shapes) => shapes.iter().rev().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    shapes
        .iter()
        .rev()
        .find_map(|shape| find(&shape.shape, label))
}

#[test]
fn pane_keys_file_navigation_stage_unstage_and_open_use_the_normal_git_workers()
-> Result<(), String> {
    let mut t = Test::new("file-actions")?;
    t.frame(vec![]);
    t.pane(Key::Num1);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Sidebar));
    let shapes = t.frame(vec![]);
    let accent = Tokens::get(&t.ctx).accent;
    assert!(
        shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r)
        if r.stroke.width == 2.0 && r.stroke.color == accent && r.rect.left() < 5.0
            && r.rect.width() > 170.0 && r.rect.width() < 520.0)),
        "sidebar focus must have a visible outline"
    );
    t.pane(Key::Num2);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Graph));
    t.pane(Key::Num3);
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Detail));
    assert!(
        t.tab
            .keyboard_file_selected(&DiffKey::Work(t.tab.work.unstaged[0].clone()))
    );
    t.key(Key::J);
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "b.txt")
    );
    t.key(Key::K);
    t.frame(vec![Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    t.wait()?;
    assert_eq!(t.git(&["diff", "--cached", "--name-only"])?.trim(), "a.txt");
    assert!(
        matches!(&t.tab.keyboard.borrow().cursor, Some((_, DiffKey::Work(f))) if f.path == "a.txt" && f.area == Area::Staged)
    );
    t.frame(keys(Key::Space, Modifiers::NONE, true));
    t.wait()?;
    assert_eq!(
        t.git(&["diff", "--cached", "--name-only"])?.trim(),
        "a.txt",
        "held Space must not unstage it again"
    );
    t.key(Key::Space);
    t.wait()?;
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        matches!(&t.tab.diff, Some(view) if matches!(&view.key, DiffKey::Work(f) if f.path == "a.txt"))
    );
    assert_eq!(t.tab.keyboard.borrow().pane, Some(Pane::Graph));
    t.pane(Key::Num3);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        t.tab.diff.is_some(),
        "Enter focuses an already open diff rather than toggling it closed"
    );
    Ok(())
}

#[test]
fn current_hunk_keys_stage_unstage_and_confirm_discard_with_undo() -> Result<(), String> {
    let mut t = Test::new("hunk-actions")?;
    t.frame(vec![]);
    t.pane(Key::Num3);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        t.tab
            .diff
            .as_ref()
            .is_some_and(|v| v.keyboard_hunk.is_some())
    );
    t.frame(vec![Event::Key {
        key: Key::S,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    t.wait()?;
    let staged = t.git(&["diff", "--cached"])?;
    assert!(staged.contains("first edit") && !staged.contains("second edit"));
    let unchanged = t.git(&["diff"])?;
    t.frame(keys(Key::S, Modifiers::NONE, true));
    t.wait()?;
    assert!(t.tab.dialog.is_none());
    assert_eq!(
        t.git(&["diff"])?,
        unchanged,
        "held S must not stage the next hunk"
    );
    t.key(Key::D);
    assert!(matches!(t.tab.dialog, Some(Dialog::KeyboardDiscard { .. })));
    assert_eq!(
        t.git(&["diff"])?,
        unchanged,
        "D must not discard before confirmation"
    );
    t.frame(vec![]); // Let the newly measured modal settle at its centered position.
    let shapes = t.frame(vec![]);
    let point = text_center(&shapes, "Discard hunk");
    assert!(point.is_some());
    if let Some(point) = point {
        t.frame(vec![Event::PointerMoved(point)]);
        t.frame(vec![Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        t.frame(vec![Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }
    t.wait()?;
    assert!(
        !t.git(&["diff"])?.contains("second edit"),
        "discard state: status={:?}, dialog={}, busy={:?}, work_read={}, pending={}, diff_request={:?}",
        t.tab.status,
        t.tab.dialog.is_some(),
        t.tab.busy_op,
        t.tab.work_read,
        t.tab.work_pending,
        t.tab.diff.as_ref().map(|v| v.request)
    );
    assert!(
        t.tab
            .timeline
            .current_entry()
            .is_some_and(|e| e.undo.is_some())
    );
    t.tab.apply(Action::Undo, &t.ctx);
    t.wait()?;
    assert!(t.git(&["diff"])?.contains("second edit"));
    t.tab.apply(Action::CloseDiff, &t.ctx);
    t.pane(Key::Num3);
    // Move from unstaged a to b, then to staged a, without staging b.
    t.key(Key::J);
    t.key(Key::J);
    t.key(Key::Enter);
    t.wait()?;
    assert!(
        matches!(&t.tab.diff, Some(v) if matches!(&v.key, DiffKey::Work(f) if f.area == Area::Staged))
    );
    t.key(Key::U);
    t.wait()?;
    assert!(
        t.git(&["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
    );
    Ok(())
}

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
        t.tab.start_terminal(
            Some(crate::terminal::ShellSpec {
                label: "test".to_owned(),
                program: "sh".to_owned(),
                args: vec![],
            }),
            None,
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

fn delay_real_hunk_preparation(
    t: &mut Test,
) -> Result<(String, crate::repo::apply::PreparedPick), String> {
    t.frame(vec![]);
    t.pane(Key::Num3);
    t.key(Key::Enter);
    t.wait()?;
    let view = t.tab.diff.as_ref().ok_or("No working diff")?;
    let data = view
        .data
        .as_ref()
        .ok_or("No diff data")?
        .as_ref()
        .map_err(Clone::clone)?;
    let hunk = view.keyboard_hunk.ok_or("No current hunk")?;
    let lines = diff::hunk_changes(&data.lines, hunk);
    // This is the production start/completion pair used by the mouse and keyboard
    // here_or_later callback; only delivery of its saved result is held by the test.
    let (patch, prepared) = t
        .tab
        .prepare_picked(diff::Pick::Stage, &lines)
        .ok_or("No patch preparation")?;
    let saved = crate::git::save_patch_for(&t.path, &patch)?;
    t.git(&["checkout", "-qb", "other"])?;
    t.git(&["commit", "--allow-empty", "-qm", "Other branch identity"])?;
    t.git(&["apply", "--cached", "--check", &saved])?;
    assert!(
        t.git(&["diff", "--cached"])?.is_empty(),
        "the other branch starts with a clean index"
    );
    Ok((saved, prepared))
}

#[test]
fn delayed_patch_completion_cannot_apply_during_a_real_full_reload() -> Result<(), String> {
    let mut t = Test::new("callback-loading")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    let read = t.tab.work_read;
    let epoch = t.tab.work_epoch;
    t.tab.reload(&t.ctx);
    assert!(t.tab.loading);
    assert_eq!(
        t.tab.work_read, read,
        "full reads retain the displayed status while pending"
    );
    assert_eq!(t.tab.work_pending, 0);
    assert_ne!(
        t.tab.work_epoch, epoch,
        "ownership must expire at full reload start"
    );
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(
        t.git(&["diff", "--cached"])?.is_empty(),
        "a retained main-branch patch must never enter the other branch's index"
    );
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
    t.wait()?;
    assert_eq!(t.git(&["branch", "--show-current"])?.trim(), "other");
    assert!(t.git(&["diff", "--cached"])?.is_empty());
    Ok(())
}

#[test]
fn delayed_patch_completion_cannot_apply_after_an_actual_failed_full_reload() -> Result<(), String>
{
    let mut t = Test::new("callback-read-error")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    let read = t.tab.work_read;
    let epoch = t.tab.work_epoch;
    // Fail the real reload worker with invalid config while retaining the same repository,
    // then restore it before delivering the saved patch. Git could apply it successfully.
    let config = t.path.join(".git/config");
    let original_config = std::fs::read(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config, "[incomplete\n").map_err(|e| e.to_string())?;
    t.tab.reload(&t.ctx);
    let deadline = Instant::now() + Duration::from_secs(10);
    while t.tab.loading {
        t.tab.poll(&t.ctx);
        if Instant::now() >= deadline {
            let _ = std::fs::write(&config, &original_config);
            return Err("The expected failed reload did not finish".to_owned());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::write(&config, &original_config).map_err(|e| e.to_string())?;
    assert!(t.tab.load_error.is_some());
    assert_eq!(
        t.tab.work_read, read,
        "a failed full read retains the old status"
    );
    assert_eq!(t.tab.work_pending, 0);
    assert_ne!(t.tab.work_epoch, epoch);
    t.git(&["apply", "--cached", "--check", &saved])?;
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(
        t.git(&["diff", "--cached"])?.is_empty(),
        "failed reloads must not revive retained patch ownership"
    );
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
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

#[test]
fn session_transfer_rejects_delayed_patch_completion_without_changing_the_index()
-> Result<(), String> {
    let mut t = Test::new("transfer-callback")?;
    let (saved, prepared) = delay_real_hunk_preparation(&mut t)?;
    t.tab.cancel_keyboard_ownership(&t.ctx);
    t.tab.picked_prepared(prepared, Ok(saved), &t.ctx);
    assert!(t.tab.busy_op.is_none());
    assert!(t.git(&["diff", "--cached"])?.is_empty());
    assert!(
        matches!(t.tab.status.as_deref(), Some((message, true)) if message.contains("Changes reloaded"))
    );
    Ok(())
}
