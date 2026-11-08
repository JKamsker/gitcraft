//! A context-menu entry: the key that does the same, small on its right, and, when it can't be
//! used, why in small text under it rather than only in a tooltip.

use egui::{Response, RichText, Ui, WidgetText};

use crate::keymap::{self, Command};
use crate::theme::Tokens;

/// The widest a reason under an entry gets before it wraps.
const REASON_WIDTH: f32 = 280.0;

/// A context menu opened with the pointer or Shift+F10 while its control has focus.
/// Keyboard openings stay anchored to the control until dismissed, independent of the pointer.
pub(crate) fn context(response: &Response) -> egui::Popup<'_> {
    let key = response
        .id
        .with(("keyboard-context-menu", response.ctx.viewport_id()));
    let keyboard = response.has_focus()
        && response
            .ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10));
    if keyboard || response.secondary_clicked() {
        response.ctx.data_mut(|d| d.insert_temp(key, keyboard));
    }
    if response
        .ctx
        .data(|d| d.get_temp::<bool>(key))
        .unwrap_or(false)
    {
        egui::Popup::menu(response).open_memory(if keyboard {
            Some(egui::SetOpenCommand::Bool(true))
        } else if response.clicked() {
            Some(egui::SetOpenCommand::Bool(false))
        } else {
            None
        })
    } else {
        egui::Popup::context_menu(response)
    }
}

/// A menu entry, built up and then shown with [`MenuItem::show`].
pub(crate) struct MenuItem<'a> {
    label: WidgetText,
    icon: &'static str,
    annotation: Option<WidgetText>,
    selected: Option<bool>,
    enabled: bool,
    keys: Option<Command>,
    refused: Option<&'a str>,
    /// Whether the reason is written under the entry (else the entry below says the same).
    explained: bool,
}

impl<'a> MenuItem<'a> {
    pub(crate) fn new(label: impl Into<WidgetText>) -> Self {
        Self {
            label: label.into(),
            icon: "command",
            annotation: None,
            selected: None,
            enabled: true,
            keys: None,
            refused: None,
            explained: true,
        }
    }

    pub(crate) fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub(crate) fn annotation(mut self, text: impl Into<WidgetText>) -> Self {
        self.annotation = Some(text.into());
        self
    }

    /// Greyed out while a repository operation runs, with a visible reason.
    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The key of `command`, shown on the right.
    pub(crate) fn keys(mut self, command: Command) -> Self {
        self.keys = Some(command);
        self
    }

    pub(crate) fn keys_when(self, command: Command, show: bool) -> Self {
        if show { self.keys(command) } else { self }
    }

    /// Why the entry can't be used, if it can't: it is greyed out and says so under it.
    pub(crate) fn refused(mut self, why: Option<&'a str>) -> Self {
        self.refused = why;
        self
    }

    /// Greyed out for its reason, but with the reason written only under the entry below, which
    /// has the same one.
    pub(crate) fn explained_below(mut self, below: bool) -> Self {
        self.explained = !below;
        self
    }

    fn button(&self, ui: &Ui) -> egui::Button<'static> {
        let t = Tokens::get(ui.ctx());
        let enabled = self.enabled && self.refused.is_none();
        let mut button = egui::Button::image_and_text(
            crate::icons::image(self.icon, 14.0, if enabled { t.icon } else { t.text_faint }),
            self.label.clone(),
        );
        if let Some(selected) = self.selected {
            button = button.selected(selected);
        }
        if let Some(text) = &self.annotation {
            button = button.right_text(text.clone());
        }
        if let Some(command) = self.keys {
            button = button.shortcut_text(keymap::label(ui.ctx(), command));
        }
        button
    }

    fn explanation(&self, ui: &mut Ui) {
        let refused = self
            .refused
            .or((!self.enabled).then_some(crate::i18n::text("interaction-presentation-busy")));
        if let Some(why) = refused.filter(|why| self.explained && !why.is_empty()) {
            reason(ui, why);
        }
    }

    pub(crate) fn show(self, ui: &mut Ui) -> Response {
        let response = ui.add_enabled(self.enabled && self.refused.is_none(), self.button(ui));
        self.explanation(ui);
        response
    }

    /// Uses egui's normal submenu navigation, with the same icon column and refusal treatment.
    pub(crate) fn submenu<R>(
        self,
        ui: &mut Ui,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let button = self.button(ui);
        let salt = ui.next_auto_id().with("bounded-menu-item");
        let content = |ui: &mut Ui| bounded_menu(ui, salt, content);
        let response = ui
            .add_enabled_ui(self.enabled && self.refused.is_none(), |ui| {
                let (response, inner) = if egui::containers::menu::is_in_menu(ui) {
                    egui::containers::menu::SubMenuButton::from_button(button).ui(ui, content)
                } else {
                    egui::containers::menu::MenuButton::from_button(button).ui(ui, content)
                };
                egui::InnerResponse::new(inner.map(|i| i.inner), response)
            })
            .inner;
        self.explanation(ui);
        response
    }
}

/// Why the entry above can't be used, small and faint, lined up with its label.
fn reason(ui: &mut Ui, why: &str) {
    let t = Tokens::get(ui.ctx());
    let pad = ui.spacing().button_padding.x as i8;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: pad + 22,
            right: pad,
            top: -2,
            bottom: 5,
        })
        .show(ui, |ui| {
            ui.set_max_width(REASON_WIDTH);
            ui.add(egui::Label::new(RichText::new(why).size(11.0).color(t.text_faint)).wrap());
        });
}

/// For entries one under the other, each with why it can't be used: whether the next entry
/// gives the same reason, which is then written once, under the last of them.
pub(crate) fn same_reason_below<T: PartialEq>(reasons: &[Option<T>]) -> Vec<bool> {
    (0..reasons.len())
        .map(|i| reasons[i].is_some() && reasons.get(i + 1) == Some(&reasons[i]))
        .collect()
}

/// The in-viewport fallback for platforms without native transient menus. egui's Popup selects
/// the best alignment and constrains its Area to the viewport; bound every nested menu too.
/// Stable salts preserve scroll/focus state while repository rows move underneath the popup.
pub(crate) fn bounded_menu<R>(
    ui: &mut Ui,
    salt: impl egui::AsIdSalt,
    content: impl FnOnce(&mut Ui) -> R,
) -> R {
    let room = ui.ctx().content_rect().size();
    // Leave a second column for hover submenus: a full-window parent would force the
    // child over its siblings, making sibling hover targets impossible to reach.
    let width = (room.x * 0.5 - 40.0).clamp(1.0, 360.0);
    ui.set_max_width(width);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    egui::ScrollArea::both()
        .id_salt(salt)
        .max_width(width)
        .max_height((room.y - 32.0).max(1.0))
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.set_max_width(width);
            content(ui)
        })
        .inner
}

/// Keep the toolkit's submenu hover, keyboard and close behavior, with reachable scrolling.
pub(crate) fn group(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    group_named(ui, label, label, content);
}

/// A raw stable menu salt and independently localized caption.
pub(crate) fn group_named(ui: &mut Ui, salt: &str, label: &str, content: impl FnOnce(&mut Ui)) {
    // A salted child UI still mixes its position into the automatic button ID. Use an
    // explicit scope ID so inserting a sibling cannot hand its open submenu to another
    // branch (and silently change the action target). Include the parent's persistent ID
    // to distinguish identical group labels in different commit/branch menus.
    let id = ui.make_persistent_id(("menu-group", salt));
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
        ui.menu_button(label, |ui| {
            bounded_menu(ui, salt, content);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::same_reason_below;

    #[test]
    fn a_reason_shared_by_entries_in_a_row_is_written_once() {
        let r = |s: &'static str| Some(s);
        assert_eq!(
            same_reason_below(&[r("a"), r("a"), r("a"), None]),
            [true, true, false, false]
        );
        assert_eq!(
            same_reason_below(&[r("a"), r("b"), r("b"), r("a")]),
            [false, true, false, false]
        );
        assert_eq!(same_reason_below::<&str>(&[None, None]), [false, false]);
        assert_eq!(same_reason_below::<&str>(&[]), Vec::<bool>::new());
    }
}

#[cfg(test)]
mod area_d_tests {
    use egui_kittest::kittest::Queryable;

    #[test]
    fn area_d_tall_menu_scrolls_to_last_action_and_preserves_identity() {
        let mut h = egui_kittest::Harness::builder()
            .with_size([420.0, 240.0])
            .build_ui_state(
                |ui, picked: &mut Option<usize>| {
                    let r = ui.button("Long menu");
                    egui::Popup::menu(&r)
                        .style(crate::theme::menu_style)
                        .show(|ui| {
                            super::bounded_menu(ui, "many-branches", |ui| {
                                for i in 0..100 {
                                    if ui.button(format!("Branch action {i}")).clicked() {
                                        *picked = Some(i);
                                    }
                                }
                            });
                        });
                },
                None,
            );
        h.run();
        h.get_by_label("Long menu").click();
        h.run();
        let first = h.get_by_label("Branch action 0").rect();
        assert!(h.ctx.content_rect().contains_rect(first));
        h.get_by_label("Branch action 99").scroll_to_me();
        h.run();
        let last = h.get_by_label("Branch action 99").rect();
        assert!(
            h.ctx.content_rect().contains_rect(last),
            "last action is unreachable: {last:?}"
        );
        h.get_by_label("Branch action 99").click();
        h.run();
        assert_eq!(*h.state(), Some(99));
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use egui::{Key, Modifiers, Sense, vec2};
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn refusals_are_visible_below_disabled_actions_and_busy_is_explained() {
        let mut h = Harness::new_ui(|ui| {
            egui_extras::install_image_loaders(ui.ctx());
            ui.set_width(300.0);
            MenuItem::new("Delete branch")
                .icon("minus")
                .refused(Some("This branch is checked out in another worktree"))
                .show(ui);
            MenuItem::new("Fetch")
                .icon("cloud-download")
                .enabled(false)
                .show(ui);
        });
        h.run();
        let action = h.get_by_label("Delete branch");
        assert!(action.accesskit_node().is_disabled());
        let why = h.get_by_label("This branch is checked out in another worktree");
        assert!(
            why.accesskit_node()
                .data()
                .bounds()
                .zip(action.accesskit_node().data().bounds())
                .is_some_and(|(why, action)| why.y0 >= action.y1 - 3.0)
        );
        assert!(h.get_by_label("Fetch").accesskit_node().is_disabled());
        h.get_by_label("Another operation is running");
    }

    #[test]
    fn effective_shortcuts_are_painted_and_unbound_shortcuts_disappear() {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::Fetch.id().into(),
            vec![
                keymap::Stroke::new(Key::K, Modifiers::COMMAND),
                keymap::Stroke::new(Key::F, Modifiers::NONE),
            ],
        );
        settings.set(&ctx);
        let render = || {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                MenuItem::new("Fetch")
                    .icon("cloud-download")
                    .keys(Command::Fetch)
                    .show(ui);
            })
        };
        let out = render();
        fn text(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => found.push(t.galley.job.text.clone()),
                egui::Shape::Vec(v) => {
                    for s in v {
                        text(s, found);
                    }
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for shape in &out.shapes {
            text(&shape.shape, &mut found);
        }
        assert!(
            found.contains(&keymap::label(&ctx, Command::Fetch)),
            "menu must paint the configured chord: {found:?}"
        );
        out.drop_without_applying_deltas();
        settings
            .key_bindings
            .insert(Command::Fetch.id().into(), vec![]);
        settings.set(&ctx);
        let out = render();
        let mut found = Vec::new();
        for shape in &out.shapes {
            text(&shape.shape, &mut found);
        }
        assert_eq!(
            found.iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
            [&"Fetch".to_owned()]
        );
        out.drop_without_applying_deltas();
    }

    #[test]
    fn shift_f10_opens_at_the_focused_control_and_the_menu_action_activates() {
        let mut h = Harness::new_ui_state(
            |ui, count: &mut usize| {
                let (_, row) = ui.allocate_exact_size(vec2(200.0, 30.0), Sense::click());
                crate::a11y::a11y(&row, egui::WidgetType::Button, "Repository row");
                context(&row).show(|ui| {
                    if MenuItem::new("Inspect repository")
                        .icon("eye")
                        .show(ui)
                        .clicked()
                    {
                        *count += 1;
                    }
                });
                crate::a11y::end_frame(ui.ctx());
            },
            0usize,
        );
        h.run();
        h.key_press(Key::Tab);
        h.run();
        assert!(h.get_by_label("Repository row").is_focused());
        h.key_press_modifiers(Modifiers::SHIFT, Key::F10);
        h.run();
        let action = h.get_by_label("Inspect repository");
        let row = h.get_by_label("Repository row");
        assert!(
            action
                .accesskit_node()
                .data()
                .bounds()
                .zip(row.accesskit_node().data().bounds())
                .is_some_and(|(action, row)| action.y0 >= row.y0)
        );
        h.key_press(Key::ArrowDown);
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(*h.state(), 1);
    }
}
