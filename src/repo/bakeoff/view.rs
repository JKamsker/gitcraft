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
                        (crate::repo::short(&cmp.base)).clone().into(),
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
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-ce199c9e"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.label(c.files.len().to_string());
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-1af85190"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.horizontal(|ui| {
                    ui.colored_label(t.success, format!("+{}", c.added));
                    ui.colored_label(t.danger, format!("−{}", c.removed));
                });
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-9c37d9c9"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                if c.new_deps.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-140bedbf"))
                            .color(t.text_faint),
                    );
                } else {
                    ui.vertical(|ui| {
                        for dep in &c.new_deps {
                            ui.colored_label(t.warning, dep);
                        }
                    });
                }
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-7a44bde0"))
                    .color(t.text_dim),
            );
            for c in &cmp.attempts {
                ui.label(if c.uncommitted {
                    crate::i18n::text("repo-workflows-bakeoff-grid-1f19288a")
                } else {
                    "no"
                });
            }
            ui.end_row();
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-bakeoff-grid-6a3fe887"))
                    .color(t.text_dim),
            );
            for (i, a) in view.attempts.iter().enumerate() {
                let built = view.builds.get(i).copied().flatten();
                ui.vertical(|ui| {
                    build_cell(ui, built, a.worktree.is_some(), i, t, act);
                    if view.statuses.get(i).is_some_and(Option::is_some)
                        && ui
                            .small_button(crate::i18n::text("repo-workflows-bakeoff-grid-7cbda0dd"))
                            .on_hover_text(crate::i18n::text(
                                "repo-workflows-bakeoff-grid-6263bbf3",
                            ))
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
            Some(Built::Preparing) => {
                ui.label(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-98cd8a0b",
                ));
            }
            Some(Built::Unavailable) => {
                ui.colored_label(
                    t.warning,
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-5dac180d"),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-63f96d79",
                ));
            }
            Some(Built::Running) => {
                ui.add(crate::spinner::Spinner::new().size(12.0));
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bakeoff-build-cell-899686f3",
                    ))
                    .color(t.text_dim),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-ef24cfb5",
                ));
            }
            Some(Built::Passed) => {
                ui.colored_label(
                    t.success,
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-d07e6c18"),
                );
            }
            Some(Built::Failed(code)) => {
                ui.colored_label(
                    t.danger,
                    crate::i18n::message(
                        "repo-workflows-bakeoff-build-cell-301c453c",
                        &[("code", (code).clone().into())],
                    ),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-build-cell-ef24cfb5",
                ));
            }
            None => {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bakeoff-build-cell-26c16489",
                    ))
                    .color(t.text_faint),
                );
            }
        }
        if built != Some(Built::Preparing) {
            let (label, tip) = match built {
                Some(Built::Running) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-6fe4abd4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-17824e4b"),
                ),
                Some(Built::Unavailable) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-6fe4abd4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-84b604d3"),
                ),
                Some(_) => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-af81e9e4"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-adccacb7"),
                ),
                None => (
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-bdd254b6"),
                    crate::i18n::text("repo-workflows-bakeoff-build-cell-3149d8b3"),
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
            &[("name-n-1", (name(*n)).clone().into())],
        ),
        Take::Hunks(list) if list.len() == 1 => {
            crate::i18n::text("repo-workflows-bakeoff-pick-label-d3d98b5d").to_owned()
        }
        Take::Hunks(list) => crate::i18n::message(
            "repo-workflows-bakeoff-pick-label-c708de81",
            &[("list-len-1", (list.len()).clone().into())],
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
            &[("hunk-old-start-1", (hunk.old_start).clone().into())],
        ),
        1 => crate::i18n::message(
            "repo-workflows-bakeoff-hunk-label-4d9d8a81",
            &[("hunk-old-start-1", (hunk.old_start).clone().into())],
        ),
        n => crate::i18n::message(
            "repo-workflows-bakeoff-hunk-label-a3052fdf",
            &[
                ("hunk-old-start-1", (hunk.old_start).clone().into()),
                (
                    "hunk-old-start-n-1-2",
                    (hunk.old_start + n - 1).clone().into(),
                ),
            ],
        ),
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
                .small_button(crate::i18n::text(
                    "repo-workflows-bakeoff-side-by-side-6bdfa343",
                ))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-bakeoff-side-by-side-76215eb4",
                ))
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
            crate::i18n::text("repo-workflows-bakeoff-side-by-side-f280944f"),
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
            RichText::new(crate::i18n::message(
                "repo-workflows-bakeoff-side-by-side-47c58b04",
                &[("unchanged-join-1", (unchanged.join(", ")).clone().into())],
            ))
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
    ui.label(
        RichText::new(format!("{name} · {}", side.display_kind())).font(theme::semibold(12.0)),
    );
    let whole = take == Some(&Take::File(n));
    let label = match side.kind() {
        "deleted" => crate::i18n::text("repo-workflows-bakeoff-column-772841f1"),
        "added" => crate::i18n::text("repo-workflows-bakeoff-column-badecf50"),
        _ => crate::i18n::text("repo-workflows-bakeoff-column-a533487d"),
    };
    if ui
        .selectable_label(whole, label)
        .on_hover_text(crate::i18n::text("repo-workflows-bakeoff-column-6e86718f"))
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
                    RichText::new(crate::i18n::text("repo-workflows-bakeoff-column-2ed1ade8"))
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
                &[("picks", (picks).clone().into())],
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
            ui.colored_label(if *error { t.danger } else { t.success }, note);
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn pure_bakeoff_unavailable_and_running_cells_offer_explicit_fresh_workspace_retry() {
        for built in [Built::Unavailable, Built::Running] {
            let ctx = egui::Context::default();
            let mut act = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let tokens = Tokens::get(ui.ctx());
                    build_cell(ui, Some(built), false, 0, &tokens, &mut act);
                },
            );
            let position = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Retry in fresh workspace" => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            });
            output.textures_delta.clear();
            let position = position.expect("retry remains reachable without a completion receipt");
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    let tokens = Tokens::get(ui.ctx());
                    build_cell(ui, Some(built), false, 0, &tokens, &mut act);
                },
            );
            output.textures_delta.clear();
            assert!(matches!(act, Some(BakeAct::Build(0))));
        }
    }
}
