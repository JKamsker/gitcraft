//! Apply theme choices, fonts, and visual tokens to egui contexts.
use super::*;

/// The theme chosen and its tweaks, as last applied.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Choice {
    pub(super) id: ThemeId,
    pub(super) tweaks: Tweaks,
}

pub(super) fn choice_id() -> egui::Id {
    egui::Id::new("gitcraft-theme-choice")
}

/// Paints the app with the theme `id`, changed by `tweaks`.
pub fn apply(ctx: &egui::Context, id: &ThemeId, tweaks: &Tweaks) {
    FOLLOWING.store(follows(id), Ordering::Relaxed);
    let t = resolve(id).tokens(tweaks);
    ctx.data_mut(|d| {
        d.insert_temp(
            choice_id(),
            Choice {
                id: id.clone(),
                tweaks: *tweaks,
            },
        )
    });
    set_tokens(ctx, t);
}

/// Follows a theme that can change while it is in use: a file in the themes folder being
/// edited, or VS Code's theme switched there. Cheap to call every frame; the first call starts
/// a thread that looks for such changes while the window is idle and wakes it for them.
pub fn poll(ctx: &egui::Context) {
    static WATCHING: std::sync::Once = std::sync::Once::new();
    WATCHING.call_once(|| {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(RECHECK);
                if FOLLOWING.load(Ordering::Relaxed) && !LOOKING.swap(true, Ordering::AcqRel) {
                    let changed = look_again();
                    LOOKING.store(false, Ordering::Release);
                    if changed {
                        ctx.request_repaint();
                    }
                }
            }
        });
    });
    let Some(choice) = ctx.data(|d| d.get_temp::<Choice>(choice_id())) else {
        return;
    };
    if !follows(&choice.id) {
        return;
    }
    // Gone, or not readable for now (a file half saved): the window stays as it is.
    let Some(info) = find(&choice.id) else {
        return;
    };
    let t = info.tokens(&choice.tweaks);
    if t != Tokens::get(ctx) {
        set_tokens(ctx, t);
        ctx.request_repaint();
    }
}

/// The theme in use can change without being chosen again: [`poll`] follows it.
pub(super) static FOLLOWING: AtomicBool = AtomicBool::new(false);

pub(super) fn follows(id: &ThemeId) -> bool {
    id.0.starts_with("file:") || id.0 == ThemeId::VSCODE
}

pub(super) fn tokens_id() -> egui::Id {
    egui::Id::new("gitcraft-theme")
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Inter and JetBrains Mono first; egui's default fonts stay last for symbols Inter lacks.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(
        "Inter",
        include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    );
    add(
        "Inter-Medium",
        include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    );
    add(
        "Inter-SemiBold",
        include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    );
    add(
        "JetBrainsMono",
        include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
    );
    // The default fonts have only a part of the emoji people write: the rest comes from here.
    add(
        "NotoEmoji",
        include_bytes!("../../assets/fonts/NotoEmoji-Regular.ttf"),
    );
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Inter".to_owned());
    proportional.push("NotoEmoji".to_owned());
    let fallback = proportional.clone();
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, "JetBrainsMono".to_owned());
    monospace.push("NotoEmoji".to_owned());
    for (family, primary) in [("medium", "Inter-Medium"), ("semibold", "Inter-SemiBold")] {
        let mut stack = vec![primary.to_owned()];
        stack.extend(fallback.iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(family.into()), stack);
    }
    ctx.set_fonts(fonts);
}

/// Menus and context menus: no border, rows highlighted from edge to edge with room for the
/// text, a margin only above and below (where the menu's corners are rounded).
pub fn menu_style(s: &mut egui::Style) {
    s.spacing.menu_margin = egui::Margin::symmetric(0, 6);
    s.spacing.button_padding = egui::vec2(16.0, 7.0);
    s.spacing.item_spacing.y = 0.0;
    s.visuals.window_stroke = Stroke::NONE;
    // A shade darker than cards on dark themes, with lighter separators.
    let fill = s.visuals.window_fill;
    let (fill, line) = if s.visuals.dark_mode {
        let fill = fill.lerp_to_gamma(Color32::BLACK, 0.3);
        (fill, fill.lerp_to_gamma(Color32::WHITE, 0.2))
    } else {
        (fill, fill.lerp_to_gamma(Color32::BLACK, 0.15))
    };
    s.visuals.window_fill = fill;
    s.visuals.popup_shadow.color = s.visuals.popup_shadow.color.gamma_multiply(0.6);
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
    let w = &mut s.visuals.widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    for wv in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.bg_stroke = Stroke::NONE;
        wv.corner_radius = CornerRadius::ZERO;
    }
}

/// Paints the app with `t` from now on.
pub fn set_tokens(ctx: &egui::Context, t: Tokens) {
    ctx.data_mut(|d| d.insert_temp(tokens_id(), t));
    let mut v = if t.dark() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = t.chrome;
    v.window_fill = t.card;
    v.window_stroke = Stroke::new(1.0, t.field_border);
    v.extreme_bg_color = t.field;
    v.faint_bg_color = t.card;
    v.code_bg_color = t.field;
    v.override_text_color = Some(t.text);
    // egui otherwise fades placeholders to 60%, bypassing the checked faint token.
    v.weak_text_color = Some(t.text_faint);
    v.weak_text_alpha = 1.0;
    v.hyperlink_color = t.accent_fg;
    // An icon still being drawn (in the background) is left out for the moment, not stood in
    // for by a spinner: on a start, a spinner for each of them all over the window.
    v.image_loading_spinners = false;
    v.warn_fg_color = t.warning;
    v.error_fg_color = t.danger;
    v.window_corner_radius = CornerRadius::same(t.radius_lg as u8);
    v.menu_corner_radius = CornerRadius::same(t.radius_lg as u8);
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 32,
        spread: 0,
        color: t.shadow,
    };
    v.popup_shadow = egui::Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: t.shadow,
    };
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.striped = false;
    let r = CornerRadius::same(t.radius_sm as u8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.card;
    w.noninteractive.weak_bg_fill = t.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_dim);
    w.noninteractive.corner_radius = r;
    for (wv, bg, stroke) in [
        (&mut w.inactive, t.field, t.field_border),
        (&mut w.hovered, t.hover, t.field_border),
        (&mut w.active, t.pressed, t.accent),
        (&mut w.open, t.hover, t.field_border),
    ] {
        wv.bg_fill = bg;
        wv.weak_bg_fill = bg;
        wv.bg_stroke = Stroke::new(1.0, stroke);
        wv.fg_stroke = Stroke::new(1.0, t.text);
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(10.5)),
            (TextStyle::Body, FontId::proportional(12.5)),
            (TextStyle::Button, FontId::proportional(12.5)),
            (TextStyle::Heading, semibold(15.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        // Tooltips and other popups (menus set their own, see `menu_style`).
        s.spacing.menu_margin = egui::Margin::symmetric(12, 7);
        s.spacing.window_margin = egui::Margin::same(16);
        s.spacing.icon_width = 14.0;
        s.visuals.indent_has_left_vline = false;
        let mut scroll = egui::style::ScrollStyle::thin();
        scroll.interact_handle_opacity = 1.0;
        scroll.interact_background_opacity = 0.4;
        scroll.floating_width = 4.0;
        s.spacing.scroll = scroll;
        s.spacing.tooltip_width = 320.0;
    });
    // egui picks its style branch from the OS appearance; both branches get our palette.
    let style = ctx.global_style();
    ctx.set_style_of(egui::Theme::Light, Arc::clone(&style));
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub(crate) fn portable_theme(id: &ThemeId) -> bool {
    catalog::BUILTIN.iter().any(|t| t.id == id.0)
}
