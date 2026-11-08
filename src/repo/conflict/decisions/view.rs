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
            crate::i18n::message("repo-core-0524", &[("test", format!("{}", d.test).into()), ("at", format!("{}", ago(run.at)).into())]),
            Some(true),
        ),
        Some(run) => (
            crate::i18n::message("repo-core-0525", &[("test", format!("{}", d.test).into()), ("at", format!("{}", ago(run.at)).into())]),
            Some(false),
        ),
        None => (crate::i18n::message("repo-core-0526", &[("test", format!("{}", d.test).into())]), None),
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
                    crate::i18n::text("repo-core-0528")
                } else if applied {
                    crate::i18n::text("repo-core-0529")
                } else {
                    crate::i18n::text("repo-core-0530")
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
                        ui.toggle_value(&mut offer.show_output, crate::i18n::text("repo-core-0531"));
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
                        crate::i18n::text("repo-core-0533"),
                    )
                    .color(t.warning)
                    .small(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if !applied {
                    let label = if offer.stale {
                        crate::i18n::text("repo-core-0534")
                    } else {
                        crate::i18n::text("repo-core-0535")
                    };
                    if ui
                        .add_enabled(!manual, egui::Button::new(label))
                        .on_hover_text(crate::i18n::text("repo-core-0536"))
                        .on_disabled_hover_text(
                            crate::i18n::text("repo-core-0537"),
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
                        .small_button(crate::i18n::text("repo-core-0538"))
                        .on_hover_text(
                            crate::i18n::text("repo-core-0539"),
                        )
                        .clicked()
                {
                    pick = Some(Pick::TakeNote);
                }
                if offer.stale {
                    ui.toggle_value(&mut offer.show_context, crate::i18n::text("repo-core-0540"));
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
    let marker = crate::i18n::text("repo-core-0541");
    let then = format!("{}{marker}{}", d.before, d.after);
    let now = format!("{}{marker}{}", offer.now.0, offer.now.1);
    ui.columns(2, |cols| {
        for (col, (title, text)) in cols
            .iter_mut()
            .zip([(crate::i18n::text("repo-core-0543"), then.as_str()), (crate::i18n::text("repo-core-0544"), now.as_str())])
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
            crate::i18n::text("repo-core-0546")
        } else {
            crate::i18n::text("repo-core-0547")
        };
        if ui
            .small_button(label)
            .on_hover_text(
                crate::i18n::text("repo-core-0548"),
            )
            .clicked()
        {
            draft.open = true;
        }
        if !draft.is_empty() {
            let note = draft.note.trim();
            let kept = if note.is_empty() {
                crate::i18n::message("repo-core-0549", &[("test", format!("{}", one_line(&draft.test)).into())])
            } else {
                crate::i18n::message("repo-core-0550", &[("note", format!("{}", note).into())])
            };
            ui.label(RichText::new(kept).color(t.text_faint).small());
        }
        return;
    }
    egui::Grid::new(("decision-draft", k))
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new(crate::i18n::text("repo-core-0552")).color(t.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.note)
                    .hint_text(crate::i18n::text("repo-core-0553"))
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
            ui.label(RichText::new(crate::i18n::text("repo-core-0554")).color(t.text_dim));
            ui.add(
                egui::TextEdit::singleline(&mut draft.test)
                    .hint_text(crate::i18n::text("repo-core-0555"))
                    .font(theme::mono(12.0))
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0556"))
                .color(t.text_faint)
                .small(),
        );
        if ui.small_button(crate::i18n::text("repo-core-0557")).clicked() {
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
    ui.checkbox(&mut memo.run_tests, crate::i18n::text("repo-core-0558"))
        .on_hover_ui(|ui| {
            let mut tests: Vec<String> = drafts
                .iter()
                .map(|d| one_line(&d.test))
                .filter(|c| !c.is_empty())
                .collect();
            tests.sort();
            tests.dedup();
            ui.label(crate::i18n::message("repo-core-0559", &[("n", format!("{}", tests.join("\n")).into())]));
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
