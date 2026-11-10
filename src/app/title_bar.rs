//! Tabs in the title bar (Windows and macOS): the window is made without the system's title bar.
//! The tab strip takes its place, as in a browser. The strip's empty space moves the window and a
//! double click on it maximizes it, the caption buttons (minimize, maximize, close) sit at the
//! strip's end, and the window's edges resize it. On macOS the maximize button enters
//! fullscreen instead, as the native green button does.
//! macOS keeps its native frame with a transparent title bar so the corners stay rounded.

mod glyph;
use glyph::*;

mod native;
use native::*;
mod resize;
pub(in crate::app) use resize::*;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::viewport::ResizeDirection;
use egui::{Color32, CornerRadius, CursorIcon, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::settings::Settings;
use crate::theme::Tokens;

/// A caption button's width, as Windows 11 draws them.
const CAPTION_W: f32 = 46.0;
/// The three caption buttons together.
const CAPTIONS_W: f32 = 3.0 * CAPTION_W;
/// Half the size of a caption button's glyph.
const GLYPH_HALF: f32 = 5.0;
/// How far in from the window's edge a press resizes it: thin, as it lies over the caption
/// buttons and the tabs.
const EDGE: f32 = 3.0;
/// How far along the edges a corner reaches.
const CORNER: f32 = 8.0;
/// Windows 11's close button, under the pointer.
const CLOSE_HOVER: Color32 = Color32::from_rgb(196, 43, 28);
const CLOSE_PRESSED: Color32 = Color32::from_rgb(200, 64, 49);

/// Whether the window was made with the system's title bar: set by `main` before it is made.
static DECORATED_AT_START: AtomicBool = AtomicBool::new(true);

/// Whether the tab strip stands in for the system's title bar.
pub(super) fn wanted(settings: &Settings) -> bool {
    cfg!(any(windows, target_os = "macos")) && settings.tabs_in_title_bar
}

/// Whether to make the window with the system's title bar, from the settings eframe saved in
/// `app_ron`. The window is made without one from the start when the tabs take its place:
/// taking it away later keeps the window's outer size, so it would grow by the title bar on
/// every start. With nothing saved, the default settings decide, as they do for the app.
pub(super) fn decorated_at_start(app_ron: Option<&Path>) -> bool {
    let decorated = decorated_by(app_ron);
    DECORATED_AT_START.store(decorated, Ordering::Relaxed);
    decorated
}

fn decorated_by(app_ron: Option<&Path>) -> bool {
    let settings = app_ron
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| saved_settings(&text))
        .unwrap_or_default();
    !wanted(&settings)
}

/// The settings in eframe's saved state: a map of keys to RON, the app's under `APP_KEY`.
fn saved_settings(app_ron: &str) -> Option<Settings> {
    /// Only the settings of what the app saves.
    #[derive(serde::Deserialize)]
    struct Saved {
        #[serde(default)]
        settings: Settings,
    }
    let kv: HashMap<String, String> = ron::from_str(app_ron).ok()?;
    let saved: Saved = ron::from_str(kv.get(eframe::APP_KEY)?).ok()?;
    Some(saved.settings)
}

/// What the window was made with, as `main` set it (with the title bar when it didn't).
pub(super) fn decorated_now() -> bool {
    DECORATED_AT_START.load(Ordering::Relaxed)
}

/// macOS needs a titled window for native rounded corners and shadows. Its content extends
/// behind a transparent title bar, with the native title and buttons hidden.
pub(super) fn viewport(decorated: bool) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default();
    if cfg!(target_os = "macos") {
        builder
            .with_decorations(true)
            .with_fullsize_content_view(!decorated)
            .with_titlebar_shown(decorated)
            .with_title_shown(decorated)
            .with_titlebar_buttons_shown(decorated)
    } else {
        builder.with_decorations(decorated)
    }
}

/// Gives the window the system's title bar back or takes it away, when that changes.
/// `decorated` is what the window has now.
pub(super) fn apply(ctx: &egui::Context, settings: &Settings, decorated: &mut bool) {
    let want = !wanted(settings);
    if *decorated != want {
        *decorated = want;
        // macOS keeps the native frame; sync_border updates its title bar in place.
        if !cfg!(target_os = "macos") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(want));
        }
    }
}

/// Windows 11 draws a light line one pixel wide around every window. Under the system's title
/// bar it goes unnoticed; along the tab strip it reads as an edge of the first tab, so it is
/// taken away while the tabs are in the title bar (the window's shadow still sets it apart).
/// `shown` is what was last asked of Windows.
/// On macOS this synchronizes the transparent native title bar instead.
pub(super) fn sync_border(
    window: &impl raw_window_handle::HasWindowHandle,
    settings: &Settings,
    shown: &mut Option<bool>,
) {
    let want = !wanted(settings);
    if *shown != Some(want) {
        *shown = Some(want);
        set_window_border(window, want);
    }
}

/// Gives the window Windows' own border, or none (`DWMWA_BORDER_COLOR`). Windows before 11
/// draw no such border and refuse the attribute, which leaves them as they are.
/// Window state the title bar is drawn by.
#[derive(Clone, Copy)]
struct WindowState {
    maximized: bool,
    fullscreen: bool,
    focused: bool,
}

fn window_state(ctx: &egui::Context) -> WindowState {
    ctx.input(|i| {
        let v = i.viewport();
        WindowState {
            maximized: v.maximized.unwrap_or(false),
            fullscreen: v.fullscreen.unwrap_or(false),
            focused: v.focused.unwrap_or(true),
        }
    })
}

/// The width of the caption buttons at the strip's end; none in fullscreen, which has none.
pub(super) fn captions_width(ctx: &egui::Context) -> f32 {
    if window_state(ctx).fullscreen {
        0.0
    } else {
        CAPTIONS_W
    }
}

/// Minimize, maximize (or restore) and close in `rect`, at the strip's end.
pub(super) fn captions(ui: &mut Ui, t: &Tokens, rect: Rect) {
    if rect.width() <= 0.0 {
        return;
    }
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
        caption_buttons(ui, t)
    });
}

/// Minimize, maximize (or restore) and close, right to left from the strip's end.
fn caption_buttons(ui: &mut Ui, t: &Tokens) {
    let state = window_state(ui.ctx());
    ui.spacing_mut().item_spacing.x = 0.0;
    let ctx = ui.ctx().clone();
    if caption_button(ui, t, Caption::Close, state.focused).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let size = if cfg!(target_os = "macos") {
        Caption::Fullscreen
    } else if state.maximized {
        Caption::Restore
    } else {
        Caption::Maximize
    };
    if caption_button(ui, t, size, state.focused).clicked() {
        if cfg!(target_os = "macos") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!state.maximized));
        }
    }
    if caption_button(ui, t, Caption::Minimize, state.focused).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

/// Whether the empty middle of the folded branches strip moves the window too. Off for now: a
/// grab just beside one of its buttons would move the window.
pub(crate) const FOLDED_STRIP_MOVES_WINDOW: bool = false;

/// Empty space `rect` elsewhere along the window's edges (the toolbar's end, a panel footer)
/// that moves the window like the strip's, while the tabs are in the title bar; `name` tells
/// the pieces apart.
pub(crate) fn window_drag(ui: &mut Ui, rect: Rect, name: &str) {
    if wanted(&Settings::get(ui.ctx())) {
        drag_area(ui, rect, name);
    }
}

/// The strip's empty space `rect` (`name` tells its pieces apart): dragged, it moves the window;
/// double-clicked, it maximizes or restores it.
pub(super) fn drag_area(ui: &mut Ui, rect: Rect, name: &str) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let id = egui::Id::new(("title-bar-drag", name));
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    crate::a11y::a11y(
        &resp,
        egui::WidgetType::Button,
        crate::i18n::text("interaction-home-window-grip"),
    );
    moves_window(ui.ctx(), &resp);
}

/// The width of [`handle`] with the space before it.
pub(super) const HANDLE_W: f32 = 28.0 + 6.0;

/// A grip the size of the strip's buttons, there only to move the window by (and maximize it
/// with a double click): the strip always has one, however many tabs scroll under its end.
pub(super) fn handle(ui: &mut Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
    let resp = ui
        .interact(
            rect,
            egui::Id::new("title-bar-handle"),
            Sense::click_and_drag(),
        )
        .on_hover_cursor(CursorIcon::Grab)
        .on_hover_text(crate::i18n::text("interaction-home-window-drag"));
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.pressed);
    }
    let tint = if resp.hovered() { t.text } else { t.text_faint };
    crate::icons::paint(ui, rect, "grip-vertical", 17.0, tint);
    crate::a11y::a11y(
        &resp,
        egui::WidgetType::Button,
        crate::i18n::text("interaction-home-window-grip"),
    );
    moves_window(ui.ctx(), &resp);
}

/// Dragged, `resp` moves the window; double-clicked, it maximizes or restores it.
fn moves_window(ctx: &egui::Context, resp: &Response) {
    if resp.double_clicked()
        || resp.clicked() && resp.has_focus() && ctx.input(|i| !i.pointer.any_released())
    {
        let maximized = window_state(ctx).maximized;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        // The system moves it from here on, snapping it to the screen's edges as it does any
        // window, and puts a maximized one back to its size.
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

#[cfg(test)]
#[path = "title_bar/tests.rs"]
mod tests;
