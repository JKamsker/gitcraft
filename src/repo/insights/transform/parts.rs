//! The repeated-edit view's parts beside the matrix: what is compared, the counts, the legend,
//! the patterns as a table, and a cell's edits as text.

use egui::{RichText, Ui};

use super::{Act, Of, State, act, open, row_label};
use crate::insights::transform::Transform;
use crate::repo::insights::common::{self, Mark};
use crate::repo::{Action, short};
use crate::theme::{self, Tokens};

/// How many edits of a cell are written out, and how many lines of each.
const SHOWN_EDITS: usize = 30;
const SHOWN_LINES: usize = 40;

/// What is compared, and whether the patterns made once are shown.
pub(super) fn controls(
    ui: &mut Ui,
    state: &State,
    prs: &[(u64, String)],
    base: Option<&str>,
    commit: Option<&str>,
    acts: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new("Edits of").size(12.0));
        let mut of = state.of;
        let commit_text = match commit {
            Some(hash) => format!("commit {}", short(hash)),
            None => "HEAD's commit".to_owned(),
        };
        ui.selectable_value(&mut of, Of::Commit, commit_text)
            .on_hover_text("The commit selected in the graph against its first parent");
        let branch_text = match base {
            Some(base) => format!("this branch since {base}"),
            None => "this branch".to_owned(),
        };
        ui.selectable_value(&mut of, Of::Branch, branch_text)
            .on_hover_text("HEAD since its merge base with the default branch");
        if prs.is_empty() {
            ui.label(
                RichText::new("no pull requests listed")
                    .size(11.5)
                    .color(Tokens::get(ui.ctx()).text_faint),
            )
            .on_hover_text("Pull requests appear once a forge is connected and has listed them");
        } else {
            let text = match of {
                Of::Pr(n) => format!("#{n}"),
                _ => "a pull request".to_owned(),
            };
            egui::ComboBox::from_id_salt("transform-pr")
                .selected_text(text)
                .show_ui(ui, |ui| {
                    for (n, title) in prs {
                        ui.selectable_value(&mut of, Of::Pr(*n), format!("#{n} {title}"));
                    }
                });
        }
        if of != state.of {
            acts.push(act(Act::Of(of)));
        }
        let mut hide = state.hide_once;
        if ui
            .checkbox(&mut hide, "Hide edits made once")
            .on_hover_text("Their count is still said; deviations stay shown")
            .changed()
        {
            acts.push(act(Act::HideOnce(hide)));
        }
    });
}

/// The counts, and how the edits were matched.
pub(super) fn summary(ui: &mut Ui, t: &Transform, loading: bool) {
    let tokens = Tokens::get(ui.ctx());
    let s = t.summary();
    let mut line = format!(
        "{} edits in {} files: {} matching in {} repeated patterns, {} deviating, {} made once",
        s.edits,
        t.files.len(),
        s.matching,
        s.repeated,
        s.deviating,
        s.one_off
    );
    if t.binary > 0 {
        line.push_str(&format!(" · {} binary files not compared", t.binary));
    }
    if loading {
        line.push_str(" · reading again…");
    }
    ui.label(RichText::new(line).size(12.0).color(tokens.text_dim));
    ui.label(
        RichText::new(
            "Matched on normalized tokens, not syntax: names both sides share become \
             placeholders ($a), so a pattern is a textual likeness, inferred.",
        )
        .size(11.5)
        .color(tokens.text_faint),
    );
}

pub(super) fn legend(ui: &mut Ui, t: &Tokens) {
    common::legend(
        ui,
        &[
            (
                Mark::Fill(common::heat(t, 0.7)),
                "edits of the pattern, darker: more",
            ),
            (
                Mark::Glyph("!", t.warning),
                "the file also has edits deviating from it",
            ),
            (Mark::Hatched(t.warning), "a deviation (row marked !)"),
            (Mark::Hollow(t.text_dim), "made once"),
            (Mark::Dotted(t.text_dim), "whitespace only"),
        ],
    );
}

/// The patterns as a table; a row picks the pattern's first file.
pub(super) fn patterns_table(
    ui: &mut Ui,
    t: &Transform,
    rows: &[usize],
    selected: Option<(usize, usize)>,
    acts: &mut Vec<Action>,
) {
    common::heading(ui, &format!("Patterns ({} shown)", rows.len()));
    let widths = [420.0, 60.0, 60.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Pattern".into(),
            "Edits".into(),
            "Files".into(),
            "Deviating".into(),
            "Kind".into(),
        ],
        true,
        false,
    );
    for &p in rows {
        let pattern = &t.patterns[p];
        let kind = match pattern.deviates_from {
            Some(of) => format!("deviates from #{}", of + 1),
            None if pattern.edits.len() == 1 => "made once".to_owned(),
            None => "repeated".to_owned(),
        };
        let cells = [
            row_label(t, p),
            pattern.edits.len().to_string(),
            pattern.per_file.len().to_string(),
            pattern.deviating.to_string(),
            kind,
        ];
        let resp = common::table_row(
            ui,
            &widths,
            &cells,
            false,
            selected.is_some_and(|(s, _)| s == p),
        );
        if resp.clicked()
            && let Some(&(f, _)) = pattern.per_file.first()
        {
            acts.push(act(Act::Select(Some((p, f)))));
        }
    }
    if let Some((p, _)) = selected
        && let Some(pattern) = t.patterns.get(p)
    {
        common::heading(ui, "Its files");
        for &(f, n) in &pattern.per_file {
            let cells = [t.files[f].clone(), n.to_string()];
            let resp =
                common::table_row(ui, &[420.0, 60.0], &cells, false, selected == Some((p, f)));
            if resp.clicked() {
                acts.push(act(Act::Select(Some((p, f)))));
            }
        }
    }
}

/// A cell's edits as text, removed and added lines, with a link to each in its diff.
pub(super) fn cell_details(ui: &mut Ui, t: &Transform, p: usize, f: usize, acts: &mut Vec<Action>) {
    let (Some(pattern), Some(path)) = (t.patterns.get(p), t.files.get(f)) else {
        return;
    };
    let tokens = Tokens::get(ui.ctx());
    common::heading(ui, &format!("{} in {path}", pattern.label()));
    let (own, deviant) = t.cell(p, f);
    if own.is_empty() && deviant.is_empty() {
        common::empty(ui, "The pattern has no edits in this file.");
        return;
    }
    ui.horizontal(|ui| {
        if ui.button("Open the diff with context").clicked()
            && let Some(a) = open(t, &[own.clone(), deviant.clone()].concat())
        {
            acts.push(a);
        }
        if let Some(of) = pattern.deviates_from {
            ui.label(
                RichText::new(format!("Deviates from #{}", of + 1))
                    .size(11.5)
                    .color(tokens.warning),
            );
        }
    });
    let total = own.len() + deviant.len();
    for (list, deviates) in [(&own, false), (&deviant, true)] {
        for &e in list.iter().take(SHOWN_EDITS) {
            let Some(edit) = t.edits.get(e) else {
                continue;
            };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let head = format!("line {}", edit.new_line.max(edit.old_line).max(1));
                if ui
                    .link(head)
                    .on_hover_text("Open the diff at this edit")
                    .clicked()
                    && let Some(a) = open(t, &[e])
                {
                    acts.push(a);
                }
                if deviates {
                    ui.label(
                        RichText::new("! deviates from this pattern")
                            .size(11.5)
                            .color(tokens.warning),
                    );
                }
            });
            let font = theme::mono(12.0);
            let mut room = SHOWN_LINES;
            for line in edit.old.iter().take(room) {
                ui.label(
                    RichText::new(format!("-{line}"))
                        .font(font.clone())
                        .color(tokens.text)
                        .background_color(tokens.diff_del_bg),
                );
            }
            room = room.saturating_sub(edit.old.len());
            for line in edit.new.iter().take(room) {
                ui.label(
                    RichText::new(format!("+{line}"))
                        .font(font.clone())
                        .color(tokens.text)
                        .background_color(tokens.diff_add_bg),
                );
            }
            let lines = edit.old.len() + edit.new.len();
            if lines > SHOWN_LINES {
                common::empty(
                    ui,
                    &format!("{} more lines: open the diff", lines - SHOWN_LINES),
                );
            }
        }
    }
    if total > 2 * SHOWN_EDITS || own.len() > SHOWN_EDITS || deviant.len() > SHOWN_EDITS {
        common::empty(
            ui,
            &format!("Not every edit of the {total} here is listed."),
        );
    }
}
