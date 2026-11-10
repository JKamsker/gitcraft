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

// Keep fixture helpers and shard-visible test names in this module.
include!("tests/mutations.rs");
include!("tests/focus_and_targets.rs");
include!("tests/async_completion.rs");
