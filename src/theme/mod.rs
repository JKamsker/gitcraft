//! Design tokens, modelled on PhotoCraft's "Pro" themes: flat grey panels, a darker tab strip,
//! a blue accent, Inter for the UI and JetBrains Mono for hashes and diffs.
//!
//! Widgets read [`Tokens::get`] instead of hard-coding colours, so every theme applies everywhere.
//! A theme is a [`Palette`]: the Pro themes and the well-known editor themes of [`catalog`],
//! your own theme files ([`files`]: gitcraft's, VS Code's, Base16), or VS Code's current
//! theme. [`Tweaks`] then change the accent, the corners and the graph's lanes of any of them.

pub mod catalog;
pub mod files;
pub mod palette;
#[cfg(test)]
pub(crate) mod test_paint;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
    Visuals,
};
use serde::{Deserialize, Serialize};

pub use palette::Palette;

/// Which theme: one of the catalogue by its id, `file:<name>` for a file in the themes folder,
/// or `vscode` for VS Code's current theme. Saved as the string, which is how the three themes
/// before were saved too.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ThemeId(pub String);

impl Default for ThemeId {
    fn default() -> Self {
        ThemeId("Dark".to_owned())
    }
}

impl<'de> Deserialize<'de> for ThemeId {
    /// A string, or the unit variant the themes were saved as when they were an enum (RON
    /// writes those bare: `theme: Light`). Untagged, RON hands over a bare name as a string.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Name(String),
        }
        let Saved::Name(name) = Saved::deserialize(d)?;
        Ok(ThemeId(name))
    }
}

impl ThemeId {
    pub const VSCODE: &'static str = "vscode";

    pub fn new(id: &str) -> Self {
        ThemeId(id.to_owned())
    }

    pub fn file(name: &str) -> Self {
        ThemeId(format!("file:{name}"))
    }

    /// The theme's name, or its id when there is no such theme (any more).
    pub fn label(&self) -> String {
        find(self).map_or_else(|| self.0.clone(), |t| t.name.clone())
    }
}

/// Where a theme comes from, which is how the theme lists group them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// gitcraft's own (the Pro themes, high contrast).
    Gitcraft,
    /// The palette of a well-known editor theme.
    Dark,
    Light,
    /// A file in the themes folder.
    Yours,
    /// VS Code's current theme.
    Editor,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Gitcraft,
        Group::Dark,
        Group::Light,
        Group::Yours,
        Group::Editor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::Gitcraft => "gitcraft",
            Group::Dark => "Dark",
            Group::Light => "Light",
            Group::Yours => "Your themes",
            Group::Editor => "Your editor",
        }
    }
}

/// A theme to choose.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeInfo {
    pub id: ThemeId,
    pub name: String,
    /// Who made it, where it is from.
    pub credit: String,
    pub group: Group,
    pub palette: Palette,
}

impl ThemeInfo {
    pub fn dark(&self) -> bool {
        self.palette.dark
    }

    pub fn tokens(&self, tweaks: &Tweaks) -> Tokens {
        let mut t = self.palette.tokens();
        tweaks.apply(&mut t);
        t
    }
}

/// The themes read last, and what they were read from.
struct Cache {
    checked: Instant,
    /// The themes folder's files as they were read (see [`files::fingerprint`]).
    folder: Option<u64>,
    themes: Arc<Vec<ThemeInfo>>,
    errors: Vec<(String, String)>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
/// A look at the themes folder and VS Code is under way on another thread.
static LOOKING: AtomicBool = AtomicBool::new(false);
/// How often the themes folder and VS Code are looked at again.
const RECHECK: Duration = Duration::from_secs(2);

/// Every theme there is to choose. Read once on the first call; then looked at again on
/// another thread when older than a few seconds, so the window never waits for the disk.
pub fn themes() -> Arc<Vec<ThemeInfo>> {
    let stale = match CACHE.lock() {
        Ok(cache) => match cache.as_ref() {
            Some(c) => {
                let themes = c.themes.clone();
                if c.checked.elapsed() >= RECHECK && !LOOKING.swap(true, Ordering::AcqRel) {
                    std::thread::spawn(|| {
                        look_again();
                        LOOKING.store(false, Ordering::Release);
                    });
                }
                return themes;
            }
            None => true,
        },
        Err(_) => return Arc::new(builtin_infos()),
    };
    if stale {
        refresh();
    }
    CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|c| c.themes.clone()))
        .unwrap_or_else(|| Arc::new(builtin_infos()))
}

/// What was read last: the folder's fingerprint, the themes and the files that didn't read.
type Snapshot = (Option<u64>, Arc<Vec<ThemeInfo>>, Vec<(String, String)>);

fn snapshot() -> Option<Snapshot> {
    let cache = CACHE.lock().ok()?;
    cache
        .as_ref()
        .map(|c| (c.folder, c.themes.clone(), c.errors.clone()))
}

/// Reads the themes again now: after a theme file was added, so it can be chosen at once.
pub fn refresh() {
    let before = snapshot();
    let next = read_themes(before, true);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(next);
    }
}

/// [`refresh`] if anything changed; true when it did. On a thread of its own.
fn look_again() -> bool {
    let before = snapshot();
    let old = before.as_ref().map(|(_, themes, _)| themes.clone());
    let next = read_themes(before, false);
    let changed = old.is_none_or(|old| *old != *next.themes);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(next);
    }
    changed
}

/// The themes: the catalogue's, the themes folder's (read again only when its files changed,
/// unless `force`), and VS Code's. A file that no longer reads keeps the theme it was last
/// read as, so a typo while editing it doesn't change the window: it is listed as an error.
fn read_themes(before: Option<Snapshot>, force: bool) -> Cache {
    let dir = files::dir();
    let folder = dir.as_deref().and_then(files::fingerprint);
    let mut list = builtin_infos();
    let (own, errors) = match &before {
        Some((was, themes, errors)) if *was == folder && !force => (
            themes
                .iter()
                .filter(|t| t.group == Group::Yours)
                .cloned()
                .collect(),
            errors.clone(),
        ),
        _ => {
            let (read, errors) = dir.as_deref().map(files::scan).unwrap_or_default();
            let mut own: Vec<ThemeInfo> = read
                .into_iter()
                .map(|f| ThemeInfo {
                    id: ThemeId::file(&f.file),
                    name: f.name,
                    credit: f.credit,
                    group: Group::Yours,
                    palette: f.palette,
                })
                .collect();
            if let Some((_, themes, _)) = &before {
                for (file, _) in &errors {
                    let id = ThemeId::file(file);
                    if let Some(last) = themes.iter().find(|t| t.id == id) {
                        own.push(last.clone());
                    }
                }
                own.sort_by(|a, b| a.id.0.cmp(&b.id.0));
            }
            (own, errors)
        }
    };
    list.extend(own);
    if let Some(e) = crate::vscode_theme::current().filter(|e| !e.workbench.is_empty()) {
        list.push(ThemeInfo {
            id: ThemeId::new(ThemeId::VSCODE),
            name: format!("Follow VS Code ({})", e.name),
            credit: "VS Code's current colour theme".to_owned(),
            group: Group::Editor,
            palette: files::from_vscode(&e.workbench, Some(e.dark)),
        });
    }
    Cache {
        checked: Instant::now(),
        folder,
        themes: Arc::new(list),
        errors,
    }
}

/// The files in the themes folder that could not be read as themes, and why.
pub fn theme_errors() -> Vec<(String, String)> {
    let _ = themes();
    CACHE
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|c| c.errors.clone()))
        .unwrap_or_default()
}

fn builtin_infos() -> Vec<ThemeInfo> {
    catalog::BUILTIN
        .iter()
        .map(|b| {
            let palette = b.palette();
            ThemeInfo {
                id: ThemeId::new(b.id),
                name: b.name.to_owned(),
                credit: b.credit.to_owned(),
                group: if b.credit == "gitcraft"
                    || b.id.chars().next().is_some_and(char::is_uppercase)
                {
                    Group::Gitcraft
                } else if palette.dark {
                    Group::Dark
                } else {
                    Group::Light
                },
                palette,
            }
        })
        .collect()
}

/// The theme `id`, if there is one.
pub fn find(id: &ThemeId) -> Option<ThemeInfo> {
    themes().iter().find(|t| t.id == *id).cloned()
}

/// The theme `id`, or gitcraft's dark one when there is none such any more (a theme file
/// removed, VS Code uninstalled).
pub fn resolve(id: &ThemeId) -> ThemeInfo {
    find(id).unwrap_or_else(|| {
        let mut fallback = builtin_infos().swap_remove(0);
        fallback.id = id.clone();
        fallback
    })
}

/// What can be changed about any theme.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tweaks {
    /// An accent of your own in place of the theme's.
    pub accent: Option<[u8; 3]>,
    /// How round corners are, 1.0 as the theme has them; 0 square.
    pub roundness: f32,
    /// The graph's lane colours.
    pub lanes: LaneColors,
}

impl Default for Tweaks {
    fn default() -> Self {
        Tweaks {
            accent: None,
            roundness: 1.0,
            lanes: LaneColors::Theme,
        }
    }
}

/// Where the graph's lanes take their colours from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneColors {
    /// The theme's own hues.
    #[default]
    Theme,
    /// gitcraft's bright lanes, the same in every theme.
    Classic,
    /// Okabe and Ito's colours, told apart with any colour blindness.
    ColorBlind,
}

impl LaneColors {
    pub const ALL: [LaneColors; 3] = [
        LaneColors::Theme,
        LaneColors::Classic,
        LaneColors::ColorBlind,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LaneColors::Theme => "The theme's colours",
            LaneColors::Classic => "Classic",
            LaneColors::ColorBlind => "Colour-blind safe",
        }
    }
}

impl Tweaks {
    pub fn apply(&self, t: &mut Tokens) {
        if let Some([r, g, b]) = self.accent {
            let accent = Color32::from_rgb(r, g, b);
            t.accent = accent;
            t.accent_fg = accent;
            t.accent_text = palette::readable_on(accent);
            t.row_selected = t
                .dock
                .lerp_to_gamma(accent, if t.dark { 0.3 } else { 0.22 });
        }
        let r = self.roundness.clamp(0.0, 3.0);
        t.radius_sm = (3.0 * r).round();
        t.radius = (4.0 * r).round();
        t.radius_lg = (6.0 * r).round();
        match self.lanes {
            LaneColors::Theme => {}
            LaneColors::Classic => {
                t.lanes = if t.dark {
                    catalog::CLASSIC_DARK_LANES
                } else {
                    catalog::CLASSIC_LIGHT_LANES
                };
            }
            LaneColors::ColorBlind => {
                t.lanes = if t.dark {
                    catalog::COLOR_BLIND_DARK_LANES
                } else {
                    catalog::COLOR_BLIND_LIGHT_LANES
                };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// Toolbars and the active tab.
    pub chrome: Color32,
    /// Behind panels (the graph area).
    pub dock: Color32,
    /// Side panels.
    pub card: Color32,
    /// Inputs and code blocks.
    pub field: Color32,
    pub field_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// Accent foreground for links and small labels, independent of button fills.
    pub accent_fg: Color32,
    pub separator: Color32,
    pub shadow: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,
    /// Window tab strip and panel headers.
    pub tab_strip: Color32,
    pub row_selected: Color32,
    pub diff_add_bg: Color32,
    pub diff_del_bg: Color32,
    /// The graph's lane colours, in order.
    pub lanes: [Color32; 10],
    /// The terminal's 16 colours.
    pub ansi: [Color32; 16],
    pub radius_sm: f32,
    pub radius: f32,
    pub radius_lg: f32,
}

impl Default for Tokens {
    /// gitcraft's dark theme.
    fn default() -> Self {
        catalog::BUILTIN[0].palette().tokens()
    }
}

impl Tokens {
    /// Tint behind small status text, capped so the composited row still meets AA.
    pub(crate) fn text_tint(&self, color: Color32, opacity: f32, background: Color32) -> Color32 {
        palette::readable_tint(color, opacity, &[background], &[color])
    }

    /// Search-match shading shares a row with every kind of metadata and status label.
    pub(crate) fn match_tint(&self) -> Color32 {
        palette::readable_tint(
            self.warning,
            0.10,
            &[self.dock],
            &[
                self.text,
                self.text_dim,
                self.text_faint,
                self.accent_fg,
                self.danger,
                self.warning,
                self.success,
            ],
        )
    }

    pub fn dark(&self) -> bool {
        self.dark
    }

    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(tokens_id()))
            .unwrap_or_default()
    }

    /// Colour of a graph lane; lanes keep their colour for their whole life.
    pub fn lane(&self, lane: usize) -> Color32 {
        self.lanes[lane % self.lanes.len()]
    }
}

/// The theme chosen and its tweaks, as last applied.
#[derive(Clone, Debug, PartialEq)]
struct Choice {
    id: ThemeId,
    tweaks: Tweaks,
}

fn choice_id() -> egui::Id {
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
static FOLLOWING: AtomicBool = AtomicBool::new(false);

fn follows(id: &ThemeId) -> bool {
    id.0.starts_with("file:") || id.0 == ThemeId::VSCODE
}

fn tokens_id() -> egui::Id {
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
