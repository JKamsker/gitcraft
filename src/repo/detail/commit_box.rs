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
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.label(
                RichText::new(match continuing {
                    Some(s) => format!("CONTINUE {}", s.label().to_uppercase()),
                    None => "COMMIT".to_owned(),
                })
                .font(theme::semibold(10.5))
                .color(t.text_faint),
            );
            let amend = ui.add_enabled(
                idle && !rebasing && !self.refs.head.is_empty(),
                |ui: &mut Ui| icons::toggle_button(ui, "history", "Amend last commit", amending),
            ).on_hover_text("Rewrite the last commit with the staged changes and this message")
                .on_disabled_hover_text(if !idle { "Wait for the operation running to end" } else if rebasing {
                    "The commits being replayed keep their own messages"
                } else { "There are no commits yet" });
            if amend.clicked() {
                amending = !amending;
                amend_started = amending && (self.commit_summary.trim().is_empty() || self.unedited_prefill());
            }
            self.dco.toggle(ui, !rebasing, acts);
            self.fold_picker(ui, amending, acts);
            self.ai_message_button(ui, !rebasing);
            if ui.add_enabled(!rebasing, |ui: &mut Ui| icons::toggle_button(ui, "wrench", "Skip hooks", self.skip_hooks.commit))
                .on_hover_text("Commit once without pre-commit and commit-msg hooks (--no-verify); resets after committing")
                .on_disabled_hover_text("Continue keeps the replayed commit's hooks")
                .clicked() && !rebasing {
                self.skip_hooks.commit = !self.skip_hooks.commit;
            }
            self.compose_toolbar(ui, !rebasing);
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
                    RichText::new(
                        "This commit is already pushed: amending rewrites shared history.",
                    )
                    .color(t.warning)
                    .small(),
                );
            }
        }
        self.compose_prefill(ui.ctx());
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
                RichText::new("The commit keeps its message: reword it once this is done.")
                    .color(t.text_faint)
                    .small(),
            );
        }
        let len = self.commit_summary.chars().count();
        if len > 72 {
            ui.label(
                RichText::new(format!("Summary is {len} characters; 50–72 reads best"))
                    .color(t.warning)
                    .small(),
            );
        }
        if rebasing {
            // `--continue` takes no `--no-verify`: nothing ticked waits for the next commit.
            self.skip_hooks.commit = false;
        }
        ui.add_space(4.0);
        let staged = self.work.staged.len();
        let restack = self.stacks.restack_here().is_some()
            && continuing == Some(crate::git::InProgress::Rebase);
        let label = if restack {
            "Continue restack".to_owned()
        } else if let Some(s) = continuing {
            format!("Continue {}", s.label().to_lowercase())
        } else if self.amend {
            "Amend last commit".to_owned()
        } else if staged > 0 {
            format!("Commit {staged} file{}", if staged == 1 { "" } else { "s" })
        } else {
            "Stage all and commit".to_owned()
        };
        // An unchanged template is refused, as git refuses it.
        let untouched = !rebasing && self.unedited_template();
        let can = idle
            && (rebasing || !self.commit_summary.trim().is_empty())
            && !untouched
            && self.work.conflicted.is_empty();
        // Only from the message fields, so Ctrl+Enter in the terminal stays the shell's.
        let shortcut = (summary.has_focus() || body.has_focus())
            && ui.input(|i| keymap::pressed(i, Scope::Repository).contains(&Command::Commit));
        let resp = widgets::primary_button(ui, &label, can, ui.available_width());
        if (resp.clicked() || shortcut) && can {
            acts.push(Action::Commit);
        }
        if untouched {
            resp.on_hover_text("Edit the message first: it is still the template");
        } else {
            resp.on_hover_text(keymap::label(Command::Commit));
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
                    "this repository".to_owned()
                } else if elsewhere.is_none() && !profile_value.is_empty() && value == profile_value
                {
                    format!("profile {}", profile.title())
                } else if let Some(host) = &elsewhere {
                    format!("global ({host})")
                } else {
                    "global".to_owned()
                }
            };
            let who = if id.name.is_empty() {
                "No identity set (git config user.name)".to_owned()
            } else {
                let name_from = source(id.local, &id.name, &profile.author_name);
                let email_from = source(id.local_email, &id.email, &profile.author_email);
                let source = if name_from == email_from {
                    format!(" · {name_from}")
                } else {
                    format!(" · name: {name_from}, email: {email_from}")
                };
                format!("as {} <{}>{source}", id.name, id.email)
            };
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
                        format!("Profile {}", p.title()),
                        name.to_owned(),
                        email.to_owned(),
                    ));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let b = icons::small_button(ui, "user-round", "Commit as…");
                egui::Popup::menu(&b)
                    .style(crate::theme::menu_style)
                    .show(|ui| {
                        if choices.is_empty() {
                            ui.label(
                                RichText::new("Add identities or profile authors in Settings")
                                    .color(t.text_faint),
                            );
                        }
                        for (label, name, email) in &choices {
                            let text = format!("{label} — {name} <{email}>");
                            if ui
                                .add_enabled(idle, egui::Button::new(text))
                                .on_hover_text("Sets it in this repository's own config")
                                .clicked()
                            {
                                acts.push(Action::SetIdentity(name.clone(), email.clone()));
                            }
                        }
                        if (id.local || id.local_email) && elsewhere.is_none() {
                            ui.separator();
                            if ui
                                .add_enabled(idle, egui::Button::new("Use the profile's author"))
                                .on_hover_text(
                                    "Takes user.name and user.email out of this repository's \
                                     own config, so that the profile's (or your global config's) \
                                     apply",
                                )
                                .clicked()
                            {
                                acts.push(Action::ClearIdentity {
                                    name: id.local,
                                    email: id.local_email,
                                });
                            }
                        }
                    });
                ui.add(
                    egui::Label::new(RichText::new(&who).color(t.text_faint).small()).truncate(),
                )
                .on_hover_text(&who);
            });
        });
    }
}
