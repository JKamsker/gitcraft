//! Timeline entry details, jump controls and changed refs.

use super::*;

impl RepoTab {
    /// The entry picked: what it changed, and the ways back to it.
    pub(in crate::repo::timeline::view) fn entry_detail(
        &self,
        ui: &mut Ui,
        id: Option<u64>,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let confirm = self.timeline_view.as_ref().and_then(|v| v.confirm);
        let Some(entry) = id.and_then(|id| self.timeline.get(id)) else {
            heading(
                ui,
                crate::i18n::text("repo-workflows-timeline-entry-detail-6fa556f4"),
            );
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-timeline-entry-detail-f1c36dc4",
                ))
                .color(t.text_dim),
            );
            ui.add_space(10.0);
            self.jump_controls(ui, None, idle, confirm, acts);
            return;
        };
        heading(ui, &crate::repo::messages::display_operation(&entry.label));
        let now = crate::git::now();
        let who = match entry.origin {
            Origin::Gitcraft => crate::i18n::text("repo-workflows-timeline-entry-detail-601272f1"),
            Origin::Outside => crate::i18n::text("repo-workflows-timeline-entry-detail-dfe997bf"),
        };
        ui.label(
            RichText::new(format!(
                "{who} · {}",
                crate::git::relative_time(entry.time, now)
            ))
            .size(12.0)
            .color(t.text_faint),
        );
        let state = if self.timeline.current == Some(entry.id) {
            (
                crate::i18n::text("repo-workflows-timeline-entry-detail-cc07e410"),
                t.success,
            )
        } else if self.timeline.is_done(entry.id) {
            (
                crate::i18n::text("repo-workflows-timeline-entry-detail-26583776"),
                t.text_dim,
            )
        } else {
            (
                crate::i18n::text("repo-workflows-timeline-entry-detail-03a933b9"),
                t.warning,
            )
        };
        ui.label(RichText::new(state.0).size(12.0).color(state.1));
        ui.add_space(10.0);
        self.jump_controls(ui, Some(entry.id), idle, confirm, acts);
        ui.add_space(14.0);
        if let Some(receipt) = &entry.receipt {
            crate::repo::timeline::receipt_view::receipt_section(ui, receipt);
            ui.add_space(14.0);
        }
        self.changed_refs(ui, entry, idle, acts);
        ui.add_space(14.0);
        self.remove_section(ui, entry, idle, acts);
        ui.add_space(14.0);
        changed_files(ui, entry);
    }

    /// Go here (and, when asked, what it would change and the button that does it).
    pub(in crate::repo::timeline::view) fn jump_controls(
        &self,
        ui: &mut Ui,
        id: Option<u64>,
        idle: bool,
        confirm: Option<Option<u64>>,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let here = self.timeline.current == id;
        if confirm != Some(id) {
            let button = ui
                .add_enabled(
                    idle && !here,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-timeline-jump-controls-26d159f6",
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-timeline-jump-controls-8020ed90",
                ))
                .on_disabled_hover_text(if here {
                    crate::i18n::text("repo-workflows-timeline-jump-controls-1dec3ad6")
                } else {
                    crate::i18n::text("repo-workflows-timeline-jump-controls-ecbb5934")
                });
            if button.clicked() {
                acts.push(Action::Timeline(TimelineAct::AskJump(id)));
            }
            return;
        }
        let Some((_, target)) = self.jump_target(id) else {
            return;
        };
        let now = self.ref_state();
        let lines = snapshot::describe(&now, &target);
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-jump-controls-c651b224",
                        ))
                        .color(t.text_dim),
                    );
                } else {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-jump-controls-3006b913",
                        ))
                        .color(t.text),
                    );
                    for line in &lines {
                        ui.label(
                            RichText::new(format!("•  {line}"))
                                .font(theme::mono(12.0))
                                .color(t.text_dim),
                        );
                    }
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-jump-controls-b968e7d2",
                        ))
                        .size(11.5)
                        .color(t.text_faint),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-workflows-timeline-jump-controls-26d159f6"),
                        idle,
                        90.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::Jump(id)));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-timeline-jump-controls-19766ed6",
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::CancelJump));
                    }
                });
            });
    }

    /// The branches the entry made, moved or deleted, each with a way back to its old tip.
    pub(in crate::repo::timeline::view) fn changed_refs(
        &self,
        ui: &mut Ui,
        entry: &Entry,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let changes = entry.changes();
        section(
            ui,
            crate::i18n::text("repo-workflows-timeline-changed-refs-2eee037c"),
        );
        if changes.is_empty() {
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-timeline-changed-refs-1b870022",
                ))
                .color(t.text_faint),
            );
            return;
        }
        for change in changes {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(&change.name)
                        .font(theme::semibold(12.5))
                        .color(t.text),
                );
                ui.label(
                    RichText::new(format!(
                        "{}  →  {}",
                        ref_value(&change.old),
                        ref_value(&change.new)
                    ))
                    .font(theme::mono(12.0))
                    .color(t.text_dim),
                );
            });
            let is_branch = change.name != "HEAD";
            if is_branch && !change.old.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    let back = crate::i18n::message(
                        "repo-workflows-timeline-changed-refs-92e3b324",
                        &[
                            ("change-name-1", (change.name).clone().into()),
                            ("short-change-old-2", (short(&change.old)).into()),
                        ],
                    );
                    if ui
                        .add_enabled(idle, egui::Button::new(back).small())
                        .on_hover_text(crate::i18n::message(
                            "repo-workflows-timeline-changed-refs-312c661d",
                            &[
                                ("change-name-1", (change.name).clone().into()),
                                ("short-change-old-2", (short(&change.old)).into()),
                            ],
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::PutBack {
                            branch: change.name.clone(),
                            commit: change.old.clone(),
                        }));
                    }
                    if ui
                        .add_enabled(
                            idle,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-timeline-changed-refs-a7884d9f",
                            ))
                            .small(),
                        )
                        .on_hover_text(crate::i18n::message(
                            "repo-workflows-timeline-changed-refs-e46853a8",
                            &[
                                (
                                    "self-free-branch-name-format-was-1",
                                    (self.free_branch_name(&format!(
                                        "{}-was-{}",
                                        change.name,
                                        short(&change.old)
                                    )))
                                    .clone()
                                    .into(),
                                ),
                                ("short-change-old-2", (short(&change.old)).into()),
                                ("change-name-3", (change.name).clone().into()),
                            ],
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::BranchAt {
                            branch: change.name.clone(),
                            commit: change.old.clone(),
                        }));
                    }
                    if ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-timeline-changed-refs-4ea9935c",
                        ))
                        .on_hover_text(change.old.as_str())
                        .clicked()
                    {
                        ui.ctx().copy_text(change.old.clone());
                    }
                });
            }
            ui.add_space(4.0);
        }
    }
}
