//! The selected commit or stash: its message (edited in place), author, hashes and files.

mod reword;
use reword::*;

mod message;
use message::*;

use egui::{Key, RichText, Sense, Ui, vec2};

use super::files_header_with;
use crate::a11y::Named;
use crate::ci::Status;
use crate::git::{CommitDetail, Stash};
use crate::icons;
use crate::repo::remerge::Mode;
use crate::repo::{
    Action, DiffKey, Refresh, RepoTab, Reword, branch_tree, ci_status, sidebar, widgets,
};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn commit_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let stash = self.stash_view.and_then(|i| self.stashes.get(i)).cloned();
        let title = if stash.is_some() {
            crate::i18n::text("repo-core-0862")
        } else {
            crate::i18n::text("repo-core-0056")
        };
        widgets::panel_header(ui, title, |ui| {
            if let Some(stash) = &stash {
                let more =
                    icons::small_button(ui, "chevron-down", crate::i18n::text("repo-core-0863"));
                egui::Popup::menu(&more)
                    .style(crate::theme::menu_style)
                    .show(|ui| {
                        sidebar::stash_menu(ui, acts, stash, idle);
                    });
            }
            branch_tree::files_toggle(ui);
        });
        if stash.is_none() && self.detail_hash.is_some() {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.prompt_session.open,
                    false,
                    crate::i18n::text("repo-core-0056"),
                );
                ui.selectable_value(
                    &mut self.prompt_session.open,
                    true,
                    crate::i18n::text("repo-core-0864"),
                );
            });
            if self.prompt_session.open {
                self.prompt_session_panel(ui, acts);
                return;
            }
        }
        if let Some(stash) = &stash {
            stash_banner(ui, acts, stash, idle);
        }
        self.want_all_files(ui.ctx());
        self.want_remerge_shown(ui.ctx());
        if let Some(err) = &self.detail_error {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.colored_label(
                    t.danger,
                    crate::git::display_error(&crate::forge::inbox::display_error(err)),
                );
            });
            return;
        }
        // Until git's read is in, what the graph already has of the commit (its message,
        // author, date, parents): shown at once, only its files wait.
        let provisional;
        let detail = match &self.detail {
            Some(detail) => detail,
            None => match self.provisional_detail() {
                Some(read_so_far) => {
                    provisional = read_so_far;
                    &provisional
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        if self.selected.is_some() || self.detail_hash.is_some() {
                            ui.add(crate::spinner::Spinner::new());
                        } else {
                            ui.label(
                                RichText::new(crate::i18n::text("repo-core-0865"))
                                    .color(t.text_dim),
                            );
                        }
                    });
                    return;
                }
            },
        };
        // Files still on their way (not read ahead either).
        let reading = self.detail.is_none() && self.files_cache.get(&detail.hash).is_none();

        self.review_all_button(
            ui,
            crate::repo::review_workflow::ReviewSource::Commit(detail.hash.clone()),
            acts,
        );
        // On a forge, the commit's builds: how they went, if that's known.
        let builds = (self.ci_project().is_some() && self.stash_view.is_none())
            .then(|| self.ci.by_sha.get(&detail.hash));
        // Taken out while the pane borrows `self`; put back after it.
        let mut reword = self.reword.take();
        let mut extras = std::mem::take(&mut self.extras);
        let mut extras_cmds = Vec::new();
        // How tall the sections are: they don't take room from the message (they scroll).
        let mut extras_h = 0.0;
        let remotes: Vec<String> = self.remotes.iter().map(|r| r.name.clone()).collect();
        let head = &self.refs.head;
        // A commit of the checked-out branch: double-click its message to edit it (the root
        // commit has no parent to rebase from).
        let editable = idle
            && detail.in_head
            && (detail.hash == *head || !detail.merges_after)
            && (detail.hash == *head || !detail.parents.is_empty());
        // How tall everything below the message was last frame: the message gets what's left.
        let view_h = ui.available_height();
        let rest_id = ui.id().with("message_rest");
        let rest: f32 = ui.data(|d| d.get_temp(rest_id)).unwrap_or(0.0);
        let issue_rules = super::compose::issue_links::rules_for(
            &super::compose::store::Store::issue_rules(ui.ctx()),
            self.hosting.as_ref(),
        );
        egui::ScrollArea::vertical()
            .id_salt("detail_scroll")
            .auto_shrink(false)
            .show(ui, |ui| {
                let top = ui.cursor().top();
                let mut message_end = top;
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        let room = view_h - (ui.cursor().top() - top) - rest;
                        message_section(ui, acts, detail, &mut reword, editable, room);
                        message_end = ui.cursor().top();
                        super::compose::issue_links::links_row(ui, &detail.message, &issue_rules);
                        ui.add_space(12.0);
                        author_line(ui, detail);
                        self.dco.commit_note(ui, detail);
                        ui.add_space(10.0);
                        meta_grid(ui, acts, detail, builds);
                        if stash.is_none() {
                            let before = ui.cursor().top();
                            extras.ui(ui, &detail.hash, &remotes, idle, &mut extras_cmds);
                            extras_h = ui.cursor().top() - before;
                        }
                    });

                if reading {
                    // Only the files are still on their way.
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
                } else {
                    self.commit_files_section(ui, acts, detail);
                }
                let below = ui.cursor().top() - message_end - extras_h;
                if (below - rest).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(rest_id, below));
                    ui.ctx().request_discard("commit message room");
                }
            });
        self.reword = reword;
        self.extras = extras;
        self.extras_cmds(extras_cmds, acts, ui.ctx());
        if stash.is_none() {
            self.extras_tick(ui.ctx());
        }
    }

    /// The commit's files: as merged again, all of the tree's, or those it changed.
    fn commit_files_section(&self, ui: &mut Ui, acts: &mut Vec<Action>, detail: &CommitDetail) {
        let remerge = self.remerge_mode(detail);
        let resolved = matches!(remerge, Mode::Checking | Mode::On);
        if !resolved {
            let note = (detail.parents.len() > 1).then_some(crate::i18n::text("repo-core-0868"));
            files_header_with(ui, &detail.files, note, |ui| {
                self.all_files_toggle(ui, acts)
            });
        }
        self.remerge_toggle(ui, acts, &remerge);
        ui.spacing_mut().item_spacing.y = 0.0;
        if resolved {
            self.remerge_files(ui, acts, detail);
        } else if self.all_files.on {
            self.all_files_list(ui, acts, detail);
        } else {
            self.file_list(
                ui,
                acts,
                "commit",
                &detail.files,
                Some(&detail.hash),
                |path| DiffKey::Commit {
                    hash: detail.hash.clone(),
                    path: path.to_owned(),
                },
            );
        }
    }

    /// The selected commit as the graph read it, for the detail pane while git's read of it is
    /// still on its way: not editable (whether HEAD contains it isn't known yet); its files if they
    /// were read ahead (see `prefetch_files`).
    fn provisional_detail(&self) -> Option<CommitDetail> {
        if self.stash_view.is_some() {
            return None;
        }
        let hash = self.detail_hash.as_deref()?;
        let commit = self
            .selected
            .and_then(|row| self.commits.get(row))
            .filter(|c| c.hash == hash)?;
        let message = if commit.body.is_empty() {
            commit.summary.clone()
        } else {
            format!("{}\n\n{}", commit.summary, commit.body)
        };
        Some(CommitDetail {
            hash: commit.hash.clone(),
            parents: commit.parents.clone(),
            author: commit.author.clone(),
            email: commit.email.clone(),
            author_date: commit.date.clone(),
            committer: commit.author.clone(),
            commit_date: commit.date.clone(),
            message,
            files: self
                .files_cache
                .get(hash)
                .map(|files| files.as_ref().clone())
                .unwrap_or_default(),
            in_head: false,
            merges_after: false,
        })
    }
}

/// The stash shown, with Pop and Apply.
fn stash_banner(ui: &mut Ui, acts: &mut Vec<Action>, stash: &Stash, idle: bool) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.accent.gamma_multiply(0.10))
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, "archive", 15.0, t.accent);
                ui.label(RichText::new(&stash.name).font(theme::medium(12.5)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (label, op) in [
                        ("Pop", sidebar::StashOp::Pop),
                        ("Apply", sidebar::StashOp::Apply),
                    ] {
                        if ui
                            .add_enabled(
                                idle,
                                egui::Button::new(crate::repo::messages::display_operation(label)),
                            )
                            .clicked()
                        {
                            acts.push(Action::Ops {
                                label: format!("{label} {}", stash.name),
                                ops: sidebar::stash_ops(stash, op),
                                refresh: Refresh::All,
                            });
                        }
                    }
                });
            });
        });
}

/// The message of `detail`, or its editor while it is being reworded. `editable`: whether a
/// double-click starts that; `room`: how tall the message may be.
fn meta_grid(
    ui: &mut Ui,
    acts: &mut Vec<Action>,
    detail: &CommitDetail,
    builds: Option<Option<&Status>>,
) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 3.0);
        // Each hash is one compact action; parents remain individually navigable.
        if ui
            .small_button(
                RichText::new(detail.hash.get(..12).unwrap_or(&detail.hash))
                    .font(theme::mono(11.0)),
            )
            .named(crate::i18n::text("repo-core-0880"))
            .on_hover_text(crate::i18n::message(
                "dev-integration-ui-copy-hash",
                &[("hash", detail.hash.clone().into())],
            ))
            .clicked()
        {
            ui.ctx().copy_text(detail.hash.clone());
        }
        for parent in &detail.parents {
            if ui
                .link(
                    RichText::new(format!("← {}", parent.get(..8).unwrap_or(parent)))
                        .font(theme::mono(11.0)),
                )
                .on_hover_text(crate::i18n::message(
                    "dev-integration-ui-select-parent",
                    &[("parent", parent.to_string().into())],
                ))
                .clicked()
            {
                acts.push(Action::SelectHash(parent.clone()));
            }
        }
        if let Some(status) = builds {
            let tally = status
                .map(|s| {
                    let label = crate::ci::tally_label(&s.checks);
                    if label.is_empty() {
                        ci_status::state_word(s.state).to_owned()
                    } else {
                        label
                    }
                })
                .unwrap_or_else(|| crate::i18n::text("repo-core-0883").to_owned());
            let b = icons::small_button(
                ui,
                "flask-conical",
                &crate::i18n::message("dev-integration-ui-show-builds", &[("tally", tally.into())]),
            );
            if let Some(status) = status {
                ci_status::paint_dot(
                    ui.painter(),
                    &t,
                    b.rect.right_top() + vec2(-2.0, 3.0),
                    2.5,
                    status.state,
                );
            }
            if b.clicked() {
                acts.push(Action::OpenCi {
                    sha: detail.hash.clone(),
                    run: None,
                });
            }
        }
    });
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
