//! Tabs in the title bar (Windows and macOS): the window is made without the system's title bar.
//! The tab strip takes its place, as in a browser. The strip's empty space moves the window and a
//! double click on it maximizes it, the caption buttons (minimize, maximize, close) sit at the
//! strip's end, and the window's edges resize it.
//! macOS keeps its native frame with a transparent title bar so the corners stay rounded.

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
/// every start.
pub(super) fn decorated_at_start(app_ron: Option<&Path>) -> bool {
    let decorated = decorated_by(app_ron);
    DECORATED_AT_START.store(decorated, Ordering::Relaxed);
    decorated
}

fn decorated_by(app_ron: Option<&Path>) -> bool {
    !app_ron
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| saved_settings(&text))
        .is_some_and(|settings| wanted(&settings))
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
#[cfg(windows)]
#[allow(unsafe_code)]
fn set_window_border(window: &impl raw_window_handle::HasWindowHandle, shown: bool) {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_COLOR_NONE, DwmSetWindowAttribute,
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let color: u32 = if shown {
        DWMWA_COLOR_DEFAULT
    } else {
        DWMWA_COLOR_NONE
    };
    // SAFETY: `win32.hwnd` is this app's window, alive while `handle` borrows it for this
    // call. `color` is a u32 on the stack that outlives the call, and its exact size is
    // passed; Windows only reads it. A failure is an error code, left alone: the border just
    // stays.
    let _ = unsafe {
        DwmSetWindowAttribute(
            win32.hwnd.get() as _,
            DWMWA_BORDER_COLOR as u32,
            std::ptr::from_ref(&color).cast(),
            size_of::<u32>() as u32,
        )
    };
}

/// Change the native title bar without replacing the titled frame that rounds the corners.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn set_window_border(window: &impl raw_window_handle::HasWindowHandle, shown: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSView, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility};
    use raw_window_handle::RawWindowHandle;

    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the borrowed window handle guarantees a live NSView, and the main-thread check
    // above ensures that AppKit is accessed only from the window's own thread.
    let view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else {
        return;
    };
    let mut style = window.styleMask();
    style.set(NSWindowStyleMask::FullSizeContentView, !shown);
    window.setStyleMask(style);
    window.setTitlebarAppearsTransparent(!shown);
    window.setTitleVisibility(if shown {
        NSWindowTitleVisibility::Visible
    } else {
        NSWindowTitleVisibility::Hidden
    });
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = window.standardWindowButton(kind) {
            button.setHidden(!shown);
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn set_window_border(_window: &impl raw_window_handle::HasWindowHandle, _shown: bool) {}

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
    let size = if state.maximized {
        Caption::Restore
    } else {
        Caption::Maximize
    };
    if caption_button(ui, t, size, state.focused).clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!state.maximized));
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
        .on_hover_text("Drag to move the window, double-click to maximize it");
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.pressed);
    }
    let tint = if resp.hovered() { t.text } else { t.text_faint };
    crate::icons::paint(ui, rect, "grip-vertical", 17.0, tint);
    moves_window(ui.ctx(), &resp);
}

/// Dragged, `resp` moves the window; double-clicked, it maximizes or restores it.
fn moves_window(ctx: &egui::Context, resp: &Response) {
    if resp.double_clicked() {
        let maximized = window_state(ctx).maximized;
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        // The system moves it from here on, snapping it to the screen's edges as it does any
        // window, and puts a maximized one back to its size.
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Caption {
    fn tooltip(self) -> &'static str {
        match self {
            Caption::Minimize => "Minimize",
            Caption::Maximize => "Maximize",
            Caption::Restore => "Restore down",
            Caption::Close => "Close",
        }
    }
}

/// A caption button as tall as the strip, drawn as Windows 11 draws its own.
fn caption_button(ui: &mut Ui, t: &Tokens, kind: Caption, focused: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(CAPTION_W, ui.available_height()), Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let hovered = resp.hovered() || pressed;
    let (fill, glyph) = match kind {
        Caption::Close if pressed => (CLOSE_PRESSED, Color32::WHITE),
        Caption::Close if hovered => (CLOSE_HOVER, Color32::WHITE),
        _ if pressed => (t.text.gamma_multiply(0.05), t.text),
        _ if hovered => (t.text.gamma_multiply(0.09), t.text),
        _ if focused => (Color32::TRANSPARENT, t.text),
        _ => (Color32::TRANSPARENT, t.text_faint),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    paint_glyph(painter, rect, kind, glyph);
    resp.on_hover_text(kind.tooltip())
}

/// Windows 11's glyph for `kind`, in the middle of `rect`, in lines one pixel wide.
fn paint_glyph(painter: &egui::Painter, rect: Rect, kind: Caption, color: Color32) {
    let ppp = painter.pixels_per_point();
    let px = 1.0 / ppp;
    let stroke = Stroke::new(px, color);
    // Whole pixels, the lines on pixel centres: crisp at any scale.
    let half = (GLYPH_HALF * ppp).round() / ppp;
    let c = pos2(
        painter.round_to_pixel_center(rect.center().x),
        painter.round_to_pixel_center(rect.center().y),
    );
    let (left, right, top, bottom) = (c.x - half, c.x + half, c.y - half, c.y + half);
    match kind {
        Caption::Minimize => {
            painter.line_segment([pos2(left, c.y), pos2(right, c.y)], stroke);
        }
        Caption::Maximize => {
            let square = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
            painter.rect_stroke(
                square,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Caption::Restore => {
            // The window in front, and the top and right edges of the one behind it.
            let step = 2.0 * px.max(1.0);
            let front = Rect::from_min_max(pos2(left, top + step), pos2(right - step, bottom));
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pos2(left + step, top), pos2(right, top)], stroke);
            painter.line_segment([pos2(right, top), pos2(right, bottom - step)], stroke);
        }
        Caption::Close => {
            painter.line_segment([pos2(left, top), pos2(right, bottom)], stroke);
            painter.line_segment([pos2(left, bottom), pos2(right, top)], stroke);
        }
    }
}

/// Resizing from the window's edges and corners, which have no system frame to do it: over
/// everything else, and only while the window is neither maximized nor fullscreen. macOS
/// resizes borderless windows natively; winit's BeginResize is unsupported there.
pub(super) fn resize_edges(ctx: &egui::Context) {
    if cfg!(target_os = "macos") {
        return;
    }
    let state = window_state(ctx);
    if state.maximized || state.fullscreen {
        return;
    }
    for (i, (dir, rect)) in edges(ctx.viewport_rect()).into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("window-edge", i)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(ctx, |ui| {
                let (_, resp) = ui.allocate_exact_size(rect.size(), Sense::click());
                let resp = resp.on_hover_cursor(cursor(dir));
                if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            });
    }
}

/// The bands along the inside of `window` that resize it: the four corners, then the four
/// edges between them, none overlapping.
fn edges(window: Rect) -> [(ResizeDirection, Rect); 8] {
    let (l, r, t, b) = (window.left(), window.right(), window.top(), window.bottom());
    let corner = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(CORNER, CORNER));
    let span_x = |y: f32| Rect::from_min_max(pos2(l + CORNER, y), pos2(r - CORNER, y + EDGE));
    let span_y = |x: f32| Rect::from_min_max(pos2(x, t + CORNER), pos2(x + EDGE, b - CORNER));
    [
        (ResizeDirection::NorthWest, corner(l, t)),
        (ResizeDirection::NorthEast, corner(r - CORNER, t)),
        (ResizeDirection::SouthWest, corner(l, b - CORNER)),
        (ResizeDirection::SouthEast, corner(r - CORNER, b - CORNER)),
        (ResizeDirection::North, span_x(t)),
        (ResizeDirection::South, span_x(b - EDGE)),
        (ResizeDirection::West, span_y(l)),
        (ResizeDirection::East, span_y(r - EDGE)),
    ]
}

fn cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 860.0))
    }

    #[test]
    fn the_edges_do_not_overlap() {
        let bands = edges(window());
        for (i, (_, a)) in bands.iter().enumerate() {
            for (_, b) in bands.iter().skip(i + 1) {
                assert!(!a.intersects(b.shrink(0.01)), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn the_edges_stay_inside_the_window() {
        for (_, band) in edges(window()) {
            assert!(window().contains_rect(band), "{band:?}");
        }
    }

    #[test]
    fn each_corner_and_edge_resizes_its_way() {
        let at = |p| {
            edges(window())
                .into_iter()
                .find(|(_, r)| r.contains(p))
                .map(|(d, _)| d)
        };
        assert_eq!(at(pos2(1.0, 1.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(at(pos2(1399.0, 859.0)), Some(ResizeDirection::SouthEast));
        assert_eq!(at(pos2(700.0, 1.0)), Some(ResizeDirection::North));
        assert_eq!(at(pos2(1.0, 430.0)), Some(ResizeDirection::West));
        assert_eq!(at(pos2(1398.0, 430.0)), Some(ResizeDirection::East));
        assert_eq!(at(pos2(700.0, 430.0)), None);
    }

    #[test]
    fn supported_platforms_can_replace_the_title_bar() {
        let settings = Settings {
            tabs_in_title_bar: true,
            ..Settings::default()
        };
        assert_eq!(wanted(&settings), cfg!(any(windows, target_os = "macos")));
        assert!(!wanted(&Settings::default()));
    }

    #[test]
    fn custom_title_bar_keeps_the_native_macos_frame() {
        let custom = viewport(false);
        if cfg!(target_os = "macos") {
            assert_eq!(custom.decorations, Some(true));
            assert_eq!(custom.fullsize_content_view, Some(true));
            assert_eq!(custom.titlebar_shown, Some(false));
            assert_eq!(custom.title_shown, Some(false));
            assert_eq!(custom.titlebar_buttons_shown, Some(false));
            let native = viewport(true);
            assert_eq!(native.fullsize_content_view, Some(false));
            assert_eq!(native.titlebar_buttons_shown, Some(true));
        } else {
            assert_eq!(custom.decorations, Some(false));
        }
    }

    #[test]
    fn the_setting_is_read_from_what_eframe_saved() {
        let saved = r#"{"app":"(open:[],settings:(tabs_in_title_bar:true))","egui":"()"}"#;
        assert!(saved_settings(saved).is_some_and(|s| s.tabs_in_title_bar));
        let off = r#"{"app":"(settings:(ui_scale:1.25))"}"#;
        assert!(saved_settings(off).is_some_and(|s| !s.tabs_in_title_bar));
        assert!(saved_settings("not ron").is_none());
        assert!(saved_settings(r#"{"egui":"()"}"#).is_none());
    }

    #[test]
    fn a_window_with_nothing_saved_has_the_title_bar() {
        assert!(decorated_by(None));
        assert!(decorated_by(Some(Path::new(
            "/nonexistent/gitcraft/app.ron"
        ))));
    }

    #[test]
    fn the_title_bar_is_switched_once_per_change() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let mut decorated = true;
        apply(&ctx, &settings, &mut decorated);
        assert!(decorated);
        settings.tabs_in_title_bar = true;
        apply(&ctx, &settings, &mut decorated);
        assert_eq!(decorated, !cfg!(any(windows, target_os = "macos")));
        settings.tabs_in_title_bar = false;
        apply(&ctx, &settings, &mut decorated);
        assert!(decorated);
    }
}
