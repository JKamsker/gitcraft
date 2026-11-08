use super::*;
use crate::{repo::Action, theme::Tokens};
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

fn calendar(ui: &mut egui::Ui, view: &mut Activity, summary: &Summary, t: &Tokens) -> bool {
    let counts = &summary.calendar;
    let Some(mut month) = view
        .month
        .or_else(|| counts.last_key_value().map(|(d, _)| d.month_start()))
    else {
        return false;
    };
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-history_activity-calendar-6a276950",
            ))
            .clicked()
        {
            month = month.shift_month(-1);
        }
        ui.label(month_heading(month));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-history_activity-calendar-74e53211",
            ))
            .clicked()
        {
            month = month.shift_month(1);
        }
        ui.checkbox(
            &mut view.months,
            crate::i18n::text("repo-workflows-history_activity-calendar-a54ddac5"),
        );
    });
    view.month = Some(month);
    if view.months {
        ui.horizontal(|ui| {
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-history_activity-calendar-91e33e3b",
                ))
                .clicked()
            {
                view.month = Some(month.shift_month(-12));
            }
            ui.label(month.year.to_string());
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-history_activity-calendar-a21a5dc6",
                ))
                .clicked()
            {
                view.month = Some(month.shift_month(12));
            }
        });
        let mut picked = None;
        egui::Grid::new("activity_months")
            .num_columns(4)
            .show(ui, |ui| {
                for m in 1..=12 {
                    let first = Day {
                        year: month.year,
                        month: m,
                        day: 1,
                    };
                    let last = Day {
                        day: first.month_days(),
                        ..first
                    };
                    let n: usize = counts.range(first..=last).map(|(_, n)| n).sum();
                    if ui
                        .button(crate::i18n::message(
                            "repo-workflows-history_activity-calendar-370eebba",
                            &[
                                (
                                    "month-year-1",
                                    (format!("{:04}", month.year)).clone().into(),
                                ),
                                ("m", (format!("{:02}", m)).clone().into()),
                                ("n", (n).into()),
                            ],
                        ))
                        .clicked()
                    {
                        picked = Some((first, last));
                    }
                    if m % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
        if let Some((first, last)) = picked {
            view.month = Some(first);
            view.months = false;
            view.range(first, last);
            return true;
        }
        return false;
    }
    let down = ui.input(|i| i.pointer.primary_down());
    let mut picked = None;
    egui::Grid::new("activity_days")
        .num_columns(7)
        .spacing(vec2(4.0, 4.0))
        .show(ui, |ui| {
            for day in [
                crate::i18n::text("repo-workflows-history_activity-calendar-f40d7f51"),
                crate::i18n::text("repo-workflows-history_activity-calendar-d1eb39b0"),
                crate::i18n::text("repo-workflows-history_activity-calendar-58339f45"),
                crate::i18n::text("repo-workflows-history_activity-calendar-7da11212"),
                crate::i18n::text("repo-workflows-history_activity-calendar-66dab40c"),
                crate::i18n::text("repo-workflows-history_activity-calendar-fdeb71b5"),
                crate::i18n::text("repo-workflows-history_activity-calendar-db18f17f"),
            ] {
                ui.label(day);
            }
            ui.end_row();
            let offset = month.month_start().weekday();
            for _ in 0..offset {
                ui.label("");
            }
            for day in 1..=month.month_days() {
                let date = Day { day, ..month };
                let n = counts.get(&date).copied().unwrap_or(0);
                let selected = view.query.from.is_some_and(|d| date >= d)
                    && view.query.through.is_some_and(|d| date <= d);
                let fill = if selected {
                    t.accent
                } else if n > 0 {
                    t.separator
                } else {
                    t.card
                };
                let response = ui
                    .add_sized(
                        [66.0, 44.0],
                        egui::Button::new(crate::i18n::message(
                            "repo-workflows-history-calendar-day",
                            &[("day", (day).into()), ("count", (n).into())],
                        ))
                        .fill(fill)
                        .sense(Sense::click_and_drag()),
                    )
                    .on_hover_text(crate::i18n::date_iso(&date.to_string()));
                if response.clicked() {
                    picked = Some((date, date));
                }
                if response.drag_started() {
                    view.drag = Some(date);
                }
                if response.contains_pointer()
                    && down
                    && let Some(anchor) = view.drag
                {
                    picked = Some((anchor, date));
                }
                if (offset + day) % 7 == 0 {
                    ui.end_row();
                }
            }
        });
    if let Some((a, b)) = picked {
        view.range(a, b);
    }
    let finished_drag = !down && view.drag.take().is_some();
    finished_drag || (picked.is_some() && !down)
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
mod fluent_calendar_tests {
    use super::*;

    #[test]
    fn fluent_partition_calendar_renders_both_languages_without_changing_iso_range() {
        use egui_kittest::{Harness, kittest::Queryable};
        let date = Day {
            year: 2026,
            month: 1,
            day: 1,
        };
        for (language, previous, day_label) in [
            ("en-US", "Previous month", "1\n1 commit"),
            ("de", "Vorheriger Monat", "1\n1 Commit"),
        ] {
            crate::i18n::set_language(language);
            // No RepoTab constructor or reader is polled: all state is supplied.
            let mut view = Activity::new(Page::Calendar);
            view.range(date, date);
            view.month = Some(date);
            let mut summary = Summary::default();
            summary.calendar.insert(date, 1);
            let mut initialized = false;
            let harness = Harness::new_ui_state(
                move |ui, view| {
                    if !initialized {
                        crate::theme::install_fonts(ui.ctx());
                        ui.ctx().request_discard("install supplied-state fonts");
                        initialized = true;
                        return;
                    }
                    let t = Tokens::get(ui.ctx());
                    calendar(ui, view, &summary, &t);
                },
                view,
            );
            harness.get_by_label(previous);
            harness.get_by_label(day_label);
            assert_eq!(harness.state().from, "2026-01-01");
            assert_eq!(harness.state().through, "2026-01-01");
            assert_eq!(
                harness.state().requested_range().map(|q| q.from),
                Ok(Some(date))
            );
        }
        crate::i18n::set_language("en-US");
    }
}

fn month_heading(month: Day) -> String {
    crate::i18n::message(
        "repo-workflows-calendar-month-heading",
        &[
            ("month", month.month.into()),
            ("year", format!("{:04}", month.year).into()),
        ],
    )
}
#[cfg(test)]
mod fluent_date_paint_tests {
    use super::*;
    #[test]
    fn fluent_partition_actual_month_heading_preserves_iso_calendar_identity() {
        let day = Day {
            year: 2026,
            month: 3,
            day: 12,
        };
        for (lang, expected) in [("en-US", "March 2026"), ("de", "März 2026")] {
            crate::i18n::set_language(lang);
            assert_eq!(month_heading(day), expected);
            assert_eq!(day.to_string(), "2026-03-12");
            assert_eq!(
                crate::i18n::number(1234usize),
                if lang == "de" { "1.234" } else { "1,234" }
            );
        }
        crate::i18n::set_language("en-US");
    }
}
