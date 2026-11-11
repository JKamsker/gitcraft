//! The picture comparison: the mode switch, zoom, what changed, the pictures in the mode
//! picked over a checkerboard, and the strip of the picture's history.

use std::sync::Arc;

use egui::{Color32, ColorImage, CursorIcon, Rect, RichText, Sense, TextureHandle, Ui, pos2, vec2};

use super::Compare;
use super::decode::{Decoded, Difference};
use super::filmstrip::{THUMB, Thumb, Thumbs};
use super::view_state::{Mode, ViewState, left_of};
use crate::git::{human_size, relative_time};
use crate::repo::{Action, DiffKey, widgets};
use crate::theme::{self, Tokens};

/// How near the swipe line (screen points) a drag moves it rather than the picture.
const LINE_GRAB: f32 = 8.0;

/// The comparison of a picture's two versions.
pub(in crate::repo::diff) fn compare_view(ui: &mut Ui, cmp: &Compare, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let key = egui::Id::new(("image-compare", &cmp.repo, &cmp.path));
    let mut state: ViewState = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    let both = cmp.old.is_some() && cmp.new.is_some();
    // With one side only, 2-up, without forgetting the mode picked.
    let mode = if both { state.mode } else { Mode::TwoUp };
    let ppp = ui.ctx().pixels_per_point();
    let diff = cmp.difference(ui.ctx());
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(10.0);
        if both {
            let labels: Vec<&str> = Mode::ALL.iter().map(|m| m.label()).collect();
            let on = Mode::ALL.iter().position(|m| *m == state.mode).unwrap_or(0);
            if let Some(i) = widgets::switch(ui, &labels, on) {
                state.mode = Mode::ALL[i];
            }
            ui.add_space(12.0);
        }
        if ui
            .button(crate::i18n::text("repo-core-1807"))
            .on_hover_text(crate::i18n::text("repo-core-1808"))
            .clicked()
        {
            state.zoom = None;
        }
        if ui
            .button("100%")
            .on_hover_text(crate::i18n::text("repo-core-1809"))
            .clicked()
        {
            state.zoom = Some(1.0 / ppp);
            state.center = vec2(0.5, 0.5);
        }
        ui.label(RichText::new(state.zoom_label(ppp)).color(t.text_dim));
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(10.0);
        ui.label(RichText::new(changes(cmp, diff.as_deref())).color(t.text_dim));
    });
    match mode {
        Mode::Swipe => slider(
            ui,
            (
                crate::i18n::text("repo-core-1810"),
                crate::i18n::text("repo-core-1811"),
            ),
            &mut state.swipe,
            Some(crate::i18n::text("repo-core-1812")),
        ),
        Mode::Onion => slider(
            ui,
            (
                crate::i18n::text("repo-core-1813"),
                crate::i18n::text("repo-core-1814"),
            ),
            &mut state.onion,
            None,
        ),
        _ => {}
    }
    let history = cmp.versions(ui.ctx());
    let strip = if history.as_ref().is_some_and(|h| h.len() > 1) {
        THUMB as f32 + 40.0
    } else {
        0.0
    };
    let height = (ui.available_height() - strip - 8.0).max(120.0);
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
    let area = area.shrink(8.0);
    let canvas = canvas(cmp);
    let ctx = ui.ctx().clone();
    let tex = |name: &str, image: &Arc<ColorImage>| texture(&ctx, cmp, name, image);
    let old = cmp.old.as_ref().map(|d| (tex("old", &d.image), d));
    let new = cmp.new.as_ref().map(|d| (tex("new", &d.image), d));
    let checker = checker_texture(&ctx, cmp, &t);
    let painter = ui.painter_at(area);
    // Every mode lays the pictures out on the same canvas, top-left aligned, so zoom and pan
    // mean the same in all of them and on both sides of 2-up.
    let at = |placed: Rect, d: &Decoded| {
        Rect::from_min_size(
            placed.min,
            size_of(d) * (placed.width() / canvas.x.max(1.0)),
        )
    };
    let mut swipe_line = None;
    match mode {
        Mode::TwoUp => {
            let gap = 12.0;
            let half = vec2((area.width() - gap) / 2.0, area.height());
            let left = Rect::from_min_size(area.min, half);
            let right = Rect::from_min_size(pos2(left.right() + gap, area.top()), half);
            for (rect, side, missing) in [
                (left, &old, crate::i18n::text("repo-core-1695")),
                (right, &new, crate::i18n::text("repo-core-1696")),
            ] {
                let p = ui.painter_at(rect);
                match side {
                    Some((tex, d)) => {
                        let shown = at(state.placed(canvas, rect), d);
                        checkerboard(&p, &checker, shown);
                        p.image(tex.id(), shown, uv_all(), Color32::WHITE);
                    }
                    None => {
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            missing,
                            theme::medium(13.0),
                            t.text_faint,
                        );
                    }
                }
            }
            input(ui, &resp, &mut state, canvas, (area, Some(left)), None);
        }
        Mode::Swipe | Mode::Onion | Mode::Difference => {
            let placed = state.placed(canvas, area);
            if let (Some((otex, od)), Some((ntex, nd))) = (&old, &new) {
                let (orect, nrect) = (at(placed, od), at(placed, nd));
                checkerboard(&painter, &checker, orect.union(nrect));
                match mode {
                    Mode::Swipe => {
                        let split = placed.left() + placed.width() * state.swipe;
                        let (screen, uv) = left_of(orect, split);
                        painter.image(otex.id(), screen, uv, Color32::WHITE);
                        let (left, luv) = left_of(nrect, split);
                        let right = Rect::from_min_max(pos2(left.right(), nrect.top()), nrect.max);
                        let ruv = Rect::from_min_max(pos2(luv.max.x, 0.0), pos2(1.0, 1.0));
                        painter.image(ntex.id(), right, ruv, Color32::WHITE);
                        let x = split.clamp(area.left(), area.right());
                        painter.line_segment(
                            [pos2(x, area.top()), pos2(x, area.bottom())],
                            egui::Stroke::new(2.0, t.accent),
                        );
                        swipe_line = Some((x, placed));
                    }
                    Mode::Onion => {
                        painter.image(otex.id(), orect, uv_all(), Color32::WHITE);
                        let alpha = (state.onion * 255.0).round() as u8;
                        painter.image(ntex.id(), nrect, uv_all(), Color32::from_white_alpha(alpha));
                    }
                    _ => {
                        painter.image(otex.id(), orect, uv_all(), Color32::from_white_alpha(60));
                        match &diff {
                            Some(d) => {
                                let dtex = tex("diff", &d.image);
                                painter.image(dtex.id(), placed, uv_all(), Color32::WHITE);
                            }
                            None => {
                                painter.text(
                                    area.center(),
                                    egui::Align2::CENTER_CENTER,
                                    crate::i18n::text("repo-core-1817"),
                                    theme::medium(13.0),
                                    t.text_dim,
                                );
                            }
                        }
                    }
                }
            }
            input(ui, &resp, &mut state, canvas, (area, None), swipe_line);
        }
    }
    let pane = if mode == Mode::TwoUp {
        Rect::from_min_size(area.min, vec2((area.width() - 12.0) / 2.0, area.height()))
    } else {
        area
    };
    let zoom = state.scale(canvas, pane.size());
    let center = if state.zoom.is_some() {
        state.center
    } else {
        vec2(0.5, 0.5)
    };
    if let Some(percent) = crate::a11y::value::slider(
        &resp,
        || {
            crate::i18n::message(
                "repo-core-1818",
                &[
                    ("path", cmp.path.to_string().into()),
                    ("x", (center.x * 100.0).into()),
                    ("y", (center.y * 100.0).into()),
                ],
            )
        },
        zoom * ppp * 100.0,
        super::view_state::MIN_ZOOM.min(zoom) * ppp * 100.0
            ..=super::view_state::MAX_ZOOM * ppp * 100.0,
        10.0,
        false,
    ) {
        state.zoom_at(
            percent / (zoom * ppp * 100.0).max(0.001),
            pane.center(),
            canvas,
            pane,
        );
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(key, state));
    if let Some(history) = history.filter(|h| h.len() > 1) {
        filmstrip(ui, &t, cmp, &history, acts);
    }
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;
    #[test]
    fn focused_canvas_pans_zooms_and_fits_without_a_pointer() {
        let ctx = egui::Context::default();
        let mut state = ViewState::default();
        for key in [egui::Key::ArrowRight, egui::Key::Equals, egui::Key::Home] {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| {
                    let (_, resp) =
                        ui.allocate_exact_size(vec2(400.0, 300.0), Sense::click_and_drag());
                    resp.request_focus();
                    input(
                        ui,
                        &resp,
                        &mut state,
                        vec2(800.0, 600.0),
                        (resp.rect, None),
                        None,
                    );
                },
            )
            .drop_without_applying_deltas();
            match key {
                egui::Key::ArrowRight => assert!(state.center.x > 0.5),
                egui::Key::Equals => assert!(state.zoom.is_some_and(|zoom| zoom > 0.5)),
                egui::Key::Home => assert!(state.zoom.is_none()),
                _ => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
mod value_accessibility_tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn image_zoom_exposes_percent_and_applies_assistive_value_to_actual_view_state()
    -> Result<(), String> {
        // All lazies are precompleted: rendering cannot start Git/history/difference work.
        let compare = Compare {
            old: None,
            new: Some(Decoded {
                image: Arc::new(ColorImage::filled([2, 2], Color32::WHITE)),
                width: 2,
                height: 2,
                scaled: false,
            }),
            diff: super::super::lazy::Lazy::ready(None),
            history: super::super::lazy::Lazy::ready(Arc::new(Vec::new())),
            old_bytes: None,
            new_bytes: Some(4),
            id: "a11y-recorded-image".into(),
            repo: std::path::PathBuf::from("/recorded"),
            path: "picture.png".into(),
            from: None,
            current: None,
            textures: Default::default(),
        };
        let key = egui::Id::new(("image-compare", &compare.repo, &compare.path));
        let mut h = Harness::new_ui_state(
            move |ui, initialized: &mut bool| {
                if !std::mem::replace(initialized, true) {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    return;
                }
                compare_view(ui, &compare, &mut Vec::new());
                crate::a11y::end_frame(ui.ctx());
            },
            false,
        );
        h.run_steps(2);
        let node = h.get_by_role(egui::accesskit::Role::Slider);
        let data = node.accesskit_node().data();
        assert!(
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Image comparison zoom for picture.png."))
        );
        assert_eq!(data.numeric_value(), Some(100.0));
        assert_eq!(
            data.orientation(),
            Some(egui::accesskit::Orientation::Horizontal)
        );
        let (target_node, target_tree) = node.accesskit_node().locate();
        h.event(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::SetValue,
                target_tree,
                target_node,
                data: Some(egui::accesskit::ActionData::NumericValue(200.0)),
            },
        ));
        h.run();
        let state: ViewState = h
            .ctx
            .data(|data| data.get_temp(key))
            .ok_or("actual image state missing")?;
        assert_eq!(state.zoom, Some(2.0));
        assert_eq!(
            h.get_by_role(egui::accesskit::Role::Slider)
                .accesskit_node()
                .data()
                .numeric_value(),
            Some(200.0)
        );
        Ok(())
    }
}

mod canvas;
use canvas::{
    canvas, checker_texture, checkerboard, filmstrip, input, size_of, slider, texture, uv_all,
};

#[cfg(test)]
pub(super) use canvas::changes;
#[cfg(not(test))]
use canvas::changes;
