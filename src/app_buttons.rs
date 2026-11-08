//! The app's own buttons: settings, the command palette, the theme menu and the manual. Each
//! sits in the title bar (the tab strip) or in the footer of a repository tab's left or right
//! side panel, where they stack up the panel's strip when it is folded. They are dragged
//! between those places and along them: the button stays under the pointer while an opening
//! for it moves to the place nearest the pointer (the others sliding apart to make it), and it
//! slides into that opening when let go (Esc puts it back). A right click moves one
//! as well. Wherever they are drawn, what they are used for is passed back to the app
//! ([`take_events`]), which carries it out.

use egui::{Align, Id, Pos2, Rect, Response, Sense, Ui, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::settings::{self, Settings, ThemePick};
use crate::theme::{self, ThemeId, Tokens};

/// Every button is this size, wherever it is.
const SIZE: f32 = 28.0;
/// A margin around a place's buttons counted as the place itself.
const CATCH: f32 = 16.0;
/// Seconds buttons take to slide into their places.
const SLIDE: f32 = 0.18;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum AppButton {
    Settings,
    Palette,
    Theme,
    Manual,
}

impl AppButton {
    pub const ALL: [AppButton; 4] = [
        AppButton::Settings,
        AppButton::Palette,
        AppButton::Theme,
        AppButton::Manual,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            AppButton::Settings => "settings",
            AppButton::Palette => "command",
            AppButton::Theme => "palette",
            AppButton::Manual => "book-open",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            AppButton::Settings => "Settings",
            AppButton::Palette => "Command palette (Ctrl+P)",
            AppButton::Theme => "Theme",
            AppButton::Manual => "Manual and keyboard shortcuts (F1)",
        }
    }

    /// Its name in a menu.
    pub fn label(self) -> &'static str {
        match self {
            AppButton::Settings => "Settings",
            AppButton::Palette => "Command palette",
            AppButton::Theme => "Theme",
            AppButton::Manual => "Manual",
        }
    }
}

/// Where buttons sit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Place {
    TitleBar,
    /// The footer of the branches panel, after its toggle.
    BottomLeft,
    /// The footer of the detail panel, before its toggle.
    BottomRight,
}

impl Place {
    pub const ALL: [Place; 3] = [Place::TitleBar, Place::BottomLeft, Place::BottomRight];

    pub fn label(self) -> &'static str {
        match self {
            Place::TitleBar => "Title bar",
            Place::BottomLeft => "Bottom left",
            Place::BottomRight => "Bottom right",
        }
    }
}

/// The buttons each place holds, in order from the place's outer end: the window's edge for the
/// title bar and the bottom right, the panel's toggle for the bottom left (a setting).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ButtonLayout {
    pub title_bar: Vec<AppButton>,
    pub bottom_left: Vec<AppButton>,
    pub bottom_right: Vec<AppButton>,
}

impl Default for ButtonLayout {
    fn default() -> Self {
        Self {
            title_bar: AppButton::ALL.to_vec(),
            bottom_left: Vec::new(),
            bottom_right: Vec::new(),
        }
    }
}

impl ButtonLayout {
    pub fn at(&self, place: Place) -> &[AppButton] {
        match place {
            Place::TitleBar => &self.title_bar,
            Place::BottomLeft => &self.bottom_left,
            Place::BottomRight => &self.bottom_right,
        }
    }

    fn at_mut(&mut self, place: Place) -> &mut Vec<AppButton> {
        match place {
            Place::TitleBar => &mut self.title_bar,
            Place::BottomLeft => &mut self.bottom_left,
            Place::BottomRight => &mut self.bottom_right,
        }
    }

    pub fn place_of(&self, button: AppButton) -> Place {
        Place::ALL
            .into_iter()
            .find(|&p| self.at(p).contains(&button))
            .unwrap_or(Place::TitleBar)
    }

    /// This layout with `button` at `place`, `index` among the buttons there once it is out of
    /// them (past the end puts it last).
    pub fn moved(&self, button: AppButton, place: Place, index: usize) -> ButtonLayout {
        let mut next = self.clone();
        for p in Place::ALL {
            next.at_mut(p).retain(|&b| b != button);
        }
        let list = next.at_mut(place);
        list.insert(index.min(list.len()), button);
        next
    }

    /// Every button in exactly one place: one missing (saved before it existed) goes at the end
    /// of the title bar, one listed twice stays where it is first.
    pub fn normalized(&self) -> ButtonLayout {
        let mut seen = Vec::new();
        let mut next = ButtonLayout {
            title_bar: Vec::new(),
            bottom_left: Vec::new(),
            bottom_right: Vec::new(),
        };
        for p in Place::ALL {
            for &b in self.at(p) {
                if !seen.contains(&b) {
                    seen.push(b);
                    next.at_mut(p).push(b);
                }
            }
        }
        for b in AppButton::ALL {
            if !seen.contains(&b) {
                next.title_bar.push(b);
            }
        }
        next
    }
}

/// The layout the settings hold, made whole.
pub fn layout(ctx: &egui::Context) -> ButtonLayout {
    Settings::get(ctx).button_layout.normalized()
}

/// What was done with a button, for the app to carry out.
#[derive(Clone, Debug)]
pub enum Event {
    Clicked(AppButton),
    Theme(ThemePick),
    /// Moved with a right click: to the end of the place.
    Move(AppButton, Place),
}

fn events_id() -> Id {
    Id::new("gitcraft-app-button-events")
}

fn push(ctx: &egui::Context, event: Event) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<Event>>(events_id())
            .push(event);
    });
}

/// What was done with the buttons since this was last asked.
pub fn take_events(ctx: &egui::Context) -> Vec<Event> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Event>>(events_id()))
        .unwrap_or_default()
}

fn theme_id() -> Id {
    Id::new("gitcraft-app-button-theme")
}

/// The theme in use, which the theme menu marks wherever it opens from.
pub fn set_current_theme(ctx: &egui::Context, id: &ThemeId) {
    ctx.data_mut(|d| d.insert_temp(theme_id(), id.clone()));
}

fn current_theme(ctx: &egui::Context) -> ThemeId {
    ctx.data(|d| d.get_temp::<ThemeId>(theme_id()))
        .unwrap_or_default()
}

/// How a place's buttons are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    /// The title bar's.
    Bar,
    /// A panel footer's, like the panel's toggle.
    Panel,
}

/// Which way a place's buttons go from its outer end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    RightToLeft,
    LeftToRight,
    BottomUp,
}

impl Flow {
    /// Whether `a` comes before `b` along the flow.
    fn before(self, a: Pos2, b: Pos2) -> bool {
        match self {
            Flow::RightToLeft => a.x > b.x,
            Flow::LeftToRight => a.x < b.x,
            Flow::BottomUp => a.y > b.y,
        }
    }

    /// One step along the flow, of `pitch`.
    fn step(self, pitch: f32) -> Vec2 {
        match self {
            Flow::RightToLeft => vec2(-pitch, 0.0),
            Flow::LeftToRight => vec2(pitch, 0.0),
            Flow::BottomUp => vec2(0.0, -pitch),
        }
    }
}

/// A button being dragged: which, how it looked where it came from, and where the pointer
/// holds it from its corner.
#[derive(Clone, Copy, Debug)]
struct Drag {
    button: AppButton,
    look: Look,
    grab: Vec2,
}

/// A place as drawn: where a dragged button is caught by it, which way it goes, and the middle
/// of each of its buttons as it would be without an opening.
#[derive(Clone, Debug)]
struct Zone {
    place: Place,
    catch: Rect,
    flow: Flow,
    slots: Vec<Pos2>,
}

/// The opening a dragged button is over: its place and its index there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Drop {
    place: Place,
    index: usize,
}

/// The dragging, kept from frame to frame.
#[derive(Clone, Debug, Default)]
struct DragState {
    drag: Option<Drag>,
    drop: Option<Drop>,
    /// The places as drawn last frame, and so far this frame.
    zones: Vec<Zone>,
    zones_now: Vec<Zone>,
    /// Until when buttons slide rather than jump to their places (a window resizing doesn't
    /// make them trail behind).
    settle_until: f64,
}

fn state_id() -> Id {
    Id::new("gitcraft-app-button-drag")
}

fn state(ctx: &egui::Context) -> DragState {
    ctx.data(|d| d.get_temp::<DragState>(state_id()))
        .unwrap_or_default()
}

fn set_state(ctx: &egui::Context, s: DragState) {
    ctx.data_mut(|d| d.insert_temp(state_id(), s));
}

fn button_id(button: AppButton) -> Id {
    Id::new(("app-button", button))
}

/// Before anything is drawn: ends a drag let go (moving the button where it was dropped, in
/// the settings, so it is drawn there at once), and finds the opening one is over.
pub fn begin_frame(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.zones = std::mem::take(&mut s.zones_now);
    let Some(drag) = s.drag else {
        set_state(ctx, s);
        return;
    };
    let now = ctx.input(|i| i.time);
    let (down, cancel, pointer) = ctx.input(|i| {
        (
            i.pointer.primary_down(),
            i.key_pressed(egui::Key::Escape),
            i.pointer.latest_pos(),
        )
    });
    if cancel || !down {
        if let Some(drop) = s.drop.filter(|_| !cancel) {
            let mut settings = Settings::get(ctx);
            settings.button_layout = layout(ctx).moved(drag.button, drop.place, drop.index);
            settings.set(ctx);
        }
        s.drag = None;
        s.drop = None;
        s.settle_until = now + f64::from(SLIDE) + 0.1;
    } else {
        s.drop = pointer.and_then(|p| drop_at(&s.zones, p));
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    set_state(ctx, s);
    ctx.request_repaint();
}

/// The opening for `pointer` among `zones`: in the place nearest to it, wherever it is, before
/// the first of that place's buttons the pointer isn't past.
fn drop_at(zones: &[Zone], pointer: Pos2) -> Option<Drop> {
    let zone = zones.iter().min_by(|a, b| {
        let near = |z: &Zone| {
            (
                z.catch.distance_to_pos(pointer),
                z.catch.center().distance(pointer),
            )
        };
        near(a)
            .0
            .total_cmp(&near(b).0)
            .then(near(a).1.total_cmp(&near(b).1))
    })?;
    let index = zone
        .slots
        .iter()
        .filter(|&&slot| zone.flow.before(slot, pointer))
        .count();
    Some(Drop {
        place: zone.place,
        index,
    })
}

/// After everything is drawn: the dragged button over everything, where the pointer holds it,
/// or pulled toward the opening it is over.
pub fn end_frame(ctx: &egui::Context) {
    let s = state(ctx);
    let Some(drag) = s.drag else {
        if ctx.input(|i| i.time) < s.settle_until {
            ctx.request_repaint();
        }
        return;
    };
    let Some(pointer) = ctx.input(|i| i.pointer.latest_pos()) else {
        return;
    };
    // Held where the pointer took it. Kept as where it is shown, so that when let go it
    // slides from there into its place.
    let at = animated(ctx, drag.button, pointer - drag.grab, 0.0);
    floating(ctx, drag.button, drag.look, at, true);
}

/// Where `button` is shown on its way to `target`.
fn animated(ctx: &egui::Context, button: AppButton, target: Pos2, time: f32) -> Pos2 {
    let id = button_id(button);
    pos2(
        ctx.animate_value_with_time(id.with("x"), target.x, time),
        ctx.animate_value_with_time(id.with("y"), target.y, time),
    )
}

/// `button` drawn at `at` over everything (dragged, or sliding across panels to its place).
fn floating(ctx: &egui::Context, button: AppButton, look: Look, at: Pos2, lifted: bool) {
    egui::Area::new(button_id(button).with("floating"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .interactable(false)
        .constrain(false)
        .show(ctx, |ui| {
            let rect = Rect::from_min_size(at, Vec2::splat(SIZE));
            if lifted {
                let t = Tokens::get(ctx);
                ui.painter().rect_filled(rect, t.radius, t.card);
                ui.painter().rect_stroke(
                    rect,
                    t.radius,
                    egui::Stroke::new(1.0, t.field_border),
                    egui::StrokeKind::Inside,
                );
            }
            paint(ui, rect, button, look, lifted, false);
        });
}

/// A button's face in `rect`.
fn paint(ui: &Ui, rect: Rect, button: AppButton, look: Look, hovered: bool, pressed: bool) {
    let t = Tokens::get(ui.ctx());
    let painter = ui.painter();
    match look {
        Look::Bar => {
            if hovered || pressed {
                painter.rect_filled(rect, t.radius, t.pressed);
            }
        }
        Look::Panel => {
            let r = egui::CornerRadius::same(t.radius_sm as u8);
            if pressed {
                painter.rect_filled(rect, r, t.pressed);
            } else if hovered {
                painter.rect_filled(rect, r, t.hover);
                painter.rect_stroke(
                    rect,
                    r,
                    egui::Stroke::new(1.0, t.field_border),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    let tint = if hovered { t.text } else { t.icon };
    let size = if look == Look::Bar { 17.0 } else { 16.0 };
    icons::paint(ui, rect, button.icon(), size, tint);
}

/// The buttons of `place`, in a row or column laid out from the place's outer end along `flow`
/// (the layout of `ui` must go that way). `fixed` are shown after them without being part of
/// the place: the title bar shows the others' buttons on tabs that have no side panels.
pub fn area(ui: &mut Ui, place: Place, look: Look, flow: Flow, fixed: &[AppButton]) {
    let ctx = ui.ctx().clone();
    let s = state(&ctx);
    let dragged = s.drag.map(|d| d.button);
    let opening_at = s.drop.filter(|d| d.place == place).map(|d| d.index);
    let buttons: Vec<AppButton> = layout(&ctx)
        .at(place)
        .iter()
        .copied()
        .filter(|&b| Some(b) != dragged)
        .collect();
    let spacing = match flow {
        Flow::BottomUp => ui.spacing().item_spacing.y,
        _ => ui.spacing().item_spacing.x,
    };
    let pitch = SIZE + spacing;
    let slide = if s.drag.is_some() || ctx.input(|i| i.time) < s.settle_until {
        SLIDE
    } else {
        0.0
    };

    let avail = ui.available_rect_before_wrap();
    let start = match flow {
        Flow::RightToLeft => pos2(avail.right(), avail.center().y),
        Flow::LeftToRight => pos2(avail.left(), avail.center().y),
        Flow::BottomUp => pos2(avail.center().x, avail.bottom()),
    };
    let mut span = Rect::from_center_size(start, Vec2::splat(SIZE));
    let mut slots = Vec::new();
    let mut opening = None;
    for (i, &button) in buttons.iter().enumerate() {
        if opening_at == Some(i) {
            opening = Some(opening_slot(ui));
        }
        // Its own id wherever it is: one made from its index would pass to the next button
        // when it leaves, with the drag egui keeps for it.
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::hover());
        let resp = ui.interact(rect, button_id(button), Sense::click_and_drag());
        crate::a11y::a11y(&resp, egui::WidgetType::Button, button.tooltip());
        span = span.union(rect);
        // Where it would be without the opening, which the drop is worked out against.
        let shift = if opening.is_some() { pitch } else { 0.0 };
        slots.push(rect.center() - flow.step(shift));
        let at = animated(&ctx, button, rect.min, slide);
        let resp = resp.on_hover_text(button.tooltip());
        if at.distance(rect.min) < 0.5 {
            // While one is dragged, the others don't light up under it.
            let idle = s.drag.is_none();
            paint(
                ui,
                rect,
                button,
                look,
                idle && resp.hovered(),
                idle && resp.is_pointer_button_down_on(),
            );
        } else {
            floating(&ctx, button, look, at, false);
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            start_drag(&ctx, button, look, place, i, at);
        }
        handle(ui, button, &resp);
    }
    if opening.is_none() && opening_at.is_some() {
        opening = Some(opening_slot(ui));
    }
    if let Some(rect) = opening {
        span = span.union(rect);
    }
    for &button in fixed {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::hover());
        let resp = ui.interact(rect, button_id(button).with("fixed"), Sense::click());
        crate::a11y::a11y(&resp, egui::WidgetType::Button, button.tooltip());
        let resp = resp.on_hover_text(button.tooltip());
        paint(
            ui,
            rect,
            button,
            look,
            resp.hovered(),
            resp.is_pointer_button_down_on(),
        );
        handle(ui, button, &resp);
    }

    // One slot more at the far end, so that the end can be dropped on.
    let far = span.translate(flow.step(pitch));
    let zone = Zone {
        place,
        catch: span.union(far).expand(CATCH),
        flow,
        slots,
    };
    let mut s = state(&ctx);
    s.zones_now.push(zone);
    set_state(&ctx, s);
}

/// The opening for a dragged button: a free slot, faintly marked.
fn opening_slot(ui: &mut Ui) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::hover());
    let t = Tokens::get(ui.ctx());
    ui.painter()
        .rect_filled(rect, t.radius, t.text.gamma_multiply(0.06));
    rect
}

fn start_drag(
    ctx: &egui::Context,
    button: AppButton,
    look: Look,
    place: Place,
    index: usize,
    at: Pos2,
) {
    let Some(pointer) = ctx.input(|i| i.pointer.press_origin()) else {
        return;
    };
    let mut s = state(ctx);
    s.drag = Some(Drag {
        button,
        look,
        grab: pointer - at,
    });
    // The opening where it was, until the pointer says otherwise.
    s.drop = Some(Drop { place, index });
    set_state(ctx, s);
}

/// What `button`, drawn as `resp`, does: the theme menu opens from the theme button, the others
/// are clicked; a right click offers to move it.
fn handle(ui: &Ui, button: AppButton, resp: &Response) {
    let ctx = ui.ctx();
    if button == AppButton::Theme {
        let current = current_theme(ctx);
        let mut pick = None;
        egui::Popup::menu(resp)
            .style(theme::menu_style)
            .show(|ui| pick = settings::theme_menu(ui, &current));
        if let Some(pick) = pick {
            push(ctx, Event::Theme(pick));
        }
    } else if resp.clicked() {
        push(ctx, Event::Clicked(button));
    }
    let here = layout(ctx).place_of(button);
    // Its own id: the theme menu opens from the same button.
    crate::menu_item::context(resp)
        .id(resp.id.with("move-to"))
        .style(theme::menu_style)
        .show(|ui| {
            ui.label(egui::RichText::new("Move to (or drag it)").small().weak());
            for place in Place::ALL {
                if ui.radio(place == here, place.label()).clicked() && place != here {
                    push(ui.ctx(), Event::Move(button, place));
                    ui.close();
                }
            }
        });
}

/// The buttons at `place` in a side panel's footer row, laid out from its outer end.
pub fn footer_row(ui: &mut Ui, place: Place) {
    let flow = if place == Place::BottomLeft {
        Flow::LeftToRight
    } else {
        Flow::RightToLeft
    };
    area(ui, place, Look::Panel, flow, &[]);
}

/// The buttons at `place`, stacked up a folded side panel's strip from just above its footer;
/// `slid` is how far the strip is slid out of place (see `widgets::slide_offset`). Returns the
/// height they take, which the strip's other buttons keep clear of.
pub fn folded_column(ui: &mut Ui, place: Place, slid: f32) -> f32 {
    let area_rect = ui.available_rect_before_wrap().translate(vec2(slid, 0.0));
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area_rect)
            .layout(egui::Layout::bottom_up(Align::Center)),
    );
    col.spacing_mut().item_spacing.y = 2.0;
    col.add_space(4.0);
    area(&mut col, place, Look::Panel, Flow::BottomUp, &[]);
    if layout(ui.ctx()).at(place).is_empty() && state(ui.ctx()).drag.is_none() {
        0.0
    } else {
        // A gap before what is above.
        col.min_rect().height() + 8.0
    }
}

/// Entries for `buttons` in a menu (the profile menu, when the title bar has no room for them).
pub fn menu_entries(ui: &mut Ui, buttons: &[AppButton]) {
    for &button in buttons {
        if button == AppButton::Theme {
            let current = current_theme(ui.ctx());
            let mut pick = None;
            ui.menu_button(button.label(), |ui| {
                pick = settings::theme_menu(ui, &current);
            });
            if let Some(pick) = pick {
                push(ui.ctx(), Event::Theme(pick));
            }
        } else if ui
            .button(button.label())
            .on_hover_text(button.tooltip())
            .clicked()
        {
            push(ui.ctx(), Event::Clicked(button));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use AppButton::{Manual, Palette, Settings as Gear, Theme};

    fn zone(place: Place, flow: Flow, slots: &[(f32, f32)]) -> Zone {
        Zone {
            place,
            catch: Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 400.0)),
            flow,
            slots: slots.iter().map(|&(x, y)| pos2(x, y)).collect(),
        }
    }

    #[test]
    fn every_button_starts_in_the_title_bar() {
        let layout = ButtonLayout::default();
        assert_eq!(layout.title_bar, AppButton::ALL);
        assert!(layout.bottom_left.is_empty() && layout.bottom_right.is_empty());
    }

    #[test]
    fn a_button_moves_to_another_place_at_an_index() {
        let layout = ButtonLayout::default().moved(Theme, Place::BottomRight, 0);
        let layout = layout.moved(Manual, Place::BottomRight, 0);
        assert_eq!(layout.bottom_right, [Manual, Theme]);
        assert_eq!(layout.title_bar, [Gear, Palette]);
    }

    #[test]
    fn a_button_is_reordered_within_its_place() {
        let layout = ButtonLayout::default().moved(Gear, Place::TitleBar, 99);
        assert_eq!(layout.title_bar, [Palette, Theme, Manual, Gear]);
        let layout = layout.moved(Manual, Place::TitleBar, 0);
        assert_eq!(layout.title_bar, [Manual, Palette, Theme, Gear]);
    }

    #[test]
    fn a_saved_layout_is_made_whole() {
        let saved = ButtonLayout {
            title_bar: vec![Theme, Theme],
            bottom_left: vec![Theme, Manual],
            bottom_right: Vec::new(),
        };
        let layout = saved.normalized();
        assert_eq!(layout.title_bar, [Theme, Gear, Palette]);
        assert_eq!(layout.bottom_left, [Manual]);
    }

    #[test]
    fn older_settings_without_a_layout_get_the_default() {
        let settings: Settings = ron::from_str("(ui_scale: 1.25)").unwrap_or_default();
        assert_eq!(settings.button_layout, ButtonLayout::default());
        let saved =
            "(title_bar: [Settings], bottom_left: [Theme], bottom_right: [Manual, Palette])";
        let layout: ButtonLayout = ron::from_str(saved).unwrap_or_default();
        assert_eq!(layout.bottom_right, [Manual, Palette]);
        assert_eq!(layout.normalized(), layout);
    }

    #[test]
    fn the_drop_is_before_the_first_button_not_passed() {
        // A title bar's buttons from the right edge leftwards.
        let zones = [zone(
            Place::TitleBar,
            Flow::RightToLeft,
            &[(300.0, 20.0), (266.0, 20.0)],
        )];
        let at = |x| drop_at(&zones, pos2(x, 20.0)).map(|d| d.index);
        assert_eq!(at(320.0), Some(0));
        assert_eq!(at(280.0), Some(1));
        assert_eq!(at(200.0), Some(2));
    }

    #[test]
    fn a_column_drops_from_the_bottom_up() {
        let zones = [zone(
            Place::BottomLeft,
            Flow::BottomUp,
            &[(18.0, 380.0), (18.0, 350.0)],
        )];
        let at = |y| drop_at(&zones, pos2(18.0, y)).map(|d| d.index);
        assert_eq!(at(395.0), Some(0));
        assert_eq!(at(360.0), Some(1));
        assert_eq!(at(300.0), Some(2));
    }

    #[test]
    fn the_nearer_place_wants_the_button_however_far_both_are() {
        let mut top = zone(Place::TitleBar, Flow::RightToLeft, &[]);
        top.catch = Rect::from_min_max(pos2(900.0, 0.0), pos2(1000.0, 40.0));
        let mut bottom = zone(Place::BottomRight, Flow::RightToLeft, &[]);
        bottom.catch = Rect::from_min_max(pos2(900.0, 800.0), pos2(1000.0, 840.0));
        let place = |y| drop_at(&[top.clone(), bottom.clone()], pos2(500.0, y)).map(|d| d.place);
        assert_eq!(place(410.0), Some(Place::TitleBar));
        assert_eq!(place(430.0), Some(Place::BottomRight));
        assert!(drop_at(&[], pos2(0.0, 0.0)).is_none());
    }
}
