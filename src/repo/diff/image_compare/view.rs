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
            .button("Fit")
            .on_hover_text("Fit the picture (double-click it)")
            .clicked()
        {
            state.zoom = None;
        }
        if ui
            .button("100%")
            .on_hover_text("One picture pixel a screen pixel")
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
            ("All new", "All old"),
            &mut state.swipe,
            Some("or drag the line"),
        ),
        Mode::Onion => slider(ui, ("Old", "New"), &mut state.onion, None),
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
    crate::a11y::a11y(&resp, egui::WidgetType::Slider, || {
        format!(
            "Image comparison for {}. Arrow keys pan, plus and minus zoom, Home fits. Use controls above to change comparison mode.",
            cmp.path
        )
    });
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
            for (rect, side, missing) in [(left, &old, "(new file)"), (right, &new, "(deleted)")] {
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
                                    "Working out the difference…",
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
    ui.data_mut(|d| d.insert_temp(key, state));
    if let Some(history) = history.filter(|h| h.len() > 1) {
        filmstrip(ui, &t, cmp, &history, acts);
    }
}

fn uv_all() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

fn size_of(d: &Decoded) -> egui::Vec2 {
    vec2(d.image.size[0] as f32, d.image.size[1] as f32)
}

/// As large as both pictures: what every mode lays them out on, top-left aligned.
fn canvas(cmp: &Compare) -> egui::Vec2 {
    [&cmp.old, &cmp.new]
        .into_iter()
        .flatten()
        .map(size_of)
        .fold(vec2(1.0, 1.0), |a, b| a.max(b))
}

/// "640 × 480 → 800 × 600 · 12.0 KB → 14.1 KB · 3.2% of pixels changed".
pub(super) fn changes(cmp: &Compare, diff: Option<&Difference>) -> String {
    let dims = |d: &Decoded| {
        let scaled = if d.scaled { " (shown smaller)" } else { "" };
        format!("{} × {}{scaled}", d.width, d.height)
    };
    let mut parts = Vec::new();
    match (&cmp.old, &cmp.new) {
        (Some(a), Some(b)) if (a.width, a.height) == (b.width, b.height) => {
            parts.push(format!("{}, same dimensions", dims(b)))
        }
        (Some(a), Some(b)) => parts.push(format!("{} → {}", dims(a), dims(b))),
        (None, Some(b)) => parts.push(dims(b)),
        (Some(a), None) => parts.push(dims(a)),
        (None, None) => {}
    }
    match (cmp.old_bytes, cmp.new_bytes) {
        (Some(a), Some(b)) => parts.push(format!(
            "{} → {}",
            human_size(a as u64),
            human_size(b as u64)
        )),
        (a, b) => parts.extend(a.or(b).map(|n| human_size(n as u64))),
    }
    if let Some(d) = diff {
        parts.push(match d.changed {
            0 => "no pixel changed".to_owned(),
            1 => "1 pixel changed".to_owned(),
            n => {
                let share = n as f64 * 100.0 / d.total.max(1) as f64;
                if share < 0.1 {
                    format!("{n} pixels changed")
                } else {
                    format!("{share:.1}% of pixels changed")
                }
            }
        });
    }
    parts.join(" · ")
}

fn slider(ui: &mut Ui, (left, right): (&str, &str), value: &mut f32, hint: Option<&str>) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(left);
        ui.add(egui::Slider::new(value, 0.0..=1.0).show_value(false));
        ui.label(right);
        if let Some(hint) = hint {
            ui.label(RichText::new(hint).small().weak());
        }
    });
}

/// The wheel zooms at the pointer, a drag pans, a double-click fits; in Swipe, a drag that
/// starts on the line (`swipe`: its x and the canvas placed) moves the line. In 2-up (`left`:
/// the left pane), a pointer over the right pane zooms as if over the same point of the left.
fn input(
    ui: &Ui,
    resp: &egui::Response,
    state: &mut ViewState,
    size: egui::Vec2,
    (area, left): (Rect, Option<Rect>),
    swipe: Option<(f32, Rect)>,
) {
    let pane = left.unwrap_or(area);
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    vertical_arrows: true,
                    horizontal_arrows: true,
                    ..Default::default()
                },
            )
        });
        for (key, delta) in [
            (egui::Key::ArrowLeft, vec2(32.0, 0.0)),
            (egui::Key::ArrowRight, vec2(-32.0, 0.0)),
            (egui::Key::ArrowUp, vec2(0.0, 32.0)),
            (egui::Key::ArrowDown, vec2(0.0, -32.0)),
        ] {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                state.pan(delta, size, pane);
            }
        }
        for (key, factor) in [
            (egui::Key::Plus, 1.25),
            (egui::Key::Equals, 1.25),
            (egui::Key::Minus, 0.8),
        ] {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                state.zoom_at(factor, pane.center(), size, pane);
            }
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home)) {
            state.zoom = None;
        }
    }
    if resp.double_clicked() {
        state.zoom = None;
        return;
    }
    let on_line = |p: egui::Pos2| swipe.is_some_and(|(x, _)| (p.x - x).abs() <= LINE_GRAB);
    let line_id = resp.id.with("swipe-line");
    let pointer = ui.input(|i| i.pointer.hover_pos());
    if resp.hovered() && pointer.is_some_and(on_line) {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        ui.data_mut(|d| d.insert_temp(line_id, on_line(p)));
    }
    let moving_line = ui.data(|d| d.get_temp::<bool>(line_id)).unwrap_or(false);
    if resp.dragged() {
        match swipe {
            Some((_, placed)) if moving_line || ui.input(|i| i.modifiers.shift) => {
                if let Some(p) = resp.interact_pointer_pos() {
                    state.swipe = ((p.x - placed.left()) / placed.width().max(1.0)).clamp(0.0, 1.0);
                }
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }
            _ => state.pan(resp.drag_delta(), size, pane),
        }
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 400.0);
        if scroll.abs() > 0.0
            && let Some(mut p) = pointer
        {
            if let Some(left) = left
                && p.x > left.right()
            {
                p.x -= area.width() - left.width();
            }
            state.zoom_at((scroll * 0.0025).exp(), p, size, pane);
        }
    }
}

/// Grey squares behind `rect`, so transparent parts of the picture there show as such: one
/// quad of a repeating 2 × 2 texture.
fn checkerboard(painter: &egui::Painter, checker: &TextureHandle, rect: Rect) {
    if !rect.is_positive() {
        return;
    }
    const SQ: f32 = 8.0;
    let uv = Rect::from_min_size(pos2(0.0, 0.0), rect.size() / (2.0 * SQ));
    painter.image(checker.id(), rect, uv, Color32::WHITE);
}

/// The checkerboard's texture: two greys, repeated.
fn checker_texture(ctx: &egui::Context, cmp: &Compare, t: &Tokens) -> TextureHandle {
    let (light, dark) = (t.field, t.field_border.gamma_multiply(0.6));
    let name = format!("checker/{light:?}/{dark:?}");
    let opts = egui::TextureOptions {
        wrap_mode: egui::TextureWrapMode::Repeat,
        ..egui::TextureOptions::NEAREST
    };
    cached(ctx, cmp, &name, opts, || {
        Arc::new(ColorImage::new([2, 2], vec![light, dark, dark, light]))
    })
}

/// The texture of `image`, named `name`, kept with the comparison (and let go of with it):
/// sharp when enlarged, smooth when shown smaller.
fn texture(
    ctx: &egui::Context,
    cmp: &Compare,
    name: &str,
    image: &Arc<ColorImage>,
) -> TextureHandle {
    let opts = egui::TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    };
    cached(ctx, cmp, name, opts, || image.clone())
}

/// A texture of the comparison, made the first time. Not inside `ctx.data_mut`: making a
/// texture takes the context's lock itself.
fn cached(
    ctx: &egui::Context,
    cmp: &Compare,
    name: &str,
    opts: egui::TextureOptions,
    image: impl FnOnce() -> Arc<ColorImage>,
) -> TextureHandle {
    let known = cmp
        .textures
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(name)
        .cloned();
    if let Some(tex) = known {
        return tex;
    }
    let tex = ctx.load_texture(format!("{}/{name}", cmp.id), image(), opts);
    cmp.textures
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(name.to_owned(), tex.clone());
    tex
}

/// The picture's history, newest first: a thumbnail per commit, the one shown marked; a click
/// opens that commit's change of the picture.
fn filmstrip(
    ui: &mut Ui,
    t: &Tokens,
    cmp: &Compare,
    history: &[super::Version],
    acts: &mut Vec<Action>,
) {
    let thumbs: Thumbs = ui.data_mut(|d| {
        d.get_temp_mut_or_default::<Thumbs>(egui::Id::new(("filmstrip", &cmp.repo, &cmp.path)))
            .clone()
    });
    let now = crate::git::now();
    egui::ScrollArea::horizontal()
        .id_salt(("filmstrip", &cmp.path))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                for v in history {
                    let current = cmp.current.as_deref() == Some(v.hash.as_str());
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(THUMB as f32 + 8.0, THUMB as f32 + 30.0),
                        Sense::click(),
                    );
                    crate::a11y::a11y_selected(
                        &resp,
                        || {
                            format!(
                                "Image version {}, {}, {}",
                                crate::repo::short(&v.hash),
                                v.summary,
                                v.path
                            )
                        },
                        current,
                    );
                    let p = ui.painter_at(rect);
                    if current {
                        p.rect_filled(rect, 4.0, t.row_selected);
                    } else if resp.hovered() {
                        p.rect_filled(rect, 4.0, t.hover);
                    }
                    let frame = Rect::from_min_size(
                        rect.min + vec2(4.0, 4.0),
                        vec2(THUMB as f32, THUMB as f32),
                    );
                    if ui.is_rect_visible(rect) {
                        match thumbs.get(ui.ctx(), &cmp.repo, &v.path, &v.hash) {
                            Thumb::Ready(img) => {
                                // Small pictures shown larger, up to the thumbnail's size.
                                let size = vec2(img.size[0] as f32, img.size[1] as f32);
                                let grow = (THUMB as f32 / size.max_elem().max(1.0))
                                    .clamp(1.0, 4.0)
                                    .floor();
                                let shown = Rect::from_center_size(frame.center(), size * grow);
                                let checker = checker_texture(ui.ctx(), cmp, t);
                                checkerboard(&p, &checker, shown);
                                let name = format!("thumb/{}/{}", v.hash, v.path);
                                let tex = texture(ui.ctx(), cmp, &name, &img);
                                p.image(tex.id(), shown, uv_all(), Color32::WHITE);
                            }
                            Thumb::Loading => {
                                p.text(
                                    frame.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "…",
                                    theme::medium(14.0),
                                    t.text_faint,
                                );
                            }
                            Thumb::Failed => {
                                p.text(
                                    frame.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "(none)",
                                    theme::medium(11.0),
                                    t.text_faint,
                                );
                            }
                        }
                    }
                    p.text(
                        pos2(rect.center().x, frame.bottom() + 12.0),
                        egui::Align2::CENTER_CENTER,
                        relative_time(v.time, now),
                        theme::medium(10.5),
                        if current { t.text } else { t.text_dim },
                    );
                    let short = v.hash.get(..8).unwrap_or(&v.hash);
                    let resp = resp.on_hover_text(format!("{short} · {}", v.summary));
                    if resp.clicked() && !current {
                        acts.push(Action::OpenDiff(DiffKey::Commit {
                            hash: v.hash.clone(),
                            path: v.path.clone(),
                        }));
                    }
                }
            });
        });
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
                egui::Key::Equals => assert!(state.zoom.unwrap() > 0.5),
                egui::Key::Home => assert!(state.zoom.is_none()),
                _ => unreachable!(),
            }
        }
    }
}
