//! The bake-off's page: the grid of attempts, the files they touched, a file side by side
//! across them with its hunks to pick, and the assemble bar.

use egui::{RichText, Ui};

use super::{BakeAct, Bakeoff, Side};
use crate::agent::bakeoff::Comparison;
use crate::agent::bakeoff::assemble::Take;
use crate::agent::bakeoff::build::Built;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// At most this many lines of a patch are shown in a column.
const MAX_PATCH_LINES: usize = 2000;

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
                    ui.label(RichText::new("Comparing the attempts…").color(t.text_dim));
                });
            }
            Some(Err(e)) => {
                ui.add_space(12.0);
                ui.colored_label(t.danger, e);
            }
            Some(Ok(_)) => {
                assemble_bar(ui, view, &t, &mut act);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("bakeoff-page")
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, view, &t, &mut act));
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
        ui.label(RichText::new("Bake-off").font(theme::semibold(15.0)));
        if let Some(Ok(cmp)) = &view.comparison {
            ui.label(
                RichText::new(format!("from {}", crate::repo::short(&cmp.base))).color(t.text_dim),
            );
            if !cmp.same_base {
                ui.colored_label(t.warning, "⚠ they didn't all start from this commit")
                    .on_hover_text(
                        "One attempt forked from elsewhere: its column shows more than it did",
                    );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").on_hover_text("Esc").clicked() {
                *act = Some(BakeAct::Close);
            }
            if ui
                .button("Compare again")
                .on_hover_text("Reads the attempts again, uncommitted work included")
                .clicked()
            {
                *act = Some(BakeAct::Refresh);
            }
            if ui.button("Other attempts…").clicked() {
                *act = Some(BakeAct::OpenPicker);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new("Build with").color(t.text_dim));
        ui.add(
            egui::TextEdit::singleline(&mut view.build_command)
                .hint_text("cargo build, npm run build…")
                .font(theme::mono(12.0))
                .desired_width(280.0),
        );
        let can = !view.build_command.trim().is_empty()
            && view.attempts.iter().any(|a| a.worktree.is_some());
        if ui
            .add_enabled(can, egui::Button::new("Build all"))
            .on_hover_text("Runs it in a terminal in each attempt's worktree")
            .on_disabled_hover_text("Type the build command; only attempts with a worktree build")
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
    ui.add_space(12.0);
    ui.label(
        RichText::new("FILES")
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    files(ui, view, cmp, t, act);
    if let Some((path, sides)) = &view.file {
        ui.add_space(12.0);
        side_by_side(ui, view, path, sides.as_ref(), t, act);
    }
}

/// One column per attempt: what it touched, how much, what it brings in, whether it builds.
fn grid(ui: &mut Ui, view: &Bakeoff, cmp: &Comparison, t: &Tokens, act: &mut Option<BakeAct>) {
    egui::Grid::new("bakeoff-grid")
        .striped(true)
        .num_columns(cmp.attempts.len() + 1)
        .spacing([18.0, 6.0])
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label("");
            for a in &view.attempts {
                ui.label(RichText::new(&a.name).font(theme::semibold(12.5)));
            }
            ui.end_row();
            ui.label(RichText::new("Files touched").color(t.text_dim));
            for c in &cmp.attempts {
                ui.label(c.files.len().to_string());
            }
            ui.end_row();
            ui.label(RichText::new("Size").color(t.text_dim));
            for c in &cmp.attempts {
                ui.horizontal(|ui| {
                    ui.colored_label(t.success, format!("+{}", c.added));
                    ui.colored_label(t.danger, format!("−{}", c.removed));
                });
            }
            ui.end_row();
            ui.label(RichText::new("New dependencies").color(t.text_dim));
            for c in &cmp.attempts {
                if c.new_deps.is_empty() {
                    ui.label(RichText::new("none").color(t.text_faint));
                } else {
                    ui.vertical(|ui| {
                        for dep in &c.new_deps {
                            ui.colored_label(t.warning, dep);
                        }
                    });
                }
            }
            ui.end_row();
            ui.label(RichText::new("Uncommitted work").color(t.text_dim));
            for c in &cmp.attempts {
                ui.label(if c.uncommitted {
                    "yes, compared too"
                } else {
                    "no"
                });
            }
            ui.end_row();
            ui.label(RichText::new("Builds").color(t.text_dim));
            for (i, a) in view.attempts.iter().enumerate() {
                build_cell(
                    ui,
                    view.builds.get(i).copied().flatten(),
                    a.worktree.is_some(),
                    i,
                    t,
                    act,
                );
            }
            ui.end_row();
        });
}

fn build_cell(
    ui: &mut Ui,
    built: Option<Built>,
    has_worktree: bool,
    i: usize,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        match built {
            Some(Built::Running) => {
                ui.add(crate::spinner::Spinner::new().size(12.0));
                ui.label(RichText::new("building…").color(t.text_dim));
            }
            Some(Built::Passed) => {
                ui.colored_label(t.success, "✔ builds");
            }
            Some(Built::Failed(code)) => {
                ui.colored_label(t.danger, format!("✘ fails ({code})"))
                    .on_hover_text("Its output is in the terminal tab");
            }
            None if has_worktree => {
                ui.label(RichText::new("not built").color(t.text_faint));
            }
            None => {
                ui.label(RichText::new("a branch: no worktree to build in").color(t.text_faint))
                    .on_hover_text("Add a worktree for it (WORKTREES +), then compare again");
            }
        }
        if has_worktree && built != Some(Built::Running) && ui.small_button("Build").clicked() {
            *act = Some(BakeAct::Build(i));
        }
    });
}

/// Every file an attempt touched: who touched it, what is picked of it.
fn files(ui: &mut Ui, view: &Bakeoff, cmp: &Comparison, t: &Tokens, act: &mut Option<BakeAct>) {
    let list = cmp.files();
    if list.is_empty() {
        ui.label(RichText::new("None of the attempts changed anything").color(t.text_faint));
        return;
    }
    egui::Grid::new("bakeoff-files")
        .num_columns(cmp.attempts.len() + 2)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for (path, touched) in list {
                let open = view.file.as_ref().is_some_and(|(p, _)| *p == path);
                if ui
                    .selectable_label(open, RichText::new(&path).font(theme::mono(12.0)))
                    .on_hover_text("Open it side by side across the attempts")
                    .clicked()
                {
                    *act = Some(if open {
                        BakeAct::CloseFile
                    } else {
                        BakeAct::OpenFile(path.clone())
                    });
                }
                for on in touched {
                    if on {
                        ui.colored_label(t.accent, "●");
                    } else {
                        ui.label(RichText::new("·").color(t.text_faint));
                    }
                }
                let picked = view.picks.get(&path).map(|take| pick_label(view, take));
                ui.label(
                    RichText::new(picked.unwrap_or_default())
                        .small()
                        .color(t.success),
                );
                ui.end_row();
            }
        });
}

/// What is taken of a file, for the list.
fn pick_label(view: &Bakeoff, take: &Take) -> String {
    let name = |n: usize| view.attempts.get(n).map_or("?", |a| a.name.as_str());
    match take {
        Take::File(n) => format!("← whole file from {}", name(*n)),
        Take::Hunks(list) => format!("← {} hunk(s)", list.len()),
    }
}

/// The file shown in a column per attempt: its patch, the button that takes it whole and its
/// hunks to take one by one.
fn side_by_side(
    ui: &mut Ui,
    view: &Bakeoff,
    path: &str,
    sides: Option<&Result<Vec<Side>, String>>,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(path).font(theme::semibold(13.0)));
        if view.picks.contains_key(path) && ui.small_button("Keep the base's").clicked() {
            *act = Some(BakeAct::ClearPick(path.to_owned()));
        }
    });
    let sides = match sides {
        None => {
            ui.add(crate::spinner::Spinner::new());
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, e);
            return;
        }
        Some(Ok(sides)) => sides,
    };
    let take = view.picks.get(path);
    ui.columns(sides.len().max(1), |cols| {
        for (n, (col, side)) in cols.iter_mut().zip(sides).enumerate() {
            let name = view.attempts.get(n).map_or("?", |a| a.name.as_str());
            col.label(RichText::new(name).font(theme::semibold(12.0)));
            if side.patch.trim().is_empty() {
                col.label(RichText::new("unchanged").color(t.text_faint));
                continue;
            }
            let whole = take == Some(&Take::File(n));
            if col.selectable_label(whole, "Take this file").clicked() {
                *act = Some(BakeAct::TakeFile(path.to_owned(), n));
            }
            for (i, hunk) in side.hunks.iter().enumerate() {
                let on = matches!(take, Some(Take::Hunks(list)) if list.contains(&(n, i)));
                let mut checked = on;
                if col
                    .checkbox(
                        &mut checked,
                        RichText::new(&hunk.header).font(theme::mono(11.0)),
                    )
                    .on_hover_text(format!(
                        "−{} +{} lines; hunks of other attempts can be taken with it",
                        hunk.removed.len(),
                        hunk.added.len()
                    ))
                    .changed()
                {
                    *act = Some(BakeAct::ToggleHunk(path.to_owned(), n, i));
                }
            }
            egui::ScrollArea::both()
                .id_salt(("bakeoff-side", n))
                .max_height(420.0)
                .show(col, |ui| patch_lines(ui, &side.patch, t));
        }
    });
}

fn patch_lines(ui: &mut Ui, patch: &str, t: &Tokens) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let body = patch.lines().skip_while(|l| !l.starts_with("@@"));
    for line in body.take(MAX_PATCH_LINES) {
        let (color, fill) = match line.chars().next() {
            Some('+') => (t.text, t.diff_add_bg),
            Some('-') => (t.text, t.diff_del_bg),
            Some('@') => (t.text_faint, egui::Color32::TRANSPARENT),
            _ => (t.text_dim, egui::Color32::TRANSPARENT),
        };
        egui::Frame::new().fill(fill).show(ui, |ui| {
            ui.label(RichText::new(line).font(theme::mono(11.5)).color(color));
        });
    }
}

fn assemble_bar(ui: &mut Ui, view: &mut Bakeoff, t: &Tokens, act: &mut Option<BakeAct>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new("Assemble into").color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut view.branch).desired_width(220.0));
        let picks = view.picks.len();
        let ready = picks > 0 && !view.branch.trim().is_empty() && !view.assembling;
        let tip = if picks == 0 {
            "Open a file below and take it whole, or some of its hunks, from any attempt"
        } else {
            "Branch name first"
        };
        if crate::repo::primary_button(ui, &format!("Assemble ({picks} files)"), ready, 130.0)
            .on_hover_text(
                "A new branch from the base with what is picked; nothing checked out changes",
            )
            .on_disabled_hover_text(tip)
            .clicked()
        {
            *act = Some(BakeAct::Assemble);
        }
        if view.assembling {
            ui.add(crate::spinner::Spinner::new().size(14.0));
        }
        if let Some((note, error)) = &view.note {
            ui.colored_label(if *error { t.danger } else { t.success }, note);
        }
    });
}
