//! Issue list filtering, stable row identities, and pointer/keyboard selection.

use super::*;

/// The filter, the issues that match it and, for the one picked, the branch's name and where it
/// goes. Whether Enter in the branch's name asks to start.
pub(super) fn choose(ui: &mut Ui, picker: &mut Picker) -> bool {
    let t = Tokens::get(ui.ctx());
    let edit = ui.add(
        egui::TextEdit::singleline(&mut picker.filter)
            .hint_text(crate::i18n::text(
                "repo-workflows-issue_work-choose-7dce44e8",
            ))
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if picker.focus == Some(Field::Filter) {
        picker.focus = None;
        edit.request_focus();
    }
    ui.add_space(6.0);
    let shown = picker.shown();
    // In the filter: ↑ and ↓ go through the list, Enter takes the one match there is.
    if edit.has_focus() || edit.lost_focus() {
        let (up, down, enter) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
            )
        });
        let at = shown.iter().position(|n| Some(*n) == picker.chosen);
        let next = match (at, up, down) {
            (None, _, true) | (None, true, _) => shown.first(),
            (Some(i), true, _) => shown.get(i.saturating_sub(1)),
            (Some(i), _, true) => shown.get(i + 1).or(shown.last()),
            _ => None,
        };
        if let Some(&number) = next {
            picker.choose(number);
        }
        if enter && edit.lost_focus() {
            if let [only] = shown[..] {
                picker.choose(only);
                picker.focus = Some(Field::Name);
            } else {
                // Enter alone doesn't pick for you: the keys stay in the filter.
                picker.focus = Some(Field::Filter);
            }
        }
    }
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-issue_work-choose-723dfd05",
                    ))
                    .color(t.text_faint),
                );
            }
            for (index, issue) in picker
                .list()
                .iter()
                .enumerate()
                .filter(|(n, _)| shown.contains(n))
            {
                if issue_row(ui, issue, picker.chosen == Some(index)) {
                    clicked = Some(index);
                }
            }
        });
    if picker.list().len() >= ISSUES_MAX {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-issue_work-choose-561b5212",
                &[("ISSUES-MAX", (ISSUES_MAX).into())],
            ))
            .size(11.5)
            .color(t.text_faint),
        );
    }
    if let Some(number) = clicked {
        picker.choose(number);
    }
    if picker.chosen_issue().is_none() {
        return false;
    }
    ui.add_space(8.0);
    let mut start = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-issue_work-choose-52656e81",
            ))
            .color(t.text_dim),
        );
        let name = ui.add(
            egui::TextEdit::singleline(&mut picker.name)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 4)),
        );
        if picker.focus == Some(Field::Name) {
            picker.focus = None;
            name.request_focus();
        }
        start = name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    });
    if picker.name.contains(' ') {
        picker.name = picker.name.replace(' ', "-");
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-issue_work-choose-104ab921",
            ))
            .color(t.text_dim),
        );
        if let Some(i) = widgets::switch(
            ui,
            &[
                crate::i18n::text("repo-workflows-issue_work-choose-aa3b1760"),
                crate::i18n::text("repo-workflows-issue_work-choose-e8f4aaee"),
            ],
            picker.worktree as usize,
        ) {
            picker.worktree = i == 1;
        }
    });
    start
}

/// Whether `issue` is one the filter `query` (lowercase) asks for.
pub(super) fn matches(issue: &Issue, query: &str) -> bool {
    query.is_empty()
        || issue.key().to_lowercase().contains(query)
        || issue.title.to_lowercase().contains(query)
        || issue
            .labels
            .iter()
            .any(|l| l.to_lowercase().contains(query))
}

/// One issue of the list: its number, title and labels. Whether it was clicked.
fn issue_row(ui: &mut Ui, issue: &Issue, chosen: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let fill = if chosen {
        t.row_selected
    } else {
        egui::Color32::TRANSPARENT
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(issue.key())
                        .font(theme::mono(12.0))
                        .color(t.accent),
                );
                ui.label(RichText::new(&issue.title).color(t.text));
                for label in &issue.labels {
                    widgets::chip(ui, label, t.text_dim);
                }
            });
        });
    if chosen {
        inner.response.scroll_to_me(None);
    }
    let response = ui.interact(
        inner.response.rect,
        ui.id().with(("issue-row", issue.identity())),
        egui::Sense::click(),
    );
    crate::a11y::a11y_selected(
        &response,
        || {
            crate::i18n::message(
                "repo-workflows-issue_work-issue-row-91765782",
                &[
                    ("issue-number-1", (issue.number).to_string().into()),
                    ("issue-title-2", (issue.title).clone().into()),
                ],
            )
        },
        chosen,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&issue.url)
        .clicked()
}
