use super::*;

impl RepoTab {
    /// While an operation runs, what it is doing and _Cancel_; else the last operation's
    /// outcome (right to left).
    pub(in crate::repo) fn op_status(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        // While one runs, only a problem (one that couldn't wait for it) is worth showing.
        if let Some(notice) = &self.status
            && (self.busy_op.is_none() || notice.1)
        {
            let (msg, is_err) = notice.raw();
            let color = if *is_err { t.danger } else { t.text_dim };
            let projected = notice.projected();
            let first_line = projected
                .as_deref()
                .map(headline)
                .unwrap_or_else(|| crate::repo::messages::display_status(&headline(msg)));
            let entry = self.status_entry();
            let mut label =
                egui::Label::new(egui::RichText::new(first_line).color(color)).truncate();
            if entry.is_some() {
                label = label.sense(egui::Sense::click());
            }
            let shown = ui.add(label).on_hover_ui(|ui| {
                ui.label(
                    projected
                        .clone()
                        .unwrap_or_else(|| crate::repo::messages::display_status(msg)),
                );
                if entry.is_some() {
                    ui.label(
                        egui::RichText::new(crate::i18n::text("repo-core-3687")).color(t.text_dim),
                    );
                }
            });
            if let Some(id) = entry
                && shown.clicked()
            {
                acts.push(Action::ShowActivity(id));
            }
        }
        let Some(op) = &self.busy_op else {
            if self.loading {
                ui.add(crate::spinner::Spinner::new());
            }
            return;
        };
        if self.can_cancel()
            && ui
                .button(crate::i18n::text("repo-core-0443"))
                .on_hover_text(if self.waiting.is_empty() {
                    crate::i18n::message(
                        "repo-core-3688",
                        &[("op", crate::repo::messages::display_operation(op).into())],
                    )
                } else {
                    crate::i18n::message(
                        "repo-core-3689",
                        &[("op", crate::repo::messages::display_operation(op).into())],
                    )
                })
                .clicked()
        {
            acts.push(Action::CancelOp);
        }
        let op = crate::repo::messages::display_operation(op);
        let progress = self.op_progress();
        let doing = match &progress {
            _ if self.was_cancelled() => crate::i18n::message(
                "repo-core-3690",
                &[("op", crate::repo::messages::display_operation(&op).into())],
            ),
            Some(p) => format!("{op}… {}", p.label()),
            None => format!("{op}…"),
        };
        let label = ui.label(egui::RichText::new(doing).color(t.text_dim));
        if let Some(p) = &progress
            && !p.detail.is_empty()
        {
            label.on_hover_text(format!("{}: {}", p.phase, p.detail));
        }
        ui.add(crate::spinner::Spinner::new());
        if !self.waiting.is_empty() {
            let names: Vec<String> = self
                .waiting
                .iter()
                .map(|w| crate::repo::messages::display_operation(w.label()))
                .collect();
            widgets::chip(
                ui,
                &crate::i18n::message(
                    "repo-core-3693",
                    &[("names", names.join(", ").to_string().into())],
                ),
                t.text_dim,
            )
            .on_hover_text(crate::i18n::message(
                "repo-core-3694",
                &[("op", crate::repo::messages::display_operation(&op).into())],
            ));
        }
    }

    /// Why a button that changes the repository waits: the operation running.
    pub(in crate::repo) fn waits_text(&self) -> Option<String> {
        self.busy_op.as_ref().map(|op| {
            crate::i18n::message(
                "repo-core-3695",
                &[("op", crate::repo::messages::display_operation(op).into())],
            )
        })
    }
}
