//! What a drag shows: the ghost by the pointer with what is dragged and what a drop there
//! would offer, the targets lit up, and the menu a drop opens.

use egui::{Color32, Pos2, Rect, RichText, Stroke, StrokeKind, Ui, vec2};

use super::offers::{DragItem, DropTarget, Offer};
use super::{Hover, hover_id};
use crate::icons;
use crate::repo::{Action, RepoTab, short};
use crate::theme::{self, Tokens};

/// The longest a name is shown in the ghost and the menu's title.
const MAX_LABEL: usize = 48;

/// The menu a drop opens: what was dropped where, and what that offers.
#[derive(Clone, Debug)]
pub(crate) struct DropMenu {
    at: Pos2,
    title: String,
    pub(crate) offers: Vec<Offer>,
}

impl DropMenu {
    pub(crate) fn new(at: Pos2, item: &DragItem, target: &DropTarget, offers: Vec<Offer>) -> Self {
        let target = match target {
            DropTarget::Branch(b) => b.name.clone(),
            DropTarget::DetachedHead => "HEAD".to_owned(),
            DropTarget::Commit(c) => format!("{} {}", short(&c.hash), c.summary),
        };
        Self {
            at,
            title: format!(
                "{} → {}",
                cut(&item.label(), MAX_LABEL / 2),
                cut(&target, MAX_LABEL / 2)
            ),
            offers,
        }
    }
}

/// `text`, cut to `n` characters with an ellipsis.
fn cut(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_owned();
    }
    text.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
}

/// Lights up a target that takes what is dragged, more so under the pointer.
pub(super) fn paint_zone(ui: &Ui, rect: Rect, takes: bool, hovered: bool) {
    if !takes {
        return;
    }
    let t = Tokens::get(ui.ctx());
    // Over the row, painted first, inside the list's clip.
    let painter = ui.painter();
    let fill = t.accent.gamma_multiply(if hovered { 0.22 } else { 0.08 });
    painter.rect_filled(rect, t.radius_sm, fill);
    let width = if hovered { 2.0 } else { 1.0 };
    painter.rect_stroke(
        rect,
        t.radius_sm,
        Stroke::new(
            width,
            t.accent.gamma_multiply(if hovered { 1.0 } else { 0.6 }),
        ),
        StrokeKind::Inside,
    );
}

impl RepoTab {
    /// Forgets the target hovered last frame: the targets drawn this frame say again.
    pub(in crate::repo) fn drag_frame_start(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.remove::<Hover>(hover_id()));
    }

    /// The drag ghost while something is dragged, and the drop menu while it is open.
    pub(in crate::repo) fn drag_overlay(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        self.drag_ghost(ctx);
        self.drop_menu_ui(ctx, acts);
    }

    fn drag_ghost(&self, ctx: &egui::Context) {
        let Some(payload) = self.dragged(ctx) else {
            return;
        };
        let Some(pos) = ctx.pointer_hover_pos() else {
            return;
        };
        let t = Tokens::get(ctx);
        let hover: Option<Hover> = ctx.data(|d| d.get_temp(hover_id()));
        let (hint, color) = match (self.drop_blocked(), hover) {
            (Some(why), _) => (why, t.warning),
            (
                None,
                Some(Hover {
                    first: Some(first),
                    more,
                    ..
                }),
            ) => {
                let hint = match more {
                    0 => first,
                    n => format!("{first} · {n} more"),
                };
                (hint, t.accent)
            }
            (None, Some(hover)) => (
                hover.why.unwrap_or_else(|| "Nothing to do here".to_owned()),
                t.text_faint,
            ),
            (None, None) => (
                "Drop on a branch or a commit · Esc cancels".to_owned(),
                t.text_faint,
            ),
        };
        egui::Area::new(egui::Id::new("drag-ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + vec2(16.0, 14.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            item_icon(ui, &payload.item, &t);
                            ui.label(
                                RichText::new(cut(&payload.item.label(), MAX_LABEL))
                                    .font(theme::medium(12.5))
                                    .color(t.text),
                            );
                        });
                        ui.label(RichText::new(hint).size(11.5).color(color));
                    });
            });
    }

    /// The menu of what the last drop offers, where it was dropped. A click elsewhere or Esc
    /// closes it; entries wait while an operation runs.
    fn drop_menu_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(menu) = &self.drop_menu else {
            return;
        };
        let t = Tokens::get(ctx);
        let idle = self.busy_op.is_none();
        let blocked = self.drop_blocked();
        let mut chosen = false;
        let shown = egui::Area::new(egui::Id::new("drop-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.at)
            .constrain(true)
            .show(ctx, |ui| {
                crate::theme::menu_style(ui.style_mut());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new(&menu.title).size(11.5).color(t.text_faint));
                        ui.add_space(16.0);
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    for offer in &menu.offers {
                        let button = ui
                            .add_enabled(idle, egui::Button::new(offer.label()))
                            .on_hover_text(offer.explain())
                            .on_disabled_hover_text(blocked.as_deref().unwrap_or_default());
                        if button.clicked() {
                            acts.push(offer.action());
                            chosen = true;
                        }
                    }
                });
            });
        // Esc closes it also while a text field has the keys, which the tab's shortcuts skip.
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let pressed_elsewhere = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !shown.response.rect.contains(p))
        });
        if chosen || pressed_elsewhere || esc {
            self.drop_menu = None;
        }
    }

    /// Esc closes the drop menu first; says whether there was one.
    pub(in crate::repo) fn close_drop_menu(&mut self) -> bool {
        self.drop_menu.take().is_some()
    }
}

/// A branch icon, or a commit's dot.
fn item_icon(ui: &mut Ui, item: &DragItem, t: &Tokens) {
    match item {
        DragItem::Branch(_) => icons::inline(ui, "git-branch", 14.0, t.icon),
        DragItem::Commit(_) => {
            let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle(
                rect.center(),
                4.5,
                Color32::TRANSPARENT,
                Stroke::new(2.0, t.icon),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::drag_drop::{DragBranch, Rev};

    /// Esc closes the drop menu even while a text field has the keys (the tab's shortcuts
    /// then don't see it).
    #[test]
    fn escape_closes_the_drop_menu() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred("/nonexistent/drop-menu-test".into(), &ctx);
        let feature = DragBranch {
            name: "feature".into(),
            remote: false,
            tip: "f".repeat(40),
        };
        let offer = Offer::Merge {
            from: Rev::Branch {
                name: "feature".into(),
                remote: false,
            },
            into: "main".into(),
        };
        tab.drop_menu = Some(DropMenu::new(
            Pos2::new(100.0, 100.0),
            &DragItem::Branch(feature.clone()),
            &DropTarget::Branch(feature),
            vec![offer],
        ));
        let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| tab.drag_overlay(ui.ctx(), &mut Vec::new()))
                .drop_without_applying_deltas();
        };
        frame(&mut tab, Vec::new());
        let open = tab.drop_menu.is_some();
        frame(
            &mut tab,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(open);
        assert!(tab.drop_menu.is_none());
    }
}
