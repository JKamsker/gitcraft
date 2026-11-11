//! Button surfaces, interaction handling, and title/footer/menu layouts.
use super::*;

/// A button's face in `rect`.
pub(super) fn paint(
    ui: &Ui,
    rect: Rect,
    button: AppButton,
    look: Look,
    hovered: bool,
    pressed: bool,
) {
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
        crate::a11y::a11y(&resp, egui::WidgetType::Button, button.tooltip(ui.ctx()));
        span = span.union(rect);
        // Where it would be without the opening, which the drop is worked out against.
        let shift = if opening.is_some() { pitch } else { 0.0 };
        slots.push(rect.center() - flow.step(shift));
        let at = animated(&ctx, button, rect.min, slide);
        let resp = resp.on_hover_text(button.tooltip(ui.ctx()));
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
        crate::a11y::a11y(&resp, egui::WidgetType::Button, button.tooltip(ui.ctx()));
        let resp = resp.on_hover_text(button.tooltip(ui.ctx()));
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
pub(super) fn opening_slot(ui: &mut Ui) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(SIZE), Sense::hover());
    let t = Tokens::get(ui.ctx());
    ui.painter()
        .rect_filled(rect, t.radius, t.text.gamma_multiply(0.06));
    rect
}

pub(super) fn start_drag(
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
pub(super) fn handle(ui: &Ui, button: AppButton, resp: &Response) {
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
            ui.label(
                egui::RichText::new(crate::i18n::text("interaction-presentation-move"))
                    .small()
                    .weak(),
            );
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
        } else {
            let mut item = crate::menu_item::MenuItem::new(button.label()).icon(button.icon());
            if let Some(command) = button.command() {
                item = item.keys(command);
            }
            if item.show(ui).clicked() {
                push(ui.ctx(), Event::Clicked(button));
            }
        }
    }
}
