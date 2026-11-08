use super::{Act, Data, State};
use crate::repo::Action;

pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("State publication receipts");
    ui.label("Exact native transaction receipts distinguish committed metadata from an uncertain acknowledgement. This inspection never rolls back files, removes locks or retries publication.");
    ui.label("Recovery directories retain original/candidate files and full access metadata. Inspect these entries and index.lock before any manual recovery. Journals are bounded to 256 operations; archive resolved journal refs explicitly after inspecting their recovery entries.");
    if ui.button("Reload publication receipts").clicked() {
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
        ui.label("No recorded state publications in this worktree.");
    }
    egui::ScrollArea::both().max_height(520.0).show(ui, |ui| {
        for report in reports {
            ui.separator();
            ui.strong(if report.committed { "Committed: exact receipt" } else { "Pending: retain and inspect" });
            ui.monospace(&report.intent.operation);
            ui.label(&report.detail);
            ui.monospace(format!("State: {}\nPrevious: {}\nProposed: {}\nReceipt: {}\nSource branch: {}\nExpected HEAD: {}\nRecovery entries: {}", report.intent.state_ref, report.intent.previous, report.intent.proposed, report.intent.receipt_ref, report.intent.branch.as_deref().unwrap_or("detached"), report.intent.expected_head.as_deref().unwrap_or("unborn"), report.intent.evidence));
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
