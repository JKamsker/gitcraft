use super::*;
use egui::{Event, Pos2, Rect, Shape, vec2};
fn text(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
    fn find(shape: &Shape, needle: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.job.text == needle => Some(t.pos + t.galley.size() / 2.0),
            Shape::Vec(v) => v.iter().find_map(|s| find(s, needle)),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| find(&s.shape, needle))
}
struct UI {
    ctx: egui::Context,
    tab: RepoTab,
    time: f64,
}
impl UI {
    fn frame(&mut self, events: Vec<Event>) -> Vec<egui::epaint::ClippedShape> {
        self.time += 0.05;
        let mut actions = Vec::new();
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 700.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| self.tab.graph_view(ui, &mut actions));
            },
        );
        for a in actions {
            if let super::super::Action::Select(i) = a {
                self.tab.selected = Some(i);
            }
        }
        out.textures_delta.clear();
        out.shapes
    }
    fn click(&mut self, p: Pos2) {
        for pressed in [true, false] {
            self.frame(vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
    }
    fn label(&mut self, label: &str) -> Result<Pos2, String> {
        let mut shapes = self.frame(vec![]);
        for _ in 0..5 {
            if let Some(p) = text(&shapes, label) {
                return Ok(p);
            }
            shapes = self.frame(vec![]);
        }
        Err(format!("Missing painted label: {label}"))
    }
    fn wait(&mut self) -> Result<(), String> {
        let start = std::time::Instant::now();
        loop {
            self.frame(vec![]);
            if self.tab.saved_views.selection.is_some() && !self.tab.saved_views.reader.busy() {
                return Ok(());
            }
            if let Some(error) = &self.tab.saved_views.error {
                return Err(error.clone());
            }
            if start.elapsed() > std::time::Duration::from_secs(5) {
                return Err("View did not finish".into());
            }
            std::thread::yield_now();
        }
    }
}
#[test]
fn pointer_select_save_restart_expand_and_return_to_all_history() -> Result<(), String> {
    let r = git::investigations::tests::Repo::new()?;
    for i in 0..12 {
        r.commit(
            if i == 11 {
                "me[exact]@test.invalid"
            } else {
                "someone@test.invalid"
            },
            1_700_000_000 + i,
        )?;
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.deferred = false;
    tab.history = git::log(&r.0, None, 0, 100)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.commits = tab.history.clone();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.refs = git::refs(&r.0)?;
    tab.identity.email = "me[exact]@test.invalid".into();
    tab.selected = Some(0);
    let mut ui = UI {
        ctx,
        tab,
        time: 0.0,
    };
    let pos = ui.label("All history")?;
    ui.click(pos);
    let pos = ui.label("My unpushed work")?;
    ui.click(pos);
    ui.wait()?;
    assert_eq!(
        ui.tab
            .saved_views
            .selection
            .as_ref()
            .map(|s| s.commits.len()),
        Some(1)
    );
    assert_eq!(ui.tab.saved_views.len(), 2);
    let pos = ui.label("⋯ 11 intervening commits · expand")?;
    ui.click(pos);
    ui.frame(vec![]);
    assert_eq!(ui.tab.saved_views.len(), 12);
    let pos = ui.label("Collapse spans")?;
    ui.click(pos);
    ui.frame(vec![]);
    assert_eq!(ui.tab.saved_views.len(), 2);
    let pos = ui.label("Name this view")?;
    ui.click(pos);
    ui.frame(vec![Event::Text("My saved relation".into())]);
    let pos = ui.label("Save view")?;
    ui.click(pos);
    assert_eq!(ui.tab.saved_views.prefs.saved.len(), 1);
    let stored = ui
        .ctx
        .data_mut(|d| d.get_persisted::<Preferences>(preferences_id(&r.0)))
        .ok_or("Missing persisted views")?;
    assert_eq!(stored.saved[0].name, "My saved relation");
    let serialized = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    let restored: Preferences = serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
    assert_eq!(
        restored.normalize().active.map(|p| p.kind),
        Some(Kind::MyUnpushed)
    );
    let pos = ui.label("My saved relation")?;
    ui.click(pos);
    let pos = ui.label("All history")?;
    ui.click(pos);
    assert!(!ui.tab.saved_views.active());
    assert_eq!(ui.tab.commits.len(), 12);
    assert_eq!(git::run(Some(&r.0), &["status", "--porcelain"])?, "");
    Ok(())
}
#[test]
fn persisted_unknown_versions_bad_presets_and_saved_counts_fail_closed() {
    let unknown = Preferences {
        version: 99,
        saved: vec![],
        active: Some(Preset {
            name: "View".into(),
            kind: Kind::MyUnpushed,
            days: 30,
        }),
    };
    assert!(unknown.normalize().active.is_none());
    let bad = Preset {
        name: "bad\nname".into(),
        kind: Kind::MyUnpushed,
        days: 30,
    };
    let p = Preferences {
        version: 1,
        saved: vec![bad.clone(); 80],
        active: Some(bad),
    }
    .normalize();
    assert!(p.saved.is_empty());
    assert!(p.active.is_none());
}

#[test]
fn actual_view_exit_park_and_drop_cancel_owned_work_and_ignore_stale_answers() -> Result<(), String>
{
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    for mode in 0..3 {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recording-only/saved-view-lifecycle".into(), &ctx);
        tab.saved_views.prefs.active = Some(Preset {
            name: "View".into(),
            kind: Kind::MyUnpushed,
            days: 30,
        });
        tab.saved_views.scope = Some(Scope {
            tips: vec![],
            remotes: vec![],
            email: "recording@test.invalid".into(),
            default_branch: None,
            now: 0,
        });
        let (entered, entry) = mpsc::channel();
        let (stopped, stop) = mpsc::channel();
        tab.saved_views.reader.queue(move |active| {
            entered.send(()).map_err(|e| e.to_string())?;
            let start = Instant::now();
            while active() {
                if start.elapsed() > Duration::from_secs(5) {
                    return Err("Cancellation was not observed".into());
                }
                std::thread::yield_now();
            }
            stopped.send(()).map_err(|e| e.to_string())?;
            Ok(Answer::Selection(Selection {
                commits: BTreeSet::from(["f".repeat(40)]),
                ..Default::default()
            }))
        });
        tab.poll_saved_views(&ctx);
        entry
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        match mode {
            0 => tab.set_graph_view(None, &ctx),
            1 => {
                tab.shown = false;
                tab.poll_saved_views(&ctx);
            }
            _ => {
                drop(tab);
                stop.recv_timeout(Duration::from_secs(5))
                    .map_err(|e| e.to_string())?;
                continue;
            }
        }
        stop.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        tab.shown = true;
        let start = Instant::now();
        while tab.saved_views.reader.busy() {
            tab.poll_saved_views(&ctx);
            if start.elapsed() > Duration::from_secs(5) {
                return Err("Canceled reader slot did not clear".into());
            }
            std::thread::yield_now();
        }
        assert!(tab.saved_views.selection.is_none());
    }
    Ok(())
}
