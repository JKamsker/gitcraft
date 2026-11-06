//! The commit box under the working-tree changes: message, amend, identity.

use egui::{Key, RichText, Ui};

use crate::icons;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn commit_box(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        // A rebase or patch stopped on conflicts: the button continues it.
        let continuing = self.work.in_progress.filter(|s| s.continues());
        let rebasing = continuing.is_some();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match continuing {
                    Some(s) => format!("CONTINUE {}", s.label().to_uppercase()),
                    None => "COMMIT".to_owned(),
                })
                .font(theme::semibold(10.5))
                .color(t.text_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let amend = ui
                    .add_enabled(
                        !rebasing && !self.refs.head.is_empty(),
                        egui::Checkbox::new(&mut self.amend, "Amend last commit"),
                    )
                    .on_hover_text(
                        "Rewrite the last commit with the staged changes and this message",
                    );
                if amend.changed() && self.amend && self.commit_summary.trim().is_empty() {
                    // Start from the message being amended.
                    if let Ok(msg) = crate::git::head_message(&self.path) {
                        let (summary, body) = msg.split_once('\n').unwrap_or((msg.as_str(), ""));
                        self.commit_summary = summary.trim().to_owned();
                        self.commit_body = body.trim().to_owned();
                    }
                }
            });
        });
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
        ui.add_space(2.0);
        // `--continue` commits with the message git kept: edits here would be lost.
        let summary = ui.add_enabled(
            !rebasing,
            egui::TextEdit::singleline(&mut self.commit_summary)
                .hint_text("Summary")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        let body = ui.add_enabled(
            !rebasing,
            egui::TextEdit::multiline(&mut self.commit_body)
                .hint_text("Description (optional)")
                .desired_width(f32::INFINITY)
                .desired_rows(3)
                .margin(egui::Margin::symmetric(8, 6)),
        );
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
        ui.add_space(4.0);
        let staged = self.work.staged.len();
        let label = if let Some(s) = continuing {
            format!("Continue {}", s.label().to_lowercase())
        } else if self.amend {
            "Amend last commit".to_owned()
        } else if staged > 0 {
            format!("Commit {staged} file{}", if staged == 1 { "" } else { "s" })
        } else {
            "Stage all and commit".to_owned()
        };
        let can = idle
            && (rebasing || !self.commit_summary.trim().is_empty())
            && self.work.conflicted.is_empty();
        // Only from the message fields, so Ctrl+Enter in the terminal stays the shell's.
        let shortcut = (summary.has_focus() || body.has_focus())
            && ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
        let resp = widgets::primary_button(ui, &label, can, ui.available_width());
        if (resp.clicked() || (shortcut && can)) && can {
            acts.push(Action::Commit);
        }
        resp.on_hover_text("Ctrl+Enter");

        ui.add_space(4.0);
        self.identity_row(ui, acts);
    }

    /// Who the commit will be by, and the saved identities to switch to.
    fn identity_row(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let settings = crate::settings::Settings::get(ui.ctx());
        let profile = settings.profile();
        ui.horizontal(|ui| {
            let id = &self.identity;
            let from_profile = !id.local
                && !profile.author_name.trim().is_empty()
                && id.name == profile.author_name.trim();
            let who = if id.name.is_empty() {
                "No identity set (git config user.name)".to_owned()
            } else {
                let source = if id.local {
                    " · this repository".to_owned()
                } else if from_profile {
                    format!(" · profile {}", profile.title())
                } else {
                    " · global".to_owned()
                };
                format!("as {} <{}>{source}", id.name, id.email)
            };
            let resp =
                ui.add(egui::Label::new(RichText::new(who).color(t.text_faint).small()).truncate());
            if id.local || id.local_email {
                resp.on_hover_text(
                    "Set in this repository's own config, which wins over the profile's author",
                );
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
                        if id.local || id.local_email {
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
            });
        });
    }
}
