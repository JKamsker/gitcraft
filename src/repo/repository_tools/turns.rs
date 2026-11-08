use super::{Act, Data, State};
use crate::{
    git::drafts::turn as logic,
    repo::{Action, Dialog, RepoTab},
};
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::MarkTurn(review, selected) => {
            update_marks(tab, *review, Some(selected), ctx);
        }
        Act::FinishAggregate(review) => {
            update_marks(tab, *review, None, ctx);
        }
        Act::PreviewInverse(review) if review.aggregate.is_some() => {
            tab.status = Some((
                crate::i18n::text("repo-workflows-repository_tools-act-6dc9a943").into(),
                true,
            ));
        }
        Act::PreviewInverse(review) => {
            let repo = tab.path.clone();
            let expected = repo.clone();
            let request = match &tab.dialog {
                Some(Dialog::RepositoryTools(s)) => s.id,
                _ => return None,
            };
            tab.then_or(
                ctx,
                false,
                Err,
                move || {
                    logic::inverse_preview(&repo, &review.parent, &review.turn, &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })
                    .map(|v| Data::Inverse(Box::new(v)))
                },
                move |tab, result, _| {
                    if tab.path == expected
                        && let Some(Dialog::RepositoryTools(s)) = &mut tab.dialog
                        && s.id == request
                    {
                        s.result = Some(result);
                        s.acknowledged = false;
                    }
                },
            );
        }
        Act::RevertTurn(preview) => {
            tab.repository_tool_mutation("Revert one saved turn", false, ctx, move |r| {
                logic::inverse_apply(r.repo, &preview, &|| !r.control.is_cancelled()).map(|id| {
                    crate::i18n::message(
                        "repo-workflows-repository_tools-act-b4e12052",
                        &[
                            ("id-draft-reference-1", (id.draft.reference).clone().into()),
                            ("id-evidence-2", (id.evidence).clone().into()),
                        ],
                    )
                })
            })
        }
        other => return Some(other),
    }
    None
}
fn update_marks(
    tab: &mut RepoTab,
    review: logic::Review,
    selected: Option<Vec<usize>>,
    ctx: &egui::Context,
) {
    let repo = tab.path.clone();
    let expected = repo.clone();
    let expected_review = review.clone();
    let request = match &tab.dialog {
        Some(Dialog::RepositoryTools(state)) => state.id,
        _ => return,
    };
    tab.then_or(
        ctx,
        true,
        Err,
        move || {
            let active = || !crate::tasks::cancellation().is_cancelled();
            if let Some(selected) = selected {
                logic::mark_reviewed(&repo, &review, &selected, &active)
            } else {
                logic::finish_aggregate(&repo, &review, &active)
            }
        },
        move |tab, result, _| {
            if tab.path == expected
                && let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                && state.id == request
                && matches!(&state.result,Some(Ok(Data::Turn(current))) if current.same_snapshot(&expected_review))
            {
                match result {
                    Ok(review) => {
                        state.result = Some(Ok(Data::Turn(review)));
                        state.acknowledged = false;
                    }
                    Err(error) => tab.status = Some((error, true)),
                }
            }
        },
    );
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-0b22ea73",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-05519c89",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e.as_str());
            return;
        }
    };
    match data {
        Data::Turn(review) => {
            ui.checkbox(
                &mut state.compatible,
                crate::i18n::text("repo-workflows-repository_tools-show-71ba13e6"),
            );
            let unreviewed = review
                .hunks
                .iter()
                .enumerate()
                .filter(|(_, h)| !h.reviewed)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-5673c401",
                &[
                    ("review-hunks-len-1", (review.hunks.len()).clone().into()),
                    (
                        "review-hunks-len-unreviewed-len-2",
                        (review.hunks.len() - unreviewed.len()).clone().into(),
                    ),
                ],
            ));
            if ui
                .add_enabled(
                    !unreviewed.is_empty(),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-1f89e0a1",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::MarkTurn(
                    Box::new(review.clone()),
                    unreviewed,
                )));
            }
            if let Some(aggregate) = &review.aggregate {
                ui.strong(crate::i18n::text(
                    "repo-workflows-repository_tools-show-739ffe8f",
                ));
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-show-181a4047",
                    &[
                        ("aggregate-session-1", (aggregate.session).clone().into()),
                        (
                            "aggregate-saved-turns-2",
                            (aggregate.saved_turns).clone().into(),
                        ),
                        ("review-parent-3", (review.parent).clone().into()),
                        ("review-turn-4", (review.turn).clone().into()),
                    ],
                ));
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-show-1fbe68fb",
                    &[(
                        "aggregate-last-looked-as-deref-u-1",
                        (aggregate
                            .last_looked
                            .as_deref()
                            .unwrap_or(crate::i18n::text(
                                "repo-workflows-repository_tools-show-9e8617f1",
                            )))
                        .clone()
                        .into(),
                    )],
                ));
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-repository_tools-show-325b12c6",
                    ))
                    .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::AggregateTurn(
                        aggregate.session.clone(),
                        review.turn.clone(),
                    )));
                }
                if aggregate.last_looked.as_deref() != Some(review.turn.as_str()) {
                    ui.checkbox(
                        &mut state.acknowledged,
                        crate::i18n::text("repo-workflows-repository_tools-show-927164d1"),
                    );
                    if ui
                        .add_enabled(
                            state.acknowledged,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-repository_tools-show-05f3fddc",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::FinishAggregate(Box::new(
                            review.clone(),
                        ))));
                    }
                }
            }
            if review.aggregate.is_none()
                && ui
                    .button(crate::i18n::text(
                        "repo-workflows-repository_tools-show-3e9abe63",
                    ))
                    .clicked()
            {
                acts.push(Action::RepositoryTools(Act::PreviewInverse(Box::new(
                    review.clone(),
                ))));
            }
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    for (i, h) in review
                        .hunks
                        .iter()
                        .enumerate()
                        .filter(|(_, h)| state.compatible || !h.reviewed)
                    {
                        ui.horizontal(|ui| {
                            ui.label(if h.reviewed {
                                crate::i18n::text("repo-workflows-repository_tools-show-fad6057b")
                            } else {
                                crate::i18n::text("repo-workflows-repository_tools-show-f5265a26")
                            });
                            if !h.reviewed
                                && ui
                                    .button(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-deee0d61",
                                    ))
                                    .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::MarkTurn(
                                    Box::new(review.clone()),
                                    vec![i],
                                )));
                            }
                        });
                        ui.monospace(h.patch.chars().take(32000).collect::<String>());
                        if h.patch.chars().count() > 32000 {
                            ui.label(crate::i18n::text(
                                "repo-workflows-repository_tools-show-72456f4c",
                            ));
                        }
                    }
                });
        }
        Data::Inverse(p) => {
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-4f480ac9",
                &[
                    ("p-parent-1", (p.parent).clone().into()),
                    ("p-turn-2", (p.turn).clone().into()),
                    ("p-paths-len-3", (p.paths.len()).clone().into()),
                ],
            ));
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    ui.monospace(
                        String::from_utf8_lossy(&p.patch)
                            .chars()
                            .take(64000)
                            .collect::<String>(),
                    );
                });
            if p.patch.len() > 64000 {
                ui.label(crate::i18n::text(
                    "repo-workflows-repository_tools-show-c29574be",
                ));
            }
            ui.checkbox(
                &mut state.acknowledged,
                crate::i18n::text("repo-workflows-repository_tools-show-7b868cc6"),
            );
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-4734d197",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::RevertTurn(p.clone())));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turn_review_hides_reviewed_text_and_keeps_new_hunk_review_reachable() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let repo = dir.path();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            crate::git::run(Some(repo), &args)?;
        }
        let base = (1..=40).map(|i| format!("line {i}\n")).collect::<String>();
        std::fs::write(repo.join("file"), &base).map_err(|e| e.to_string())?;
        crate::git::run(Some(repo), &["add", "file"])?;
        crate::git::run(Some(repo), &["commit", "-qm", "base"])?;
        let parent = crate::git::run(Some(repo), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        std::fs::write(
            repo.join("file"),
            base.replace("line 3\n", "reviewed secret phrase\n")
                .replace("line 30\n", "new hunk phrase\n"),
        )
        .map_err(|e| e.to_string())?;
        crate::git::run(Some(repo), &["add", "file"])?;
        crate::git::run(Some(repo), &["commit", "-qm", "turn"])?;
        let turn = crate::git::run(Some(repo), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        let r = logic::review(repo, &parent, &turn, "session", &|| true)?;
        logic::mark_reviewed(repo, &r, &[0], &|| true)?;
        let r = logic::review(repo, &parent, &turn, "session", &|| true)?;
        let mut state = State {
            id: 1,
            query: String::new(),
            result: Some(Ok(Data::Turn(r))),
            filter: String::new(),
            tool: super::super::Tool::Turn,
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        };
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        let texts = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.iter().any(|s| s.contains("new hunk phrase")));
        assert!(!texts.iter().any(|s| s.contains("reviewed secret phrase")));
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Mark this hunk reviewed" => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .ok_or("No review button")?;
        out.textures_delta.clear();
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        out.textures_delta.clear();
        assert!(acts.iter().any(
            |a| matches!(a,Action::RepositoryTools(Act::MarkTurn(_,indices)) if indices==&vec![1])
        ));
        Ok(())
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;
    #[test]
    fn pure_aggregate_review_panel_names_its_endpoint_and_keeps_boundary_approval_explicit() {
        let mut state = State {
            id: 1,
            query: String::new(),
            result: Some(Ok(Data::Turn(logic::recorded_aggregate_fixture()))),
            filter: String::new(),
            tool: super::super::Tool::AggregateTurn,
            acknowledged: true,
            compatible: false,
            parent: String::new(),
            effective: true,
        };
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 1400.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let position = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == "Mark snapshot reviewed and save last-looked boundary" =>
            {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        });
        output.textures_delta.clear();
        assert!(texts.contains("saved turns only"));
        assert!(texts.contains("current unsaved files are excluded"));
        assert!(!texts.contains("Preview reverting only this turn"));
        let position = position.expect("explicit saved-endpoint approval action");
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
            |ui| show(ui, &mut state, &mut acts),
        );
        output.textures_delta.clear();
        assert!(acts.iter().any(|act|matches!(act,Action::RepositoryTools(Act::FinishAggregate(review)) if review.turn=="c".repeat(40))));
    }
}
