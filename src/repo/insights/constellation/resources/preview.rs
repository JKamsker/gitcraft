//! Exact alternative scopes, never an aggregate reclaim estimate or an execution action.
use crate::git::worktree_resources::{Inventory, Operation, previews};
use crate::i18n::text;
pub(super) fn show(ui: &mut egui::Ui, found: &Inventory) {
    ui.label(text("resources-preview-note"));
    if crate::git::now().saturating_sub(found.collected) > 30 {
        ui.label(text("resources-stale"));
        return;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(1));
    for unknown in &found.uncertain {
        ui.label(format!(
            "{}: {}",
            text("resources-protected-data"),
            found.root.join(unknown).display()
        ));
    }
    if found.uncertain_truncated {
        ui.label(text("resources-detail-truncated"));
    }
    for preview in previews(found) {
        ui.strong(text(match preview.operation {
            Operation::BuildOutputs => "resources-build",
            Operation::RetireToRecovery => "resources-retire",
        }));
        for root in &preview.roots {
            ui.monospace(root.to_string_lossy());
        }
        ui.label(&preview.relative_scope);
        let bytes = |value: Option<u64>| {
            value
                .map(crate::insights::constellation::read::human_bytes)
                .unwrap_or_else(|| text("resources-unavailable").into())
        };
        ui.label(format!(
            "{}: {}",
            text("resources-logical"),
            bytes(preview.logical_bytes)
        ));
        ui.label(format!(
            "{}: {}",
            text("resources-reclaim"),
            bytes(preview.reclaim_now)
        ));
        if preview.operation == Operation::RetireToRecovery {
            ui.label(format!(
                "{}: {}",
                text("resources-purge"),
                bytes(preview.potential_purge)
            ));
        }
        ui.label(format!("{} · {}", preview.observed, preview.fingerprint));
        for blocker in &preview.blockers {
            ui.label(format!("⚠ {}", crate::i18n::message(blocker, &[])));
        }
    }
}
