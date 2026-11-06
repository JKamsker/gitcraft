//! The settings' Profile page: which profile is in use, and what is its own (name, git author,
//! commit signing, theme).

use std::time::Instant;

use egui::{Align2, Color32, RichText, Ui, vec2};

use super::layout::{self, Filter, section};
use super::profiles::{self, UserProfile};
use super::signing::{self, RECHECK};
use super::{FIELD_WIDTH, Settings};
use crate::theme::{self, ThemeId, Tokens};

/// What the profile page asks the app for: profiles switch with the tabs, which are the app's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileRequest {
    Switch(u64),
    /// A new, empty profile, switched to.
    New,
    /// A copy of the profile in use, switched to.
    Duplicate,
    Delete(u64),
    Theme(ThemeId),
}

/// What the settings page needs of the app, and what its Profile page asks the app to do.
pub struct SettingsView<'a> {
    /// The app's theme, which is the profile's.
    pub theme: ThemeId,
    /// What `git --version` said.
    pub git_version: &'a str,
    /// A dialog is over the page (the manual, the palette, the clone or remote dialog): the
    /// keys are the dialog's.
    pub dialog_open: bool,
    /// Where the look for a newer version stands.
    pub update: super::UpdateCheck,
    /// Set by the page: look for a newer version now.
    pub check_updates_now: bool,
    /// The version the last update replaced, when it can be gone back to.
    pub previous: Option<String>,
    /// The version gone back to and stayed on; empty when none.
    pub pinned: String,
    /// Set by the page: go back to `previous`.
    pub go_back: bool,
    /// Set by the page: unpin `pinned`.
    pub unpin: bool,
    /// Set by the page: profiles switch with the tabs, which are the app's.
    pub request: Option<ProfileRequest>,
}

/// The author the user's own global git config sets, read again now and then: it can change
/// outside gitcraft.
#[derive(Clone)]
struct OwnAuthor {
    name: String,
    email: String,
    read_at: Instant,
}

fn own_author(ctx: &egui::Context) -> OwnAuthor {
    let id = egui::Id::new("settings-own-author");
    ctx.data(|d| d.get_temp::<OwnAuthor>(id))
        .filter(|a| a.read_at.elapsed() < RECHECK)
        .unwrap_or_else(|| {
            // `--global` reads the user's own file, not the profile's.
            let get = |key: &str| {
                crate::git::run(None, &["config", "--global", "--get", key])
                    .map(|v| v.trim().to_owned())
                    .unwrap_or_default()
            };
            let author = OwnAuthor {
                name: get("user.name"),
                email: get("user.email"),
                read_at: Instant::now(),
            };
            ctx.data_mut(|d| d.insert_temp(id, author.clone()));
            author
        })
}

/// A profile's round badge: its initials on a colour of its own.
pub fn paint_badge(ui: &Ui, center: egui::Pos2, radius: f32, profile: &UserProfile) {
    let t = Tokens::get(ui.ctx());
    let painter = ui.painter();
    painter.circle_filled(center, radius, t.lane(profile.id as usize));
    let initials = profile.initials();
    let size = if initials.chars().count() > 1 {
        radius * 0.75
    } else {
        radius * 0.95
    };
    painter.text(
        center,
        Align2::CENTER_CENTER,
        initials,
        theme::semibold(size),
        Color32::WHITE,
    );
}

/// A badge laid out inline, like an icon.
pub fn badge(ui: &mut Ui, size: f32, profile: &UserProfile) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    paint_badge(ui, rect.center(), size / 2.0, profile);
}

impl Settings {
    /// The profile in use and its own settings, or what a search matches of them.
    pub(super) fn profile_ui(&mut self, ui: &mut Ui, f: &Filter, view: &mut SettingsView) {
        let t = Tokens::get(ui.ctx());
        let current = self.profile();
        let mut request = None;

        section(
            ui,
            f,
            "Profile",
            Some(
                "Each profile has its own tabs, recent repositories, theme, git author and \
                 commit signing; the other pages are shared. Switching closes this profile's \
                 tabs and opens the other one's.",
            ),
            |card| {
                card.row("In use", "switch profile new duplicate delete", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, 20.0, &current);
                        egui::ComboBox::from_id_salt("settings-profile-pick")
                            .selected_text(current.title())
                            .width(layout::fit(ui, 180.0))
                            .show_ui(ui, |ui| {
                                for p in &self.user_profiles {
                                    if ui.selectable_label(p.id == current.id, p.title()).clicked()
                                        && p.id != current.id
                                    {
                                        request = Some(ProfileRequest::Switch(p.id));
                                    }
                                }
                            });
                        if ui
                            .button("New")
                            .on_hover_text("A new profile, with no tabs, switched to")
                            .clicked()
                        {
                            request = Some(ProfileRequest::New);
                        }
                        if ui
                            .button("Duplicate")
                            .on_hover_text(
                                "A copy of this profile's settings (not its tabs), switched to",
                            )
                            .clicked()
                        {
                            request = Some(ProfileRequest::Duplicate);
                        }
                        if request.is_none() {
                            request = delete_button(ui, &current, self.user_profiles.len());
                        }
                    });
                });
                card.row("Name", "rename title", |ui| {
                    ui.add(
                        layout::field(&mut self.profile_mut().name)
                            .hint_text("Work")
                            .desired_width(layout::fit(ui, 200.0)),
                    );
                });
                card.row_hint(
                    "Theme",
                    "More on the Appearance page",
                    "dark light colours colors appearance",
                    |ui| {
                        egui::ComboBox::from_id_salt("settings-profile-theme")
                            .width(layout::fit(ui, 230.0))
                            .selected_text(view.theme.label())
                            .height(360.0)
                            .show_ui(ui, |ui| {
                                for info in theme::themes().iter() {
                                    if ui
                                        .selectable_label(view.theme == info.id, &info.name)
                                        .clicked()
                                        && view.theme != info.id
                                    {
                                        request = Some(ProfileRequest::Theme(info.id.clone()));
                                    }
                                }
                            });
                    },
                );
            },
        );

        let own = own_author(ui.ctx());
        let hint = |value: &str, key: &str| {
            if value.is_empty() {
                format!("{key}, not set")
            } else {
                value.to_owned()
            }
        };
        section(
            ui,
            f,
            "Git author",
            Some(
                "Commits from gitcraft are by this author. Empty: as in your global git config \
                 (shown greyed). A repository that sets its own user.name or user.email (git \
                 config --local, or \"Commit as…\" in its commit box) keeps them: the \
                 repository's config wins over the profile. A repository in WSL or over SSH \
                 goes by that machine's git config instead.",
            ),
            |card| {
                let profile = self.profile_mut();
                card.row("Name", "user.name author committer", |ui| {
                    ui.add(
                        layout::field(&mut profile.author_name)
                            .hint_text(hint(&own.name, "user.name"))
                            .desired_width(layout::fit(ui, FIELD_WIDTH)),
                    );
                });
                card.row("Email", "user.email author committer", |ui| {
                    ui.add(
                        layout::field(&mut profile.author_email)
                            .hint_text(hint(&own.email, "user.email"))
                            .desired_width(layout::fit(ui, FIELD_WIDTH)),
                    );
                });
            },
        );

        section(ui, f, "Profile's commit signing", None, |card| {
            let profile = self.profile_mut();
            card.row("Signing", "gpg sign key profile own global", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.radio_value(
                        &mut profile.own_signing,
                        false,
                        "As in your global git config",
                    )
                    .on_hover_text("Set on the Commit signing page");
                    let own =
                        ui.radio_value(&mut profile.own_signing, true, "Just for this profile");
                    // Starts from how the global config signs, rather than over it with
                    // nothing (openpgp, signing nothing).
                    if own.changed() && profile.signing == signing::Signing::default() {
                        profile.signing = signing::read(&["--global"]);
                    }
                });
            });
            if profile.own_signing {
                let format = profile.signing.format().to_owned();
                signing::fields_rows(card, &mut profile.signing, "settings-profile-signing");
                // A key and a program are for one format: an SSH key file is no GPG key, and
                // ssh-keygen doesn't sign as gpg.
                if profile.signing.format() != format {
                    profile.signing.key.clear();
                    profile.signing.program.clear();
                }
                card.full("profile signing global override", |ui| {
                    layout::note(
                        ui,
                        "Kept with the profile, over your global git config; empty fields \
                             leave what it sets. A repository's own config can still override it.",
                    );
                });
            }
        });

        if !f.searching() {
            if !profiles::git_reads_profiles(view.git_version) {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "This git ({}) is older than 2.32 and ignores what a profile sets for \
                         git: the author and signing above are not used. Update git to use them.",
                        view.git_version
                    ))
                    .color(t.warning)
                    .small(),
                );
            }
            if let Some(e) = profiles::error() {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(t.danger).small());
            }
        }
        if request.is_some() {
            view.request = request;
        }
    }
}

/// The delete button, which asks once more before deleting.
fn delete_button(ui: &mut Ui, profile: &UserProfile, count: usize) -> Option<ProfileRequest> {
    let id = egui::Id::new(("settings-profile-delete", profile.id));
    let asking = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let mut request = None;
    if asking {
        if ui
            .button(RichText::new("Delete it").color(Tokens::get(ui.ctx()).danger))
            .clicked()
        {
            request = Some(ProfileRequest::Delete(profile.id));
            ui.data_mut(|d| d.remove::<bool>(id));
        }
        if ui.button("Keep").clicked() {
            ui.data_mut(|d| d.remove::<bool>(id));
        }
    } else if ui
        .add_enabled(count > 1, egui::Button::new("Delete"))
        .on_hover_text("Deletes this profile and its list of tabs; gitcraft switches to another")
        .on_disabled_hover_text("The only profile")
        .clicked()
    {
        ui.data_mut(|d| d.insert_temp(id, true));
    }
    request
}
