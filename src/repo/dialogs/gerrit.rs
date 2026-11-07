//! Review submission selects a fetched target and previews the exact object being pushed.

use super::{Outcome, buttons, title};
use crate::repo::{Action, Refresh};
use crate::theme::Tokens;

pub(super) fn review(
    ui: &mut egui::Ui,
    remote: &str,
    head: &str,
    targets: &[(String, String)],
    target: &mut String,
    topic: &mut String,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        "Push for review",
        &format!(
            "Submit {} to Gerrit on {remote}.",
            &head[..head.len().min(8)]
        ),
        &t,
    );
    ui.label("Target branch");
    egui::ComboBox::from_id_salt("gerrit-target")
        .width(ui.available_width())
        .selected_text(target.as_str())
        .show_ui(ui, |ui| {
            for (name, _) in targets {
                ui.selectable_value(target, name.clone(), name);
            }
        });
    ui.add_space(6.0);
    ui.label("Topic (optional)");
    ui.add(
        egui::TextEdit::singleline(topic)
            .desired_width(f32::INFINITY)
            .hint_text("Group related changes"),
    );
    let base = targets
        .iter()
        .find(|(name, _)| name == target)
        .map(|(_, sha)| sha.as_str())
        .unwrap_or_default();
    let plan = crate::gerrit::review_ops(remote, target, topic, head, base);
    ui.add_space(6.0);
    if targets.is_empty() {
        ui.label(egui::RichText::new("Fetch the remote's target branches first.").color(t.warning));
    } else {
        ui.label(format!("{remote}: {head} → refs/for/{target}"));
        ui.label(egui::RichText::new("Checks every commit above the fetched target for a unique Change-Id. Amend a change, keep its Change-Id, then push here again to upload a new patch set.").color(t.text_dim));
    }
    if !crate::gerrit::valid_topic(topic) {
        ui.label(
            egui::RichText::new("The topic cannot contain control characters.").color(t.warning),
        );
    }
    ui.add_space(10.0);
    // A deliberate click is needed; Enter in the topic field must not send a review.
    buttons(ui, "Push for review", plan.is_ok(), false, || Action::Ops {
        label: "Push for review".to_owned(),
        ops: plan.unwrap_or_default(),
        refresh: Refresh::All,
    })
}
