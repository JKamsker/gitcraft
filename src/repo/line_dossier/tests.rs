use super::*;
use egui::{Event, Pos2, Rect, Shape, vec2};
use std::time::{Duration, Instant};
fn label(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
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
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| self.tab.ui(ui),
        );
        out.textures_delta.clear();
        out.shapes
    }
    fn click(&mut self, name: &str) -> Result<(), String> {
        let shapes = self.frame(vec![]);
        let p = label(&shapes, name).ok_or_else(|| format!("Missing dossier control {name}"))?;
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
        Ok(())
    }
    fn wait(&mut self) -> Result<(), String> {
        let start = Instant::now();
        loop {
            self.frame(vec![]);
            if self.tab.dossier.result.is_some() && !self.tab.dossier.reader.busy() {
                return Ok(());
            }
            if let Some(e) = &self.tab.dossier.error {
                return Err(e.clone());
            }
            if start.elapsed() > Duration::from_secs(8) {
                return Err("Dossier did not finish".into());
            }
            std::thread::yield_now();
        }
    }
}
#[test]
fn actual_dossier_controls_trace_versions_open_the_pinned_file_and_escape_closes()
-> Result<(), String> {
    let r = crate::git::investigations::tests::Repo::new()?;
    for (i, text) in [
        "before\nlet answer=1;\nafter\n",
        "before\nlet answer=2;\nafter\n",
    ]
    .into_iter()
    .enumerate()
    {
        std::fs::write(r.0.join("f.rs"), text).map_err(|e| e.to_string())?;
        r.git(&["add", "f.rs"])?;
        r.commit("line@test.invalid", 1_700_000_000 + i as i64)?;
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.deferred = false;
    tab.history_gen = 1;
    tab.refs = crate::git::refs(&r.0)?;
    tab.history = crate::git::log(&r.0, None, 0, 10)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.commits = tab.history.clone();
    tab.graph = crate::graph::layout(&tab.commits);
    let head = tab.refs.head.clone();
    let seed = Seed {
        commit: head.clone(),
        path: "f.rs".into(),
        line: 2,
    };
    tab.apply(Action::OpenLineDossier(Some(seed.clone())), &ctx);
    let mut ui = UI {
        ctx,
        tab,
        time: 0.0,
    };
    ui.wait()?;
    assert_eq!(
        ui.tab.dossier.result.as_ref().map(|r| r.cards.len()),
        Some(2)
    );
    ui.click("Open file at this version")?;
    assert!(!ui.tab.dossier.open);
    assert!(
        ui.tab
            .file_view
            .as_ref()
            .is_some_and(|v| v.path == "f.rs" && v.rev.as_deref() == Some(head.as_str()))
    );
    ui.tab.apply(Action::OpenLineDossier(Some(seed)), &ui.ctx);
    ui.wait()?;
    ui.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }]);
    assert!(!ui.tab.dossier.open);
    assert!(ui.tab.dossier.result.is_none());
    assert_eq!(r.git(&["status", "--porcelain"])?, "");
    Ok(())
}
#[test]
fn closing_dossier_cancels_live_read_and_reopening_cannot_publish_its_old_answer()
-> Result<(), String> {
    use std::sync::mpsc;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/dossier".into(), &ctx);
    tab.dossier.open = true;
    let (entered, entry) = mpsc::channel();
    let (stopped, stop) = mpsc::channel();
    tab.dossier.reader.queue(move |active| {
        entered.send(()).map_err(|e| e.to_string())?;
        let start = Instant::now();
        while active() {
            if start.elapsed() > Duration::from_secs(5) {
                return Err("Dossier cancellation not observed".into());
            }
            std::thread::yield_now();
        }
        stopped.send(()).map_err(|e| e.to_string())?;
        Err("old private error".into())
    });
    tab.poll_line_dossier(&ctx);
    entry
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    tab.apply(Action::CloseLineDossier, &ctx);
    tab.apply(Action::OpenLineDossier(None), &ctx);
    stop.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    while tab.dossier.reader.busy() {
        tab.poll_line_dossier(&ctx);
        if start.elapsed() > Duration::from_secs(5) {
            return Err("Dossier worker did not release slot".into());
        }
        std::thread::yield_now();
    }
    assert!(tab.dossier.error.is_none());
    assert!(tab.dossier.result.is_none());
    assert!(tab.dossier.open);
    Ok(())
}

#[test]
fn historical_file_and_shifted_working_blame_menus_trace_the_exact_committed_line()
-> Result<(), String> {
    let r = crate::git::investigations::tests::Repo::new()?;
    for (i, text) in [
        "before\nlet answer=1;\nafter\n",
        "before\nlet answer=2;\nafter\n",
    ]
    .into_iter()
    .enumerate()
    {
        std::fs::write(r.0.join("f.rs"), text).map_err(|e| e.to_string())?;
        r.git(&["add", "f.rs"])?;
        r.commit("line@test.invalid", 1_700_000_000 + i as i64)?;
    }
    std::fs::write(
        r.0.join("f.rs"),
        "inserted working line\nbefore\nlet answer=2;\nafter\n",
    )
    .map_err(|e| e.to_string())?;
    let before = std::fs::read(r.0.join("f.rs")).map_err(|e| e.to_string())?;
    let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.deferred = false;
    tab.history_gen = 1;
    tab.refs = crate::git::refs(&r.0)?;
    tab.history = crate::git::log(&r.0, None, 0, 10)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.commits = tab.history.clone();
    tab.graph = crate::graph::layout(&tab.commits);
    let head = tab.refs.head.clone();
    let mut ui = UI {
        ctx,
        tab,
        time: 0.0,
    };
    for working_blame in [false, true] {
        ui.tab.apply(
            Action::FileView {
                path: "f.rs".into(),
                rev: if working_blame {
                    None
                } else {
                    Some(head.clone())
                },
                tab: if working_blame {
                    super::super::file_view::FileTab::Blame
                } else {
                    super::super::file_view::FileTab::File
                },
            },
            &ui.ctx,
        );
        let began = Instant::now();
        loop {
            ui.frame(vec![]);
            let ready = ui.tab.file_view.as_ref().is_some_and(|v| {
                if working_blame {
                    v.blame.is_some()
                } else {
                    v.text.is_some()
                }
            });
            if ready {
                break;
            }
            if began.elapsed() > Duration::from_secs(8) {
                return Err("File did not load".into());
            }
            std::thread::yield_now();
        }
        let shapes = ui.frame(vec![]);
        let p =
            label(&shapes, "let answer=2;").ok_or("Selected file/blame code was not painted")?;
        for pressed in [true, false] {
            ui.frame(vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
        }
        ui.click("Explain history")?;
        ui.wait()?;
        let result = ui.tab.dossier.result.as_ref().ok_or("No dossier")?;
        assert_eq!(result.start.commit, head);
        assert_eq!(result.start.path, "f.rs");
        assert_eq!(
            result.start.line, 2,
            "Working insertion must not become HEAD line3"
        );
        assert_eq!(result.cards[0].text, "let answer=2;");
        ui.tab.apply(Action::CloseLineDossier, &ui.ctx);
    }
    assert_eq!(
        before,
        std::fs::read(r.0.join("f.rs")).map_err(|e| e.to_string())?
    );
    assert_eq!(
        index,
        std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[test]
fn ignore_flow_requires_review_and_confirmation_and_preserves_staged_state() -> Result<(), String> {
    let r = crate::git::investigations::tests::Repo::new()?;
    for (i, text) in ["fn f() { return 1; }\n", "fn f(){return 1;}\n"]
        .into_iter()
        .enumerate()
    {
        std::fs::write(r.0.join("f.rs"), text).map_err(|e| e.to_string())?;
        r.git(&["add", "f.rs"])?;
        r.commit("ignore@test.invalid", 1_700_000_000 + i as i64)?;
    }
    let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
    let refs = r.git(&["show-ref"])?;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred(r.0.clone(), &ctx);
    tab.deferred = false;
    tab.history_gen = 1;
    tab.refs = crate::git::refs(&r.0)?;
    tab.history = crate::git::log(&r.0, None, 0, 10)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.commits = tab.history.clone();
    tab.graph = crate::graph::layout(&tab.commits);
    let seed = Seed {
        commit: tab.refs.head.clone(),
        path: "f.rs".into(),
        line: 1,
    };
    tab.apply(Action::OpenLineDossier(Some(seed)), &ctx);
    let mut ui = UI {
        ctx,
        tab,
        time: 0.0,
    };
    ui.wait()?;
    ui.click("Review formatting ignore…")?;
    let start = Instant::now();
    loop {
        ui.frame(vec![]);
        if ui.tab.dossier.ignore.is_some() {
            break;
        }
        if let Some(error) = &ui.tab.dossier.error {
            return Err(error.clone());
        }
        if start.elapsed() > Duration::from_secs(8) {
            return Err("Ignore preview did not load".into());
        }
        std::thread::yield_now();
    }
    assert!(!r.0.join(".git-blame-ignore-revs").exists());
    ui.click("Append reviewed revision")?;
    assert!(
        !r.0.join(".git-blame-ignore-revs").exists(),
        "Unchecked confirmation must not write"
    );
    ui.click("I reviewed the full patch and want this revision ignored")?;
    ui.click("Append reviewed revision")?;
    let start = Instant::now();
    while ui.tab.busy_op.is_some() {
        ui.frame(vec![]);
        if start.elapsed() > Duration::from_secs(8) {
            return Err("Confirmed append did not finish".into());
        }
        std::thread::yield_now();
    }
    let bytes = std::fs::read(r.0.join(".git-blame-ignore-revs")).map_err(|e| e.to_string())?;
    assert!(String::from_utf8_lossy(&bytes).contains(&ui.tab.refs.head));
    assert_eq!(
        index,
        std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?
    );
    assert_eq!(refs, r.git(&["show-ref"])?);
    Ok(())
}
