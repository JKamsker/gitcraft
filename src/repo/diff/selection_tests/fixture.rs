//! Pointer harness and event delivery shared by selection regressions.
use super::*;

pub(super) struct State {
    pub(super) view: DiffView,
    pub(super) split: bool,
    pub(super) partial: bool,
    pub(super) installed: bool,
    pub(super) acts: Vec<Action>,
    pub(super) review_local: Option<bool>,
}

/// Build a real diff widget with deterministic fonts, keyboard bindings, and pointer timing.
pub(super) fn harness(
    split: bool,
    wrap: bool,
    partial: bool,
    new_file: bool,
) -> Harness<'static, State> {
    let text = if new_file {
        "@@ -0,0 +1,2 @@\n+new_αlpha beta\n+second value\n".to_owned()
    } else {
        format!(
            "@@ -1,4 +1,4 @@\n context value\n-old_αlpha beta\n+new_αlpha beta\n-old_second value\n+new_second value\n {} tail\n",
            "long_token ".repeat(50)
        )
    };
    let lines = super::super::super::parse(&text);
    let data = DiffData {
        split: super::super::super::parse::pair_lines(&lines).1,
        spans: vec![None; lines.len()],
        changed: vec![None; lines.len()],
        longest: lines
            .iter()
            .map(|l| l.text.chars().count())
            .max()
            .unwrap_or(0),
        lines,
        new_file,
        ..Default::default()
    };
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "recorded".into(),
        path: "file.txt".into(),
    });
    view.data = Some(Ok(data));
    let mut h = Harness::builder()
        .with_size([740.0, 380.0])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(30)
        .build_ui_state(
            move |ui, state: &mut State| {
                if !state.installed {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings {
                        diff_wrap: wrap,
                        ..Default::default()
                    };
                    settings.key_bindings.insert(
                        crate::keymap::Command::SelectionHistory.id().into(),
                        vec![crate::keymap::Stroke::new(egui::Key::F8, Modifiers::NONE)],
                    );
                    settings.put_for_test(ui.ctx());
                    state.installed = true;
                    return;
                }
                crate::keymap::active(ui.ctx(), vec![crate::keymap::Scope::Selection]);
                let Some(Ok(data)) = &state.view.data else {
                    return;
                };
                let review = state
                    .review_local
                    .map(|local| crate::repo::pr_review::DiffReview {
                        marks: Default::default(),
                        target: None,
                        path: "file.txt".into(),
                        local,
                    });
                let mut rows = Rows::new(
                    ui,
                    &state.view,
                    data,
                    review.as_ref(),
                    state.partial,
                    data.shows_split(state.split),
                );
                let sel = rows.show(ui, "file.txt", &mut state.acts);
                let (clicked, started) = (rows.clicked, rows.text_started);
                drop(rows);
                state.view.text_sel = sel;
                if started {
                    state.view.chosen.clear();
                    state.view.anchor = None;
                }
                if let Some(i) = clicked {
                    state.view.pick_line(i, ui.input(|i| i.modifiers.shift));
                }
            },
            State {
                view,
                split,
                partial,
                installed: false,
                acts: Vec::new(),
                review_local: None,
            },
        );
    h.run_steps(3);
    h
}

/// Locate a painted logical character so tests use the same galley geometry as the widget.
pub(super) fn position(h: &Harness<'_, State>, prefix: &str, char_index: usize) -> Pos2 {
    fn find(shape: &egui::Shape, prefix: &str, c: usize) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                let rect = t.galley.pos_from_cursor(egui::text::CCursor::new(c));
                Some(t.pos + rect.left_center().to_vec2() + egui::vec2(0.1, 0.0))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, prefix, c)),
            _ => None,
        }
    }
    h.output()
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, prefix, char_index))
        .unwrap_or_else(|| unreachable!("line {prefix:?} is not painted"))
}

/// Deliver one primary-button transition with the requested keyboard modifiers.
pub(super) fn button(h: &mut Harness<'_, State>, pos: Pos2, pressed: bool, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    });
    h.step();
}

/// Deliver a complete primary click, including hover and release frames.
pub(super) fn click(h: &mut Harness<'_, State>, pos: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerMoved(pos));
    h.step();
    button(h, pos, true, modifiers);
    button(h, pos, false, modifiers);
}

/// Read the selected diff text through the production copy path.
pub(super) fn selected(h: &Harness<'_, State>) -> String {
    let Some(Ok(data)) = &h.state().view.data else {
        return String::new();
    };
    h.state()
        .view
        .text_sel
        .map(|s| s.text(&data.lines, &data.uncut))
        .unwrap_or_default()
}
