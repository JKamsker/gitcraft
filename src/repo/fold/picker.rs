//! Fold availability and target selection UI.

use super::*;

impl RepoTab {
    /// Why nothing can be folded now, whatever the commit, as the last read saw it.
    pub(super) fn fold_blocked(&self) -> Option<String> {
        if self.refs.head.is_empty() {
            return Some(crate::i18n::text("repo-core-4112").to_owned());
        }
        plan::blocked(
            self.work.staged.len(),
            self.refs.current.as_deref(),
            self.work.in_progress,
        )
    }

    /// Why the staged changes can't go into `hash` now.
    pub(super) fn fold_refusal(&self, line: &[&Commit], hash: &str) -> Option<String> {
        self.fold_blocked()
            .or_else(|| plan::refusal(line, hash).map(str::to_owned))
    }

    /// _Fold staged changes into this commit_ in a commit's menu, disabled with the reason.
    pub(in crate::repo) fn fold_item(
        &self,
        ui: &mut Ui,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let line = first_parents(&self.history, &self.history_index, &self.refs.head);
        let refused = self.fold_refusal(&line, hash);
        let button = ui
            .add_enabled(
                idle && refused.is_none(),
                egui::Button::new(crate::i18n::text("repo-core-4113")),
            )
            .on_hover_text(crate::i18n::text("repo-core-4114"));
        let button = match &refused {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        if button.clicked() {
            acts.push(Action::FoldInto(hash.to_owned()));
        }
    }

    /// _Fold…_ beside _Amend last commit_: the latest commits of the branch to fold the staged
    /// changes into. `amending`: that box is ticked, which the fold would not follow.
    pub(in crate::repo) fn fold_picker(&self, ui: &mut Ui, amending: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let blocked = self
            .fold_blocked()
            .or_else(|| amending.then(|| crate::i18n::text("repo-core-4115").to_owned()));
        let button = ui
            .add_enabled(idle && blocked.is_none(), |ui: &mut Ui| {
                crate::icons::small_button(
                    ui,
                    "git-merge",
                    crate::i18n::text("dev-baseline-fold-staged-label"),
                )
            })
            .on_hover_text(crate::i18n::text("repo-core-4117"));
        let button = match &blocked {
            Some(why) => button.on_disabled_hover_text(why),
            None => button.on_disabled_hover_text("Wait for the operation running to end"),
        };
        egui::Popup::menu(&button)
            .style(crate::theme::menu_style)
            .show(|ui| {
                // Walked only while the picker is open.
                let line = first_parents(&self.history, &self.history_index, &self.refs.head);
                ui.set_max_width(360.0);
                ui.label(
                    RichText::new(crate::i18n::text("repo-core-4118"))
                        .color(t.text_faint)
                        .small(),
                );
                for commit in line.iter().take(PICKER_COMMITS) {
                    let refused = self.fold_refusal(&line, &commit.hash);
                    let text = format!("{}  {}", short(&commit.hash), commit.summary);
                    let item =
                        ui.add_enabled(refused.is_none(), egui::Button::new(text).truncate());
                    let item = match &refused {
                        Some(why) => item.on_disabled_hover_text(why),
                        None => item,
                    };
                    if item.clicked() {
                        acts.push(Action::FoldInto(commit.hash.clone()));
                    }
                }
                if line.len() > PICKER_COMMITS {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-core-4119"))
                            .color(t.text_faint)
                            .small(),
                    );
                }
            });
    }
}
