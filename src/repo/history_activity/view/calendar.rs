//! Activity calendar selection and month heading presentation.

use super::*;

pub(super) fn calendar(
    ui: &mut egui::Ui,
    view: &mut Activity,
    summary: &Summary,
    t: &Tokens,
) -> bool {
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

pub(super) fn month_heading(month: Day) -> String {
    crate::i18n::message(
        "repo-workflows-calendar-month-heading",
        &[
            ("month", month.month.into()),
            ("year", format!("{:04}", month.year).into()),
        ],
    )
}
