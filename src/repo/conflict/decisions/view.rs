//! Each conflict's decision in the conflict editor: the one made before (its reason, its test,
//! whether it is stale) and the note and test written for this one.

use egui::{RichText, Stroke, Ui};

use super::memo::{Draft, Memo, Offer, one_line};
use crate::theme::{self, Tokens};

/// What was clicked on a conflict's decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Pick {
    /// Resolve the conflict with the decision's text.
    Use,
    /// Take its note and test as the user's own.
    TakeNote,
}

/// Under conflict `k`'s sides: its offered decision and its note. `state`: the conflict is
/// resolved with the offered decision's text, the result is edited by hand.
pub(in crate::repo) fn card(
    ui: &mut Ui,
    t: &Tokens,
    k: usize,
    memo: &mut Memo,
    state: (bool, bool),
) -> Option<Pick> {
    let mut pick = None;
    let draft_empty = memo.drafts.get(k).is_none_or(Draft::is_empty);
    if let Some(offer) = memo
        .offers
        .as_mut()
        .and_then(|o| o.get_mut(k))
        .and_then(Option::as_mut)
    {
        ui.add_space(4.0);
        pick = offer_box(ui, t, k, offer, (state, draft_empty));
    }
    if let Some(draft) = memo.drafts.get_mut(k) {
        ui.add_space(4.0);
        draft_ui(ui, t, k, draft);
    }
    pick
}

/// "3 days ago", or nothing for a time not known.
fn ago(at: i64) -> String {
    if at <= 0 {
        String::new()
    } else {
        crate::git::relative_time(at, crate::git::now())
    }
}

/// What is said of a decision's test: the command, and how it last went.
pub(in crate::repo) fn test_line(offer: &Offer) -> Option<(String, Option<bool>)> {
    let d = &offer.decision;
    if d.test.is_empty() {
        return None;
    }
    Some(match &d.last_test {
        Some(run) if run.passed => (
            format!("Test “{}” passed {}", d.test, ago(run.at)),
            Some(true),
        ),
        Some(run) => (
            format!("Test “{}” failed {}", d.test, ago(run.at)),
            Some(false),
        ),
        None => (format!("Test “{}”: not run yet", d.test), None),
    })
}

/// The decision made before for conflict `k`; `applied`: it resolves the conflict now,
/// `manual`: the result is edited by hand, `draft_empty`: no note written for it yet.
fn offer_box(
    ui: &mut Ui,
    t: &Tokens,
    k: usize,
    offer: &mut Offer,
    ((applied, manual), draft_empty): ((bool, bool), bool),
) -> Option<Pick> {
    let mut pick = None;
    let border = if offer.stale { t.warning } else { t.accent };
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let d = &offer.decision;
            ui.horizontal_wrapped(|ui| {
                crate::icons::inline(ui, "history", 13.0, border);
                let title = if offer.stale {
                    "Decided before, but the code around it changed since"
                } else if applied {
                    "Resolved as you decided before"
                } else {
                    "Decided before"
                };
                ui.label(
                    RichText::new(title)
                        .font(theme::semibold(12.0))
                        .color(t.text),
                );
                let mut who = vec![];
                if !d.by.is_empty() {
                    who.push(d.by.clone());
                }
                let when = ago(d.at);
                if !when.is_empty() {
                    who.push(when);
                }
                if !who.is_empty() {
                    ui.label(RichText::new(who.join(", ")).color(t.text_faint).small());
                }
            });
            if !d.note.is_empty() {
                ui.label(RichText::new(format!("“{}”", d.note)).color(t.text));
            }
            if let Some((line, passed)) = test_line(offer) {
                let color = match passed {
                    Some(true) => t.success,
                    Some(false) => t.danger,
                    None => t.text_dim,
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(line).color(color).small());
                    let has_output = offer
                        .decision
                        .last_test
                        .as_ref()
                        .is_some_and(|r| !r.output.is_empty());
                    if has_output {
                        ui.toggle_value(&mut offer.show_output, "Output");
                    }
                });
                if offer.show_output
                    && let Some(run) = &offer.decision.last_test
                {
                    egui::ScrollArea::both()
                        .id_salt(("decision-output", k))
                        .max_height(140.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&run.output).font(theme::mono(11.0)),
                                )
                                .extend(),
                            );
                        });
                }
            }
            if offer.stale {
                ui.label(
                    RichText::new(
                        "Not applied by itself: check that it still fits before you use it.",
                    )
                    .color(t.warning)
                    .small(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if !applied {
                    let label = if offer.stale {
                        "Use it anyway"
                    } else {
                        "Use this decision"
                    };
                    if ui
                        .add_enabled(!manual, egui::Button::new(label))
                        .on_hover_text("Resolve this conflict with the text decided before")
                        .on_disabled_hover_text(
                            "The result is edited by hand: Back to choices first",
                        )
                        .clicked()
                    {
                        pick = Some(Pick::Use);
                    }
                }
                let has_note = !offer.decision.note.is_empty() || !offer.decision.test.is_empty();
                if draft_empty
                    && has_note
                    && ui
                        .small_button("Keep its note and test")
                        .on_hover_text(
                            "Take its reason and test as yours for this resolution: kept when \
                             you save, and the test run then if you ask",
                        )
                        .clicked()
                {
                    pick = Some(Pick::TakeNote);
                }
                if offer.stale {
                    ui.toggle_value(&mut offer.show_context, "What changed around it");
                }
            });
            if offer.stale && offer.show_context {
                context_ui(ui, t, k, offer);
            }
        });
    pick
}

/// The lines around the conflict when the decision was made (`then`), beside those now.
fn context_ui(ui: &mut Ui, t: &Tokens, k: usize, offer: &Offer) {
    let d = &offer.decision;
    let marker = "  ⋯ the conflict ⋯\n";
    let then = format!("{}{marker}{}", d.before, d.after);
    let now = format!("{}{marker}{}", offer.now.0, offer.now.1);
    ui.columns(2, |cols| {
        for (col, (title, text)) in cols
            .iter_mut()
            .zip([("Then", then.as_str()), ("Now", now.as_str())])
        {
            col.label(RichText::new(title).color(t.text_faint).small());
            egui::ScrollArea::horizontal()
                .id_salt(("decision-context", k, title))
                .show(col, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(text.trim_end()).font(theme::mono(11.5)))
                            .extend(),
                    );
                });
        }
    });
}

/// The note and test for conflict `k`, kept with its resolution when the file is saved.
fn draft_ui(ui: &mut Ui, t: &Tokens, k: usize, draft: &mut Draft) {
    if !draft.open {
        let label = if draft.is_empty() {
            "Note why…"
        } else {
            "Edit the note…"
        };
        if ui
            .small_button(label)
            .on_hover_text(
                "Keep a note on why it is resolved so, and a command that tests it: when the \
                 same conflict comes back, it is resolved the same way, with the note shown.",
            )
            .clicked()
        {
            draft.open = true;
        }
        if !draft.is_empty() {
            let note = draft.note.trim();
            let kept = if note.is_empty() {
                format!("Kept when saved: the test “{}”", one_line(&draft.test))
            } else {
                format!("Kept when saved: “{note}”")
            };
            ui.label(RichText::new(kept).color(t.text_faint).small());
        }
        return;
    }
    egui::Grid::new(("decision-draft", k))
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Why").color(t.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.note)
                    .hint_text("keep the new validation, keep the old wire format")
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
            ui.label(RichText::new("Test").color(t.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.test)
                    .hint_text("a command that checks it, e.g. cargo test wire_format")
                    .font(theme::mono(12.0))
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Kept in git with the resolution when you save.")
                .color(t.text_faint)
                .small(),
        );
        if ui.small_button("Done").clicked() {
            draft.test = one_line(&draft.test);
            draft.open = false;
        }
    });
}

/// In the header, when a conflict has a test: whether to run it once the file is saved.
pub(in crate::repo) fn run_tests_switch(ui: &mut Ui, memo: &mut Memo, has_tests: bool) {
    if !has_tests {
        return;
    }
    let drafts = &memo.drafts;
    ui.checkbox(&mut memo.run_tests, "Test after saving")
        .on_hover_ui(|ui| {
            let mut tests: Vec<String> = drafts
                .iter()
                .map(|d| one_line(&d.test))
                .filter(|c| !c.is_empty())
                .collect();
            tests.sort();
            tests.dedup();
            ui.label(format!(
                "Runs the tests you wrote, in the repository's folder, once the file is \
                 saved:\n{}\nHow each went is kept with its decision.",
                tests.join("\n")
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::super::record::{Decision, TestRun};
    use super::*;

    #[test]
    fn the_test_line_says_how_it_last_went() {
        let mut offer = Offer {
            decision: Decision {
                test: "cargo test".into(),
                ..Decision::default()
            },
            stale: false,
            show_context: false,
            now: Default::default(),
            show_output: false,
        };
        assert_eq!(
            test_line(&offer),
            Some(("Test “cargo test”: not run yet".to_owned(), None))
        );
        offer.decision.last_test = Some(TestRun {
            passed: false,
            at: 0,
            output: String::new(),
        });
        assert_eq!(test_line(&offer).map(|l| l.1), Some(Some(false)));
        offer.decision.test.clear();
        assert_eq!(test_line(&offer), None);
    }
}
