//! The picture comparison: the mode switch, zoom, what changed, the pictures in the mode
//! picked over a checkerboard, and the strip of the picture's history.

use egui::{Color32, Rect, RichText, Sense, TextureHandle, Ui, pos2, vec2};

use super::Compare;
use super::decode::Decoded;
use super::filmstrip::{THUMB, Thumb, Thumbs};
use super::view_state::{Mode, ViewState, left_of};
use crate::git::{human_size, relative_time};
use crate::repo::{Action, DiffKey, widgets};
use crate::theme::{self, Tokens};

/// The comparison of a picture's two versions.
pub(in crate::repo::diff) fn compare_view(ui: &mut Ui, cmp: &Compare, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let key = egui::Id::new(("image-compare", &cmp.path));
    let mut state: ViewState = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    let both = cmp.old.is_some() && cmp.new.is_some();
    if !both {
        state.mode = Mode::TwoUp;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
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
            state.zoom = Some(1.0 / ui.ctx().pixels_per_point());
            state.center = vec2(0.5, 0.5);
        }
        ui.label(RichText::new(state.zoom_label()).color(t.text_dim));
        ui.add_space(12.0);
        ui.label(RichText::new(changes(cmp)).color(t.text_dim));
    });
    match state.mode {
        Mode::Swipe => slider(ui, "Old", "New", &mut state.swipe, true),
        Mode::Onion => slider(ui, "Old", "New", &mut state.onion, false),
        _ => {}
    }
    let strip = if cmp.history.len() > 1 {
        THUMB as f32 + 40.0
    } else {
        0.0
    };
    let height = (ui.available_height() - strip - 8.0).max(120.0);
    let (area, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
    let area = area.shrink(8.0);
    let tex = |side: &str, d: &Decoded| texture(ui.ctx(), &cmp.id, side, &d.image);
    let (old, new) = (
        cmp.old.as_ref().map(|d| (tex("old", d), d)),
        cmp.new.as_ref().map(|d| (tex("new", d), d)),
    );
    let painter = ui.painter_at(area);
    match state.mode {
        Mode::TwoUp => {
            let gap = 12.0;
            let half = vec2((area.width() - gap) / 2.0, area.height());
            let left = Rect::from_min_size(area.min, half);
            let right = Rect::from_min_size(pos2(left.right() + gap, area.top()), half);
            for (rect, side, missing) in [(left, &old, "(new file)"), (right, &new, "(deleted)")] {
                match side {
                    Some((tex, d)) => {
                        let placed = state.placed(size_of(d), rect);
                        let p = ui.painter_at(rect);
                        checkerboard(&p, placed.intersect(rect), &t);
                        p.image(tex.id(), placed, uv_all(), Color32::WHITE);
                    }
                    None => {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            missing,
                            theme::medium(13.0),
                            t.text_faint,
                        );
                    }
                }
            }
            input(
                ui,
                &resp,
                &mut state,
                canvas(cmp),
                left.union(right),
                Some(left),
            );
        }
        Mode::Swipe | Mode::Onion | Mode::Difference => {
            let canvas = canvas(cmp);
            let placed = state.placed(canvas, area);
            checkerboard(&painter, placed.intersect(area), &t);
            let at = |d: &Decoded| {
                Rect::from_min_size(
                    placed.min,
                    size_of(d) * (placed.width() / canvas.x.max(1.0)),
                )
            };
            if let (Some((otex, od)), Some((ntex, nd))) = (&old, &new) {
                match state.mode {
                    Mode::Swipe => {
                        let split = placed.left() + placed.width() * state.swipe;
                        let (screen, uv) = left_of(at(od), split);
                        painter.image(otex.id(), screen, uv, Color32::WHITE);
                        let new_rect = at(nd);
                        let (left, luv) = left_of(new_rect, split);
                        let right =
                            Rect::from_min_max(pos2(left.right(), new_rect.top()), new_rect.max);
                        let ruv = Rect::from_min_max(pos2(luv.max.x, 0.0), pos2(1.0, 1.0));
                        painter.image(ntex.id(), right, ruv, Color32::WHITE);
                        let x = split.clamp(area.left(), area.right());
                        painter.line_segment(
                            [pos2(x, area.top()), pos2(x, area.bottom())],
                            egui::Stroke::new(2.0, t.accent),
                        );
                    }
                    Mode::Onion => {
                        painter.image(otex.id(), at(od), uv_all(), Color32::WHITE);
                        let alpha = (state.onion * 255.0).round() as u8;
                        painter.image(
                            ntex.id(),
                            at(nd),
                            uv_all(),
                            Color32::from_white_alpha(alpha),
                        );
                    }
                    _ => {
                        painter.image(otex.id(), at(od), uv_all(), Color32::from_white_alpha(60));
                        if let Some(diff) = &cmp.diff {
                            let dtex = texture(ui.ctx(), &cmp.id, "diff", &diff.image);
                            painter.image(dtex.id(), placed, uv_all(), Color32::WHITE);
                        }
                    }
                }
            }
            input(ui, &resp, &mut state, canvas, area, None);
            if state.mode == Mode::Swipe
                && resp.dragged()
                && ui.input(|i| i.modifiers.shift)
                && let Some(p) = resp.interact_pointer_pos()
            {
                state.swipe = ((p.x - placed.left()) / placed.width().max(1.0)).clamp(0.0, 1.0);
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(key, state));
    if strip > 0.0 {
        filmstrip(ui, &t, cmp, acts);
    }
}

fn uv_all() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

fn size_of(d: &Decoded) -> egui::Vec2 {
    vec2(d.image.size[0] as f32, d.image.size[1] as f32)
}

/// As large as both pictures: what the overlaid modes are laid out on, top-left aligned.
fn canvas(cmp: &Compare) -> egui::Vec2 {
    [&cmp.old, &cmp.new]
        .into_iter()
        .flatten()
        .map(size_of)
        .fold(vec2(1.0, 1.0), |a, b| a.max(b))
}

/// "640 × 480 → 800 × 600 · 12.0 KB → 14.1 KB · 3.2% of pixels changed".
pub(super) fn changes(cmp: &Compare) -> String {
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
    if let Some(d) = &cmp.diff {
        parts.push(match d.changed {
            0 => "no pixel changed".to_owned(),
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

fn slider(ui: &mut Ui, left: &str, right: &str, value: &mut f32, swipe: bool) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(left);
        ui.add(egui::Slider::new(value, 0.0..=1.0).show_value(false));
        ui.label(right);
        if swipe {
            ui.label(
                RichText::new("Shift+drag on the picture moves the line")
                    .small()
                    .weak(),
            );
        }
    });
}

/// The wheel zooms at the pointer, a drag pans, a double-click fits. In 2-up (`first`: the
/// left pane), a pointer over the right pane zooms as if over the same point of the left.
fn input(
    ui: &Ui,
    resp: &egui::Response,
    state: &mut ViewState,
    size: egui::Vec2,
    area: Rect,
    first: Option<Rect>,
) {
    let pane = first.unwrap_or(area);
    if resp.double_clicked() {
        state.zoom = None;
        return;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 200.0);
        if scroll.abs() > 0.0
            && let Some(mut p) = ui.input(|i| i.pointer.hover_pos())
        {
            if let Some(left) = first
                && p.x > left.right()
            {
                p.x -= area.width() - left.width();
            }
            state.zoom_at((scroll * 0.0025).exp(), p, size, pane);
        }
    }
    let shift = ui.input(|i| i.modifiers.shift);
    if resp.dragged() && !shift {
        state.pan(resp.drag_delta(), size, pane);
    }
}

/// Grey squares behind the picture, so its transparent parts show as such.
fn checkerboard(painter: &egui::Painter, rect: Rect, t: &Tokens) {
    if !rect.is_positive() {
        return;
    }
    const SQ: f32 = 8.0;
    painter.rect_filled(rect, 0.0, t.field);
    let dark = t.field_border.gamma_multiply(0.6);
    let painter = painter.with_clip_rect(rect);
    let (cols, rows) = (
        (rect.width() / SQ).ceil() as usize,
        (rect.height() / SQ).ceil() as usize,
    );
    // Past this many squares (a huge zoomed rect is clipped anyway) only those on screen count.
    if cols.saturating_mul(rows) > 40_000 {
        return;
    }
    for r in 0..rows {
        for c in (r % 2..cols).step_by(2) {
            let min = rect.min + vec2(c as f32 * SQ, r as f32 * SQ);
            painter.rect_filled(Rect::from_min_size(min, vec2(SQ, SQ)), 0.0, dark);
        }
    }
}

/// The textures of the comparison shown: made once and kept while it is shown; those of the
/// one shown before are let go of when another is.
#[derive(Clone, Default)]
struct Textures {
    of: String,
    made: std::collections::HashMap<String, TextureHandle>,
}

/// The texture of `image`, named `name`, for the comparison `of`.
fn texture(
    ctx: &egui::Context,
    of: &str,
    name: &str,
    image: &std::sync::Arc<egui::ColorImage>,
) -> TextureHandle {
    let id = egui::Id::new("image-compare-textures");
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Textures>(id);
        if cache.of != of {
            cache.of = of.to_owned();
            cache.made.clear();
        }
        cache
            .made
            .entry(name.to_owned())
            .or_insert_with(|| {
                ctx.load_texture(name, image.as_ref().clone(), egui::TextureOptions::NEAREST)
            })
            .clone()
    })
}

/// The picture's history, newest first: a thumbnail per commit, the one shown marked; a click
/// opens that commit's change of the picture.
fn filmstrip(ui: &mut Ui, t: &Tokens, cmp: &Compare, acts: &mut Vec<Action>) {
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
                for v in &cmp.history {
                    let current = cmp.current.as_deref() == Some(v.hash.as_str());
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(THUMB as f32 + 8.0, THUMB as f32 + 30.0),
                        Sense::click(),
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
                                let size = vec2(img.size[0] as f32, img.size[1] as f32);
                                let at = Rect::from_center_size(frame.center(), size);
                                checkerboard(&p, at, t);
                                let name = format!("thumb/{}/{}", v.hash, v.path);
                                let tex = texture(ui.ctx(), &cmp.id, &name, &img);
                                p.image(tex.id(), at, uv_all(), Color32::WHITE);
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
