//! The files of a pull request by purpose, in the right pane: a switch to the standard order,
//! what the grouping rests on and how fresh it is, links to the insights about the change,
//! then the groups.

use egui::{RichText, Ui};

use super::super::Kind;
use super::{Act, State, act, rows};
use crate::pull_request::PrFiles;
use crate::repo::{Action, RepoTab, insights::InsightsAct, short, widgets};
use crate::theme::Tokens;

const STANDARD: &str = "Standard order";
const BY_PURPOSE: &str = "By purpose";

impl RepoTab {
    /// The switch between the standard order and the groups, then the groups when they are
    /// on. `false`: the standard list is to be shown (asked for, or the grouping can't be).
    pub(in crate::repo) fn purpose_files(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        files: &PrFiles,
    ) -> bool {
        let state = &self.review_purpose;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            if let Some(i) = widgets::switch(ui, &[STANDARD, BY_PURPOSE], usize::from(state.on)) {
                acts.push(act(Act::Show(i == 1)));
            }
        });
        if !state.on {
            return false;
        }
        let Some((subject, _)) = self.purpose_subject() else {
            ui.weak("  The groups come once the files are read at the pull request's head.");
            return false;
        };
        if state.asked.as_ref() != Some(&subject) {
            acts.push(act(Act::Load));
        }
        let unknown = self.folding.unknown(files.files.iter().map(|f| f.path.as_str()));
        if !unknown.is_empty() {
            acts.push(Action::FoldAttributes(unknown));
        }
        let t = Tokens::get(ui.ctx());
        let (units, arranged) = match (&state.units, &state.arranged) {
            (Some(Ok(units)), Some(Ok(arranged))) if state.asked.as_ref() == Some(&subject) => {
                (units, arranged)
            }
            (Some(Err(e)), _) | (_, Some(Err(e))) => {
                unsupported(ui, &t, e);
                return false;
            }
            _ => {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
                return true;
            }
        };
        if state.generation != self.folding.generation {
            acts.push(act(Act::Refresh));
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                header(ui, &t, state, units, arranged, acts);
            });
        ui.separator();
        let threads = self
            .pr_view
            .as_ref()
            .and_then(|v| v.details.as_ref()?.as_ref().ok())
            .map(|d| d.extras.threads.as_slice())
            .unwrap_or_default();
        let open = self.diff.as_ref().map(|d| &d.key);
        egui::ScrollArea::vertical()
            .id_salt("pr-purpose-groups")
            .auto_shrink(false)
            .show(ui, |ui| {
                let ctx = rows::Ctx {
                    units,
                    arranged,
                    files,
                    threads,
                    selected: &state.selected,
                    open,
                };
                for (g, shown) in arranged.groups.iter().enumerate() {
                    rows::group(ui, &ctx, g, shown, acts);
                }
            });
        true
    }
}

/// Why there are no groups: the standard order follows.
fn unsupported(ui: &mut Ui, t: &Tokens, e: &str) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.colored_label(
                t.warning,
                format!("Review by purpose isn't available for this pull request: {e}"),
            );
            ui.label(RichText::new("Showing the standard order.").color(t.text_faint));
        });
}

/// What the groups rest on: the revisions, how complete the reading is, whether an arrangement
/// of the reviewer's applies; and the insights about the same change.
fn header(
    ui: &mut Ui,
    t: &Tokens,
    state: &State,
    units: &super::Units,
    arranged: &super::Arranged,
    acts: &mut Vec<Action>,
) {
    let files: std::collections::BTreeSet<&str> =
        units.units.iter().map(|u| u.path.as_str()).collect();
    ui.label(
        RichText::new(format!(
            "{} changes in {} files, each in exactly one of {} groups · {}..{}",
            units.units.len(),
            files.len(),
            arranged.groups.len(),
            short(&units.base),
            short(&units.head),
        ))
        .small()
        .color(t.text_dim),
    );
    let mine = if arranged.edited {
        "Groups proposed by the rules, arranged by you (✓ marks what you placed)."
    } else {
        "Groups proposed by the rules: hover a change for the rule that placed it."
    };
    ui.label(RichText::new(mine).small().color(t.text_faint));
    for note in units.notes.iter().chain(
        state
            .proposal
            .iter()
            .flat_map(|p| &p.errors)
            .collect::<Vec<_>>(),
    ) {
        ui.colored_label(t.warning, RichText::new(note).small());
    }
    if let Some((base, head)) = &arranged.stale {
        ui.colored_label(
            t.warning,
            format!(
                "Your grouping was made at {}..{}; the pull request changed since (new commits \
                 or a force push), so it isn't applied. These are the rules' groups.",
                short(base),
                short(head)
            ),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Carry over what still matches")
                .on_hover_text("Changes whose content is the same go back where you put them")
                .clicked()
            {
                acts.push(act(Act::CarryOver));
            }
            if ui.button("Discard it").clicked() {
                acts.push(act(Act::Reset));
            }
        });
    }
    if let Some((text, error)) = &state.notice {
        let color = if *error { t.danger } else { t.text_dim };
        ui.colored_label(color, RichText::new(text).small());
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Insights:").small().color(t.text_faint));
        for kind in [Kind::Impact, Kind::ReviewHeat, Kind::Transform] {
            if ui
                .small_button(kind.title())
                .on_hover_text(kind.blurb())
                .clicked()
            {
                acts.push(Action::Insights(InsightsAct::Open(kind)));
            }
        }
        if arranged.edited
            && ui
                .small_button("Reset to the rules' groups")
                .on_hover_text("Forget your renames, moves, splits and merges")
                .clicked()
        {
            acts.push(act(Act::Reset));
        }
    });
}
