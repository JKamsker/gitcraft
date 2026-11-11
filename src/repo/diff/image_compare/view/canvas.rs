use super::*;

pub(super) fn uv_all() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

pub(super) fn size_of(d: &Decoded) -> egui::Vec2 {
    vec2(d.image.size[0] as f32, d.image.size[1] as f32)
}

/// As large as both pictures: what every mode lays them out on, top-left aligned.
pub(super) fn canvas(cmp: &Compare) -> egui::Vec2 {
    [&cmp.old, &cmp.new]
        .into_iter()
        .flatten()
        .map(size_of)
        .fold(vec2(1.0, 1.0), |a, b| a.max(b))
}

/// "640 × 480 → 800 × 600 · 12.0 KB → 14.1 KB · 3.2% of pixels changed".
pub(in crate::repo::diff::image_compare) fn changes(
    cmp: &Compare,
    diff: Option<&Difference>,
) -> String {
    let dims = |d: &Decoded| {
        let scaled = if d.scaled {
            crate::i18n::text("repo-core-1819")
        } else {
            ""
        };
        crate::i18n::message(
            "repo-core-1820",
            &[
                ("width", format!("{}", d.width).into()),
                ("height", format!("{}", d.height).into()),
                ("scaled", scaled.to_string().into()),
            ],
        )
    };
    let mut parts = Vec::new();
    match (&cmp.old, &cmp.new) {
        (Some(a), Some(b)) if (a.width, a.height) == (b.width, b.height) => parts.push(
            crate::i18n::message("repo-core-1821", &[("b", dims(b).to_string().into())]),
        ),
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
            0 => crate::i18n::text("repo-core-1822").to_owned(),
            1 => crate::i18n::text("repo-core-1823").to_owned(),
            n => {
                let share = n as f64 * 100.0 / d.total.max(1) as f64;
                if share < 0.1 {
                    crate::i18n::message("repo-core-1824", &[("n", (n).into())])
                } else {
                    crate::i18n::message("repo-core-1825", &[("share", (share).into())])
                }
            }
        });
    }
    parts.join(" · ")
}

pub(super) fn slider(
    ui: &mut Ui,
    (left, right): (&str, &str),
    value: &mut f32,
    hint: Option<&str>,
) {
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
pub(super) fn input(
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
pub(super) fn checkerboard(painter: &egui::Painter, checker: &TextureHandle, rect: Rect) {
    if !rect.is_positive() {
        return;
    }
    const SQ: f32 = 8.0;
    let uv = Rect::from_min_size(pos2(0.0, 0.0), rect.size() / (2.0 * SQ));
    painter.image(checker.id(), rect, uv, Color32::WHITE);
}

/// The checkerboard's texture: two greys, repeated.
pub(super) fn checker_texture(ctx: &egui::Context, cmp: &Compare, t: &Tokens) -> TextureHandle {
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
pub(super) fn texture(
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
pub(super) fn cached(
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
pub(super) fn filmstrip(
    ui: &mut Ui,
    t: &Tokens,
    cmp: &Compare,
    history: &[super::super::Version],
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
                            crate::i18n::message(
                                "repo-core-1830",
                                &[
                                    ("hash", crate::repo::short(&v.hash).to_string().into()),
                                    ("summary", v.summary.to_string().into()),
                                    ("path", v.path.to_string().into()),
                                ],
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
                                    crate::i18n::text("repo-core-1832"),
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
