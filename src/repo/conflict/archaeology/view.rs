//! The conflict editor's history strip: each side's commits that touched the file, _Find where
//! it started_ with its progress and answer, and a commit's change to the file.

use egui::{RichText, Stroke, Ui};

use super::{Act, Found, History, Read, Step, Timelines};
use crate::repo::Action;
use crate::repo::conflict::ConflictEditor;
use crate::theme::{self, Tokens};

/// How tall each side's list of commits may grow before it scrolls.
const LIST_HEIGHT: f32 = 110.0;
/// How tall a commit's change may grow before it scrolls.
const CHANGE_HEIGHT: f32 = 220.0;

impl ConflictEditor {
    /// Under the header: the file's history on each side (`names`: theirs, as the editor
    /// calls them), read as soon as the editor shows.
    pub(in crate::repo) fn history_strip(
        &mut self,
        ui: &mut Ui,
        t: &Tokens,
        names: (&str, &str),
        acts: &mut Vec<Action>,
    ) {
        let history = &mut self.history;
        if history.timelines == Read::NotAsked {
            acts.push(Action::Archaeology(Act::Load));
        }
        egui::Frame::new()
            .fill(t.card)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                history_header(ui, t, history, acts);
                if history.folded {
                    return;
                }
                match &history.timelines {
                    Read::NotAsked | Read::Reading => {
                        ui.horizontal(|ui| {
                            ui.add(crate::spinner::Spinner::new());
                            ui.label(
                                RichText::new("Reading each side's commits…").color(t.text_dim),
                            );
                        });
                    }
                    Read::Failed(e) => {
                        ui.label(RichText::new(e).color(t.danger));
                    }
                    Read::Done(timelines) => {
                        let pair = found_pair(history);
                        let open = history.change.as_ref().map(|(h, _)| h.as_str());
                        timelines_ui(ui, t, timelines, names, (pair, open), acts);
                        search_answer(ui, t, history, timelines, acts);
                    }
                }
                change_ui(ui, t, history);
            });
    }
}

/// The pair found, as indexes in each side's commits.
fn found_pair(history: &History) -> Option<(usize, usize)> {
    match history.search.as_ref().map(|s| &s.result) {
        Some(Read::Done(Found::Pair { ours, theirs })) => Some((*ours, *theirs)),
        _ => None,
    }
}

/// The strip's title, folding it, and _Find where it started_ (or the running search).
fn history_header(ui: &mut Ui, t: &Tokens, history: &mut History, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let chevron = if history.folded {
            "chevron-right"
        } else {
            "chevron-down"
        };
        if crate::icons::small_button(ui, chevron, "Show or hide each side's history").clicked() {
            history.folded = !history.folded;
        }
        ui.label(
            RichText::new("HOW THIS CONFLICT CAME ABOUT")
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        let ready = matches!(&history.timelines, Read::Done(_));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let running = history
                .search
                .as_ref()
                .filter(|s| s.result == Read::Reading);
            if let Some(search) = running {
                if ui.button("Cancel").on_hover_text("Stop looking").clicked() {
                    history.cancel();
                }
                let merges = search.merges.load(std::sync::atomic::Ordering::Relaxed);
                ui.label(RichText::new(format!("{merges} test merges so far…")).color(t.text_dim));
                ui.add(crate::spinner::Spinner::new());
            } else if ui
                .add_enabled(ready, egui::Button::new("Find where it started"))
                .on_hover_text(
                    "Find the first pair of commits, one from each side, whose merge already \
                     conflicts in this file. Each test merge is done apart from your files \
                     (git merge-tree): nothing here changes.",
                )
                .clicked()
            {
                acts.push(Action::Archaeology(Act::Find));
            }
        });
    });
}

/// Each side's commits, newest first, side by side; the pair found and the commit whose change
/// is open lit (`marks`). A click on one shows its change.
fn timelines_ui(
    ui: &mut Ui,
    t: &Tokens,
    timelines: &Timelines,
    names: (&str, &str),
    marks: (Option<(usize, usize)>, Option<&str>),
    acts: &mut Vec<Action>,
) {
    if timelines.truncated {
        ui.label(
            RichText::new(format!(
                "Only the latest {} commits of a side are read.",
                super::git::MAX_COMMITS
            ))
            .color(t.warning)
            .small(),
        );
    }
    let (pair, open) = marks;
    ui.columns(2, |cols| {
        for (side, (col, (name, steps))) in cols
            .iter_mut()
            .zip([(names.0, &timelines.ours), (names.1, &timelines.theirs)])
            .enumerate()
        {
            let lit = pair.map(|(o, th)| if side == 0 { o } else { th });
            side_ui(col, t, (side, name), steps, (lit, open), acts);
        }
    });
}

/// One side's commits under its name.
fn side_ui(
    ui: &mut Ui,
    t: &Tokens,
    (side, name): (usize, &str),
    steps: &[Step],
    (lit, open): (Option<usize>, Option<&str>),
    acts: &mut Vec<Action>,
) {
    let count = match steps.len() {
        1 => "1 commit".to_owned(),
        n => format!("{n} commits"),
    };
    ui.label(
        RichText::new(format!("{name} · {count} on this file"))
            .color(t.text_faint)
            .small(),
    );
    if steps.is_empty() {
        ui.label(
            RichText::new("Didn't change this file since they diverged.")
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
        .show(ui, |ui| {
            for (k, step) in steps.iter().enumerate().rev() {
                let on = lit == Some(k) || open == Some(step.hash.as_str());
                let text = RichText::new(format!(
                    "{}  {}  · {}",
                    step.short,
                    step.summary,
                    crate::git::relative_time(step.time, now)
                ))
                .font(theme::mono(11.5));
                let text = if on {
                    text.color(t.accent)
                } else {
                    text.color(t.text)
                };
                let row = ui
                    .add(
                        egui::Label::new(text)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text(format!(
                        "{} {}\n{}\nClick to see what it changed in this file",
                        step.short, step.summary, step.author
                    ));
                if row.clicked() {
                    acts.push(Action::Archaeology(Act::ShowChange(step.hash.clone())));
                }
            }
        });
}

/// What _Find where it started_ came to.
fn search_answer(
    ui: &mut Ui,
    t: &Tokens,
    history: &History,
    timelines: &Timelines,
    acts: &mut Vec<Action>,
) {
    let Some(search) = &history.search else {
        return;
    };
    match &search.result {
        Read::NotAsked | Read::Reading => {}
        Read::Failed(e) => {
            ui.label(RichText::new(e).color(t.danger));
        }
        Read::Done(found) => {
            let text = answer_text(found, timelines);
            ui.horizontal_wrapped(|ui| {
                crate::icons::inline(ui, "search", 13.0, t.accent);
                ui.label(RichText::new(text).color(t.text));
                if let Found::Pair { ours, theirs } = found {
                    for step in [timelines.ours.get(*ours), timelines.theirs.get(*theirs)]
                        .into_iter()
                        .flatten()
                    {
                        if ui
                            .small_button(format!("Change of {}", step.short))
                            .on_hover_text("What this commit changed in the file")
                            .clicked()
                        {
                            acts.push(Action::Archaeology(Act::ShowChange(step.hash.clone())));
                        }
                    }
                }
            });
        }
    }
}

/// The answer in words.
fn answer_text(found: &Found, timelines: &Timelines) -> String {
    match found {
        Found::Pair { ours, theirs } => {
            let (Some(a), Some(b)) = (timelines.ours.get(*ours), timelines.theirs.get(*theirs))
            else {
                return "The commits changed meanwhile: read them again".to_owned();
            };
            format!(
                "It started with {} “{}” and {} “{}”: merged together they already conflict in \
                 this file, while each merges cleanly with the other side's earlier commits.",
                a.short, a.summary, b.short, b.summary
            )
        }
        Found::OneSided => "Only one side changed this file since they diverged, so no pair of \
                            commits conflicts in it: the conflict comes from elsewhere (a \
                            rename, a deletion, the file's mode)."
            .to_owned(),
        Found::TipsMergeCleanly => "Merged on their own, the two sides' latest commits don't \
                                    conflict in this file: the conflict comes from what was \
                                    replayed or merged before it, or from a rename."
            .to_owned(),
    }
}

/// A commit's change to the file, when one is open.
fn change_ui(ui: &mut Ui, t: &Tokens, history: &mut History) {
    let Some((hash, read)) = &history.change else {
        return;
    };
    let mut close = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let short: String = hash.chars().take(9).collect();
        ui.label(
            RichText::new(format!("What {short} changed in this file"))
                .color(t.text_faint)
                .small(),
        );
        close = crate::icons::small_button(ui, "x", "Hide").clicked();
    });
    match read {
        Read::NotAsked | Read::Reading => {
            ui.add(crate::spinner::Spinner::new());
        }
        Read::Failed(e) => {
            ui.label(RichText::new(e).color(t.danger));
        }
        Read::Done(text) => {
            egui::ScrollArea::both()
                .id_salt(("conflict-history-change", hash.as_str()))
                .max_height(CHANGE_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if text.trim().is_empty() {
                        ui.label(RichText::new("(no change to this file)").color(t.text_dim));
                    }
                    for line in text.lines() {
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
mod tests {
    use super::*;

    fn step(short: &str, summary: &str) -> Step {
        Step {
            hash: format!("{short}000"),
            short: short.to_owned(),
            author: "a".to_owned(),
            time: 0,
            summary: summary.to_owned(),
        }
    }

    #[test]
    fn the_answer_names_the_pair() {
        let timelines = Timelines {
            ours: vec![step("a1", "add caching"), step("a2", "tune")],
            theirs: vec![step("b1", "change ownership")],
            truncated: false,
        };
        let text = answer_text(&Found::Pair { ours: 0, theirs: 0 }, &timelines);
        assert!(text.contains("a1 “add caching”"), "{text}");
        assert!(text.contains("b1 “change ownership”"), "{text}");
        let gone = answer_text(&Found::Pair { ours: 5, theirs: 0 }, &timelines);
        assert!(gone.contains("changed meanwhile"));
        assert!(answer_text(&Found::OneSided, &timelines).contains("Only one side"));
        assert!(answer_text(&Found::TipsMergeCleanly, &timelines).contains("latest commits"));
    }
}
