//! The conflict editor's history strip: each side's commits that touched the file (pick one on
//! each side to test-merge them), _Find where it started_ with its progress and answer, and a
//! commit's change to the file.

mod answer;
use answer::*;
use egui::{RichText, Stroke, Ui};

use super::{Act, Found, History, Read, Step, Timelines, applies};
use crate::git::InProgress;
use crate::repo::Action;
use crate::repo::conflict::ConflictEditor;
use crate::theme::{self, Tokens};

/// How tall each side's list of commits may grow before it scrolls.
const LIST_HEIGHT: f32 = 110.0;
/// The height of one commit's row.
const ROW_HEIGHT: f32 = 18.0;
/// The share of the editor's height the strip may take before it scrolls.
const MAX_SHARE: f32 = 0.35;

/// The commit picked on each side (indexes).
type Picked = (Option<usize>, Option<usize>);

/// Where whether the strip is folded is kept: for every conflict editor of the session.
fn folded_id() -> egui::Id {
    egui::Id::new("gitcraft-conflict-history-folded")
}

impl ConflictEditor {
    /// Under the header, for a merge, rebase, cherry-pick or revert (`state`): the file's
    /// history on each side (`names`: theirs, as the editor calls them), read once shown.
    pub(in crate::repo) fn history_strip(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        (names, state): ((&str, &str), Option<InProgress>),
        acts: &mut Vec<Action>,
    ) {
        if !applies(state) {
            return;
        }
        let ctx = ui.ctx().clone();
        let mut folded = ctx.data(|d| d.get_temp::<bool>(folded_id()).unwrap_or(false));
        let history = &mut self.history;
        if history.timelines == Read::NotAsked && !folded {
            acts.push(Action::Archaeology(Act::Load));
        }
        let most = (ui.available_height() * MAX_SHARE).max(80.0);
        egui::Frame::new()
            .fill(t.card)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                history_header(ui, t, history, &mut folded, acts);
                if folded {
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("conflict-history-strip")
                    .max_height(most)
                    .auto_shrink([false, true])
                    .show(ui, |ui| body(ui, t, history, names, acts));
            });
        ctx.data_mut(|d| d.insert_temp(folded_id(), folded));
    }
}

/// What the strip shows unfolded.
fn body(
    ui: &mut Ui,
    t: &Tokens,
    history: &mut History,
    names: (&str, &str),
    acts: &mut Vec<Action>,
) {
    match &history.timelines {
        Read::NotAsked | Read::Reading => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(RichText::new(crate::i18n::text("repo-core-0436")).color(t.text_dim));
            });
        }
        Read::Failed(e) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(e).color(t.danger));
                if ui
                    .small_button(crate::i18n::text("repo-core-0437"))
                    .clicked()
                {
                    acts.push(Action::Archaeology(Act::Load));
                }
            });
        }
        Read::Done(_) if history.change.is_some() => change_ui(ui, t, history),
        Read::Done(timelines) => {
            let timelines = timelines.clone();
            let found = found_pair(history);
            timelines_ui(ui, t, &timelines, names, (found, &mut history.picked));
            picked_ui(ui, t, history, &timelines, acts);
            search_answer(ui, t, history, &timelines, names);
        }
    }
}

/// The pair found, as indexes in each side's commits.
fn found_pair(history: &History) -> Option<(usize, usize)> {
    match history.search.as_ref().map(|s| &s.result) {
        Some(Read::Done(Found::Pair { ours, theirs, .. })) => Some((*ours, *theirs)),
        _ => None,
    }
}

/// "N test merges so far".
fn merges_text(merges: usize) -> String {
    match merges {
        0 => crate::i18n::text("repo-core-0438").to_owned(),
        1 => crate::i18n::text("repo-core-0439").to_owned(),
        n => crate::i18n::message("repo-core-0440", &[("n", (n).into())]),
    }
}

/// The strip's title, folding it, and _Find where it started_ (or the running search).
fn history_header(
    ui: &mut Ui,
    t: &Tokens,
    history: &mut History,
    folded: &mut bool,
    acts: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        let chevron = if *folded {
            "chevron-right"
        } else {
            "chevron-down"
        };
        if crate::icons::small_button(ui, chevron, crate::i18n::text("repo-core-0441")).clicked() {
            *folded = !*folded;
        }
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0442"))
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        if *folded {
            return;
        }
        let ready = matches!(&history.timelines, Read::Done(_));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(search) = history
                .search
                .as_ref()
                .filter(|s| s.result == Read::Reading)
            {
                let merges = search.merges.load(std::sync::atomic::Ordering::Relaxed);
                let cancel = ui
                    .button(crate::i18n::text("repo-core-0443"))
                    .on_hover_text(crate::i18n::text("repo-core-0444"))
                    .clicked();
                ui.label(RichText::new(merges_text(merges)).color(t.text_dim));
                ui.add(crate::spinner::Spinner::new());
                if cancel {
                    history.cancel();
                }
            } else if ui
                .add_enabled(
                    ready,
                    egui::Button::new(crate::i18n::text("repo-core-0445")),
                )
                .on_hover_text(crate::i18n::text("repo-core-0446"))
                .on_disabled_hover_text(crate::i18n::text("repo-core-0447"))
                .clicked()
            {
                acts.push(Action::Archaeology(Act::Find));
            }
        });
    });
}

/// Each side's commits, newest first, side by side: the pair found marked, the commit picked
/// on each side (`picked`) lit; a click picks (or unpicks) one.
fn timelines_ui(
    ui: &mut Ui,
    t: &Tokens,
    timelines: &Timelines,
    names: (&str, &str),
    (found, picked): (Option<(usize, usize)>, &mut Picked),
) {
    ui.columns(2, |cols| {
        let [left, right] = cols else { return };
        let marks = |side: usize| found.map(|(o, th)| if side == 0 { o } else { th });
        side_ui(
            left,
            t,
            (0, names.0),
            &timelines.ours,
            (marks(0), &mut picked.0),
        );
        side_ui(
            right,
            t,
            (1, names.1),
            &timelines.theirs,
            (marks(1), &mut picked.1),
        );
    });
}

/// One side's commits under its name; `found`: the one the search named, `picked`: the one
/// picked.
fn side_ui(
    ui: &mut Ui,
    t: &Tokens,
    (side, name): (usize, &str),
    steps: &[Step],
    (found, picked): (Option<usize>, &mut Option<usize>),
) {
    let count = match steps.len() {
        1 => crate::i18n::text("repo-core-0448").to_owned(),
        n => crate::i18n::message("repo-core-0449", &[("n", (n).into())]),
    };
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-core-0450",
            &[("name", name.to_string().into()), ("count", (count).into())],
        ))
        .color(t.text_faint)
        .small(),
    );
    if steps.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0451"))
                .color(t.text_dim)
                .small(),
        );
        return;
    }
    let now = crate::git::now();
    egui::ScrollArea::vertical()
        .id_salt(("conflict-history", side))
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_HEIGHT, steps.len(), |ui, rows| {
            for row in rows {
                // Newest first.
                let k = steps.len() - 1 - row;
                let Some(step) = steps.get(k) else { continue };
                let first = if found == Some(k) { "⚑ " } else { "" };
                let text = RichText::new(format!(
                    "{first}{}  {}  · {}",
                    step.short,
                    step.summary,
                    crate::git::relative_time(step.time, now)
                ))
                .font(theme::mono(11.5))
                .color(if found == Some(k) { t.accent } else { t.text });
                let response = ui
                    .add(egui::Button::selectable(*picked == Some(k), text).truncate())
                    .on_hover_ui(|ui| {
                        ui.label(format!("{} {}\n{}", step.short, step.summary, step.author));
                        if found == Some(k) {
                            ui.label(crate::i18n::text("repo-core-0454"));
                        }
                        ui.label(crate::i18n::text("repo-core-0455"));
                    });
                if response.clicked() {
                    *picked = if *picked == Some(k) { None } else { Some(k) };
                }
            }
        });
}

/// For the commits picked: what each changed, and whether the two conflict.
fn picked_ui(
    ui: &mut Ui,
    t: &Tokens,
    history: &History,
    timelines: &Timelines,
    acts: &mut Vec<Action>,
) {
    let (o, th) = history.picked;
    let ours = o.and_then(|k| timelines.ours.get(k));
    let theirs = th.and_then(|k| timelines.theirs.get(k));
    if ours.is_none() && theirs.is_none() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for step in [ours, theirs].into_iter().flatten() {
            if ui
                .small_button(crate::i18n::message(
                    "repo-core-0456",
                    &[("short", step.short.to_string().into())],
                ))
                .on_hover_text(crate::i18n::text("repo-core-0457"))
                .clicked()
            {
                acts.push(Action::Archaeology(Act::ShowChange(
                    step.hash.clone(),
                    step.short.clone(),
                )));
            }
        }
        if let (Some(i), Some(j), Some(a), Some(b)) = (o, th, ours, theirs) {
            if ui
                .small_button(crate::i18n::text("repo-core-0458"))
                .on_hover_text(crate::i18n::text("repo-core-0459"))
                .clicked()
            {
                acts.push(Action::Archaeology(Act::TestPair(i, j)));
            }
            match history.pair_test.as_ref().filter(|(p, _)| *p == (i, j)) {
                Some((_, Read::Reading)) => {
                    ui.add(crate::spinner::Spinner::new());
                }
                Some((_, Read::Done(conflicts))) => {
                    let (text, color) = if *conflicts {
                        (
                            crate::i18n::message(
                                "repo-core-0460",
                                &[
                                    ("short", a.short.to_string().into()),
                                    ("short2", b.short.to_string().into()),
                                ],
                            ),
                            t.warning,
                        )
                    } else {
                        (
                            crate::i18n::message(
                                "repo-core-0461",
                                &[
                                    ("short", a.short.to_string().into()),
                                    ("short2", b.short.to_string().into()),
                                ],
                            ),
                            t.success,
                        )
                    };
                    ui.label(RichText::new(text).color(color));
                }
                Some((_, Read::Failed(e))) => {
                    ui.label(RichText::new(e).color(t.danger));
                }
                _ => {}
            }
        }
    });
}

/// What _Find where it started_ came to.
fn change_ui(ui: &mut Ui, t: &Tokens, history: &mut History) {
    let Some((_, short, read)) = &history.change else {
        return;
    };
    let mut close = false;
    ui.horizontal(|ui| {
        close = ui
            .small_button(crate::i18n::text("repo-core-0470"))
            .clicked();
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-core-0471",
                &[("short", short.to_string().into())],
            ))
            .color(t.text_faint)
            .small(),
        );
    });
    match read {
        Read::NotAsked | Read::Reading => {
            ui.add(crate::spinner::Spinner::new());
        }
        Read::Failed(e) => {
            ui.label(RichText::new(e).color(t.danger));
        }
        Read::Done(text) if text.trim().is_empty() => {
            ui.label(RichText::new(crate::i18n::text("repo-core-0472")).color(t.text_dim));
        }
        Read::Done(text) => {
            let lines: Vec<&str> = text.lines().collect();
            egui::ScrollArea::horizontal()
                .id_salt("conflict-history-change")
                .show(ui, |ui| {
                    for line in lines {
                        let color = match line.chars().next() {
                            Some('+') if !line.starts_with("+++") => t.success,
                            Some('-') if !line.starts_with("---") => t.danger,
                            Some('@') => t.text_faint,
                            _ => t.text,
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(line).font(theme::mono(11.5)).color(color),
                            )
                            .extend(),
                        );
                    }
                });
        }
    }
    if close {
        history.change = None;
    }
}

#[cfg(test)]
mod tests;
