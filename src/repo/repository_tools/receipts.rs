use super::{Act, Data, State};
use crate::repo::Action;

pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-8f8df690",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-625c67be",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-f41d4ab9",
    ));
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-7eb9930d",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::StateReceipts));
    }
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let reports = match result {
        Ok(Data::StateReceipts(reports)) => reports,
        Ok(_) => return,
        Err(error) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, error);
            return;
        }
    };
    if reports.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-77c3b92d",
        ));
    }
    egui::ScrollArea::both().max_height(520.0).show(ui, |ui| {
        for report in reports {
            ui.separator();
            ui.strong(if report.committed {
                crate::i18n::text("repo-workflows-repository_tools-show-ef0a338b")
            } else {
                crate::i18n::text("repo-workflows-repository_tools-show-9d8f0223")
            });
            ui.monospace(&report.intent.operation);
            ui.label(&report.detail);
            ui.monospace(crate::i18n::message(
                "repo-workflows-repository_tools-show-ea303dfe",
                &[
                    (
                        "report-intent-state-ref-1",
                        (report.intent.state_ref).clone().into(),
                    ),
                    (
                        "report-intent-previous-2",
                        (report.intent.previous).clone().into(),
                    ),
                    (
                        "report-intent-proposed-3",
                        (report.intent.proposed).clone().into(),
                    ),
                    (
                        "report-intent-receipt-ref-4",
                        (report.intent.receipt_ref).clone().into(),
                    ),
                    (
                        "report-intent-branch-as-deref-un-5",
                        (report
                            .intent
                            .branch
                            .as_deref()
                            .unwrap_or_else(|| crate::i18n::text("repo-workflows-paint-detached")))
                        .clone()
                        .into(),
                    ),
                    (
                        "report-intent-expected-head-as-d-6",
                        (report
                            .intent
                            .expected_head
                            .as_deref()
                            .unwrap_or_else(|| crate::i18n::text("repo-workflows-paint-unborn")))
                        .clone()
                        .into(),
                    ),
                    (
                        "report-intent-evidence-7",
                        (report.intent.evidence).clone().into(),
                    ),
                ],
            ));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_publication_panel_displays_pending_exact_context_without_mutation_action() {
        use crate::git::drafts::publication::{Intent, Report};
        let mut state = State {
            id: 1,
            query: String::new(),
            filter: String::new(),
            result: Some(Ok(Data::StateReceipts(vec![Report {
                intent: Intent {
                    operation: "lists-owned-operation".into(),
                    state_ref: "refs/gitcraft/lists/state".into(),
                    previous: "a".repeat(40),
                    proposed: "b".repeat(40),
                    branch: Some("refs/heads/topic".into()),
                    expected_head: Some("c".repeat(40)),
                    evidence: "/owned/recovery/original".into(),
                    receipt_ref: "refs/gitcraft/receipt/operation".into(),
                },
                committed: false,
                detail: "No automatic rollback or retry".into(),
            }]))),
            tool: super::super::Tool::StateReceipts,
            acknowledged: false,
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
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.textures_delta.clear();
        for expected in [
            "Pending: retain and inspect",
            "lists-owned-operation",
            "refs/heads/topic",
            "/owned/recovery/original",
            "No automatic rollback or retry",
        ] {
            assert!(text.contains(expected), "Missing {expected}: {text}");
        }
        assert!(acts.is_empty());
    }
}
