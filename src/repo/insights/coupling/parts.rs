//! The change-coupling view's parts beside the network: its controls, what was counted, the
//! legend, and the lists of what a change leaves out, of a pair's commits and of the pairs.

use egui::{RichText, Ui};

use super::{Act, BULKS, Change, LIMITS, Model, RANGES, State, TOPS, act, label, percent};
use crate::insights::coupling::{self, Coupling, LeftOut};
use crate::repo::file_view::FileTab;
use crate::repo::insights::common::{self, Mark};
use crate::repo::insights::graph;
use crate::repo::{Action, DiffKey, short};
use crate::theme::Tokens;

/// The limit, what counts as a unit and what is left out, and the change looked at.
pub(super) fn controls(ui: &mut Ui, state: &mut State, acts: &mut Vec<Action>) {
    let mut options = state.options.clone();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut limit = state.limit;
        egui::ComboBox::from_id_salt("coupling-limit")
            .selected_text(format!("Last {limit} commits"))
            .show_ui(ui, |ui| {
                for n in LIMITS {
                    ui.selectable_value(&mut limit, n, format!("Last {n} commits"));
                }
            })
            .response
            .on_hover_text("How much history of HEAD is read");
        if limit != state.limit {
            acts.push(act(Act::Limit(limit)));
        }
        let mut range = state.range;
        egui::ComboBox::from_id_salt("coupling-range")
            .selected_text(RANGES[range].0)
            .show_ui(ui, |ui| {
                for (i, (name, _)) in RANGES.iter().enumerate() {
                    ui.selectable_value(&mut range, i, *name);
                }
            })
            .response
            .on_hover_text("Only commits made in this time are counted");
        if range != state.range {
            state.range = range;
            options.since = RANGES[range].1.map(|s| crate::git::now() - s);
        }
        let depth_text = |d: Option<usize>| match d {
            None => "Files".to_owned(),
            Some(d) => format!("Folders, {d} deep"),
        };
        egui::ComboBox::from_id_salt("coupling-depth")
            .selected_text(depth_text(options.depth))
            .show_ui(ui, |ui| {
                for d in [None, Some(1), Some(2), Some(3)] {
                    ui.selectable_value(&mut options.depth, d, depth_text(d));
                }
            });
        ui.label("At least");
        ui.add(egui::DragValue::new(&mut options.min_support).range(1..=100))
            .on_hover_text("Pairs changed together fewer times aren't kept: too few to tell");
        ui.label("times together");
        egui::ComboBox::from_id_salt("coupling-top")
            .selected_text(format!("Strongest {}", options.top))
            .show_ui(ui, |ui| {
                for n in TOPS {
                    ui.selectable_value(&mut options.top, n, format!("Strongest {n}"));
                }
            })
            .response
            .on_hover_text("How many pairs the network draws");
        egui::ComboBox::from_id_salt("coupling-bulk")
            .selected_text(format!("Bulk: over {} files", options.bulk))
            .show_ui(ui, |ui| {
                for n in BULKS {
                    ui.selectable_value(&mut options.bulk, n, format!("Bulk: over {n} files"));
                }
            })
            .response
            .on_hover_text(
                "Commits changing more files than this (a reformat, a moved folder) aren't counted",
            );
        let edit = ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("Folder, e.g. src/")
                .desired_width(140.0),
        );
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            options.filter = state.filter.trim().to_owned();
        }
        edit.on_hover_text("Only files under this folder are counted (Enter applies)");
    });
    if options != state.options {
        acts.push(act(Act::Options(options)));
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Left out of").size(12.0));
        let mut change = state.change;
        ui.selectable_value(&mut change, Change::None, "nothing");
        ui.selectable_value(&mut change, Change::Work, "the working tree's changes");
        ui.selectable_value(&mut change, Change::Commit, "the selected commit");
        if change != state.change {
            acts.push(act(Act::Change(change)));
        }
    });
}

/// What was counted and left out, and that co-change is only a hint.
pub(super) fn summary(ui: &mut Ui, model: &Model, counting: bool) {
    let t = Tokens::get(ui.ctx());
    let c = &model.coupling;
    let mut parts = vec![format!("{} commits counted", c.counted)];
    if c.bulk > 0 {
        parts.push(format!(
            "{} bulk changes (over {} files) left out",
            c.bulk, c.options.bulk
        ));
    }
    if c.skipped > 0 {
        parts.push(format!(
            "{} merges or commits outside the filter count nothing",
            c.skipped
        ));
    }
    if c.older > 0 {
        parts.push(format!("{} older than the range", c.older));
    }
    parts.push(format!(
        "{} pairs at {}+ times together",
        c.pairs.len(),
        c.options.min_support
    ));
    if counting {
        parts.push("counting again…".to_owned());
    }
    ui.label(
        RichText::new(parts.join(" · "))
            .size(12.0)
            .color(t.text_dim),
    );
    ui.label(
        RichText::new(
            "Changing together is a correlation read from the history, a hint and not a proven \
             dependency: every edge is inferred (dashed).",
        )
        .size(11.5)
        .color(t.text_faint),
    );
}

pub(super) fn legend(ui: &mut Ui, t: &Tokens, change: bool) {
    let mut entries = vec![
        (
            Mark::Line(t.text_dim, true),
            "changed together (inferred), wider: more often",
        ),
        (
            Mark::Hollow(t.accent),
            "file or folder, larger: changed more",
        ),
    ];
    if change {
        entries.push((Mark::Glyph("C", t.success), "in the change"));
        entries.push((Mark::Glyph("!", t.warning), "coupled, but left out of it"));
    }
    common::legend(ui, &entries);
}

/// What the change leaves out that the history couples with it.
pub(super) fn left_out_list(
    ui: &mut Ui,
    c: &Coupling,
    what: &str,
    files: &[String],
    left: &[LeftOut],
    acts: &mut Vec<Action>,
) {
    common::heading(ui, &format!("Left out of {what} ({} files)", files.len()));
    if files.is_empty() {
        common::empty(ui, "The change has no files.");
        return;
    }
    if left.is_empty() {
        common::empty(
            ui,
            "Nothing the history couples with these files is left out (at this minimum).",
        );
        return;
    }
    let widths = [260.0, 260.0, 70.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Left out".into(),
            "Changes with".into(),
            "Together".into(),
            "Confidence".into(),
            "".into(),
        ],
        true,
        false,
    );
    for l in left.iter().take(100) {
        let cells = [
            c.units[l.unit].name.clone(),
            c.units[l.with].name.clone(),
            l.support.to_string(),
            percent(l.confidence),
            "inferred".into(),
        ];
        let resp = common::table_row(ui, &widths, &cells, false, false).on_hover_text(format!(
            "{} changed in {} of the {} commits that changed {}: a hint, not a dependency",
            c.units[l.unit].name, l.support, c.units[l.with].changes, c.units[l.with].name
        ));
        if resp.clicked() {
            acts.push(act(Act::Pair(Some(l.pair))));
        }
    }
    if left.len() > 100 {
        common::empty(ui, &format!("{} more not listed", left.len() - 100));
    }
}

/// The commits behind a pair, each opening the commit or either file's diff in it.
pub(super) fn pair_commits(ui: &mut Ui, model: &Model, p: &coupling::Pair, acts: &mut Vec<Action>) {
    let c = &model.coupling;
    let (a, b) = (&c.units[p.a].name, &c.units[p.b].name);
    common::heading(
        ui,
        &format!("{a} ↔ {b}: changed together in {} commits", p.support),
    );
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(format!(
            "{} of {a}'s {} changes, {} of {b}'s {} — inferred from the history",
            percent(c.confidence(p, p.a)),
            c.units[p.a].changes,
            percent(c.confidence(p, p.b)),
            c.units[p.b].changes,
        ))
        .size(11.5)
        .color(t.text_dim),
    );
    if (p.support as usize) > p.commits.len() {
        ui.label(
            RichText::new(format!("The first {} are listed.", p.commits.len()))
                .size(11.5)
                .color(t.warning),
        );
    }
    let files = c.options.depth.is_none();
    for &i in &p.commits {
        let Some(commit) = model.history.commits.get(i) else {
            continue;
        };
        ui.horizontal(|ui| {
            let when = crate::git::relative_time(commit.time, crate::git::now());
            if ui
                .link(format!("{} {}", short(&commit.hash), commit.subject))
                .on_hover_text(format!("{} · {when} · select the commit", commit.author))
                .clicked()
            {
                acts.push(Action::SelectHash(commit.hash.clone()));
            }
            if files {
                for name in [a, b] {
                    if ui
                        .small_button(graph::elide(&label(name), 24))
                        .on_hover_text(format!("The diff of {name} in this commit"))
                        .clicked()
                    {
                        acts.push(Action::OpenDiff(DiffKey::Commit {
                            hash: commit.hash.clone(),
                            path: name.clone(),
                        }));
                    }
                }
            }
        });
    }
}

/// A unit, what it changes with, and its history.
pub(super) fn unit_details(ui: &mut Ui, model: &Model, u: usize, acts: &mut Vec<Action>) {
    let c = &model.coupling;
    let unit = &c.units[u];
    common::heading(
        ui,
        &format!("{}: changed in {} commits", unit.name, unit.changes),
    );
    if c.options.depth.is_none() && ui.button("History of the file").clicked() {
        acts.push(Action::FileView {
            path: unit.name.clone(),
            rev: Some(model.history.tip.clone()),
            tab: FileTab::History,
        });
    }
    let widths = [320.0, 80.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Changes with".into(),
            "Together".into(),
            "Confidence".into(),
        ],
        true,
        false,
    );
    for (i, p) in c
        .pairs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.a == u || p.b == u)
        .take(50)
    {
        let other = if p.a == u { p.b } else { p.a };
        let cells = [
            c.units[other].name.clone(),
            p.support.to_string(),
            percent(c.confidence(p, u)),
        ];
        if common::table_row(ui, &widths, &cells, false, false).clicked() {
            acts.push(act(Act::Pair(Some(i))));
        }
    }
}

/// Every pair kept, strongest first: the network as a table.
pub(super) fn pairs_table(
    ui: &mut Ui,
    c: &Coupling,
    selected: Option<usize>,
    acts: &mut Vec<Action>,
) {
    common::heading(ui, &format!("Pairs ({} kept)", c.pairs.len()));
    let widths = [240.0, 240.0, 70.0, 70.0, 70.0];
    common::table_row(
        ui,
        &widths,
        &[
            "File".into(),
            "Changes with".into(),
            "Together".into(),
            "Of first".into(),
            "Of second".into(),
            "Drawn".into(),
        ],
        true,
        false,
    );
    for (i, p) in c.pairs.iter().enumerate().take(500) {
        let cells = [
            c.units[p.a].name.clone(),
            c.units[p.b].name.clone(),
            p.support.to_string(),
            percent(c.confidence(p, p.a)),
            percent(c.confidence(p, p.b)),
            if i < c.shown { "yes" } else { "no" }.to_owned(),
        ];
        let resp = common::table_row(ui, &widths, &cells, false, selected == Some(i));
        if resp.clicked() {
            acts.push(act(Act::Pair(Some(i))));
        }
    }
    if c.pairs.len() > 500 {
        common::empty(
            ui,
            &format!("{} weaker pairs not listed", c.pairs.len() - 500),
        );
    }
}
