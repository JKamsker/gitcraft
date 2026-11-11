#[path = "view/calendar.rs"]
mod calendar;
use super::*;
use crate::{repo::Action, theme::Tokens};
use calendar::*;
use egui::{RichText, Sense, vec2};

impl RepoTab {
    pub(in crate::repo) fn activity_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let Some(view) = &mut self.history_activity else {
            return;
        };
        let mut refresh = false;
        let mut query = false;
        let mut close = false;
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-ab181f77",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui
                    .button(crate::i18n::text(
                        "repo-workflows-history_activity-activity-view-8efbb1ca",
                    ))
                    .clicked();
                refresh = ui
                    .button(crate::i18n::text(
                        "repo-workflows-history_activity-activity-view-4a7fda65",
                    ))
                    .clicked();
                if view.loading
                    && ui
                        .button(crate::i18n::text(
                            "repo-workflows-history_activity-activity-view-cc4e9aaf",
                        ))
                        .clicked()
                {
                    view.cancel();
                }
            });
        });
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut view.page,
                Page::Calendar,
                crate::i18n::text("repo-workflows-history_activity-activity-view-60b300fd"),
            );
            ui.selectable_value(
                &mut view.page,
                Page::Contributors,
                crate::i18n::text("repo-workflows-history_activity-activity-view-0e407d89"),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-956a51f6",
            ));
            query |= ui
                .selectable_value(
                    &mut view.clock,
                    Clock::Author,
                    crate::i18n::text("repo-workflows-history_activity-activity-view-1556feb2"),
                )
                .changed();
            query |= ui
                .selectable_value(
                    &mut view.clock,
                    Clock::Committer,
                    crate::i18n::text("repo-workflows-history_activity-activity-view-4562ddd7"),
                )
                .changed();
            ui.label(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-21819769",
            ));
            ui.add(
                egui::TextEdit::singleline(&mut view.from)
                    .desired_width(92.0)
                    .hint_text("YYYY-MM-DD"),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-afc6abac",
            ));
            ui.add(
                egui::TextEdit::singleline(&mut view.through)
                    .desired_width(92.0)
                    .hint_text("YYYY-MM-DD"),
            );
            query |= ui
                .button(crate::i18n::text(
                    "repo-workflows-history_activity-activity-view-04b6c8fb",
                ))
                .clicked();
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-history_activity-activity-view-126236b2",
                ))
                .clicked()
            {
                view.from.clear();
                view.through.clear();
                query = true;
            }
        });
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-ac3896c6",
            ))
            .small()
            .color(t.text_dim),
        );
        if view.loading {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(16.0));
                ui.label(crate::i18n::message(
                    "repo-workflows-history_activity-activity-view-e23552d7",
                    &[(
                        "view-progress-load-ordering-rela-1",
                        (view.progress.load(Ordering::Relaxed)).into(),
                    )],
                ));
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(error) = &view.error {
            ui.colored_label(
                t.danger,
                crate::repo::activity::display_workflow_error(error),
            );
        }
        if let Some(history) = &view.history {
            let coverage = if history.more {
                crate::i18n::text("repo-workflows-history_activity-activity-view-fe26c97a")
            } else {
                crate::i18n::text("repo-workflows-history_activity-activity-view-fd008e90")
            };
            ui.label(crate::i18n::message(
                "repo-workflows-history_activity-activity-view-e136531e",
                &[
                    ("coverage", (coverage).into()),
                    ("history-commits-len-1", (history.commits.len()).into()),
                    ("history-tips-2", (history.tips).into()),
                    (
                        "if-history-shallow-shallow-clone-3",
                        (if history.shallow {
                            crate::i18n::text(
                                "repo-workflows-history_activity-activity-view-101c65f2",
                            )
                        } else {
                            ""
                        })
                        .into(),
                    ),
                ],
            ));
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-history_activity-activity-view-7c0879db",
                ))
                .small()
                .color(t.text_dim),
            );
            if history.bytes_limited {
                ui.colored_label(
                    t.text_dim,
                    crate::i18n::text("repo-workflows-history_activity-activity-view-d402be35"),
                );
            }
            if history.more
                && !history.bytes_limited
                && history.limit < 100_000
                && ui
                    .add_enabled(
                        !view.loading,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-history_activity-activity-view-a20fa1bb",
                        )),
                    )
                    .clicked()
            {
                view.limit = (history.limit * 2).min(100_000);
                refresh = true;
            }
            if history.more && history.limit >= 100_000 {
                ui.label(crate::i18n::text(
                    "repo-workflows-history_activity-activity-view-4d167723",
                ));
            }
        }
        if let Some(summary) = &view.summary
            && let (Some((first, _)), Some((last, _))) = (
                summary.calendar.first_key_value(),
                summary.calendar.last_key_value(),
            )
        {
            ui.label(crate::i18n::message(
                "repo-workflows-history_activity-activity-view-6561da04",
                &[
                    ("first", crate::i18n::date_iso(&first.to_string()).into()),
                    ("last", crate::i18n::date_iso(&last.to_string()).into()),
                ],
            ));
        }
        if view.history.as_ref().is_some_and(|h| h.shallow) {
            ui.label(crate::i18n::text(
                "repo-workflows-history_activity-activity-view-e6f89512",
            ));
        }
        ui.separator();
        let summary = view.summary.take();
        egui::ScrollArea::vertical()
            .id_salt("activity_scroll")
            .show(ui, |ui| {
                let Some(summary) = &summary else { return };
                if summary.calendar.is_empty() {
                    ui.label(crate::i18n::text(
                        "repo-workflows-history_activity-activity-view-baa24799",
                    ));
                    return;
                }
                if view.page == Page::Calendar {
                    query |= calendar(ui, view, summary, &t);
                } else {
                    authors(ui, view, summary, &t);
                }
                ui.separator();
                if view.drag.is_some() {
                    ui.label(crate::i18n::text(
                        "repo-workflows-history_activity-activity-view-7e33d4ef",
                    ));
                } else {
                    commits(ui, view, summary, acts);
                }
            });
        view.summary = summary;
        if close {
            self.history_activity = None;
        } else if refresh {
            self.analyze_activity(true, ui.ctx());
        } else if query {
            self.analyze_activity(false, ui.ctx());
        }
    }
}

fn authors(ui: &mut egui::Ui, view: &mut Activity, summary: &Summary, t: &Tokens) {
    ui.label(crate::i18n::text(
        "repo-workflows-history_activity-authors-6e25b1da",
    ));
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-history_activity-authors-76cb89fa",
        ))
        .small()
        .color(t.text_dim),
    );
    let range = page(ui, "authors", summary.contributors.len());
    egui::ScrollArea::horizontal()
        .id_salt("activity_author_columns")
        .show(ui, |ui| {
            egui::Grid::new("activity_authors")
                .striped(true)
                .show(ui, |ui| {
                    for s in [
                        crate::i18n::text("repo-workflows-history_activity-authors-d95082a2"),
                        crate::i18n::text("repo-workflows-history_activity-authors-d09648c6"),
                        crate::i18n::text("repo-workflows-history_activity-authors-ce199c9e"),
                        crate::i18n::text("repo-workflows-history_activity-authors-6cbebfa2"),
                        crate::i18n::text("repo-workflows-history_activity-authors-e707bedf"),
                    ] {
                        ui.strong(s);
                    }
                    ui.end_row();
                    for author in &summary.contributors[range] {
                        let identity = (author.name.clone(), author.email.clone());
                        if ui
                            .selectable_label(
                                view.author.as_ref() == Some(&identity),
                                format!("{} <{}>", author.name, author.email),
                            )
                            .clicked()
                        {
                            view.author = Some(identity);
                        }
                        ui.label(crate::i18n::number(author.commits));
                        ui.label(crate::i18n::number(author.files));
                        ui.label(crate::i18n::number(author.days));
                        ui.label(format!(
                            "{} – {}",
                            crate::i18n::date_iso(&author.first.to_string()),
                            crate::i18n::date_iso(&author.last.to_string())
                        ));
                        ui.end_row();
                    }
                });
        });
    if summary.contributors.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-history_activity-authors-65845cf6",
        ));
    }
    if view.author.is_some()
        && ui
            .button(crate::i18n::text(
                "repo-workflows-history_activity-authors-626a79db",
            ))
            .clicked()
    {
        view.author = None;
    }
}

fn commits(ui: &mut egui::Ui, view: &Activity, summary: &Summary, acts: &mut Vec<Action>) {
    let Some(history) = &view.history else { return };
    let selected: Vec<_> = summary
        .selected
        .iter()
        .filter_map(|i| history.commits.get(*i))
        .filter(|c| {
            view.author
                .as_ref()
                .is_none_or(|(name, email)| c.name == *name && c.email == *email)
        })
        .collect();
    ui.strong(crate::i18n::message(
        "repo-workflows-history_activity-commits-640e56ec",
        &[("selected-len-1", (selected.len()).into())],
    ));
    let range = page(ui, "commits", selected.len());
    for commit in &selected[range] {
        let label = format!(
            "{}  {}  {} · {}",
            crate::i18n::date_iso(&commit.day(view.query.clock).to_string()),
            commit.hash.get(..8).unwrap_or(&commit.hash),
            commit.subject,
            commit.name
        );
        if ui
            .button(label)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-history_activity-commits-f0d98513",
            ))
            .clicked()
        {
            acts.push(Action::ActivityCommit(commit.hash.clone()));
        }
    }
}

/// Both lists have bounded widget trees even when the history has 100,000 distinct authors.
fn page(ui: &mut egui::Ui, name: &str, len: usize) -> std::ops::Range<usize> {
    let id = ui.id().with(("activity_page", name));
    let mut page = ui.data_mut(|d| *d.get_temp_mut_or_default::<usize>(id));
    page = page.min(len.saturating_sub(1) / 100);
    ui.push_id(name, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    page > 0,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-history_activity-page-1208ec01",
                    )),
                )
                .clicked()
            {
                page -= 1;
            }
            ui.label(crate::i18n::message(
                "repo-workflows-history_activity-page-dbaceb71",
                &[
                    ("page-1-1", (page + 1).into()),
                    (
                        "len-div-ceil-100-max-1-2",
                        (len.div_ceil(100).max(1)).into(),
                    ),
                ],
            ));
            if ui
                .add_enabled(
                    (page + 1) * 100 < len,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-history_activity-page-c08ac736",
                    )),
                )
                .clicked()
            {
                page += 1;
            }
        })
    });
    ui.data_mut(|d| d.insert_temp(id, page));
    page * 100..((page + 1) * 100).min(len)
}

#[cfg(test)]
#[path = "view/fluent_calendar_tests.rs"]
mod fluent_calendar_tests;

#[cfg(test)]
#[path = "view/fluent_date_paint_tests.rs"]
mod fluent_date_paint_tests;
