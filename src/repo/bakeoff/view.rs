//! The bake-off's page: the grid of attempts, the files they touched, a file side by side
//! across them with its hunks to pick, and the assemble bar.

use egui::{RichText, Ui};

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
        ui.label(RichText::new("Bake-off").font(theme::semibold(15.0)));
        if let Some(Ok(cmp)) = &view.comparison {
            ui.label(
                RichText::new(format!("from {}", crate::repo::short(&cmp.base))).color(t.text_dim),
            )
            .on_hover_text("The commit they all start from (their merge base)");
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
                    format!("⚠ {} started elsewhere", elsewhere.join(", ")),
                )
                .on_hover_text(
                    "It didn't start from the same commit as the others: its column also \
                     shows what came before it started",
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
        )
        .on_hover_text("Kept for the repository");
        let can = !view.build_command.trim().is_empty() && matches!(view.comparison, Some(Ok(_)));
        let why = if view.build_command.trim().is_empty() {
            "Type the command that builds the project"
        } else {
            "Compare the attempts first"
        };
        if ui
            .add_enabled(can, egui::Button::new("Build all"))
            .on_hover_text("Explicitly runs this command on each compared snapshot in a fresh detached worktree. No setup recipe runs. Workspaces remain in Worktrees for inspection and removal.")
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
            RichText::new("FILES")
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.label(
            RichText::new("click one to see it across the attempts and pick from it")
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
                let built = view.builds.get(i).copied().flatten();
                ui.vertical(|ui| {
                    build_cell(ui, built, a.worktree.is_some(), i, t, act);
                    if view.statuses.get(i).is_some_and(Option::is_some)
                        && ui
                            .small_button("Build output…")
                            .on_hover_text(
                                "Open this run's complete saved output in the configured editor",
                            )
                            .clicked()
                    {
                        *act = Some(BakeAct::Output(i));
                    }
                });
            }
            ui.end_row();
        });
}

fn build_cell(
    ui: &mut Ui,
    built: Option<Built>,
    _has_worktree: bool,
    i: usize,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        match built {
            Some(Built::Running) => {
                ui.add(crate::spinner::Spinner::new().size(12.0));
                ui.label(RichText::new("building…").color(t.text_dim))
                    .on_hover_text("Its output is in the terminal tab");
            }
            Some(Built::Passed) => {
                ui.colored_label(t.success, "✔ builds");
            }
            Some(Built::Failed(code)) => {
                ui.colored_label(t.danger, format!("✘ fails ({code})"))
                    .on_hover_text("Its output is in the terminal tab");
            }
            None => {
                ui.label(RichText::new("not built").color(t.text_faint));
            }
        }
        if built != Some(Built::Running) {
            let (label, tip) = match built {
                Some(Built::Running) => (
                    "Build again",
                    "Stopped or closed its terminal? Starts the build over",
                ),
                Some(_) => ("Build again", "Runs the build command again"),
                None => (
                    "Build",
                    "Runs your command on the exact compared snapshot in a fresh detached worktree",
                ),
            };
            if ui.small_button(label).on_hover_text(tip).clicked() {
                *act = Some(BakeAct::Build(i));
            }
        }
    });
}

/// Every file an attempt touched: who touched it, what is picked of it.
fn files(ui: &mut Ui, view: &Bakeoff, height: f32, t: &Tokens, act: &mut Option<BakeAct>) {
    if view.rows.is_empty() {
        ui.label(RichText::new("None of the attempts changed anything").color(t.text_faint));
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
                        .on_hover_text(format!("{path}\nOpen it side by side across the attempts"))
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
                            ("●", t.accent, format!("{} changed it", a.name))
                        } else {
                            ("·", t.text_faint, format!("{} left it as it was", a.name))
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
        Take::File(n) => format!("← whole file from {}", name(*n)),
        Take::Hunks(list) if list.len() == 1 => "← 1 hunk".to_owned(),
        Take::Hunks(list) => format!("← {} hunks", list.len()),
    }
}

/// "Lines 12–14: −3 +5", or where lines are added.
fn hunk_label(hunk: &Hunk) -> String {
    let (removed, added) = (hunk.removed.len(), hunk.added.len());
    let place = match removed {
        0 if hunk.old_start == 0 => "At the top".to_owned(),
        0 => format!("After line {}", hunk.old_start),
        1 => format!("Line {}", hunk.old_start),
        n => format!("Lines {}–{}", hunk.old_start, hunk.old_start + n - 1),
    };
    format!("{place}: −{removed} +{added}")
}

/// Which picked hunks of this file change the same base lines: (attempt, hunk) of each.
fn clashes(sides: &[Side], picked: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let span = |(n, i): (usize, usize)| {
        let h = sides.get(n)?.hunks.get(i)?;
        let start = if h.removed.is_empty() {
            h.old_start
        } else {
            h.old_start - 1
        };
        Some((start, start + h.removed.len(), h.removed.is_empty()))
    };
    let mut out = Vec::new();
    for (a, &x) in picked.iter().enumerate() {
        for &y in &picked[a + 1..] {
            if x.0 == y.0 {
                continue;
            }
            if let (Some((s1, e1, ins1)), Some((s2, e2, ins2))) = (span(x), span(y)) {
                // Two that replace lines overlap when their ranges do; an insertion clashes
                // with a replacement whose range has it inside.
                let overlap = match (ins1, ins2) {
                    (true, true) => false,
                    (true, false) => s1 > s2 && s1 < e2,
                    (false, true) => s2 > s1 && s2 < e1,
                    (false, false) => s1 < e2 && s2 < e1,
                };
                if overlap {
                    out.extend([x, y]);
                }
            }
        }
    }
    out
}

/// The file shown in a column per attempt that changed it: its patch, the button that takes
/// it whole and its hunks to take one by one.
fn side_by_side(
    ui: &mut Ui,
    view: &Bakeoff,
    path: &str,
    sides: Option<&Result<Vec<Side>, String>>,
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new(path).font(theme::semibold(13.0)));
        if view.picks.contains_key(path)
            && ui
                .small_button("Clear pick")
                .on_hover_text("The assembled branch keeps the base's version of this file")
                .clicked()
        {
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
    let picked: Vec<(usize, usize)> = match take {
        Some(Take::Hunks(list)) => list.clone(),
        _ => Vec::new(),
    };
    let clashing = clashes(sides, &picked);
    if !clashing.is_empty() {
        ui.colored_label(
            t.danger,
            "Hunks of two attempts picked here change the same lines: untick one of them",
        );
    }
    let changed: Vec<usize> = (0..sides.len())
        .filter(|n| sides[*n].kind() != "unchanged")
        .collect();
    let unchanged: Vec<&str> = (0..sides.len())
        .filter(|n| !changed.contains(n))
        .filter_map(|n| view.attempts.get(n).map(|a| a.name.as_str()))
        .collect();
    if !unchanged.is_empty() {
        ui.label(
            RichText::new(format!("Unchanged in {}", unchanged.join(", ")))
                .small()
                .color(t.text_faint),
        );
    }
    ui.columns(changed.len().max(1), |cols| {
        for (col, &n) in cols.iter_mut().zip(&changed) {
            let side = &sides[n];
            let name = view.attempts.get(n).map_or("?", |a| a.name.as_str());
            column(col, side, name, n, path, take, &clashing, t, act);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn column(
    ui: &mut Ui,
    side: &Side,
    name: &str,
    n: usize,
    path: &str,
    take: Option<&Take>,
    clashing: &[(usize, usize)],
    t: &Tokens,
    act: &mut Option<BakeAct>,
) {
    ui.label(RichText::new(format!("{name} · {}", side.kind())).font(theme::semibold(12.0)));
    let whole = take == Some(&Take::File(n));
    let label = match side.kind() {
        "deleted" => "Take the deletion",
        "added" => "Take the new file",
        _ => "Take the whole file",
    };
    if ui
        .selectable_label(whole, label)
        .on_hover_text("Replaces the hunks picked; click again to take it back")
        .clicked()
    {
        *act = Some(BakeAct::TakeFile(path.to_owned(), n));
    }
    egui::ScrollArea::vertical()
        .id_salt(("bakeoff-hunks", n))
        .max_height(140.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if side.hunks.is_empty() {
                ui.label(
                    RichText::new("No hunks to pick (a binary file): take it whole")
                        .small()
                        .color(t.text_faint),
                );
            }
            for (i, hunk) in side.hunks.iter().enumerate() {
                let on = matches!(take, Some(Take::Hunks(list)) if list.contains(&(n, i)));
                let mut checked = on;
                let text = RichText::new(hunk_label(hunk)).font(theme::mono(11.0));
                let text = if clashing.contains(&(n, i)) {
                    text.color(t.danger)
                } else {
                    text
                };
                if ui
                    .checkbox(&mut checked, text)
                    .on_hover_text(hunk_preview(hunk))
                    .changed()
                {
                    *act = Some(BakeAct::ToggleHunk(path.to_owned(), n, i));
                }
            }
        });
    ui.separator();
    let lines: Vec<&str> = side
        .patch
        .lines()
        .skip_while(|l| !l.starts_with("@@"))
        .take(MAX_PATCH_LINES)
        .collect();
    // Only the lines in sight are laid out: a patch can be thousands of lines.
    egui::ScrollArea::both()
        .id_salt(("bakeoff-side", n))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_H, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in &lines[range] {
                patch_line(ui, line, t);
            }
        });
}

/// The first lines a hunk removes and adds, for its tooltip.
fn hunk_preview(hunk: &Hunk) -> String {
    let lines: Vec<String> = hunk
        .removed
        .iter()
        .map(|l| format!("- {l}"))
        .chain(hunk.added.iter().map(|l| format!("+ {l}")))
        .take(12)
        .collect();
    lines.join("\n")
}

fn patch_line(ui: &mut Ui, line: &str, t: &Tokens) {
    let (color, fill) = match line.chars().next() {
        Some('+') => (t.text, t.diff_add_bg),
        Some('-') => (t.text, t.diff_del_bg),
        Some('@') => (t.text_faint, egui::Color32::TRANSPARENT),
        _ => (t.text_dim, egui::Color32::TRANSPARENT),
    };
    egui::Frame::new().fill(fill).show(ui, |ui| {
        ui.set_min_height(ROW_H);
        ui.label(RichText::new(line).font(theme::mono(11.5)).color(color));
    });
}

fn assemble_bar(ui: &mut Ui, view: &mut Bakeoff, t: &Tokens, act: &mut Option<BakeAct>) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(RichText::new("Assemble into").color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut view.branch).desired_width(220.0));
        let picks = view.picks.len();
        let ready = picks > 0 && !view.branch.trim().is_empty() && !view.assembling;
        let tip = if view.assembling {
            "Assembling…"
        } else if picks == 0 {
            "Open a file below and take it whole, or some of its hunks, from any attempt"
        } else {
            "Name the new branch first"
        };
        let label = if picks == 1 {
            "Assemble (1 file)".to_owned()
        } else {
            format!("Assemble ({picks} files)")
        };
        if crate::repo::primary_button(ui, &label, ready, 130.0)
            .on_hover_text(
                "A new branch from the base with what is picked; the rest stays as the base has it. Nothing checked out changes",
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
        if view.made.is_some() && ui.button("Show in graph").clicked() {
            *act = Some(BakeAct::ShowMade);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(patch: &str) -> Side {
        Side {
            patch: patch.to_owned(),
            hunks: crate::agent::bakeoff::hunks::parse(patch),
        }
    }

    #[test]
    fn hunks_are_named_by_their_lines() {
        let s = side(
            "@@ -2 +2 @@\n-b\n+B\n@@ -4,2 +4 @@\n-d\n-e\n+x\n@@ -5,0 +6 @@\n+y\n@@ -0,0 +1 @@\n+top\n",
        );
        let names: Vec<String> = s.hunks.iter().map(hunk_label).collect();
        assert_eq!(
            names,
            [
                "Line 2: −1 +1",
                "Lines 4–5: −2 +1",
                "After line 5: −0 +1",
                "At the top: −0 +1"
            ]
        );
    }

    #[test]
    fn picked_hunks_of_two_attempts_on_the_same_lines_are_flagged() {
        let a = side("@@ -2,2 +2 @@\n-b\n-c\n+x\n");
        let b = side("@@ -3 +3 @@\n-c\n+y\n@@ -6,0 +7 @@\n+z\n");
        let sides = [a, b];
        assert_eq!(clashes(&sides, &[(0, 0), (1, 0)]).len(), 2);
        assert!(clashes(&sides, &[(0, 0), (1, 1)]).is_empty());
    }

    #[test]
    fn the_next_name_counts_up() {
        assert_eq!(super::super::next_name("bakeoff/x"), "bakeoff/x-2");
        assert_eq!(super::super::next_name("bakeoff/x-2"), "bakeoff/x-3");
        assert_eq!(super::super::next_name("agent/fix-1"), "agent/fix-1-2");
    }
}
