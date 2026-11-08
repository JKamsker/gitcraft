//! A folding section of the commit's details: its title and what it found in a few words,
//! its body below when opened. Which ones are open is remembered.

use egui::{Color32, RichText, Sense, Ui};

use crate::icons;
use crate::theme::{self, Tokens};

/// What a section's header says it found.
pub(super) enum Summary {
    /// Still being read.
    Loading,
    /// A few words, in a colour.
    Text(String, Color32),
}

/// A section titled `title`, open at first when `open_at_first`; `refresh` puts a button
/// on its header that sets `*refreshed`. Its body is drawn by `body` when it is open.
pub(super) fn section(
    ui: &mut Ui,
    title: &str,
    summary: Summary,
    open_at_first: bool,
    refresh: Option<(&str, &mut bool)>,
    body: impl FnOnce(&mut Ui),
) {
    let t = Tokens::get(ui.ctx());
    let id = egui::Id::new(("commit-extras-open", title));
    // Kept only once folded or unfolded by hand: until then, `open_at_first` decides (which
    // may change as what the section shows is read).
    let mut open = ui
        .data_mut(|d| d.get_persisted::<bool>(id))
        .unwrap_or(open_at_first);
    ui.add_space(4.0);
    let has_refresh = refresh.is_some();
    let header = ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if let Some((tip, refreshed)) = refresh
                && icons::small_button(ui, "refresh-cw", tip).clicked()
            {
                *refreshed = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                icons::inline(
                    ui,
                    if open {
                        "chevron-down"
                    } else {
                        "chevron-right"
                    },
                    12.0,
                    t.text_faint,
                );
                let compact = ui.available_width() < 220.0;
                if !compact {
                    ui.label(
                        RichText::new(title.to_uppercase())
                            .font(theme::semibold(10.5))
                            .color(t.text_faint),
                    );
                }
                match summary {
                    Summary::Loading => {
                        if compact {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(title.to_uppercase())
                                        .font(theme::semibold(10.5))
                                        .color(t.text_faint),
                                )
                                .truncate(),
                            );
                        }
                        if ui.available_width() >= 10.0 {
                            ui.add(crate::spinner::Spinner::new().size(10.0));
                        }
                    }
                    Summary::Text(text, color) => {
                        let shown = if compact {
                            format!("{} · {text}", title.to_uppercase())
                        } else {
                            text.clone()
                        };
                        ui.add(
                            egui::Label::new(RichText::new(shown).color(color).small()).truncate(),
                        )
                        .on_hover_text(text);
                    }
                }
            });
        });
    });
    // The whole row folds and unfolds, but for the button on it.
    let row = header.response.rect;
    let fold = ui.interact(
        row.with_max_x(row.right() - if has_refresh { 26.0 } else { 0.0 }),
        id.with("header"),
        Sense::click(),
    );
    crate::a11y::expanded(&fold, title, open);
    if fold.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if fold.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    if open {
        ui.indent(id.with("body"), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            body(ui);
        });
    }
    ui.add_space(2.0);
}

/// A line of `text` in the faint colour, wrapping.
pub(super) fn faint(ui: &mut Ui, text: impl Into<String>) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text.into()).color(t.text_faint).small()).wrap());
}

/// An error, wrapping.
pub(super) fn error(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).color(t.danger)).wrap());
}

/// `text` (git's output) under a `title` with a button that copies it, in a box, selectable
/// and wrapping.
pub(super) fn output_box(ui: &mut Ui, title: &str, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        faint(ui, title);
        if icons::small_button(ui, "copy", crate::i18n::text("repo-core-1133")).clicked() {
            ui.ctx().copy_text(text.to_owned());
        }
    });
    egui::Frame::new()
        .fill(t.field)
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .font(theme::mono(11.0))
                        .color(t.text_dim),
                )
                .wrap()
                .selectable(true),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Until folded or unfolded by hand, a section follows what it is told at first (Notes
    /// opens once a note turns out to be there); then it keeps what was chosen.
    #[test]
    fn a_section_opens_as_told_until_chosen_by_hand() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let drawn_open = |open_at_first: bool| {
            let mut open = false;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                section(ui, "Test", Summary::Loading, open_at_first, None, |_| {
                    open = true;
                });
            })
            .drop_without_applying_deltas();
            open
        };
        assert!(!drawn_open(false));
        assert!(drawn_open(true));
        let id = egui::Id::new(("commit-extras-open", "Test"));
        ctx.data_mut(|d| d.insert_persisted(id, false));
        assert!(!drawn_open(true));
    }
}

#[cfg(test)]
mod responsive_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn responsive_extras_summary_reserves_refresh_and_preserves_fold_action() {
        for width in [240.0, 420.0, 800.0] {
            for scale in [1.0, 1.25, 1.5] {
                let mut styled = false;
                let mut h = Harness::builder().with_size([width / scale, 180.0]).with_pixels_per_point(scale)
                    .build_ui_state(move |ui, refreshed: &mut bool| {
                        if !styled { crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(ui.ctx(), &crate::theme::ThemeId::default(), &crate::theme::Tweaks::default()); egui_extras::install_image_loaders(ui.ctx()); styled = true;
                    return; }
                        egui::CentralPanel::default().show(ui, |ui| {
                            section(ui, "Contained in", Summary::Text("9 branches · 2 tags · first tag v0.3.5-dev.28-with-a-long-name".into(), egui::Color32::WHITE), false,
                                Some(("Read again", refreshed)), |ui| { ui.label("Expanded branches"); });
                        });
                    }, false);
                h.step();
                let refresh = h.get_by_label("Read again").rect();
                let summary = h
                    .get_by_label_contains(
                        "9 branches · 2 tags · first tag v0.3.5-dev.28-with-a-long-name",
                    )
                    .rect();
                assert!(
                    summary.right() <= refresh.left(),
                    "summary crosses refresh at {width}/{scale}: {summary:?} / {refresh:?}"
                );
                h.get_by_label("Read again").click();
                h.step();
                assert!(*h.state());
                assert!(
                    h.query_by_label("Expanded branches").is_none(),
                    "refresh toggled section"
                );
                h.get_by_label("Contained in").focus();
                h.key_press(egui::Key::Enter);
                h.step();
                assert!(h.query_by_label("Expanded branches").is_some());
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("extras-{width}-{scale}"),
                );
            }
        }
    }
}
