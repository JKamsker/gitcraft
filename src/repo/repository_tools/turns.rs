use super::{Act, Data, State};
use crate::{
    git::drafts::turn as logic,
    repo::{Action, Dialog, RepoTab},
};
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::MarkTurn(review, selected) => {
            let repo = tab.path.clone();
            let expected = repo.clone();
            let request = match &tab.dialog {
                Some(Dialog::RepositoryTools(s)) => s.id,
                _ => return None,
            };
            let marked = selected.clone();
            tab.then_or(
                ctx,
                true,
                Err,
                move || {
                    logic::mark_reviewed(&repo, &review, &selected, &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })?;
                    Ok(())
                },
                move |tab, result: Result<(), String>, _| {
                    if tab.path == expected
                        && let Some(Dialog::RepositoryTools(s)) = &mut tab.dialog
                        && s.id == request
                    {
                        match result {
                            Ok(()) => {
                                if let Some(Ok(Data::Turn(r))) = &mut s.result {
                                    for i in &marked {
                                        if let Some(h) = r.hunks.get_mut(*i) {
                                            h.reviewed = true;
                                        }
                                    }
                                }
                            }
                            Err(e) => tab.status = Some((e, true)),
                        }
                    }
                },
            );
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
                    format!(
                        "Turn reverted. Staged contents retained. Undo from Earlier versions: {}",
                        id.reference
                    )
                })
            })
        }
        other => return Some(other),
    }
    None
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Review saved agent turn");
    ui.label("Reviewed identical hunks stay hidden across this session's turns. Changed hunks reappear. Marks stay local. Reverting checks the inverse against current files and retains later non-overlapping changes and staged contents.");
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
            ui.checkbox(&mut state.compatible, "Show reviewed unchanged hunks");
            let unreviewed = review
                .hunks
                .iter()
                .enumerate()
                .filter(|(_, h)| !h.reviewed)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            ui.label(format!(
                "{} hunks · {} hidden reviewed",
                review.hunks.len(),
                review.hunks.len() - unreviewed.len()
            ));
            if ui
                .add_enabled(
                    !unreviewed.is_empty(),
                    egui::Button::new("Mark visible unreviewed hunks reviewed"),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::MarkTurn(
                    Box::new(review.clone()),
                    unreviewed,
                )));
            }
            if ui.button("Preview reverting only this turn…").clicked() {
                acts.push(Action::RepositoryTools(Act::PreviewInverse(Box::new(
                    review.clone(),
                ))));
            }
            egui::ScrollArea::vertical().max_height(420.0).show(ui,|ui|{
                for (i,h) in review.hunks.iter().enumerate().filter(|(_,h)|state.compatible||!h.reviewed){
                    ui.horizontal(|ui|{ui.label(if h.reviewed{"Reviewed"}else{"Unreviewed"});if !h.reviewed && ui.button("Mark this hunk reviewed").clicked(){acts.push(Action::RepositoryTools(Act::MarkTurn(Box::new(review.clone()),vec![i])));}});
                    ui.monospace(h.patch.chars().take(32000).collect::<String>());
                    if h.patch.chars().count()>32000{ui.label("Display truncated at 32000 characters; open the turn comparison for more.");}
                }
            });
        }
        Data::Inverse(p) => {
            ui.label(format!(
                "Inverse of {} → {} against current working files; {} affected paths",
                p.parent,
                p.turn,
                p.paths.len()
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
                ui.label("Preview truncated; compare this turn before accepting.");
            }
            ui.checkbox(&mut state.acknowledged,"Apply this inverse; stop on overlapping or stale edits. A safety draft will preserve the current files.");
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new("Revert only this turn"),
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
