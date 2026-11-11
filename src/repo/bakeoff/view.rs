//! The bake-off's page: the grid of attempts, the files they touched, a file side by side
//! across them with its hunks to pick, and the assemble bar.

mod hunks;
use hunks::*;
mod grid;
use egui::{RichText, Ui};
use grid::*;

use super::{BakeAct, Bakeoff, Side};
use crate::agent::bakeoff::Comparison;
use crate::agent::bakeoff::assemble::Take;
use crate::agent::bakeoff::build::Built;
use crate::agent::bakeoff::hunks::Hunk;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// At most this many lines of a patch are shown in a column.
const MAX_PATCH_LINES: usize = 5000;
const ROW_H: f32 = 18.0;

impl RepoTab {
    /// The bake-off in the big area.
    pub(in crate::repo) fn bakeoff_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        self.poll_builds(&ctx);
        let Some(view) = &mut self.bakeoff.view else {
            return;
        };
        let t = Tokens::get(&ctx);
        let mut act = None;
        header(ui, view, &t, &mut act);
        ui.separator();
        match &view.comparison {
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-bakeoff-bakeoff-view-9754333a",
                        ))
                        .color(t.text_dim),
                    );
                });
            }
            Some(Err(e)) => {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            }
            Some(Ok(_)) => {
                assemble_bar(ui, view, &t, &mut act);
                ui.separator();
                body(ui, view, &t, &mut act);
            }
        }
        if let Some(act) = act {
            acts.push(Action::Bake(act));
        }
    }
}

fn header(ui: &mut Ui, view: &mut Bakeoff, t: &Tokens, act: &mut Option<BakeAct>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-header-43c40423"))
                .font(theme::semibold(15.0)),
        );
        if let Some(Ok(cmp)) = &view.comparison {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-bakeoff-header-50cd264e",
                    &[(
                        "crate-repo-short-cmp-base-1",
                        (crate::repo::short(&cmp.base)).into(),
                    )],
                ))
                .color(t.text_dim),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-header-bb683fa4"));
            let elsewhere: Vec<&str> = cmp
                .attempts
                .iter()
                .zip(&view.attempts)
                .filter(|(c, _)| c.forked_elsewhere)
                .map(|(_, a)| a.name.as_str())
                .collect();
            if !cmp.same_base && !elsewhere.is_empty() {
                ui.colored_label(
                    t.warning,
                    crate::i18n::message(
                        "repo-workflows-bakeoff-header-acb0a622",
                        &[("elsewhere-join-1", (elsewhere.join(", ")).clone().into())],
                    ),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-header-dd4159a5"));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(crate::i18n::text("repo-workflows-bakeoff-header-7d9eb7ac"))
                .on_hover_text("Esc")
                .clicked()
            {
                *act = Some(BakeAct::Close);
            }
            if ui
                .button(crate::i18n::text("repo-workflows-bakeoff-header-502f6dbd"))
                .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-header-b1ce0225"))
                .clicked()
            {
                *act = Some(BakeAct::Refresh);
            }
            if ui
                .button(crate::i18n::text("repo-workflows-bakeoff-header-0e2a45b6"))
                .clicked()
            {
                *act = Some(BakeAct::OpenPicker);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-header-46334791"))
                .color(t.text_dim),
        );
        ui.add(
            egui::TextEdit::singleline(&mut view.build_command)
                .hint_text("cargo build, npm run build…")
                .font(theme::mono(12.0))
                .desired_width(280.0),
        )
        .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-header-c38e956d"));
        let can = !view.build_command.trim().is_empty() && matches!(view.comparison, Some(Ok(_)));
        let why = if view.build_command.trim().is_empty() {
            crate::i18n::text("repo-workflows-bakeoff-header-a195ce68")
        } else {
            crate::i18n::text("repo-workflows-bakeoff-header-9cec669f")
        };
        if ui
            .add_enabled(
                can,
                egui::Button::new(crate::i18n::text("repo-workflows-bakeoff-header-175f9ff6")),
            )
            .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-header-862a0723"))
            .on_disabled_hover_text(why)
            .clicked()
        {
            *act = Some(BakeAct::BuildAll);
        }
    });
}

fn body(ui: &mut Ui, view: &Bakeoff, t: &Tokens, act: &mut Option<BakeAct>) {
    let Some(Ok(cmp)) = &view.comparison else {
        return;
    };
    grid(ui, view, cmp, t, act);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-body-faff10f7"))
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-body-bfd30d1e"))
                .small()
                .color(t.text_faint),
        );
    });
    let open = view.file.is_some();
    let list_h = if open {
        (ui.available_height() * 0.3).max(90.0)
    } else {
        ui.available_height()
    };
    files(ui, view, list_h, t, act);
    if let Some((path, sides)) = &view.file {
        ui.separator();
        side_by_side(ui, view, path, sides.as_ref(), t, act);
    }
}

/// One column per attempt: what it touched, how much, what it brings in, whether it builds.
fn files(ui: &mut Ui, view: &Bakeoff, height: f32, t: &Tokens, act: &mut Option<BakeAct>) {
    if view.rows.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-files-7e17d722"))
                .color(t.text_faint),
        );
        return;
    }
    let names = |ui: &mut Ui| {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.add_sized([PATH_W, ROW_H], egui::Label::new(""));
            for a in &view.attempts {
                ui.add_sized(
                    [DOT_W, ROW_H],
                    egui::Label::new(RichText::new(short_name(&a.name)).small().color(t.text_dim))
                        .truncate(),
                )
                .on_hover_text(&a.name);
            }
        });
    };
    names(ui);
    egui::ScrollArea::vertical()
        .id_salt("bakeoff-files")
        .max_height(height)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_H, view.rows.len(), |ui, range| {
            for (path, touched) in &view.rows[range] {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let open = view.file.as_ref().is_some_and(|(p, _)| p == path);
                    let label = RichText::new(path).font(theme::mono(12.0));
                    if ui
                        .add_sized(
                            [PATH_W, ROW_H],
                            egui::Button::selectable(open, label).truncate(),
                        )
                        .on_hover_text(crate::i18n::message(
                            "repo-workflows-bakeoff-files-fe616a60",
                            &[("path", (path).clone().into())],
                        ))
                        .clicked()
                    {
                        *act = Some(if open {
                            BakeAct::CloseFile
                        } else {
                            BakeAct::OpenFile(path.clone())
                        });
                    }
                    for (on, a) in touched.iter().zip(&view.attempts) {
                        let (dot, color, tip) = if *on {
                            (
                                "●",
                                t.accent,
                                crate::i18n::message(
                                    "repo-workflows-bakeoff-files-83d88f06",
                                    &[("a-name-1", (a.name).clone().into())],
                                ),
                            )
                        } else {
                            (
                                "·",
                                t.text_faint,
                                crate::i18n::message(
                                    "repo-workflows-bakeoff-files-51aec897",
                                    &[("a-name-1", (a.name).clone().into())],
                                ),
                            )
                        };
                        ui.add_sized(
                            [DOT_W, ROW_H],
                            egui::Label::new(RichText::new(dot).color(color)),
                        )
                        .on_hover_text(tip);
                    }
                    if let Some(take) = view.picks.get(path) {
                        ui.label(
                            RichText::new(pick_label(view, take))
                                .small()
                                .color(t.success),
                        );
                    }
                });
            }
        });
}

const PATH_W: f32 = 320.0;
const DOT_W: f32 = 70.0;

fn short_name(name: &str) -> String {
    let tail = name.rsplit('/').next().unwrap_or(name);
    if tail.chars().count() > 10 {
        format!("{}…", tail.chars().take(9).collect::<String>())
    } else {
        tail.to_owned()
    }
}

/// What is taken of a file, for the list.
fn pick_label(view: &Bakeoff, take: &Take) -> String {
    let name = |n: usize| view.attempts.get(n).map_or("?", |a| a.name.as_str());
    match take {
        Take::File(n) => crate::i18n::message(
            "repo-workflows-bakeoff-pick-label-77dbc108",
            &[("name-n-1", (name(*n)).into())],
        ),
        Take::Hunks(list) if list.len() == 1 => {
            crate::i18n::text("repo-workflows-bakeoff-pick-label-d3d98b5d").to_owned()
        }
        Take::Hunks(list) => crate::i18n::message(
            "repo-workflows-bakeoff-pick-label-c708de81",
            &[("list-len-1", (list.len()).into())],
        ),
    }
}

/// "Lines 12–14: −3 +5", or where lines are added.
fn hunk_label(hunk: &Hunk) -> String {
    let (removed, added) = (hunk.removed.len(), hunk.added.len());
    let place = match removed {
        0 if hunk.old_start == 0 => {
            crate::i18n::text("repo-workflows-bakeoff-hunk-label-0cf292e8").to_owned()
        }
        0 => crate::i18n::message(
            "repo-workflows-bakeoff-hunk-label-7647677f",
            &[("hunk-old-start-1", (hunk.old_start).into())],
        ),
        1 => crate::i18n::message(
            "repo-workflows-bakeoff-hunk-label-4d9d8a81",
            &[("hunk-old-start-1", (hunk.old_start).into())],
        ),
        n => crate::i18n::message(
            "repo-workflows-bakeoff-hunk-label-a3052fdf",
            &[
                ("hunk-old-start-1", (hunk.old_start).into()),
                ("hunk-old-start-n-1-2", (hunk.old_start + n - 1).into()),
            ],
        ),
    };
    format!("{place}: −{removed} +{added}")
}

/// Which picked hunks of this file change the same base lines: (attempt, hunk) of each.
fn assemble_bar(ui: &mut Ui, view: &mut Bakeoff, t: &Tokens, act: &mut Option<BakeAct>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-bakeoff-assemble-bar-8462ef57",
            ))
            .color(t.text_dim),
        );
        ui.add(egui::TextEdit::singleline(&mut view.branch).desired_width(220.0));
        let picks = view.picks.len();
        let ready = picks > 0 && !view.branch.trim().is_empty() && !view.assembling;
        let tip = if view.assembling {
            crate::i18n::text("repo-workflows-bakeoff-assemble-bar-5a22f453")
        } else if picks == 0 {
            crate::i18n::text("repo-workflows-bakeoff-assemble-bar-3766ae43")
        } else {
            crate::i18n::text("repo-workflows-bakeoff-assemble-bar-f19adc05")
        };
        let label = if picks == 1 {
            crate::i18n::text("repo-workflows-bakeoff-assemble-bar-ccbdbfe8").to_owned()
        } else {
            crate::i18n::message(
                "repo-workflows-bakeoff-assemble-bar-86fd9bbb",
                &[("picks", (picks).into())],
            )
        };
        if crate::repo::primary_button(ui, &label, ready, 130.0)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-bakeoff-assemble-bar-aee10357",
            ))
            .on_disabled_hover_text(tip)
            .clicked()
        {
            *act = Some(BakeAct::Assemble);
        }
        if view.assembling {
            ui.add(crate::spinner::Spinner::new().size(14.0));
        }
        if let Some((note, error)) = &view.note {
            ui.colored_label(
                if *error { t.danger } else { t.success },
                view.display_note(note),
            );
        }
        if view.made.is_some()
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-bakeoff-assemble-bar-916e34d4",
                ))
                .clicked()
        {
            *act = Some(BakeAct::ShowMade);
        }
    });
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifecycle_tests;
