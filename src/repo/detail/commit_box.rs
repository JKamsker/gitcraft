//! The commit box under the working-tree changes: message, amend, identity.

use egui::{RichText, Ui};

use crate::icons;
use crate::keymap::{self, Command, Scope};
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn commit_box(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        // A rebase or patch stopped on conflicts: the button continues it.
        let continuing = self.work.in_progress.filter(|s| s.continues());
        let rebasing = continuing.is_some();
        let mut amend_started = false;
        // Taken out while the row is drawn, which reads the whole tab too (the fold picker).
        let mut amending = self.amend;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match continuing {
                    Some(s) => crate::i18n::message(
                        "repo-core-0892",
                        &[(
                            "to-uppercase",
                            format!("{}", s.label().to_uppercase()).into(),
                        )],
                    ),
                    None => crate::i18n::text("repo-core-0893").to_owned(),
                })
                .font(theme::semibold(10.5))
                .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let amend = ui
                    .add_enabled(
                        !rebasing && !self.refs.head.is_empty(),
                        egui::Checkbox::new(&mut amending, crate::i18n::text("repo-core-0894")),
                    )
                    .on_hover_text(crate::i18n::text("repo-core-0895"));
                // Over an empty box, or what the template and hook put there.
                amend_started = amend.changed()
                    && amending
                    && (self.commit_summary.trim().is_empty() || self.unedited_prefill());
                if !rebasing {
                    self.fold_picker(ui, amending, acts);
                }
                self.ai_message_button(ui, !rebasing);
                self.dco.toggle(ui, !rebasing, acts);
            });
        });
        self.amend = amending;
        if amend_started {
            // Start from the message being amended (read in the background for a repository
            // on another machine), unless one was typed meanwhile.
            let repo = self.path.clone();
            self.here_or_later(
                ui.ctx(),
                move || crate::git::head_message(&repo),
                |tab, msg, _| {
                    let Ok(msg) = msg else { return };
                    if tab.amend {
                        tab.take_message(&msg);
                    }
                },
            );
        }
        if self.amend {
            let pushed = self
                .refs
                .current
                .as_ref()
                .and_then(|c| self.refs.local.iter().find(|b| &b.name == c))
                .is_some_and(|b| !b.upstream.is_empty() && b.ahead_behind().0 == 0);
            if pushed {
                ui.label(
                    RichText::new(crate::i18n::text("repo-core-0896"))
                        .color(t.warning)
                        .small(),
                );
            }
        }
        self.compose_prefill(ui.ctx());
        self.compose_toolbar(ui, !rebasing);
        ui.add_space(2.0);
        // `--continue` commits with the message git kept: edits here would be lost.
        let (summary, body) = self.message_fields(ui, !rebasing);
        self.compose_notice(ui);
        self.ai_message_status(ui);
        if std::mem::take(&mut self.focus_commit_summary) && !rebasing {
            summary.request_focus();
        }
        if self
            .guide
            .take_focus_summary(ui.ctx().cumulative_frame_nr())
        {
            summary.request_focus();
            summary.scroll_to_me(None);
        }
        if rebasing {
            ui.label(
                RichText::new(crate::i18n::text("repo-core-0897"))
                    .color(t.text_faint)
                    .small(),
            );
        }
        let len = self.commit_summary.chars().count();
        if len > 72 {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-core-0898",
                    &[("len", (len).into())],
                ))
                .color(t.warning)
                .small(),
            );
        }
        if rebasing {
            // `--continue` takes no `--no-verify`: nothing ticked waits for the next commit.
            self.skip_hooks.commit = false;
        } else {
            ui.checkbox(
                &mut self.skip_hooks.commit,
                crate::i18n::text("repo-core-0899"),
            )
            .on_hover_text(crate::i18n::text("repo-core-0900"));
        }
        ui.add_space(4.0);
        let staged = self.work.staged.len();
        let restack = self.stacks.restack_here().is_some()
            && continuing == Some(crate::git::InProgress::Rebase);
        let label = if restack {
            crate::i18n::text("repo-core-0901").to_owned()
        } else if let Some(s) = continuing {
            crate::i18n::message(
                "repo-core-0352",
                &[(
                    "to-lowercase",
                    format!("{}", s.label().to_lowercase()).into(),
                )],
            )
        } else if self.amend {
            crate::i18n::text("repo-core-0894").to_owned()
        } else if staged > 0 {
            crate::i18n::message("repo-core-0902", &[("staged", (staged).into())])
        } else {
            crate::i18n::text("repo-core-0903").to_owned()
        };
        // An unchanged template is refused, as git refuses it.
        let untouched = !rebasing && self.unedited_template();
        let can = idle
            && (rebasing || !self.commit_summary.trim().is_empty())
            && !untouched
            && self.work.conflicted.is_empty();
        // Only from the message fields, so Ctrl+Enter in the terminal stays the shell's.
        let shortcut = (summary.has_focus() || body.has_focus())
            && keymap::pressed(ui.ctx(), Scope::Repository).contains(&Command::Commit);
        let resp = widgets::primary_button(ui, &label, can, ui.available_width());
        if (resp.clicked() || shortcut) && can {
            acts.push(Action::Commit);
        }
        if untouched {
            resp.on_hover_text(crate::i18n::text("repo-core-0904"));
        } else {
            resp.on_hover_text(keymap::label(ui.ctx(), Command::Commit));
        }

        ui.add_space(4.0);
        self.identity_row(ui, acts);
    }

    /// Who the commit will be by, and the saved identities to switch to.
    fn identity_row(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let settings = crate::settings::Settings::get(ui.ctx());
        let profile = settings.profile();
        // A repository on another machine is by that machine's git config: the profile's
        // author and signing are for git run here.
        let elsewhere = crate::remote::parse(&self.path).map(|target| target.host.label());
        ui.horizontal(|ui| {
            let id = &self.identity;
            // Where the name and the email each come from: the repository's own config, the
            // profile (here only), or the global config (that machine's, for one elsewhere).
            let source = |local: bool, value: &str, profile_value: &str| {
                let profile_value = profile_value.trim();
                if local {
                    crate::i18n::text("repo-core-0905").to_owned()
                } else if elsewhere.is_none() && !profile_value.is_empty() && value == profile_value
                {
                    crate::i18n::message(
                        "repo-core-0906",
                        &[("title", format!("{}", profile.display_title()).into())],
                    )
                } else if let Some(host) = &elsewhere {
                    crate::i18n::message("repo-core-0907", &[("host", format!("{}", host).into())])
                } else {
                    crate::i18n::text("repo-core-0908").to_owned()
                }
            };
            let who = if id.name.is_empty() {
                crate::i18n::text("repo-core-0909").to_owned()
            } else {
                let name_from = source(id.local, &id.name, &profile.author_name);
                let email_from = source(id.local_email, &id.email, &profile.author_email);
                let source = if name_from == email_from {
                    format!(" · {name_from}")
                } else {
                    crate::i18n::message(
                        "repo-core-0911",
                        &[
                            ("name-from", format!("{}", name_from).into()),
                            ("email-from", format!("{}", email_from).into()),
                        ],
                    )
                };
                crate::i18n::message(
                    "repo-core-0912",
                    &[
                        ("name", format!("{}", id.name).into()),
                        ("email", format!("{}", id.email).into()),
                        ("source", format!("{}", source).into()),
                    ],
                )
            };
            let resp =
                ui.add(egui::Label::new(RichText::new(who).color(t.text_faint).small()).truncate());
            if id.local || id.local_email {
                resp.on_hover_text(if elsewhere.is_some() {
                    crate::i18n::text("repo-core-0913")
                } else {
                    crate::i18n::text("repo-core-0914")
                });
            }
            // The saved identities, then the authors of the profiles, each once.
            let mut choices: Vec<(String, String, String)> = settings
                .identities
                .iter()
                .map(|p| (p.label.clone(), p.name.clone(), p.email.clone()))
                .collect();
            for p in &settings.user_profiles {
                let (name, email) = (p.author_name.trim(), p.author_email.trim());
                if !name.is_empty()
                    && !email.is_empty()
                    && !choices.iter().any(|(_, n, e)| n == name && e == email)
                {
                    choices.push((
                        crate::i18n::message(
                            "repo-core-0915",
                            &[("title", format!("{}", p.display_title()).into())],
                        ),
                        name.to_owned(),
                        email.to_owned(),
                    ));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = icons::small_button(ui, "user-round", crate::i18n::text("repo-core-0917"));
                egui::Popup::menu(&b)
                    .style(crate::theme::menu_style)
                    .show(|ui| {
                        if choices.is_empty() {
                            ui.label(
                                RichText::new(crate::i18n::text("repo-core-0918"))
                                    .color(t.text_faint),
                            );
                        }
                        for (label, name, email) in &choices {
                            let text = format!("{label} — {name} <{email}>");
                            if ui
                                .add_enabled(idle, egui::Button::new(text))
                                .on_hover_text(crate::i18n::text("repo-core-0920"))
                                .clicked()
                            {
                                acts.push(Action::SetIdentity(name.clone(), email.clone()));
                            }
                        }
                        if (id.local || id.local_email) && elsewhere.is_none() {
                            ui.separator();
                            if ui
                                .add_enabled(
                                    idle,
                                    egui::Button::new(crate::i18n::text("repo-core-0154")),
                                )
                                .on_hover_text(crate::i18n::text("repo-core-0921"))
                                .clicked()
                            {
                                acts.push(Action::ClearIdentity {
                                    name: id.local,
                                    email: id.local_email,
                                });
                            }
                        }
                    });
            });
        });
    }
}
