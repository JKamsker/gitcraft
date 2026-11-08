//! Conversation discussions without a file coordinate still have native reply/resolution controls.
use super::{PrAct, PrDraft, pr_acts};
use crate::{pr_threads::PrExtras, repo::Action};
pub(super) fn show(
    ui: &mut egui::Ui,
    draft: &mut PrDraft,
    extras: &PrExtras,
    acts: &mut Vec<Action>,
) {
    if !extras.native_policy {
        return;
    }
    for thread in extras
        .threads
        .iter()
        .filter(|thread| thread.path.is_empty())
    {
        ui.collapsing(
            format!(
                "Discussion · {}{}",
                thread.comments.first().map_or("", |c| c.author.as_str()),
                if thread.resolved { " · resolved" } else { "" }
            ),
            |ui| {
                for comment in &thread.comments {
                    ui.label(egui::RichText::new(&comment.author).strong());
                    ui.add(egui::Label::new(&comment.body).selectable(true));
                }
                if thread.can_resolve
                    && ui
                        .add_enabled(!draft.posting, egui::Button::new("Resolve discussion"))
                        .clicked()
                {
                    pr_acts(acts, PrAct::SetResolved(thread.id.clone(), true));
                }
                if thread.can_unresolve
                    && ui
                        .add_enabled(!draft.posting, egui::Button::new("Reopen discussion"))
                        .clicked()
                {
                    pr_acts(acts, PrAct::SetResolved(thread.id.clone(), false));
                }
                if thread.can_reply {
                    ui.add_enabled(
                        !draft.posting,
                        egui::TextEdit::multiline(&mut draft.comment)
                            .hint_text("Reply to this discussion")
                            .desired_rows(2),
                    );
                    if ui
                        .add_enabled(
                            !draft.posting && !draft.comment.trim().is_empty(),
                            egui::Button::new("Reply to discussion"),
                        )
                        .clicked()
                    {
                        pr_acts(acts, PrAct::ReplyConversation(thread.id.clone()));
                    }
                }
            },
        );
    }
}
